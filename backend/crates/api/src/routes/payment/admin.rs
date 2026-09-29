//! `/admin/payment-channels` and `/admin/payments` (compliance gated **[C]**).

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::payment::channel::{ChannelPatch, new_draft};
use zs_domain::payment::channel::{ChannelConfig, ChannelFilter, PaymentChannel};
use zs_domain::payment::errors::keys;
use zs_domain::payment::gateway::GatewaySecurityTestResult;
use zs_domain::payment::model::{AdminPayment, AdminPaymentFilter};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::{PageRequest, Pagination};

use crate::extract::{Bind, BindField, BindRules, Body, Query, req};
use crate::middleware::compliance::ComplianceAcked;
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Admin routes of the payment group.
pub(super) fn routes() -> Routes {
    Routes::new("/admin")
        .post("/payment-channels", create_channel)
        .get("/payment-channels", list_channels)
        .get("/payment-channels/{id}", get_channel)
        .post(
            "/payment-channels/{id}/wechatpay-public-key-test",
            test_wechatpay_public_key,
        )
        .get("/payment-channels/{id}/trade-bill", query_trade_bill)
        .get(
            "/payment-channels/{id}/trade-bill/download",
            download_trade_bill,
        )
        .put("/payment-channels/{id}", update_channel)
        .delete("/payment-channels/{id}", delete_channel)
        .get("/payments", list_payments)
        .get("/payments/export", export_payments)
        .get("/payments/{id}", get_payment)
}

type Params = Query<HashMap<String, String>>;

fn parse_id(raw: &str, key: &'static str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(key).into())
}

fn page_of(q: &HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

fn text(q: &HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

/// `ginutil.ParseQueryUint(raw, zeroInvalid=true)`.
fn opt_uint(q: &HashMap<String, String>, key: &str) -> Result<Id, ApiError> {
    match q.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(0),
        Some(s) => parse_id(s, "error.bad_request"),
    }
}

fn opt_time(q: &HashMap<String, String>, key: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    match q.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|t| Some(t.with_timezone(&Utc)))
            .map_err(|_| Error::invalid().into()),
    }
}

/// Go `strconv.ParseBool`; missing/blank = false.
fn parse_bool(raw: Option<&String>) -> Result<bool, ApiError> {
    match raw.map(|s| s.trim()).unwrap_or("") {
        "" => Ok(false),
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
        _ => Err(Error::invalid().into()),
    }
}

async fn list_channels(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Params,
) -> ApiResult<Paged<PaymentChannel>> {
    let page = page_of(&q);
    let filter = ChannelFilter {
        page: Some(page),
        provider_type: q.get("provider_type").cloned().unwrap_or_default(),
        channel_type: q.get("channel_type").cloned().unwrap_or_default(),
        active_only: parse_bool(q.get("active_only"))?,
    };
    let list = s.svc.payment.channels.list(&filter).await?;
    Ok(Paged(list.items, Pagination::new(page, list.total)))
}

async fn get_channel(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
) -> ApiResult<Data<PaymentChannel>> {
    let id = parse_id(&id, keys::CHANNEL_INVALID)?;
    ok(s.svc.payment.channels.get(id).await?)
}

/// `CreatePaymentChannelRequest` / `UpdatePaymentChannelRequest` body.
#[derive(Debug, Deserialize)]
struct ChannelRequest {
    #[serde(default)]
    name: String,
    icon: Option<String>,
    #[serde(default)]
    provider_type: String,
    #[serde(default)]
    channel_type: String,
    #[serde(default)]
    interaction_mode: String,
    fee_rate: Option<Amount>,
    fixed_fee: Option<Amount>,
    min_amount: Option<Amount>,
    max_amount: Option<Amount>,
    hide_amount_out_range: Option<bool>,
    payment_roles: Option<Vec<String>>,
    member_levels: Option<Vec<Id>>,
    payment_types: Option<Vec<String>>,
    config_json: Option<ChannelConfig>,
    is_active: Option<bool>,
    sort_order: Option<i32>,
}

/// `CreatePaymentChannelRequest`: same body as update, plus `binding:"required"` fields.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct CreateChannelRequest(ChannelRequest);

impl BindRules for CreateChannelRequest {
    const FIELDS: &'static [BindField] = &[
        req("name", "Name"),
        req("provider_type", "ProviderType"),
        req("channel_type", "ChannelType"),
        req("interaction_mode", "InteractionMode"),
    ];
}

async fn create_channel(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Bind(CreateChannelRequest(req)): Bind<CreateChannelRequest>,
) -> ApiResult<Data<PaymentChannel>> {
    let mut draft = new_draft();
    draft.name = req.name;
    draft.icon = req.icon.unwrap_or_default();
    draft.provider_type = req.provider_type;
    draft.channel_type = req.channel_type;
    draft.interaction_mode = req.interaction_mode;
    draft.fee_rate = req.fee_rate.unwrap_or_default();
    draft.fixed_fee = req.fixed_fee.unwrap_or_default();
    draft.min_amount = req.min_amount.unwrap_or_default();
    draft.max_amount = req.max_amount.unwrap_or_default();
    draft.hide_amount_out_range = req.hide_amount_out_range.unwrap_or(false);
    draft.payment_roles = req.payment_roles.unwrap_or_default();
    draft.member_levels = req.member_levels.unwrap_or_default();
    draft.payment_types = req.payment_types.unwrap_or_default();
    draft.config_json = req.config_json.unwrap_or_default();
    draft.is_active = req.is_active.unwrap_or(true);
    draft.sort_order = req.sort_order.unwrap_or(0);
    ok(s.svc.payment.channels.create(draft).await?)
}

async fn update_channel(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
    Body(req): Body<ChannelRequest>,
) -> ApiResult<Data<PaymentChannel>> {
    let id = parse_id(&id, keys::CHANNEL_INVALID)?;
    let patch = ChannelPatch {
        name: req.name,
        icon: req.icon,
        provider_type: req.provider_type,
        channel_type: req.channel_type,
        interaction_mode: req.interaction_mode,
        fee_rate: req.fee_rate,
        fixed_fee: req.fixed_fee,
        min_amount: req.min_amount,
        max_amount: req.max_amount,
        hide_amount_out_range: req.hide_amount_out_range,
        payment_roles: req.payment_roles,
        member_levels: req.member_levels,
        payment_types: req.payment_types,
        config_json: req.config_json,
        is_active: req.is_active,
        sort_order: req.sort_order,
    };
    ok(s.svc.payment.channels.update(id, patch).await?)
}

async fn delete_channel(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
) -> ApiResult<Data<Value>> {
    let id = parse_id(&id, keys::CHANNEL_INVALID)?;
    s.svc.payment.channels.delete(id).await?;
    ok(json!({"deleted": true}))
}

async fn test_wechatpay_public_key(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
) -> ApiResult<Data<GatewaySecurityTestResult>> {
    let id = parse_id(&id, keys::CHANNEL_INVALID)?;
    ok(s.svc.payment.channels.test_security(id).await?)
}

fn bill_date(q: &HashMap<String, String>) -> Result<String, ApiError> {
    let date = text(q, "file_date");
    if date.len() != 8 || !date.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::invalid().into());
    }
    Ok(date)
}

async fn query_trade_bill(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
    Query(q): Params,
) -> ApiResult<Data<zs_domain::payment::gateway::GatewayTradeBillQuery>> {
    let result = s
        .svc
        .payment
        .channels
        .query_trade_bill(parse_id(&id, keys::CHANNEL_INVALID)?, &bill_date(&q)?)
        .await?;
    ok(result)
}

async fn download_trade_bill(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
    Query(q): Params,
) -> ApiResult<Response> {
    let file_id = text(&q, "file_id");
    if file_id.is_empty() {
        return Err(Error::invalid().into());
    }
    let result = s
        .svc
        .payment
        .channels
        .download_trade_bill(
            parse_id(&id, keys::CHANNEL_INVALID)?,
            &bill_date(&q)?,
            &file_id,
        )
        .await?;
    let filename: String = result
        .file_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    Ok((
        [
            (header::CONTENT_TYPE, "application/octet-stream".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        result.body,
    )
        .into_response())
}

fn payment_filter(
    q: &HashMap<String, String>,
    page: PageRequest,
) -> Result<AdminPaymentFilter, ApiError> {
    Ok(AdminPaymentFilter {
        page,
        order_id: opt_uint(q, "order_id")?,
        user_id: opt_uint(q, "user_id")?,
        channel_id: opt_uint(q, "channel_id")?,
        created_from: opt_time(q, "created_from")?,
        created_to: opt_time(q, "created_to")?,
        provider_type: text(q, "provider_type"),
        channel_type: text(q, "channel_type"),
        status: text(q, "status"),
        skip_count: false,
    })
}

async fn list_payments(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Params,
) -> ApiResult<Paged<AdminPayment>> {
    let page = page_of(&q);
    let filter = payment_filter(&q, page)?;
    let list = s.svc.payment.payments.list(&filter).await?;
    Ok(Paged(list.items, Pagination::new(page, list.total)))
}

async fn get_payment(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Path(id): Path<String>,
) -> ApiResult<Data<AdminPayment>> {
    let id = parse_id(&id, keys::PAYMENT_INVALID)?;
    ok(s.svc.payment.payments.get(id).await?)
}

async fn export_payments(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Params,
) -> ApiResult<Response> {
    let filter = payment_filter(&q, PageRequest::default())?;
    let csv = s.svc.payment.payments.export_csv(&filter).await?;
    let filename = format!("payments_{}.csv", Utc::now().format("%Y%m%d_%H%M%S"));
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        csv,
    )
        .into_response())
}
