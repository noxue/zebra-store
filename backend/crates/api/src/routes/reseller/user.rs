//! Reseller console `/api/v1/reseller/*` (`transport/http/user`). Every handler
//! takes [`ConsoleUser`], which rejects requests coming from a reseller site.

use axum::extract::multipart::MultipartRejection;
use axum::extract::{DefaultBodyLimit, Multipart, Path, State};
use axum::handler::Handler;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::reseller::product_setting::{Owner, ProductQuery};
use zs_domain::reseller::ports::{FinanceFilter, OrderFilter};
use zs_domain::reseller::site::SiteConfigInput;
use zs_domain::reseller::{keys, not_opened};
use zs_domain::{Error, Id};

use super::dto::{
    self, BalanceResp, DomainResp, LedgerResp, ManagementProfileResp, OrderResp, WithdrawResp,
};
use super::{Num, Params, SettingsRequest, console, num};
use crate::extract::{Bind, BindField, BindRules, Body, Query, req};
use crate::middleware::tenant::ConsoleUser;
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Upload scene of reseller images.
const UPLOAD_SCENE: &str = "reseller";

pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/reseller/profile", profile)
        .post("/reseller/apply", apply)
        .get("/reseller/domains", list_domains)
        .post("/reseller/domains", submit_domain)
        .get("/reseller/site-config", get_site_config)
        .put("/reseller/site-config", put_site_config)
        // The multipart body is size-checked while streaming (config.upload.max_size).
        .post(
            "/reseller/upload",
            upload.layer(DefaultBodyLimit::disable()),
        )
        .get("/reseller/product-settings", list_products)
        .get("/reseller/product-settings/{product_id}", get_product)
        .post(
            "/reseller/product-settings/{product_id}/preview",
            preview_product,
        )
        .put("/reseller/product-settings/{product_id}", save_product)
        .delete("/reseller/product-settings/{product_id}", reset_product)
        .get("/reseller/dashboard", dashboard)
        .get("/reseller/balance-accounts", balances)
        .get("/reseller/ledger-entries", ledger)
        .get("/reseller/withdraws", withdraws)
        .post("/reseller/withdraws", apply_withdraw)
        .get("/reseller/orders", orders)
        .get("/reseller/orders/stats", order_stats)
        .get("/reseller/orders/{order_no}", order_detail)
}

fn user_fetch(e: Error) -> ApiError {
    console(e, "error.user_fetch_failed")
}

fn save_failed(e: Error) -> ApiError {
    console(e, "error.save_failed")
}

async fn profile(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
) -> ApiResult<Data<Value>> {
    let snap = s
        .svc
        .reseller
        .management
        .snapshot(uid)
        .await
        .map_err(user_fetch)?;
    let mut out = json!({
        "opened": snap.profile.is_some(),
        "can_apply": snap.can_apply,
        "domains": dto::domains(&snap.domains),
    });
    if let Some(p) = &snap.profile {
        out["profile"] = json!(ManagementProfileResp::from(p));
    }
    ok(out)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ApplyRequest {
    reason: String,
}

async fn apply(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Body(req): Body<ApplyRequest>,
) -> ApiResult<Data<ManagementProfileResp>> {
    let p = s
        .svc
        .reseller
        .management
        .apply(uid, &req.reason)
        .await
        .map_err(save_failed)?;
    ok(ManagementProfileResp::from(&p))
}

async fn list_domains(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
) -> ApiResult<Data<Vec<DomainResp>>> {
    let snap = s
        .svc
        .reseller
        .management
        .snapshot(uid)
        .await
        .map_err(user_fetch)?;
    if snap.profile.is_none() {
        return Err(not_opened().into());
    }
    ok(dto::domains(&snap.domains))
}

#[derive(Debug, Deserialize)]
struct DomainRequest {
    domain: String,
}

impl BindRules for DomainRequest {
    const FIELDS: &'static [BindField] = &[req("domain", "Domain")];
}

async fn submit_domain(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Bind(req): Bind<DomainRequest>,
) -> ApiResult<Data<DomainResp>> {
    if req.domain.is_empty() {
        return Err(Error::invalid().into());
    }
    let d = s
        .svc
        .reseller
        .management
        .submit_custom_domain(uid, &req.domain)
        .await
        .map_err(save_failed)?;
    ok(DomainResp::from(&d))
}

async fn get_site_config(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
) -> ApiResult<Data<Value>> {
    let snap = s
        .svc
        .reseller
        .site_config
        .user_snapshot(uid)
        .await
        .map_err(user_fetch)?;
    let mut out = json!({"opened": true, "can_edit": snap.can_edit});
    if let Some(c) = &snap.config {
        out["config"] = dto::site_config(c);
    }
    ok(out)
}

async fn put_site_config(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Body(req): Body<SiteConfigInput>,
) -> ApiResult<Data<Value>> {
    let saved = s
        .svc
        .reseller
        .site_config
        .update_user(uid, &req)
        .await
        .map_err(save_failed)?;
    ok(dto::site_config(&saved))
}

/// `POST /reseller/upload` (multipart `file`, scene `reseller`); active resellers only.
async fn upload(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    form: Result<Multipart, MultipartRejection>,
) -> ApiResult<Data<Value>> {
    let can_edit = s
        .svc
        .reseller
        .site_config
        .can_edit(uid)
        .await
        .map_err(|e| console(e, "error.forbidden"))?;
    if !can_edit {
        return Err(Error::forbidden(keys::FORBIDDEN).into());
    }
    let mut form = form.map_err(|_| Error::bad_request("error.file_missing"))?;
    let upload = &s.svc.content.upload;
    let mut file: Option<(String, Vec<u8>)> = None;
    while let Some(mut field) = form
        .next_field()
        .await
        .map_err(|_| Error::bad_request("error.file_missing"))?
    {
        if field.name() != Some("file") || file.is_some() {
            continue;
        }
        let name = field.file_name().unwrap_or_default().to_owned();
        let mut buf = Vec::new();
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|_| Error::bad_request("error.file_missing"))?
        {
            if (buf.len() + chunk.len()) as u64 > upload.max_size() {
                return Err(upload.too_large().into());
            }
            buf.extend_from_slice(&chunk);
        }
        file = Some((name, buf));
    }
    let Some((name, bytes)) = file else {
        return Err(Error::bad_request("error.file_missing").into());
    };
    let stored = upload
        .save(&name, &bytes, UPLOAD_SCENE)
        .await
        .map_err(|e| ApiError::from(e.or_internal("error.upload_failed")))?;
    ok(json!({"url": stored.url, "filename": stored.filename, "size": stored.size}))
}

fn product_id(raw: &str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::invalid().into())
}

async fn list_products(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Query(q): Query<Params>,
) -> ApiResult<Paged<Value>> {
    let page = q.page();
    let query = ProductQuery {
        category_id: q.id("category_id"),
        keyword: q.str("keyword"),
        configured: q.str("configured"),
        listed: q.str("listed"),
    };
    let rows = s
        .svc
        .reseller
        .product_settings
        .list(uid, &query, page)
        .await
        .map_err(user_fetch)?;
    let items = rows.items.iter().map(dto::setting_detail).collect();
    Ok(Paged(
        items,
        zs_shared::page::Pagination::new(page, rows.total),
    ))
}

async fn get_product(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Path(pid): Path<String>,
) -> ApiResult<Data<Value>> {
    let view = s
        .svc
        .reseller
        .product_settings
        .get(Owner::User(uid), product_id(&pid)?)
        .await
        .map_err(user_fetch)?;
    ok(dto::setting_detail(&view))
}

async fn preview_product(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Path(pid): Path<String>,
    Body(req): Body<SettingsRequest>,
) -> ApiResult<Data<Value>> {
    let pid = product_id(&pid)?;
    let items = s
        .svc
        .reseller
        .product_settings
        .preview(Owner::User(uid), pid, &req.inputs()?)
        .await
        .map_err(user_fetch)?;
    ok(dto::preview(&items))
}

async fn save_product(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Path(pid): Path<String>,
    Body(req): Body<SettingsRequest>,
) -> ApiResult<Data<Value>> {
    let pid = product_id(&pid)?;
    let view = s
        .svc
        .reseller
        .product_settings
        .save(Owner::User(uid), pid, &req.inputs()?)
        .await
        .map_err(save_failed)?;
    ok(dto::setting_detail(&view))
}

async fn reset_product(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Path(pid): Path<String>,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let pid = product_id(&pid)?;
    let sku_id = q.strict_id("sku_id")?.unwrap_or(0);
    s.svc
        .reseller
        .product_settings
        .reset(Owner::User(uid), pid, sku_id)
        .await
        .map_err(save_failed)?;
    ok(json!({"deleted": true}))
}

async fn dashboard(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
) -> ApiResult<Data<Value>> {
    let d = s
        .svc
        .reseller
        .finance
        .dashboard(uid)
        .await
        .map_err(|e| ApiError::from(e.or_internal("error.user_fetch_failed")))?;
    let Some(p) = &d.profile else {
        return ok(json!({"opened": false, "withdraw_enabled": false}));
    };
    let mut out = json!({
        "opened": true,
        "profile": {
            "id": p.id,
            "status": p.status,
            "settlement_status": p.settlement_status,
            "created_at": p.created_at,
        },
        "withdraw_enabled": d.withdraw_enabled,
    });
    if !d.balances.is_empty() {
        out["balances"] = json!(d.balances.iter().map(BalanceResp::from).collect::<Vec<_>>());
    }
    if !d.withdraw_disabled_reason.is_empty() {
        out["withdraw_disabled_reason"] = json!(d.withdraw_disabled_reason);
    }
    ok(out)
}

/// Finance errors are shown verbatim (`respondUserFinanceError`).
fn finance(e: Error, fallback: &'static str) -> ApiError {
    e.or_internal(fallback).into()
}

async fn balances(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Query(q): Query<Params>,
) -> ApiResult<Paged<BalanceResp>> {
    let page = q.page();
    let filter = FinanceFilter {
        status: q.str("status"),
        ..FinanceFilter::default()
    };
    let rows = s
        .svc
        .reseller
        .finance
        .user_balances(uid, filter, page)
        .await
        .map_err(|e| finance(e, "error.user_fetch_failed"))?;
    let items = rows.items.iter().map(BalanceResp::from).collect();
    Ok(Paged(
        items,
        zs_shared::page::Pagination::new(page, rows.total),
    ))
}

async fn ledger(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Query(q): Query<Params>,
) -> ApiResult<Paged<LedgerResp>> {
    let page = q.page();
    let filter = FinanceFilter {
        kind: q.str("type"),
        status: q.str("status"),
        order_id: q.strict_id("order_id")?,
        ..FinanceFilter::default()
    };
    let rows = s
        .svc
        .reseller
        .finance
        .user_ledger(uid, filter, page)
        .await
        .map_err(|e| finance(e, "error.user_fetch_failed"))?;
    let items = rows.items.iter().map(LedgerResp::from).collect();
    Ok(Paged(
        items,
        zs_shared::page::Pagination::new(page, rows.total),
    ))
}

async fn withdraws(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Query(q): Query<Params>,
) -> ApiResult<Paged<WithdrawResp>> {
    let page = q.page();
    let filter = FinanceFilter {
        status: q.str("status"),
        ..FinanceFilter::default()
    };
    let rows = s
        .svc
        .reseller
        .finance
        .user_withdraws(uid, filter, page)
        .await
        .map_err(|e| finance(e, "error.user_fetch_failed"))?;
    let items = rows.items.iter().map(WithdrawResp::from).collect();
    Ok(Paged(
        items,
        zs_shared::page::Pagination::new(page, rows.total),
    ))
}

/// `POST /reseller/withdraws` — all fields required (`binding:"required"`).
#[derive(Debug, Deserialize)]
struct WithdrawRequestBody {
    amount: Num,
    currency: String,
    channel: String,
    account: String,
}

impl BindRules for WithdrawRequestBody {
    const FIELDS: &'static [BindField] = &[
        req("amount", "Amount"),
        req("currency", "Currency"),
        req("channel", "Channel"),
        req("account", "Account"),
    ];
}

async fn apply_withdraw(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Bind(req): Bind<WithdrawRequestBody>,
) -> ApiResult<Data<WithdrawResp>> {
    if [&req.currency, &req.channel, &req.account]
        .iter()
        .any(|v| v.is_empty())
    {
        return Err(Error::invalid().into());
    }
    let amount = num(Some(&req.amount))?;
    let row = s
        .svc
        .reseller
        .finance
        .apply_withdraw(
            uid,
            amount.decimal(),
            &req.currency,
            &req.channel,
            &req.account,
        )
        .await
        .map_err(|e| finance(e, "error.save_failed"))?;
    ok(WithdrawResp::from(&row))
}

fn order_filter(q: &Params) -> Result<OrderFilter, ApiError> {
    Ok(OrderFilter {
        status: q.str("status"),
        order_no: q.str("order_no"),
        created_from: q.day_time("created_from", false)?,
        created_to: q.day_time("created_to", true)?,
        paid_from: q.day_time("paid_from", false)?,
        paid_to: q.day_time("paid_to", true)?,
    })
}

fn order_error(e: Error) -> ApiError {
    console(e, "error.order_fetch_failed")
}

async fn orders(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Query(q): Query<Params>,
) -> ApiResult<Paged<OrderResp>> {
    let page = q.page();
    let filter = order_filter(&q)?;
    let rows = s
        .svc
        .reseller
        .orders
        .list_user(uid, &filter, page)
        .await
        .map_err(order_error)?;
    let items = rows.items.iter().map(OrderResp::from).collect();
    Ok(Paged(
        items,
        zs_shared::page::Pagination::new(page, rows.total),
    ))
}

async fn order_stats(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let filter = order_filter(&q)?;
    let stats = s
        .svc
        .reseller
        .orders
        .stats_user(uid, &filter)
        .await
        .map_err(order_error)?;
    ok(dto::order_stats(&stats))
}

async fn order_detail(
    State(s): State<AppState>,
    ConsoleUser(uid): ConsoleUser,
    Path(order_no): Path<String>,
) -> ApiResult<Data<Value>> {
    let order_no = order_no.trim();
    if order_no.is_empty() {
        return Err(Error::invalid().into());
    }
    let (item, lines) = s
        .svc
        .reseller
        .orders
        .detail_user(uid, order_no)
        .await
        .map_err(order_error)?;
    ok(dto::order_detail(&item, &lines))
}
