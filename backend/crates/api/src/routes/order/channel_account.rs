//! Channel API wallet, gift card and affiliate endpoints used by the Telegram bot
//! (`/api/v1/channel/wallet*`, `/channel/affiliate/*`; port of the wallet, gift card and
//! affiliate `channel_handler.go`). The buyer is the shop user provisioned for
//! `channel_user_id`.

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::affiliate::ClickInput;
use zs_app::wallet::RechargeRequest;
use zs_domain::affiliate::{Commission, WithdrawRequest};
use zs_domain::marketing::gift_card::keys as card_keys;
use zs_domain::notify::channel::TelegramIdentityInput;
use zs_domain::payment::wallet_info::extract_crypto_wallet_info;
use zs_domain::{Error, ErrorKind};
use zs_shared::money::Amount;
use zs_shared::page::PageRequest;

use super::channel::{
    Ctx, Failure, Params, VALIDATION, channel_user_of, failure, first_non_empty, fixed, opt_time,
    parse, time, user_from_query, user_of,
};
use crate::client::Client;
use crate::extract::{BindField, BindRules, Query, req};
use crate::routes::Routes;
use crate::state::AppState;

/// Page bounds of `/channel/wallet/transactions` (original default 5, maximum 20).
const TXN_DEFAULT_PAGE_SIZE: u64 = 5;
const TXN_MAX_PAGE_SIZE: u64 = 20;

pub(super) fn routes() -> Routes {
    Routes::new("/channel")
        .get("/wallet", wallet)
        .get("/wallet/transactions", transactions)
        .post("/wallet/recharge", recharge)
        .post("/wallet/gift-card/redeem", redeem)
        .post("/affiliate/click", click)
        .post("/affiliate/open", open)
        .get("/affiliate/dashboard", dashboard)
        .get("/affiliate/commissions", commissions)
        .get("/affiliate/withdraws", withdraws)
        .post("/affiliate/withdraws", apply_withdraw)
}

fn bad(code: &'static str, key: &'static str) -> Failure {
    failure(StatusCode::BAD_REQUEST, code, key)
}

fn paged(items: Vec<Value>, page: u64, page_size: u64, total: u64) -> Value {
    json!({
        "items": items,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total.div_ceil(page_size.max(1)),
    })
}

fn num(q: &std::collections::HashMap<String, String>, key: &str) -> Option<u64> {
    q.get(key)
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
}

/// `channelUserID` + legacy alias of a JSON body.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Who {
    channel_user_id: String,
    telegram_user_id: String,
}

impl Who {
    fn input(&self) -> TelegramIdentityInput {
        TelegramIdentityInput {
            channel_user_id: first_non_empty(&self.channel_user_id, &self.telegram_user_id),
            ..TelegramIdentityInput::default()
        }
    }
}

// ---------------------------------------------------------------------------
// wallet
// ---------------------------------------------------------------------------

async fn wallet(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    match s.svc.wallet.wallet.account(user_id).await {
        Ok(a) => ctx.ok(json!({"balance": fixed(a.balance), "currency": "CNY"})),
        Err(e) => ctx.internal(&e),
    }
}

async fn transactions(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    if channel_user_of(&q).is_empty() {
        return ctx.fail(VALIDATION);
    }
    let page = num(&q, "page").unwrap_or(1);
    let page_size = num(&q, "page_size")
        .unwrap_or(TXN_DEFAULT_PAGE_SIZE)
        .min(TXN_MAX_PAGE_SIZE);
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let result = s
        .svc
        .wallet
        .wallet
        .transactions(user_id, PageRequest { page, page_size })
        .await;
    match result {
        Ok(p) => {
            let items = p
                .items
                .iter()
                .map(|t| {
                    json!({
                        "type": t.kind,
                        "direction": t.direction,
                        "amount": fixed(t.amount),
                        "balance_after": fixed(t.balance_after),
                        "remark": t.remark,
                        "created_at": t.created_at.format("%Y-%m-%d %H:%M").to_string(),
                    })
                })
                .collect();
            ctx.ok(paged(items, page, page_size, p.total))
        }
        Err(e) => ctx.internal(&e),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RechargeBody {
    channel_user_id: String,
    telegram_user_id: String,
    amount: String,
    channel_id: i64,
}

impl BindRules for RechargeBody {
    const FIELDS: &'static [BindField] = &[req("amount", "Amount"), req("channel_id", "ChannelID")];
}

async fn recharge(
    ctx: Ctx,
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> Response {
    let req: RechargeBody = match ctx.bind(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if req.amount.trim().is_empty() || req.channel_id <= 0 {
        return ctx.fail(VALIDATION);
    }
    let input = TelegramIdentityInput {
        channel_user_id: first_non_empty(&req.channel_user_id, &req.telegram_user_id),
        ..TelegramIdentityInput::default()
    };
    let user_id = match user_of(&s, &ctx, &input).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    if req.amount.trim().parse::<Amount>().is_err() {
        return ctx.fail(VALIDATION);
    }
    let view = s
        .svc
        .wallet
        .recharge
        .create(&RechargeRequest {
            user_id,
            channel_id: req.channel_id,
            amount: req.amount.trim().to_owned(),
            currency: String::new(),
            remark: String::new(),
            client_ip: client.ip,
        })
        .await;
    let view = match view {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, user_id, "channel_wallet_recharge_create");
            return ctx.fail(bad("payment_create_failed", "error.payment_create_failed"));
        }
    };
    let payment = view.payment.as_ref().map_or(Value::Null, |p| {
        let mut block = json!({
            "id": p.id,
            "amount": fixed(p.amount),
            "fee_amount": fixed(p.fee_amount),
            "currency": p.currency,
            "status": p.status.as_str(),
            "interaction_mode": p.interaction_mode,
            "pay_url": p.pay_url,
            "qr_code": p.qr_code,
            "expires_at": opt_time(p.expired_at),
        });
        let info =
            extract_crypto_wallet_info(&p.provider_type, &p.interaction_mode, &p.provider_payload);
        if let Some(obj) = block.as_object_mut() {
            for (k, v) in [
                ("wallet_address", info.address),
                ("chain_amount", info.chain_amount),
                ("chain", info.chain),
                ("token_id", info.token_id),
            ] {
                if !v.is_empty() {
                    obj.insert(k.into(), Value::String(v));
                }
            }
        }
        block
    });
    ctx.ok(json!({"recharge_no": view.recharge.recharge_no, "payment": payment}))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct RedeemBody {
    channel_user_id: String,
    telegram_user_id: String,
    code: String,
}

impl BindRules for RedeemBody {
    const FIELDS: &'static [BindField] = &[req("code", "Code")];
}

/// Gift card errors (`respondChannelGiftCardError`).
fn card_failure(e: &Error) -> Failure {
    match e.key() {
        k if k == card_keys::INVALID => bad("gift_card_invalid", card_keys::INVALID),
        k if k == card_keys::NOT_FOUND => failure(
            StatusCode::NOT_FOUND,
            "gift_card_not_found",
            card_keys::NOT_FOUND,
        ),
        k if k == card_keys::EXPIRED => bad("gift_card_expired", card_keys::EXPIRED),
        k if k == card_keys::DISABLED => bad("gift_card_disabled", card_keys::DISABLED),
        k if k == card_keys::REDEEMED => bad("gift_card_redeemed", card_keys::REDEEMED),
        _ => failure(
            StatusCode::INTERNAL_SERVER_ERROR,
            "gift_card_redeem_failed",
            card_keys::REDEEM_FAILED,
        ),
    }
}

async fn redeem(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let req: RedeemBody = match ctx.bind(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if req.code.trim().is_empty() {
        return ctx.fail(VALIDATION);
    }
    let input = TelegramIdentityInput {
        channel_user_id: first_non_empty(&req.channel_user_id, &req.telegram_user_id),
        ..TelegramIdentityInput::default()
    };
    let user_id = match user_of(&s, &ctx, &input).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    match s
        .svc
        .wallet
        .wallet
        .redeem_gift_card(user_id, req.code.trim())
        .await
    {
        Ok(r) => ctx.ok(json!({
            "gift_card": {
                "id": r.card.id,
                "name": r.card.name,
                "code": r.card.code,
                "amount": fixed(r.card.amount),
                "currency": r.card.currency,
                "status": r.card.status,
                "redeemed_at": opt_time(r.card.redeemed_at),
            },
            "wallet": {"balance": fixed(r.account.balance)},
            "transaction": {
                "id": r.transaction.id,
                "type": r.transaction.kind,
                "direction": r.transaction.direction,
                "amount": fixed(r.transaction.amount),
                "balance_after": fixed(r.transaction.balance_after),
                "remark": r.transaction.remark,
                "created_at": time(r.transaction.created_at),
            },
            "wallet_delta": fixed(r.card.amount),
        })),
        Err(e) => {
            tracing::warn!(error = %e, user_id, "channel_wallet_gift_card_redeem_failed");
            ctx.fail(card_failure(&e))
        }
    }
}

// ---------------------------------------------------------------------------
// affiliate
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ClickBody {
    channel_user_id: String,
    telegram_user_id: String,
    affiliate_code: String,
    visitor_key: String,
    landing_path: String,
    referrer: String,
}

impl BindRules for ClickBody {
    const FIELDS: &'static [BindField] = &[req("affiliate_code", "AffiliateCode")];
}

async fn click(
    ctx: Ctx,
    State(s): State<AppState>,
    Client(client): Client,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let req: ClickBody = match ctx.bind(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if req.affiliate_code.trim().is_empty() {
        return ctx.fail(VALIDATION);
    }
    let channel_user = first_non_empty(&req.channel_user_id, &req.telegram_user_id);
    if channel_user.is_empty() {
        return ctx.fail(VALIDATION);
    }
    let visitor_key = match req.visitor_key.trim() {
        "" => channel_user,
        v => v.to_owned(),
    };
    let user_agent = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let tracked = s
        .svc
        .affiliate
        .service
        .track_click(&ClickInput {
            affiliate_code: req.affiliate_code,
            visitor_key,
            landing_path: req.landing_path.trim().to_owned(),
            referrer: req.referrer.trim().to_owned(),
            client_ip: client.ip,
            user_agent,
        })
        .await;
    match tracked {
        Ok(()) => ctx.ok(json!({"ok": true})),
        Err(e) => {
            tracing::error!(error = %e, "channel_affiliate_track_click_failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "affiliate_track_click_failed",
                "error.save_failed",
            ))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OpenBody {
    channel_user_id: String,
    telegram_user_id: String,
    username: String,
    telegram_username: String,
    first_name: String,
    last_name: String,
    avatar_url: String,
}

fn disabled_failure() -> Failure {
    bad("affiliate_disabled", "error.forbidden")
}

async fn open(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let req: OpenBody = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let input = TelegramIdentityInput {
        channel_user_id: first_non_empty(&req.channel_user_id, &req.telegram_user_id),
        username: first_non_empty(&req.username, &req.telegram_username),
        first_name: req.first_name.trim().to_owned(),
        last_name: req.last_name.trim().to_owned(),
        avatar_url: req.avatar_url.trim().to_owned(),
    };
    let user_id = match user_of(&s, &ctx, &input).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    match s.svc.affiliate.service.open(user_id).await {
        Ok(p) => ctx.ok(json!({
            "id": p.id,
            "user_id": p.user_id,
            "code": p.affiliate_code,
            "status": p.status,
            "created_at": time(p.created_at),
            "updated_at": time(p.updated_at),
        })),
        Err(e) if e.key() == "error.forbidden" => ctx.fail(disabled_failure()),
        Err(e) if e.key() == "error.user_not_found" => ctx.fail(failure(
            StatusCode::NOT_FOUND,
            "user_not_found",
            "error.user_not_found",
        )),
        Err(e) => {
            tracing::error!(error = %e, user_id, "channel_affiliate_open_failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "affiliate_open_failed",
                "error.save_failed",
            ))
        }
    }
}

async fn dashboard(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let fetch = failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "affiliate_dashboard_failed",
        "error.user_fetch_failed",
    );
    let svc = &s.svc.affiliate.service;
    let (d, setting) = match (svc.dashboard(user_id).await, svc.setting().await) {
        (Ok(d), Ok(setting)) => (d, setting),
        (Err(e), _) | (_, Err(e)) => {
            tracing::error!(error = %e, user_id, "channel_affiliate_dashboard_failed");
            return ctx.fail(fetch);
        }
    };
    ctx.ok(json!({
        "opened": d.opened,
        "affiliate_code": d.affiliate_code,
        "promotion_path": d.promotion_path,
        "click_count": d.click_count,
        "valid_order_count": d.valid_order_count,
        "conversion_rate": d.conversion_rate,
        "pending_commission": d.pending_commission,
        "available_commission": d.available_commission,
        "withdrawn_commission": d.withdrawn_commission,
        "min_withdraw_amount": setting.min_withdraw_amount,
        "withdraw_channels": setting.withdraw_channels,
    }))
}

fn time_or_empty(t: Option<chrono::DateTime<chrono::Utc>>) -> String {
    t.map(time).unwrap_or_default()
}

fn commission_json(c: &Commission) -> Value {
    json!({
        "id": c.id,
        "affiliate_profile_id": c.affiliate_profile_id,
        "order_id": c.order_id,
        "order_no": c.order.as_ref().map(|o| o.order_no.trim().to_owned()).unwrap_or_default(),
        "order_item_id": c.order_item_id.unwrap_or(0),
        "commission_type": c.commission_type,
        "base_amount": c.base_amount,
        "rate_percent": c.rate_percent,
        "commission_amount": c.commission_amount,
        "status": c.status,
        "confirm_at": time_or_empty(c.confirm_at),
        "available_at": time_or_empty(c.available_at),
        "withdraw_request_id": c.withdraw_request_id.unwrap_or(0),
        "invalid_reason": c.invalid_reason,
        "created_at": time(c.created_at),
        "updated_at": time(c.updated_at),
    })
}

fn withdraw_json(w: &WithdrawRequest) -> Value {
    json!({
        "id": w.id,
        "affiliate_profile_id": w.affiliate_profile_id,
        "amount": w.amount,
        "channel": w.channel,
        "account": w.account,
        "status": w.status,
        "reject_reason": w.reject_reason,
        "processed_by": w.processed_by.unwrap_or(0),
        "processed_at": time_or_empty(w.processed_at),
        "created_at": time(w.created_at),
        "updated_at": time(w.updated_at),
    })
}

async fn commissions(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let page = PageRequest::new(num(&q, "page"), num(&q, "page_size"));
    let status = q
        .get("status")
        .map(|v| v.trim().to_owned())
        .unwrap_or_default();
    match s
        .svc
        .affiliate
        .service
        .user_commissions(user_id, &status, page)
        .await
    {
        Ok(p) => ctx.ok(paged(
            p.items.iter().map(commission_json).collect(),
            page.page,
            page.page_size,
            p.total,
        )),
        Err(e) => {
            tracing::error!(error = %e, user_id, "channel_affiliate_commissions_failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "affiliate_commissions_failed",
                "error.user_fetch_failed",
            ))
        }
    }
}

async fn withdraws(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let page = PageRequest::new(num(&q, "page"), num(&q, "page_size"));
    let status = q
        .get("status")
        .map(|v| v.trim().to_owned())
        .unwrap_or_default();
    match s
        .svc
        .affiliate
        .service
        .user_withdraws(user_id, &status, page)
        .await
    {
        Ok(p) => ctx.ok(paged(
            p.items.iter().map(withdraw_json).collect(),
            page.page,
            page.page_size,
            p.total,
        )),
        Err(e) => {
            tracing::error!(error = %e, user_id, "channel_affiliate_withdraws_failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "affiliate_withdraws_failed",
                "error.user_fetch_failed",
            ))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WithdrawBody {
    channel_user_id: String,
    telegram_user_id: String,
    amount: String,
    channel: String,
    account: String,
}

impl BindRules for WithdrawBody {
    const FIELDS: &'static [BindField] = &[
        req("amount", "Amount"),
        req("channel", "Channel"),
        req("account", "Account"),
    ];
}

/// `ApplyAffiliateWithdraw`: the failure classes of the original (disabled, not opened,
/// amount, channel, insufficient) are resolved before the ledger transaction.
async fn apply_withdraw(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let req: WithdrawBody = match ctx.bind(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if req.amount.trim().is_empty()
        || req.channel.trim().is_empty()
        || req.account.trim().is_empty()
    {
        return ctx.fail(VALIDATION);
    }
    let who = Who {
        channel_user_id: req.channel_user_id.clone(),
        telegram_user_id: req.telegram_user_id.clone(),
    };
    if who.input().channel_user_id.is_empty() {
        return ctx.fail(VALIDATION);
    }
    let amount_invalid = bad("affiliate_withdraw_amount_invalid", "error.bad_request");
    let Ok(amount) = req.amount.trim().parse::<Amount>() else {
        return ctx.fail(amount_invalid);
    };
    let user_id = match user_of(&s, &ctx, &who.input()).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let svc = &s.svc.affiliate.service;
    let setting = match svc.setting().await {
        Ok(v) => v,
        Err(e) => return ctx.internal(&e),
    };
    if !setting.enabled {
        return ctx.fail(disabled_failure());
    }
    match svc.dashboard(user_id).await {
        Ok(d) if !d.opened => return ctx.fail(bad("affiliate_not_opened", "error.bad_request")),
        Ok(_) => {}
        Err(e) => return ctx.internal(&e),
    }
    let min = req_min(setting.min_withdraw_amount);
    if !amount.is_positive() || amount < min {
        return ctx.fail(amount_invalid);
    }
    let channel = req.channel.trim().to_lowercase();
    if !setting.withdraw_channels.is_empty()
        && !setting
            .withdraw_channels
            .iter()
            .any(|c| c.trim().to_lowercase() == channel)
    {
        return ctx.fail(bad(
            "affiliate_withdraw_channel_invalid",
            "error.bad_request",
        ));
    }
    match svc
        .apply_withdraw(
            user_id,
            req.amount.trim(),
            req.channel.trim(),
            req.account.trim(),
        )
        .await
    {
        Ok(w) => ctx.ok(withdraw_json(&w)),
        Err(e) if e.key() == "error.forbidden" => ctx.fail(disabled_failure()),
        Err(e) if e.kind() != ErrorKind::Internal => {
            ctx.fail(bad("affiliate_withdraw_insufficient", "error.bad_request"))
        }
        Err(e) => {
            tracing::error!(error = %e, user_id, "channel_affiliate_apply_withdraw_failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "affiliate_withdraw_apply_failed",
                "error.save_failed",
            ))
        }
    }
}

/// `min_withdraw_amount` (a float setting) as an amount.
fn req_min(v: f64) -> Amount {
    format!("{v:.2}").parse().unwrap_or(Amount::ZERO)
}

#[cfg(test)]
mod tests {
    use super::super::channel::INTERNAL;
    use super::*;

    #[test]
    fn gift_card_errors_keep_codes() {
        assert_eq!(
            card_failure(&Error::bad_request(card_keys::REDEEMED)).error_code,
            "gift_card_redeemed"
        );
        assert_eq!(
            card_failure(&Error::not_found(card_keys::NOT_FOUND)).http,
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            card_failure(&Error::internal_msg("x")).error_code,
            "gift_card_redeem_failed"
        );
    }

    #[test]
    fn min_withdraw_amount_rounds_to_cents() {
        assert_eq!(req_min(10.0).to_string(), "10.00");
        assert_eq!(req_min(0.0), Amount::ZERO);
    }

    #[test]
    fn internal_failure_is_500() {
        assert_eq!(INTERNAL.http, StatusCode::INTERNAL_SERVER_ERROR);
    }
}
