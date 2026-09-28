//! `wallet` endpoints: the user's wallet, top-ups and gift card redemption, and the
//! admin wallet pages (compliance gated **[C]**).

use std::collections::{BTreeMap, HashMap};

use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_app::wallet::{
    AdminRechargeItem, CAPTCHA_SCENE_GIFT_CARD_REDEEM, RechargeRequest, RechargeView,
};
use zs_domain::identity::captcha::CaptchaPayload;
use zs_domain::payment::wallet_info::extract_crypto_wallet_info;
use zs_domain::wallet::{Account, RechargeFilter, RechargeOrder, Transaction, keys};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::{PageRequest, Pagination};

use super::{RouteSet, Routes};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Query, req};
use crate::middleware::auth::{CurrentAdmin, CurrentUser};
use crate::middleware::compliance::ComplianceAcked;
use crate::middleware::rate_limit::{MSG_RATE_LIMITED, check};
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::state::AppState;

/// Routes of the `wallet` group.
pub fn routes() -> RouteSet {
    RouteSet {
        user: Routes::new("")
            .get("/wallet", wallet)
            .get("/wallet/transactions", transactions)
            .post("/wallet/payment-channels", payment_channels)
            .post("/wallet/recharge", recharge)
            .get("/wallet/recharges", recharges)
            .get("/wallet/recharges/stats", recharge_stats)
            .get("/wallet/recharges/{recharge_no}", recharge_detail)
            .post("/wallet/recharge/payments/{id}/capture", capture)
            .post("/gift-cards/redeem", redeem),
        admin: Routes::new("/admin")
            .get("/users/{id}/wallet", admin_wallet)
            .get("/users/{id}/wallet/transactions", admin_transactions)
            .post("/users/{id}/wallet/adjust", admin_adjust)
            .get("/wallet/recharges", admin_recharges),
        ..RouteSet::default()
    }
}

type Params = Query<HashMap<String, String>>;

/// Lenient `page` / `page_size` (`strconv.Atoi` failures fall back to defaults).
fn page_of(q: &HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

fn text(q: &HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

/// `ginutil.ParseQueryUint(raw, false)`: blank = 0, garbage = `error.bad_request`.
fn opt_id(q: &HashMap<String, String>, key: &str) -> Result<Id, ApiError> {
    match q.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(0),
        Some(s) => s.parse::<Id>().map_err(|_| Error::invalid().into()),
    }
}

/// `ginutil.ParseTimeNullable` (RFC 3339).
fn opt_time(q: &HashMap<String, String>, key: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    match q.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|t| Some(t.with_timezone(&Utc)))
            .map_err(|_| Error::invalid().into()),
    }
}

/// A required amount field sent as a string (numbers are accepted too).
fn amount_text(v: Option<&Value>) -> Result<String, ApiError> {
    match v {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err(Error::invalid().into()),
    }
}

/// Parses the `:id` of admin user routes (`error.user_id_invalid`).
fn user_id(raw: &str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(keys::USER_ID_INVALID).into())
}

// ---------------------------------------------------------------------------
// Response shapes (original `walletpresenter`)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct AccountResp {
    balance: Amount,
}

impl From<&Account> for AccountResp {
    fn from(a: &Account) -> Self {
        Self { balance: a.balance }
    }
}

#[derive(Debug, Serialize)]
struct TransactionResp {
    id: Id,
    #[serde(rename = "type")]
    kind: String,
    direction: String,
    amount: Amount,
    balance_after: Amount,
    remark: String,
    created_at: DateTime<Utc>,
}

impl From<Transaction> for TransactionResp {
    fn from(t: Transaction) -> Self {
        Self {
            id: t.id,
            kind: t.kind,
            direction: t.direction,
            amount: t.amount,
            balance_after: t.balance_after,
            remark: t.remark,
            created_at: t.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct RechargeResp {
    id: Id,
    recharge_no: String,
    amount: Amount,
    payable_amount: Amount,
    fee_rate: Amount,
    fee_amount: Amount,
    currency: String,
    status: String,
    remark: String,
    paid_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl From<RechargeOrder> for RechargeResp {
    fn from(r: RechargeOrder) -> Self {
        Self {
            id: r.id,
            recharge_no: r.recharge_no,
            amount: r.amount,
            payable_amount: r.payable_amount,
            fee_rate: r.fee_rate,
            fee_amount: r.fee_amount,
            currency: r.currency,
            status: r.status,
            remark: r.remark,
            paid_at: r.paid_at,
            created_at: r.created_at,
        }
    }
}

fn skip_empty(s: &str) -> bool {
    s.is_empty()
}

/// `WalletRechargePaymentPayload` (every field omitted when empty).
#[derive(Debug, Serialize)]
struct RechargePayload {
    recharge: RechargeResp,
    recharge_no: String,
    recharge_status: String,
    account: AccountResp,
    #[serde(skip_serializing_if = "Option::is_none")]
    payment_id: Option<Id>,
    #[serde(skip_serializing_if = "skip_empty")]
    provider_type: String,
    #[serde(skip_serializing_if = "skip_empty")]
    channel_type: String,
    #[serde(skip_serializing_if = "skip_empty")]
    interaction_mode: String,
    #[serde(skip_serializing_if = "skip_empty")]
    pay_url: String,
    #[serde(skip_serializing_if = "skip_empty")]
    qr_code: String,
    #[serde(skip_serializing_if = "skip_empty")]
    wallet_address: String,
    #[serde(skip_serializing_if = "skip_empty")]
    chain_amount: String,
    #[serde(skip_serializing_if = "skip_empty")]
    chain: String,
    #[serde(skip_serializing_if = "skip_empty")]
    token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "skip_empty")]
    status: String,
    #[serde(skip_serializing_if = "skip_empty")]
    fee_policy: String,
}

impl From<RechargeView> for RechargePayload {
    fn from(v: RechargeView) -> Self {
        let mut p = Self {
            recharge_no: v.recharge.recharge_no.clone(),
            recharge_status: v.recharge.status.clone(),
            recharge: v.recharge.into(),
            account: (&v.account).into(),
            payment_id: None,
            provider_type: String::new(),
            channel_type: String::new(),
            interaction_mode: String::new(),
            pay_url: String::new(),
            qr_code: String::new(),
            wallet_address: String::new(),
            chain_amount: String::new(),
            chain: String::new(),
            token_id: String::new(),
            expires_at: None,
            status: String::new(),
            fee_policy: String::new(),
        };
        if let Some(pay) = v.payment {
            let info = extract_crypto_wallet_info(
                &pay.provider_type,
                &pay.interaction_mode,
                &pay.provider_payload,
            );
            p.payment_id = Some(pay.id);
            p.provider_type = pay.provider_type;
            p.channel_type = pay.channel_type;
            p.interaction_mode = pay.interaction_mode;
            p.pay_url = pay.pay_url;
            p.qr_code = pay.qr_code;
            p.expires_at = pay.expired_at;
            p.status = pay.status.as_str().to_owned();
            p.fee_policy = pay.fee_policy;
            p.wallet_address = info.address;
            p.chain_amount = info.chain_amount;
            p.chain = info.chain;
            p.token_id = info.token_id;
        }
        p
    }
}

// ---------------------------------------------------------------------------
// User endpoints
// ---------------------------------------------------------------------------

async fn wallet(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
) -> ApiResult<Data<AccountResp>> {
    let account = s.svc.wallet.wallet.account(u.id).await?;
    ok((&account).into())
}

async fn transactions(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Params,
) -> ApiResult<Paged<TransactionResp>> {
    let req = page_of(&q);
    let page = s.svc.wallet.wallet.transactions(u.id, req).await?;
    Ok(Paged(
        page.items.into_iter().map(Into::into).collect(),
        Pagination::new(req, page.total),
    ))
}

#[derive(Debug, Deserialize)]
struct ChannelsRequest {
    amount: Option<Value>,
}

impl BindRules for ChannelsRequest {
    const FIELDS: &'static [BindField] = &[req("amount", "Amount")];
}

async fn payment_channels(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Bind(req): Bind<ChannelsRequest>,
) -> ApiResult<Data<Value>> {
    let amount = amount_text(req.amount.as_ref())?;
    let channels = s.svc.wallet.recharge.channels(u.id, &amount).await?;
    ok(serde_json::to_value(channels).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct RechargeBody {
    amount: Option<Value>,
    #[serde(default)]
    channel_id: Id,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    remark: String,
}

impl BindRules for RechargeBody {
    const FIELDS: &'static [BindField] = &[req("amount", "Amount"), req("channel_id", "ChannelID")];
}

async fn recharge(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Client(client): Client,
    Bind(req): Bind<RechargeBody>,
) -> ApiResult<Data<RechargePayload>> {
    let amount = amount_text(req.amount.as_ref())?;
    if req.channel_id <= 0 {
        return Err(Error::invalid().into());
    }
    let view = s
        .svc
        .wallet
        .recharge
        .create(&RechargeRequest {
            user_id: u.id,
            channel_id: req.channel_id,
            amount,
            currency: req.currency.trim().to_owned(),
            remark: req.remark.trim().to_owned(),
            client_ip: client.ip,
        })
        .await?;
    ok(view.into())
}

async fn recharges(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Params,
) -> ApiResult<Paged<RechargeResp>> {
    let req = page_of(&q);
    let page = s
        .svc
        .wallet
        .wallet
        .user_recharges(u.id, &text(&q, "status"), &text(&q, "recharge_no"), req)
        .await?;
    Ok(Paged(
        page.items.into_iter().map(Into::into).collect(),
        Pagination::new(req, page.total),
    ))
}

#[derive(Debug, Serialize)]
struct StatsResp {
    total: i64,
    by_status: BTreeMap<String, i64>,
}

async fn recharge_stats(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Params,
) -> ApiResult<Data<StatsResp>> {
    let (total, by_status) = s
        .svc
        .wallet
        .wallet
        .recharge_stats(u.id, &text(&q, "recharge_no"))
        .await?;
    ok(StatsResp { total, by_status })
}

async fn recharge_detail(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(recharge_no): Path<String>,
) -> ApiResult<Data<RechargePayload>> {
    let view = s.svc.wallet.recharge.detail(u.id, &recharge_no).await?;
    ok(view.into())
}

async fn capture(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Path(raw): Path<String>,
) -> ApiResult<Data<RechargePayload>> {
    let payment_id = raw
        .trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(keys::PAYMENT_INVALID))?;
    let view = s.svc.wallet.recharge.capture(u.id, payment_id).await?;
    ok(view.into())
}

#[derive(Debug, Deserialize)]
struct RedeemRequest {
    #[serde(default)]
    code: String,
    #[serde(default)]
    captcha_payload: CaptchaPayload,
}

impl BindRules for RedeemRequest {
    const FIELDS: &'static [BindField] = &[req("code", "Code")];
}

#[derive(Debug, Serialize)]
struct GiftCardResp {
    id: Id,
    name: String,
    code: String,
    amount: Amount,
    currency: String,
    status: String,
    redeemed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct RedeemResp {
    gift_card: GiftCardResp,
    wallet: AccountResp,
    transaction: TransactionResp,
    wallet_delta: Amount,
}

/// `POST /gift-cards/redeem`: rate limited per `user_id|ip` (RISK-02), captcha scene
/// `gift_card_redeem`, then one transaction locking the card and crediting the wallet.
async fn redeem(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Client(client): Client,
    Bind(req): Bind<RedeemRequest>,
) -> ApiResult<Data<RedeemResp>> {
    check(
        &s.svc.wallet.redeem_limiter,
        &format!("{}|{}", u.id, client.ip),
        MSG_RATE_LIMITED,
    )?;
    if req.code.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .identity
        .captcha
        .verify(
            CAPTCHA_SCENE_GIFT_CARD_REDEEM,
            &req.captcha_payload,
            &client.ip,
        )
        .await?;
    let r = s
        .svc
        .wallet
        .wallet
        .redeem_gift_card(u.id, &req.code)
        .await?;
    ok(RedeemResp {
        wallet_delta: r.card.amount,
        gift_card: GiftCardResp {
            id: r.card.id,
            name: r.card.name,
            code: r.card.code,
            amount: r.card.amount,
            currency: r.card.currency,
            status: r.card.status,
            redeemed_at: r.card.redeemed_at,
        },
        wallet: (&r.account).into(),
        transaction: r.transaction.into(),
    })
}

// ---------------------------------------------------------------------------
// Admin endpoints (all compliance gated)
// ---------------------------------------------------------------------------

async fn admin_wallet(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(raw): Path<String>,
) -> ApiResult<Data<Value>> {
    let id = user_id(&raw)?;
    let (user, account) = s.svc.wallet.wallet.admin_user_wallet(id).await?;
    ok(json!({ "user": user, "account": account }))
}

async fn admin_transactions(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(raw): Path<String>,
    Query(q): Params,
) -> ApiResult<Paged<Transaction>> {
    let id = user_id(&raw)?;
    let req = page_of(&q);
    let page = s
        .svc
        .wallet
        .wallet
        .admin_transactions(id, &text(&q, "type"), &text(&q, "direction"), req)
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

#[derive(Debug, Deserialize)]
struct AdjustRequest {
    amount: Option<Value>,
    #[serde(default)]
    operation: String,
    #[serde(default)]
    currency: String,
    #[serde(default)]
    remark: String,
}

impl BindRules for AdjustRequest {
    const FIELDS: &'static [BindField] = &[req("amount", "Amount")];
}

async fn admin_adjust(
    State(s): State<AppState>,
    _: ComplianceAcked,
    CurrentAdmin(admin): CurrentAdmin,
    Path(raw): Path<String>,
    Bind(req): Bind<AdjustRequest>,
) -> ApiResult<Data<Value>> {
    let id = user_id(&raw)?;
    let amount = amount_text(req.amount.as_ref())?;
    let (account, transaction) = s
        .svc
        .wallet
        .wallet
        .admin_adjust(
            admin.id,
            id,
            &amount,
            &req.operation,
            &req.currency,
            &req.remark,
        )
        .await?;
    ok(json!({ "account": account, "transaction": transaction }))
}

async fn admin_recharges(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Params,
) -> ApiResult<Paged<AdminRechargeItem>> {
    let lower = |k: &str| text(&q, k).to_lowercase();
    let filter = RechargeFilter {
        recharge_no: text(&q, "recharge_no"),
        user_id: opt_id(&q, "user_id")?,
        user_keyword: text(&q, "user_keyword"),
        payment_id: opt_id(&q, "payment_id")?,
        channel_id: opt_id(&q, "channel_id")?,
        provider_type: lower("provider_type"),
        channel_type: lower("channel_type"),
        status: lower("status"),
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
        paid_from: opt_time(&q, "paid_from")?,
        paid_to: opt_time(&q, "paid_to")?,
    };
    let req = page_of(&q);
    let page = s.svc.wallet.wallet.admin_recharges(&filter, req).await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}
