//! Channel API order and payment endpoints used by the Telegram bot
//! (`/api/v1/channel/orders*`, `/api/v1/channel/payments*`; port of
//! `channel_order_*.go`). Requests are authenticated by the notify group's channel HMAC
//! middleware; the buyer is the shop user provisioned for `channel_user_id`.

use std::collections::HashMap;

use axum::body::Bytes;
use axum::extract::FromRequestParts;
use axum::extract::{Path, State};
use axum::http::request::Parts;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use zs_app::order::checkout::{CheckoutRequest, ItemRequest};
use zs_app::order::payment::PayRequest;
use zs_app::order::query::Viewer;
use zs_domain::notify::channel::TelegramIdentityInput;
use zs_domain::order::model::{JsonMap, Order, OrderItem, OrderStatus, fulfillment_type, keys};
use zs_domain::payment::model::Payment;
use zs_domain::payment::wallet_info::extract_crypto_wallet_info;
use zs_domain::reseller::tenant::ResellerTenant;
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::PageRequest;

use crate::client::Client;
use crate::extract::{BindField, BindRules, Query, bind_value, req};
use crate::i18n;
use crate::middleware::request_id::RequestId;
use crate::routes::Routes;
use crate::state::AppState;

pub(super) type Params = Query<HashMap<String, String>>;

const DEFAULT_LOCALE: &str = "zh-CN";
/// Page bounds of `/channel/orders` (original default 5, maximum 20).
const LIST_DEFAULT_PAGE_SIZE: u64 = 5;
const LIST_MAX_PAGE_SIZE: u64 = 20;

pub(super) fn routes() -> Routes {
    Routes::new("/channel")
        .post("/orders/preview", preview)
        .post("/orders", create)
        .get("/orders", list)
        .get("/orders/by-order-no/{order_no}", by_order_no)
        .get("/orders/{id}", by_id)
        .post("/orders/{id}/cancel", cancel)
        .get("/payments/latest", latest_payment)
        .get("/payments/{id}", payment_detail)
        .post("/payments", create_payment)
}

// ---------------------------------------------------------------------------
// envelope
// ---------------------------------------------------------------------------

/// A channel error: HTTP status, `error_code` and message key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) http: StatusCode,
    pub(super) error_code: &'static str,
    pub(super) key: &'static str,
}

pub(super) const fn failure(
    http: StatusCode,
    error_code: &'static str,
    key: &'static str,
) -> Failure {
    Failure {
        http,
        error_code,
        key,
    }
}

pub(super) const VALIDATION: Failure = failure(
    StatusCode::BAD_REQUEST,
    "validation_error",
    "error.bad_request",
);
pub(super) const INTERNAL: Failure = failure(
    StatusCode::INTERNAL_SERVER_ERROR,
    "internal_error",
    "error.internal_error",
);
const ORDER_NOT_FOUND: Failure = failure(
    StatusCode::NOT_FOUND,
    "order_not_found",
    "error.order_not_found",
);
const PAYMENT_NOT_FOUND: Failure = failure(
    StatusCode::NOT_FOUND,
    "payment_not_found",
    "error.payment_not_found",
);
const ORDER_STATUS_INVALID: Failure = failure(
    StatusCode::BAD_REQUEST,
    "order_status_invalid",
    "error.order_status_invalid",
);

/// Per-request locale and request id.
pub(super) struct Ctx {
    locale: &'static str,
    request_id: String,
}

impl<S: Send + Sync> FromRequestParts<S> for Ctx {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self {
            locale: i18n::resolve_locale(parts.uri.query(), &parts.headers),
            request_id: parts
                .extensions
                .get::<RequestId>()
                .map(|r| r.0.clone())
                .unwrap_or_default(),
        })
    }
}

impl Ctx {
    /// The original `ChannelResponse` body.
    fn envelope(
        &self,
        http: StatusCode,
        msg: String,
        data: Option<Value>,
        error_code: &str,
    ) -> Response {
        let mut body = Map::new();
        let code = if http.is_success() { 0 } else { http.as_u16() };
        body.insert("status_code".into(), code.into());
        body.insert("msg".into(), msg.into());
        if let Some(data) = data.filter(|d| !d.is_null()) {
            body.insert("data".into(), data);
        }
        if !error_code.is_empty() {
            body.insert("error_code".into(), error_code.into());
        }
        if !self.request_id.is_empty() {
            body.insert("request_id".into(), self.request_id.clone().into());
        }
        (http, axum::Json(Value::Object(body))).into_response()
    }

    pub(super) fn ok(&self, data: Value) -> Response {
        self.envelope(StatusCode::OK, "success".into(), Some(data), "")
    }

    pub(super) fn fail(&self, f: Failure) -> Response {
        self.envelope(
            f.http,
            i18n::translate(self.locale, f.key),
            None,
            f.error_code,
        )
    }

    /// Binds a JSON body like `ShouldBindJSON` + `respondChannelBindError`: rule failures
    /// list the Go fields (`"OrderID: 不能为空"`), other errors are `error.bad_request`.
    pub(super) fn bind<T: DeserializeOwned + BindRules>(
        &self,
        body: &[u8],
    ) -> Result<T, Box<Response>> {
        bind_value(body).map_err(|f| {
            Box::new(self.envelope(
                VALIDATION.http,
                f.message(self.locale),
                None,
                VALIDATION.error_code,
            ))
        })
    }

    pub(super) fn internal(&self, error: &Error) -> Response {
        tracing::error!(%error, "channel order handler error");
        self.fail(INTERNAL)
    }

    /// Identity provisioning failures (`respondChannelIdentityServiceError`).
    fn identity_error(&self, e: &Error) -> Response {
        let f = match e.key() {
            "error.bad_request" => VALIDATION,
            "error.email_invalid" => failure(
                StatusCode::BAD_REQUEST,
                "validation_error",
                "error.email_invalid",
            ),
            "error.user_not_found" => failure(
                StatusCode::NOT_FOUND,
                "user_not_found",
                "error.user_not_found",
            ),
            "error.user_disabled" => failure(
                StatusCode::UNAUTHORIZED,
                "user_disabled",
                "error.user_disabled",
            ),
            _ => return self.internal(e),
        };
        self.fail(f)
    }

    /// Mapped business error (`respondChannelMappedError`): the order rate limit also sets
    /// `Retry-After`.
    fn mapped(&self, e: &Error, rules: fn(&str) -> Option<Failure>, fallback: Failure) -> Response {
        let mut resp = match rules(e.key()) {
            Some(f) => self.fail(f),
            None => {
                tracing::error!(error = %e, "channel order request failed");
                self.fail(fallback)
            }
        };
        if e.key() == keys::RISK_ORDER_RATE_LIMITED
            && let Some(secs) = e
                .args()
                .first()
                .filter(|s| s.parse::<i64>().is_ok_and(|n| n > 0))
            && let Ok(v) = HeaderValue::from_str(secs)
        {
            resp.headers_mut().insert(header::RETRY_AFTER, v);
        }
        resp
    }
}

/// `channelOrderCreateErrorRules`.
fn order_rule(key: &str) -> Option<Failure> {
    use StatusCode as S;
    let f = |http, code, key| Some(failure(http, code, key));
    match key {
        "error.risk_ip_blacklisted" => f(S::FORBIDDEN, "risk_blocked", "error.risk_ip_blacklisted"),
        "error.risk_client_ip_unavailable" => f(
            S::FORBIDDEN,
            "risk_blocked",
            "error.risk_client_ip_unavailable",
        ),
        "error.risk_too_many_pending_orders" => f(
            S::TOO_MANY_REQUESTS,
            "risk_blocked",
            "error.risk_too_many_pending_orders",
        ),
        "error.risk_product_quantity_limit" => f(
            S::BAD_REQUEST,
            "quantity_limit_exceeded",
            "error.risk_product_quantity_limit",
        ),
        "error.risk_pending_product_quantity_limit" => f(
            S::TOO_MANY_REQUESTS,
            "risk_blocked",
            "error.risk_pending_product_quantity_limit",
        ),
        "error.risk_order_rate_limited" => f(
            S::TOO_MANY_REQUESTS,
            "risk_blocked",
            "error.risk_order_rate_limited",
        ),
        "error.order_item_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.order_item_invalid",
        ),
        "error.order_amount_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.order_amount_invalid",
        ),
        "error.product_purchase_not_allowed" => f(
            S::BAD_REQUEST,
            "product_unavailable",
            "error.product_purchase_not_allowed",
        ),
        "error.product_max_purchase_exceeded" => f(
            S::BAD_REQUEST,
            "quantity_limit_exceeded",
            "error.product_max_purchase_exceeded",
        ),
        "error.product_min_purchase_not_met" => f(
            S::BAD_REQUEST,
            "quantity_below_minimum",
            "error.product_min_purchase_not_met",
        ),
        "error.product_not_available" => f(
            S::BAD_REQUEST,
            "product_unavailable",
            "error.product_not_available",
        ),
        "error.manual_stock_insufficient" => f(
            S::BAD_REQUEST,
            "sku_out_of_stock",
            "error.manual_stock_insufficient",
        ),
        "error.card_secret_insufficient" => f(
            S::BAD_REQUEST,
            "sku_out_of_stock",
            "error.card_secret_insufficient",
        ),
        "error.order_currency_mismatch" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.order_currency_mismatch",
        ),
        "error.product_price_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.product_price_invalid",
        ),
        "error.coupon_invalid" => f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_invalid"),
        "error.coupon_not_found" => f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_not_found"),
        "error.coupon_inactive" => f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_inactive"),
        "error.coupon_not_started" => {
            f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_not_started")
        }
        "error.coupon_expired" => f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_expired"),
        "error.coupon_usage_limit" => {
            f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_usage_limit")
        }
        "error.coupon_per_user_limit" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_per_user_limit",
        ),
        "error.coupon_min_amount" => f(S::BAD_REQUEST, "coupon_invalid", "error.coupon_min_amount"),
        "error.coupon_scope_invalid" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_scope_invalid",
        ),
        "error.coupon_payment_role_not_allowed" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_payment_role_not_allowed",
        ),
        "error.coupon_payment_role_guest_only" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_payment_role_guest_only",
        ),
        "error.coupon_payment_role_member_only" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_payment_role_member_only",
        ),
        "error.coupon_member_level_not_allowed" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_member_level_not_allowed",
        ),
        "error.coupon_wholesale_disabled" => f(
            S::BAD_REQUEST,
            "coupon_invalid",
            "error.coupon_wholesale_disabled",
        ),
        "error.promotion_invalid" => f(S::BAD_REQUEST, "coupon_invalid", "error.promotion_invalid"),
        "error.manual_form_schema_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.manual_form_schema_invalid",
        ),
        "error.manual_form_required_missing" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.manual_form_required_missing",
        ),
        "error.manual_form_field_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.manual_form_field_invalid",
        ),
        "error.manual_form_type_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.manual_form_type_invalid",
        ),
        "error.manual_form_option_invalid" => f(
            S::BAD_REQUEST,
            "validation_error",
            "error.manual_form_option_invalid",
        ),
        _ => None,
    }
}

/// `channelPaymentCreateErrorRules`.
fn payment_rule(key: &str) -> Option<Failure> {
    use StatusCode as S;
    let f = |http, code, key| Some(failure(http, code, key));
    match key {
        "error.payment_invalid" => f(S::BAD_REQUEST, "validation_error", "error.payment_invalid"),
        "error.order_not_found" => Some(ORDER_NOT_FOUND),
        "error.order_status_invalid" => Some(ORDER_STATUS_INVALID),
        "error.payment_channel_not_found" => f(
            S::NOT_FOUND,
            "payment_method_unavailable",
            "error.payment_channel_not_found",
        ),
        "error.payment_channel_inactive" => f(
            S::BAD_REQUEST,
            "payment_method_unavailable",
            "error.payment_channel_inactive",
        ),
        "error.payment_provider_not_supported" => f(
            S::BAD_REQUEST,
            "payment_method_unavailable",
            "error.payment_provider_not_supported",
        ),
        "error.payment_channel_config_invalid" => f(
            S::BAD_REQUEST,
            "payment_method_unavailable",
            "error.payment_channel_config_invalid",
        ),
        "error.payment_gateway_request_failed" => f(
            S::BAD_REQUEST,
            "payment_create_failed",
            "error.payment_gateway_request_failed",
        ),
        "error.payment_gateway_response_invalid" => f(
            S::BAD_REQUEST,
            "payment_create_failed",
            "error.payment_gateway_response_invalid",
        ),
        "error.payment_currency_mismatch" => f(
            S::BAD_REQUEST,
            "payment_create_failed",
            "error.payment_currency_mismatch",
        ),
        "error.wallet_only_payment_required" => f(
            S::BAD_REQUEST,
            "wallet_only_payment_required",
            "error.wallet_only_payment_required",
        ),
        _ => None,
    }
}

pub(super) fn parse<T: DeserializeOwned>(body: &Bytes) -> Result<T, Failure> {
    serde_json::from_slice(body).map_err(|_| VALIDATION)
}

pub(super) fn first_non_empty(a: &str, b: &str) -> String {
    let a = a.trim();
    if a.is_empty() { b.trim() } else { a }.to_owned()
}

pub(super) fn channel_user_of(q: &HashMap<String, String>) -> String {
    let get = |k: &str| q.get(k).map(String::as_str).unwrap_or_default();
    first_non_empty(get("channel_user_id"), get("telegram_user_id"))
}

fn locale_of(ctx: &Ctx, explicit: &str) -> String {
    match explicit.trim() {
        "" => ctx.locale.to_owned(),
        v => v.to_owned(),
    }
}

/// Localized text of a `{locale: text}` object (`resolveLocalizedJSON`).
fn localized(values: &JsonMap, locale: &str) -> String {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    for key in [locale, DEFAULT_LOCALE] {
        if let Some(t) = values.get(key).map(text).filter(|t| !t.is_empty()) {
            return t;
        }
    }
    values
        .values()
        .map(text)
        .find(|t| !t.is_empty())
        .unwrap_or_default()
}

/// `channelLocalizedValue` of a SKU snapshot's `spec_values`.
fn sku_name(item_snapshot: &JsonMap, locale: &str) -> String {
    match item_snapshot.get("spec_values") {
        Some(Value::Object(m)) => localized(m, locale),
        None | Some(Value::Null) => String::new(),
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(other) => other.to_string(),
    }
}

pub(super) fn fixed(a: Amount) -> String {
    a.to_string()
}

pub(super) fn time(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(super) fn opt_time(t: Option<DateTime<Utc>>) -> Value {
    t.map_or(Value::Null, |t| Value::String(time(t)))
}

fn masked(kind: &str) -> String {
    if kind.trim() == fulfillment_type::UPSTREAM {
        fulfillment_type::MANUAL.to_owned()
    } else {
        kind.to_owned()
    }
}

/// Wallet + online amounts (`channelOrderPaidAmount`).
fn paid_amount(o: &Order) -> String {
    fixed(o.wallet_paid_amount + o.online_paid_amount)
}

/// First delivery / item type of the order (`channelOrderFulfillmentType`), masked.
fn order_fulfillment_type(o: &Order) -> String {
    let non_empty = |s: &str| (!s.trim().is_empty()).then(|| masked(s.trim()));
    if let Some(t) = o.fulfillment.as_ref().and_then(|f| non_empty(&f.kind)) {
        return t;
    }
    if let Some(t) = o.items.iter().find_map(|i| non_empty(&i.fulfillment_type)) {
        return t;
    }
    for child in &o.children {
        if let Some(t) = child.fulfillment.as_ref().and_then(|f| non_empty(&f.kind)) {
            return t;
        }
        if let Some(t) = child
            .items
            .iter()
            .find_map(|i| non_empty(&i.fulfillment_type))
        {
            return t;
        }
    }
    String::new()
}

fn joined_instructions(items: &[OrderItem], locale: &str) -> String {
    let mut seen = Vec::<String>::new();
    for item in items {
        let text = localized(&item.instructions, locale).trim().to_owned();
        if !text.is_empty() && !seen.contains(&text) {
            seen.push(text);
        }
    }
    seen.join("\n\n")
}

/// Common order fields of create / detail responses.
fn order_summary(o: &Order) -> Map<String, Value> {
    let v = json!({
        "order_id": o.id,
        "order_no": o.order_no,
        "status": o.status.as_str(),
        "fulfillment_type": order_fulfillment_type(o),
        "currency": o.currency,
        "item_count": o.items.len(),
        "original_amount": fixed(o.original_amount),
        "coupon_discount": fixed(o.discount_amount),
        "promotion_discount": fixed(o.promotion_discount_amount),
        "wholesale_discount": fixed(o.wholesale_discount_amount),
        "total_amount": fixed(o.total_amount),
        "wallet_paid_amount": fixed(o.wallet_paid_amount),
        "online_paid_amount": fixed(o.online_paid_amount),
        "paid_amount": paid_amount(o),
        "refunded_amount": fixed(o.refunded_amount),
        "expires_at": opt_time(o.expires_at),
        "created_at": time(o.created_at),
    });
    v.as_object().cloned().unwrap_or_default()
}

fn item_json(item: &OrderItem, locale: &str, instructions: Option<String>) -> Value {
    let mut v = json!({
        "product_id": item.product_id,
        "product_title": localized(&item.title, locale),
        "sku_id": item.sku_id,
        "sku_name": sku_name(&item.sku_snapshot, locale),
        "quantity": item.quantity,
        "original_unit_price": fixed(item.original_unit_price),
        "unit_price": fixed(item.unit_price),
        "original_total_price": fixed(item.original_total_price),
        "subtotal": fixed(item.total_price),
        "coupon_discount": fixed(item.coupon_discount),
        "promotion_discount": fixed(item.promotion_discount),
        "wholesale_discount": fixed(item.wholesale_discount),
        "fulfillment_type": masked(&item.fulfillment_type),
    });
    if let (Some(text), Some(obj)) = (instructions, v.as_object_mut()) {
        obj.insert("instructions".into(), Value::String(text));
    }
    v
}

/// `buildChannelOrderDetailResponse` (costs never exposed, upstream masked).
fn order_detail(o: &Order, locale: &str) -> Value {
    let mut resp = order_summary(o);
    resp.insert("updated_at".into(), Value::String(time(o.updated_at)));
    resp.insert("paid_at".into(), opt_time(o.paid_at));
    resp.insert("cancelled_at".into(), opt_time(o.canceled_at));
    let paid = o.paid_at.is_some();
    let items: Vec<Value> = o
        .items
        .iter()
        .map(|i| {
            let text = if paid {
                localized(&i.instructions, locale)
            } else {
                String::new()
            };
            item_json(i, locale, Some(text))
        })
        .collect();
    resp.insert("items".into(), Value::Array(items));
    let children: Vec<Value> = o
        .children
        .iter()
        .map(|c| {
            let instructions = if paid {
                joined_instructions(&c.items, locale)
            } else {
                String::new()
            };
            let fulfillment = c.fulfillment.as_ref().map_or(Value::Null, |f| {
                json!({
                    "status": f.status,
                    "type": masked(&f.kind),
                    "payload": f.payload,
                    "delivered_at": opt_time(f.delivered_at),
                    "instructions": instructions,
                })
            });
            json!({
                "order_id": c.id,
                "order_no": c.order_no,
                "status": c.status.as_str(),
                "fulfillment": fulfillment,
            })
        })
        .collect();
    resp.insert("children".into(), Value::Array(children));
    match &o.fulfillment {
        Some(f) => {
            let instructions = if paid {
                joined_instructions(&o.items, locale)
            } else {
                String::new()
            };
            resp.insert("fulfillment_status".into(), Value::String(f.status.clone()));
            resp.insert(
                "fulfillment_result".into(),
                Value::String(f.payload.clone()),
            );
            resp.insert("fulfillment_delivered_at".into(), opt_time(f.delivered_at));
            resp.insert(
                "fulfillment_instructions".into(),
                Value::String(instructions),
            );
        }
        None => {
            resp.insert("fulfillment_status".into(), Value::String(String::new()));
            resp.insert("fulfillment_result".into(), Value::Null);
            resp.insert("fulfillment_delivered_at".into(), Value::Null);
            resp.insert(
                "fulfillment_instructions".into(),
                Value::String(String::new()),
            );
        }
    }
    Value::Object(resp)
}

/// `buildChannelPaymentResponse`.
fn payment_json(order: Option<&Order>, p: &Payment) -> Map<String, Value> {
    let v = json!({
        "payment_id": p.id,
        "order_id": p.order_id,
        "channel_id": p.channel_id,
        "status": p.status.as_str(),
        "provider_type": p.provider_type,
        "channel_type": p.channel_type,
        "interaction_mode": p.interaction_mode,
        "amount": fixed(p.amount),
        "fee_rate": fixed(p.fee_rate),
        "fee_amount": fixed(p.fee_amount),
        "currency": p.currency,
        "pay_url": p.pay_url,
        "qr_code": p.qr_code,
        "paid_at": opt_time(p.paid_at),
        "expires_at": opt_time(p.expired_at),
        "callback_at": opt_time(p.callback_at),
        "created_at": time(p.created_at),
        "updated_at": time(p.updated_at),
    });
    let mut m = v.as_object().cloned().unwrap_or_default();
    let info =
        extract_crypto_wallet_info(&p.provider_type, &p.interaction_mode, &p.provider_payload);
    for (k, val) in [
        ("wallet_address", info.address),
        ("chain_amount", info.chain_amount),
        ("chain", info.chain),
        ("token_id", info.token_id),
    ] {
        if !val.is_empty() {
            m.insert(k.into(), Value::String(val));
        }
    }
    if let Some(o) = order {
        m.insert("order_no".into(), Value::String(o.order_no.clone()));
        m.insert("total_amount".into(), Value::String(fixed(o.total_amount)));
        m.insert(
            "wallet_paid_amount".into(),
            Value::String(fixed(o.wallet_paid_amount)),
        );
        m.insert(
            "online_paid_amount".into(),
            Value::String(fixed(o.online_paid_amount)),
        );
        m.insert("paid_amount".into(), Value::String(paid_amount(o)));
    }
    m
}

// ---------------------------------------------------------------------------
// requests
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ItemBody {
    product_id: Id,
    sku_id: Id,
    quantity: i32,
}

/// Preview / create body (identity fields, lines, legacy single-line fields).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OrderBody {
    channel_user_id: String,
    telegram_user_id: String,
    username: String,
    telegram_username: String,
    first_name: String,
    last_name: String,
    avatar_url: String,
    locale: String,
    items: Vec<ItemBody>,
    product_id: Id,
    sku_id: Id,
    quantity: i32,
    coupon_code: String,
    affiliate_code: String,
    affiliate_visitor_key: String,
    manual_form_data: Option<Value>,
}

impl OrderBody {
    fn identity(&self) -> TelegramIdentityInput {
        TelegramIdentityInput {
            channel_user_id: first_non_empty(&self.channel_user_id, &self.telegram_user_id),
            username: first_non_empty(&self.username, &self.telegram_username),
            first_name: self.first_name.trim().to_owned(),
            last_name: self.last_name.trim().to_owned(),
            avatar_url: self.avatar_url.trim().to_owned(),
        }
    }

    /// `buildChannelOrderItems`: `items`, else the legacy single line (create only).
    fn lines(&self, legacy: bool) -> Option<Vec<ItemRequest>> {
        let mut items: Vec<ItemRequest> = self
            .items
            .iter()
            .map(|i| ItemRequest {
                product_id: i.product_id,
                sku_id: i.sku_id,
                quantity: i.quantity,
            })
            .collect();
        if items.is_empty() {
            if !legacy || self.product_id <= 0 || self.quantity <= 0 {
                return None;
            }
            items.push(ItemRequest {
                product_id: self.product_id,
                sku_id: self.sku_id,
                quantity: self.quantity,
            });
        }
        items
            .iter()
            .all(|i| i.product_id > 0 && i.quantity > 0)
            .then_some(items)
    }

    fn checkout(
        &self,
        user_id: Id,
        items: Vec<ItemRequest>,
        ip: &str,
        skip_ip_risk: bool,
    ) -> CheckoutRequest {
        CheckoutRequest {
            user_id,
            guest: None,
            tenant: ResellerTenant::default(),
            items,
            coupon_code: self.coupon_code.clone(),
            affiliate_code: self.affiliate_code.clone(),
            affiliate_visitor_key: self.affiliate_visitor_key.clone(),
            client_ip: ip.to_owned(),
            manual_form_data: match &self.manual_form_data {
                Some(Value::Object(m)) => m.clone(),
                _ => JsonMap::new(),
            },
            skip_risk: false,
            // The bot's server IP is shared by all its users.
            skip_ip_risk,
        }
    }
}

/// Provisions the shop user of a channel user id.
pub(super) async fn user_of(
    s: &AppState,
    ctx: &Ctx,
    input: &TelegramIdentityInput,
) -> Result<Id, Box<Response>> {
    if input.channel_user_id.is_empty() {
        return Err(Box::new(ctx.fail(VALIDATION)));
    }
    s.svc
        .notify
        .channel
        .provision_user_id(input)
        .await
        .map_err(|e| Box::new(ctx.identity_error(&e)))
}

pub(super) async fn user_from_query(
    s: &AppState,
    ctx: &Ctx,
    q: &HashMap<String, String>,
) -> Result<Id, Box<Response>> {
    let input = TelegramIdentityInput {
        channel_user_id: channel_user_of(q),
        ..TelegramIdentityInput::default()
    };
    user_of(s, ctx, &input).await
}

/// The user's root order by id (`GetOrderByUser`).
async fn order_by_id(
    s: &AppState,
    ctx: &Ctx,
    user_id: Id,
    id: Id,
    fetch_key: Failure,
) -> Result<Order, Box<Response>> {
    let svc = &s.svc.order.service;
    match svc
        .get_order_by_id(&Viewer::User(user_id), &ResellerTenant::default(), id)
        .await
    {
        Ok(mut o) => {
            o.fill_items_from_children();
            Ok(o)
        }
        Err(e) if e.is_not_found() => Err(Box::new(ctx.fail(ORDER_NOT_FOUND))),
        Err(e) => {
            tracing::error!(error = %e, order_id = id, "channel order fetch failed");
            Err(Box::new(ctx.fail(fetch_key)))
        }
    }
}

fn parse_id(raw: &str) -> Option<Id> {
    raw.trim().parse::<Id>().ok().filter(|v| *v > 0)
}

// ---------------------------------------------------------------------------
// handlers
// ---------------------------------------------------------------------------

async fn preview(
    ctx: Ctx,
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> Response {
    let req: OrderBody = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let Some(items) = req.lines(false) else {
        return ctx.fail(VALIDATION);
    };
    let user_id = match user_of(&s, &ctx, &req.identity()).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let preview = match s
        .svc
        .order
        .service
        .preview(&req.checkout(user_id, items, &client.ip, false))
        .await
    {
        Ok(p) => p,
        Err(e) => {
            return ctx.mapped(
                &e,
                order_rule,
                failure(
                    StatusCode::BAD_REQUEST,
                    "order_preview_failed",
                    "error.order_create_failed",
                ),
            );
        }
    };
    let locale = locale_of(&ctx, &req.locale);
    let items: Vec<Value> = preview
        .items
        .iter()
        .map(|i| {
            json!({
                "product_id": i.product_id,
                "product_title": localized(&i.title, &locale),
                "sku_id": i.sku_id,
                "sku_name": sku_name(&i.sku_snapshot, &locale),
                "quantity": i.quantity,
                "original_unit_price": fixed(i.original_unit_price),
                "unit_price": fixed(i.unit_price),
                "original_total_price": fixed(i.original_total_price),
                "subtotal": fixed(i.total_price),
                "coupon_discount": fixed(i.coupon_discount_amount),
                "promotion_discount": fixed(i.promotion_discount_amount),
                "wholesale_discount": fixed(i.wholesale_discount_amount),
                "fulfillment_type": masked(&i.fulfillment_type),
            })
        })
        .collect();
    ctx.ok(json!({
        "item_count": items.len(),
        "original_amount": fixed(preview.original_amount),
        "items": items,
        "coupon_discount": fixed(preview.discount_amount),
        "promotion_discount": fixed(preview.promotion_discount_amount),
        "wholesale_discount": fixed(preview.wholesale_discount_amount),
        "total_amount": fixed(preview.total_amount),
        "currency": preview.currency,
        "valid": true,
        "validation_errors": [],
    }))
}

async fn create(
    ctx: Ctx,
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> Response {
    let req: OrderBody = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let Some(items) = req.lines(true) else {
        return ctx.fail(VALIDATION);
    };
    let user_id = match user_of(&s, &ctx, &req.identity()).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    match s
        .svc
        .order
        .service
        .create_order(&req.checkout(user_id, items, &client.ip, true))
        .await
    {
        Ok(order) => ctx.ok(Value::Object(order_summary(&order))),
        Err(e) => ctx.mapped(
            &e,
            order_rule,
            failure(
                StatusCode::BAD_REQUEST,
                "order_create_failed",
                "error.order_create_failed",
            ),
        ),
    }
}

async fn list(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let page = q
        .get("page")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(1);
    let page_size = q
        .get("page_size")
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(LIST_DEFAULT_PAGE_SIZE)
        .min(LIST_MAX_PAGE_SIZE);
    let status = q
        .get("status")
        .map(|v| v.trim().to_owned())
        .unwrap_or_default();
    let locale = locale_of(
        &ctx,
        q.get("locale").map(String::as_str).unwrap_or_default(),
    );
    let result = s
        .svc
        .order
        .service
        .list_orders(
            &Viewer::User(user_id),
            &ResellerTenant::default(),
            &status,
            "",
            PageRequest { page, page_size },
        )
        .await;
    let result = match result {
        Ok(r) => r,
        Err(e) => return ctx.internal(&e),
    };
    let items: Vec<Value> = result
        .items
        .iter()
        .map(|o| {
            let mut o = o.clone();
            o.fill_items_from_children();
            let mut v = json!({
                "order_id": o.id,
                "order_no": o.order_no,
                "status": o.status.as_str(),
                "currency": o.currency,
                "total_amount": fixed(o.total_amount),
                "paid_amount": paid_amount(&o),
                "wallet_paid_amount": fixed(o.wallet_paid_amount),
                "online_paid_amount": fixed(o.online_paid_amount),
                "product_title": o.items.first().map(|i| localized(&i.title, &locale)).unwrap_or_default(),
                "item_count": o.items.len(),
                "created_at": time(o.created_at),
            });
            if let (Some(exp), Some(obj)) = (o.expires_at, v.as_object_mut()) {
                obj.insert("expires_at".into(), Value::String(time(exp)));
            }
            v
        })
        .collect();
    let total = result.total;
    ctx.ok(json!({
        "items": items,
        "page": page,
        "page_size": page_size,
        "total": total,
        "total_pages": total.div_ceil(page_size),
    }))
}

async fn by_id(
    ctx: Ctx,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Params,
) -> Response {
    let Some(id) = parse_id(&id) else {
        return ctx.fail(VALIDATION);
    };
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    match order_by_id(&s, &ctx, user_id, id, INTERNAL).await {
        Ok(o) => ctx.ok(order_detail(
            &o,
            &locale_of(
                &ctx,
                q.get("locale").map(String::as_str).unwrap_or_default(),
            ),
        )),
        Err(r) => *r,
    }
}

async fn by_order_no(
    ctx: Ctx,
    State(s): State<AppState>,
    Path(order_no): Path<String>,
    Query(q): Params,
) -> Response {
    if order_no.trim().is_empty() {
        return ctx.fail(VALIDATION);
    }
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let svc = &s.svc.order.service;
    match svc
        .get_order(
            &Viewer::User(user_id),
            &ResellerTenant::default(),
            order_no.trim(),
        )
        .await
    {
        Ok(mut o) => {
            o.fill_items_from_children();
            ctx.ok(order_detail(
                &o,
                &locale_of(
                    &ctx,
                    q.get("locale").map(String::as_str).unwrap_or_default(),
                ),
            ))
        }
        Err(e) if e.is_not_found() => ctx.fail(ORDER_NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "channel order fetch failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "error.order_fetch_failed",
            ))
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CancelBody {
    channel_user_id: String,
    telegram_user_id: String,
}

async fn cancel(
    ctx: Ctx,
    State(s): State<AppState>,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let Some(id) = parse_id(&id) else {
        return ctx.fail(VALIDATION);
    };
    let req: CancelBody = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let input = TelegramIdentityInput {
        channel_user_id: first_non_empty(&req.channel_user_id, &req.telegram_user_id),
        ..TelegramIdentityInput::default()
    };
    let user_id = match user_of(&s, &ctx, &input).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let order = match order_by_id(&s, &ctx, user_id, id, INTERNAL).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    match s
        .svc
        .order
        .service
        .cancel_order(user_id, &ResellerTenant::default(), &order.order_no)
        .await
    {
        Ok(o) => ctx.ok(json!({
            "order_id": o.id,
            "order_no": o.order_no,
            "status": o.status.as_str(),
            "cancelled_at": opt_time(o.canceled_at),
        })),
        Err(e) if e.is_not_found() => ctx.fail(ORDER_NOT_FOUND),
        Err(e) => {
            tracing::warn!(error = %e, order_id = id, "channel_order_cancel");
            ctx.fail(ORDER_STATUS_INVALID)
        }
    }
}

/// The original `latestPaymentQuery` (only its binding rules are used).
#[derive(Debug, Deserialize)]
struct LatestPaymentQuery {}

impl BindRules for LatestPaymentQuery {
    const FIELDS: &'static [BindField] = &[req("order_id", "OrderID")];
}

async fn latest_payment(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    // `latestPaymentQuery.OrderID` is `form:"order_id" binding:"required"`: an absent or
    // empty/zero value fails `required`, an unparsable one is a plain bind error.
    let raw = q.get("order_id").map_or("", |v| v.trim());
    let order_id = if raw.is_empty() || raw.parse::<u64>() == Ok(0) {
        let rules = ctx.bind::<LatestPaymentQuery>(b"{}").err();
        return rules.map_or_else(|| ctx.fail(VALIDATION), |r| *r);
    } else if let Some(id) = parse_id(raw) {
        id
    } else {
        return ctx.fail(VALIDATION);
    };
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let fetch = failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "error.order_fetch_failed",
    );
    let order = match order_by_id(&s, &ctx, user_id, order_id, fetch).await {
        Ok(o) => o,
        Err(r) => return *r,
    };
    let svc = &s.svc.order.service;
    let now = svc.deps().clock.now();
    if order.status != OrderStatus::PendingPayment || order.expires_at.is_some_and(|e| e <= now) {
        return ctx.fail(ORDER_STATUS_INVALID);
    }
    match svc.deps().repo.latest_pending_payment(order.id, now).await {
        Ok(Some(p)) => ctx.ok(Value::Object(payment_json(Some(&order), &p))),
        Ok(None) => ctx.fail(PAYMENT_NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "channel latest payment failed");
            ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "error.payment_fetch_failed",
            ))
        }
    }
}

async fn payment_detail(
    ctx: Ctx,
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Params,
) -> Response {
    let Some(id) = parse_id(&id) else {
        return ctx.fail(VALIDATION);
    };
    let user_id = match user_from_query(&s, &ctx, &q).await {
        Ok(id) => id,
        Err(r) => return *r,
    };
    let svc = &s.svc.order.service;
    let payment = match svc.deps().repo.payment(id).await {
        Ok(Some(p)) if p.order_id > 0 => p,
        Ok(_) => return ctx.fail(PAYMENT_NOT_FOUND),
        Err(e) => {
            tracing::error!(error = %e, "channel payment fetch failed");
            return ctx.fail(failure(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "error.payment_fetch_failed",
            ));
        }
    };
    let fetch = failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "error.order_fetch_failed",
    );
    match order_by_id(&s, &ctx, user_id, payment.order_id, fetch).await {
        Ok(o) => ctx.ok(Value::Object(payment_json(Some(&o), &payment))),
        Err(_) => ctx.fail(PAYMENT_NOT_FOUND),
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct PaymentBody {
    channel_user_id: String,
    telegram_user_id: String,
    order_id: Id,
    channel_id: Id,
    use_balance: bool,
}

impl BindRules for PaymentBody {
    const FIELDS: &'static [BindField] = &[req("order_id", "OrderID")];
}

async fn create_payment(
    ctx: Ctx,
    State(s): State<AppState>,
    Client(client): Client,
    body: Bytes,
) -> Response {
    let req: PaymentBody = match ctx.bind(&body) {
        Ok(v) => v,
        Err(r) => return *r,
    };
    if req.order_id <= 0 {
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
    if let Err(r) = order_by_id(&s, &ctx, user_id, req.order_id, INTERNAL).await {
        return *r;
    }
    let svc = &s.svc.order.service;
    let view = match svc
        .pay(&PayRequest {
            order_id: req.order_id,
            channel_id: req.channel_id,
            channel_type: String::new(),
            use_balance: req.use_balance,
            client_ip: client.ip.clone(),
            tenant: ResellerTenant::default(),
            scheme: "https".into(),
        })
        .await
    {
        Ok(v) => v,
        Err(e) => {
            return ctx.mapped(
                &e,
                payment_rule,
                failure(
                    StatusCode::BAD_REQUEST,
                    "payment_create_failed",
                    "error.payment_create_failed",
                ),
            );
        }
    };
    let mut resp = Map::new();
    if let Some(pid) = view.payment_id
        && let Ok(Some(p)) = svc.deps().repo.payment(pid).await
    {
        resp = payment_json(None, &p);
    }
    resp.insert("order_paid".into(), Value::Bool(view.order_paid));
    resp.insert(
        "wallet_paid_amount".into(),
        Value::String(fixed(view.wallet_paid_amount)),
    );
    resp.insert(
        "online_pay_amount".into(),
        Value::String(fixed(view.online_pay_amount)),
    );
    if !view.channel_name.is_empty() {
        resp.insert("channel_name".into(), Value::String(view.channel_name));
    }
    ctx.ok(Value::Object(resp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_prefers_locale_then_default() {
        let m: JsonMap = json!({"en-US": "Card", "zh-CN": "卡"})
            .as_object()
            .cloned()
            .unwrap_or_default();
        assert_eq!(localized(&m, "en-US"), "Card");
        assert_eq!(localized(&m, "ja"), "卡");
        assert_eq!(localized(&JsonMap::new(), "en-US"), "");
    }

    #[test]
    fn order_rules_cover_risk_and_stock() {
        assert_eq!(
            order_rule("error.risk_order_rate_limited").map(|f| f.http),
            Some(StatusCode::TOO_MANY_REQUESTS)
        );
        assert_eq!(
            order_rule("error.card_secret_insufficient").map(|f| f.error_code),
            Some("sku_out_of_stock")
        );
        assert!(order_rule("error.internal_error").is_none());
        assert_eq!(payment_rule("error.order_not_found"), Some(ORDER_NOT_FOUND));
    }
}
