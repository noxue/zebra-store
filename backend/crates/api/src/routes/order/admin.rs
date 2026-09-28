//! Admin order, refund and delivery endpoints.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::response::Response;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::order::OrderService;
use zs_app::order::admin::AdminOrderRow;
use zs_app::order::refund::AdminRefundItem;
use zs_domain::order::model::keys;
use zs_domain::order::ports::{AdminOrderFilter, RefundFilter};
use zs_domain::{Error, Id};
use zs_shared::page::Pagination;

use super::common::download;
use super::user::{Params, page_of, text};
use crate::extract::{Bind, BindField, BindRules, Query, req, req_ptr};
use crate::middleware::auth::CurrentAdmin;
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/admin")
        .get("/orders", list)
        .get("/orders/{id}", detail)
        .patch("/orders/{id}", update_status)
        .post("/orders/{id}/refund-to-wallet", refund_to_wallet)
        .post("/orders/{id}/manual-refund", manual_refund)
        .get("/orders/{id}/fulfillment/download", download_fulfillment)
        .get("/order-refunds", list_refunds)
        .get("/order-refunds/{id}", refund_detail)
        .patch("/order-refunds/{id}/payment-fee", refund_fee)
        .post("/fulfillments", create_fulfillment)
}

fn order_id(raw: &str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(keys::ORDER_ITEM_INVALID).into())
}

fn opt_time(q: &HashMap<String, String>, key: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    match q.get(key).map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|t| Some(t.with_timezone(&Utc)))
            .map_err(|_| Error::invalid().into()),
    }
}

async fn list(State(s): State<AppState>, Query(q): Params) -> ApiResult<Paged<AdminOrderRow>> {
    let page = page_of(&q);
    let filter = AdminOrderFilter {
        page,
        user_id: q
            .get("user_id")
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0),
        user_keyword: text(&q, "user_keyword"),
        status: text(&q, "status"),
        order_no: text(&q, "order_no"),
        guest_email: text(&q, "guest_email"),
        product_keyword: text(&q, "product_keyword"),
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
        sort_by: text(&q, "sort_by"),
        sort_asc: text(&q, "sort_order").eq_ignore_ascii_case("asc"),
        procurement_issue: matches!(text(&q, "procurement_issue").as_str(), "1" | "true"),
        reseller: text(&q, "reseller"),
    };
    let result = s.svc.order.service.admin_orders(&filter).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}

async fn detail(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Data<Value>> {
    let detail = s
        .svc
        .order
        .service
        .admin_order_detail(order_id(&id)?)
        .await?;
    ok(serde_json::to_value(detail).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct StatusBody {
    #[serde(default)]
    status: String,
}

impl BindRules for StatusBody {
    const FIELDS: &'static [BindField] = &[req("status", "Status")];
}

async fn update_status(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Bind(req): Bind<StatusBody>,
) -> ApiResult<Data<Value>> {
    let id = order_id(&id)?;
    if req.status.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    let order = s
        .svc
        .order
        .service
        .admin_set_status(id, &req.status)
        .await?;
    ok(serde_json::to_value(order).map_err(Error::from)?)
}

#[derive(Debug, Deserialize)]
struct RefundBody {
    #[serde(default)]
    amount: Value,
    #[serde(default)]
    remark: String,
    #[serde(default)]
    payment_fee_refunded: bool,
}

impl BindRules for RefundBody {
    const FIELDS: &'static [BindField] = &[req("amount", "Amount")];
}

fn amount_text(v: &Value) -> Result<String, ApiError> {
    match v {
        Value::String(s) if !s.is_empty() => Ok(s.clone()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(Error::invalid().into()),
    }
}

async fn refund_to_wallet(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Bind(req): Bind<RefundBody>,
) -> ApiResult<Data<Value>> {
    let id = order_id(&id)?;
    let done = s
        .svc
        .order
        .service
        .refund_to_wallet(id, &amount_text(&req.amount)?, &req.remark)
        .await?;
    ok(json!({"order": done.order, "transaction": done.transaction}))
}

async fn manual_refund(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Bind(req): Bind<RefundBody>,
) -> ApiResult<Data<Value>> {
    let id = order_id(&id)?;
    let done = s
        .svc
        .order
        .service
        .manual_refund(
            id,
            &amount_text(&req.amount)?,
            &req.remark,
            req.payment_fee_refunded,
        )
        .await?;
    ok(json!({"order": done.order, "refund_record": done.record}))
}

async fn download_fulfillment(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let order = s
        .svc
        .order
        .service
        .admin_order(order_id(&id)?)
        .await
        .map_err(|_| Error::not_found(keys::ORDER_NOT_FOUND))?;
    let payload = OrderService::fulfillment_payload(&order)?;
    Ok(download(&order.order_no, payload))
}

async fn list_refunds(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Paged<AdminRefundItem>> {
    let page = page_of(&q);
    let user_id = match q.get("user_id").map(|v| v.trim()).filter(|v| !v.is_empty()) {
        Some(v) => v.parse::<Id>().map_err(|_| Error::invalid())?,
        None => 0,
    };
    let mut product_keyword = text(&q, "product_keyword");
    if product_keyword.is_empty() {
        product_keyword = text(&q, "product_name");
    }
    let filter = RefundFilter {
        page,
        user_id,
        user_keyword: text(&q, "user_keyword"),
        order_no: text(&q, "order_no"),
        guest_email: text(&q, "guest_email"),
        product_keyword,
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
    };
    let result = s.svc.order.service.admin_refunds(&filter).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}

async fn refund_detail(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Data<AdminRefundItem>> {
    let id: Id = id.trim().parse().map_err(|_| Error::invalid())?;
    ok(s.svc.order.service.admin_refund(id).await?)
}

#[derive(Debug, Deserialize)]
struct FeeBody {
    #[serde(default)]
    payment_fee_refunded: Option<bool>,
}

impl BindRules for FeeBody {
    const FIELDS: &'static [BindField] = &[req_ptr("payment_fee_refunded", "PaymentFeeRefunded")];
}

async fn refund_fee(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Bind(req): Bind<FeeBody>,
) -> ApiResult<Data<AdminRefundItem>> {
    let id: Id = id.trim().parse().map_err(|_| Error::invalid())?;
    let flag = req.payment_fee_refunded.ok_or_else(Error::invalid)?;
    ok(s.svc.order.service.set_refund_fee(id, flag).await?)
}

#[derive(Debug, Deserialize)]
struct FulfillmentBody {
    #[serde(default)]
    order_id: Id,
    #[serde(default)]
    payload: String,
    #[serde(default)]
    delivery_data: Option<Value>,
}

impl BindRules for FulfillmentBody {
    const FIELDS: &'static [BindField] = &[req("order_id", "OrderID")];
}

async fn create_fulfillment(
    State(s): State<AppState>,
    CurrentAdmin(admin): CurrentAdmin,
    Bind(req): Bind<FulfillmentBody>,
) -> ApiResult<Data<Value>> {
    if req.order_id <= 0 {
        return Err(Error::invalid().into());
    }
    let data = match req.delivery_data {
        Some(Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    };
    let fulfillment = s
        .svc
        .order
        .service
        .manual_fulfill(req.order_id, admin.id, &req.payload, &data)
        .await?;
    ok(serde_json::to_value(fulfillment).map_err(Error::from)?)
}
