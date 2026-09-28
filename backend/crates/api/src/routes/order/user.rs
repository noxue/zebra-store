//! Logged-in storefront endpoints: cart, preview, create (and pay), order list / stats /
//! detail / download / cancel, payment channels.

use std::collections::HashMap;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::order::OrderService;
use zs_app::order::checkout::CheckoutRequest;
use zs_app::order::payment::PayRequest;
use zs_app::order::query::Viewer;
use zs_domain::order::model::{Order, keys};
use zs_domain::order::view::OrderSummary;
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::{PageRequest, Pagination};

use super::common::{
    ItemBody, OrderError, TenantCtx, checkout_error, download, form_data, items_of, request_scheme,
};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Query, req};
use crate::middleware::auth::CurrentUser;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/cart", get_cart)
        .post("/cart/items", upsert_cart_item)
        .delete("/cart/items/{product_id}", delete_cart_item)
        .post("/orders/preview", preview)
        .post("/orders", create)
        .post("/orders/create-and-pay", create_and_pay)
        .post("/order/payment-channels", payment_channels)
        .get("/orders", list)
        .get("/orders/stats", stats)
        .get("/orders/{order_no}", detail)
        .get(
            "/orders/{order_no}/fulfillment/download",
            download_fulfillment,
        )
        .post("/orders/{order_no}/cancel", cancel)
}

pub(super) type Params = Query<HashMap<String, String>>;

/// Lenient `page` / `page_size` (invalid values fall back to the defaults).
pub(super) fn page_of(q: &HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

pub(super) fn text(q: &HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

#[derive(Debug, Deserialize)]
struct CartItemBody {
    #[serde(default)]
    product_id: Id,
    #[serde(default)]
    sku_id: Id,
    #[serde(default)]
    quantity: i32,
}

impl BindRules for CartItemBody {
    const FIELDS: &'static [BindField] =
        &[req("product_id", "ProductID"), req("quantity", "Quantity")];
}

async fn get_cart(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Data<Value>> {
    let items = s.svc.order.service.cart(user.id).await?;
    ok(json!({ "items": items }))
}

async fn upsert_cart_item(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    Bind(req): Bind<CartItemBody>,
) -> ApiResult<Data<Value>> {
    if req.product_id <= 0 {
        return Err(Error::invalid().into());
    }
    let svc = &s.svc.order.service;
    if req.quantity <= 0 {
        svc.remove_cart_item(user.id, req.product_id, req.sku_id)
            .await?;
    } else {
        svc.upsert_cart_item(user.id, req.product_id, req.sku_id, req.quantity)
            .await?;
    }
    ok(json!({ "updated": true }))
}

async fn delete_cart_item(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    Path(product_id): Path<String>,
    Query(q): Params,
) -> ApiResult<Data<Value>> {
    let product_id: Id = product_id
        .trim()
        .parse()
        .map_err(|_| Error::bad_request(keys::ORDER_ITEM_INVALID))?;
    let sku_id: Id = match q.get("sku_id").map(|v| v.trim()).filter(|v| !v.is_empty()) {
        Some(v) => v
            .parse()
            .map_err(|_| Error::bad_request(keys::ORDER_ITEM_INVALID))?,
        None => 0,
    };
    s.svc
        .order
        .service
        .remove_cart_item(user.id, product_id, sku_id)
        .await?;
    ok(json!({ "deleted": true }))
}

/// Preview / create body of logged-in users.
#[derive(Debug, Default, Deserialize)]
pub(super) struct OrderBody {
    #[serde(default)]
    items: Vec<ItemBody>,
    #[serde(default)]
    coupon_code: String,
    #[serde(default)]
    affiliate_code: String,
    #[serde(default)]
    affiliate_visitor_key: String,
    #[serde(default)]
    manual_form_data: Option<Value>,
    #[serde(default)]
    channel_id: Id,
    #[serde(default)]
    use_balance: bool,
}

impl BindRules for OrderBody {
    const FIELDS: &'static [BindField] = &[req("items", "Items")];
}

fn checkout(user_id: Id, tenant: TenantCtx, ip: &str, req: OrderBody) -> CheckoutRequest {
    CheckoutRequest {
        user_id,
        guest: None,
        tenant: tenant.0,
        items: items_of(&req.items),
        coupon_code: req.coupon_code,
        affiliate_code: req.affiliate_code,
        affiliate_visitor_key: req.affiliate_visitor_key,
        client_ip: ip.to_owned(),
        manual_form_data: form_data(req.manual_form_data),
        skip_risk: false,
        skip_ip_risk: false,
    }
}

async fn preview(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Client(client): Client,
    uri: Uri,
    headers: HeaderMap,
    Bind(req): Bind<OrderBody>,
) -> Result<Data<Value>, OrderError> {
    let preview = s
        .svc
        .order
        .service
        .preview(&checkout(user.id, tenant, &client.ip, req))
        .await
        .map_err(|e| checkout_error(e, &uri, &headers))?;
    Ok(Data(serde_json::to_value(preview).map_err(Error::from)?))
}

/// Order detail JSON (full delivered content) with the channel whitelist.
pub(super) async fn detail_json(
    svc: &OrderService,
    order: &Order,
    truncate: bool,
) -> Result<Value, Error> {
    Ok(serde_json::to_value(
        svc.order_detail(order, truncate).await?,
    )?)
}

async fn create(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Client(client): Client,
    uri: Uri,
    headers: HeaderMap,
    Bind(req): Bind<OrderBody>,
) -> Result<Data<Value>, OrderError> {
    let svc = &s.svc.order.service;
    let order = svc
        .create_order(&checkout(user.id, tenant, &client.ip, req))
        .await
        .map_err(|e| checkout_error(e, &uri, &headers))?;
    Ok(Data(detail_json(svc, &order, false).await?))
}

/// Pays a freshly created order and builds the create-and-pay response
/// (`respondCreateAndPay`): payment failures are reported in `payment_error`.
pub(super) async fn pay_created(
    svc: &OrderService,
    order: &Order,
    pay: PayRequest,
    uri: &Uri,
    headers: &HeaderMap,
) -> Result<Value, Error> {
    let order_json = detail_json(svc, order, false).await?;
    let mut out = json!({"order": order_json, "order_no": order.order_no});
    match svc.pay(&pay).await {
        Ok(view) => {
            if let (Some(obj), Value::Object(v)) =
                (out.as_object_mut(), serde_json::to_value(&view)?)
            {
                obj.insert("order_paid".into(), json!(view.order_paid));
                obj.insert("wallet_paid_amount".into(), json!(view.wallet_paid_amount));
                obj.insert("online_pay_amount".into(), json!(view.online_pay_amount));
                if view.payment_id.is_some() {
                    for key in [
                        "payment_id",
                        "provider_type",
                        "channel_type",
                        "interaction_mode",
                        "pay_url",
                        "qr_code",
                        "wallet_address",
                        "chain_amount",
                        "chain",
                        "token_id",
                    ] {
                        if let Some(value) = v.get(key) {
                            obj.insert(key.into(), value.clone());
                        }
                    }
                    for key in ["pay_url", "qr_code"] {
                        obj.entry(key).or_insert_with(|| json!(""));
                    }
                    obj.insert("expires_at".into(), json!(view.expires_at));
                }
            }
        }
        Err(e) => {
            let locale = crate::i18n::resolve_locale(uri.query(), headers);
            if let Some(obj) = out.as_object_mut() {
                obj.insert(
                    "payment_error".into(),
                    json!(crate::i18n::translate_args(locale, e.key(), e.args())),
                );
            }
        }
    }
    Ok(out)
}

async fn create_and_pay(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Client(client): Client,
    uri: Uri,
    headers: HeaderMap,
    Bind(req): Bind<OrderBody>,
) -> Result<Data<Value>, OrderError> {
    let svc = &s.svc.order.service;
    let (channel_id, use_balance) = (req.channel_id, req.use_balance);
    let tenant_value = tenant.0.clone();
    let order = svc
        .create_order(&checkout(user.id, tenant, &client.ip, req))
        .await
        .map_err(|e| checkout_error(e, &uri, &headers))?;
    if channel_id == 0 && !use_balance {
        let order_json = detail_json(svc, &order, false).await?;
        return Ok(Data(
            json!({"order": order_json, "order_no": order.order_no}),
        ));
    }
    let pay = PayRequest {
        order_id: order.id,
        channel_id,
        use_balance,
        client_ip: client.ip.clone(),
        tenant: tenant_value,
        scheme: request_scheme(&headers),
    };
    Ok(Data(pay_created(svc, &order, pay, &uri, &headers).await?))
}

#[derive(Debug, Deserialize)]
struct ChannelsBody {
    #[serde(default)]
    amount: Value,
    #[serde(default)]
    order_no: String,
    #[serde(default)]
    items: Vec<ItemBody>,
}

impl BindRules for ChannelsBody {
    const FIELDS: &'static [BindField] = &[req("amount", "Amount")];
}

async fn payment_channels(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Bind(req): Bind<ChannelsBody>,
) -> ApiResult<Data<Value>> {
    let raw = match &req.amount {
        Value::String(s) => s.trim().to_owned(),
        Value::Number(n) => n.to_string(),
        _ => String::new(),
    };
    if raw.is_empty() {
        return Err(Error::invalid().into());
    }
    let amount: Amount = raw.parse().map_err(|_| Error::invalid())?;
    if !amount.is_positive() {
        return ok(json!([]));
    }
    let svc = &s.svc.order.service;
    let product_ids: Vec<Id> = if !req.order_no.trim().is_empty() {
        let order = svc
            .get_order(&Viewer::User(user.id), &tenant.0, &req.order_no)
            .await?;
        order.all_items().iter().map(|i| i.product_id).collect()
    } else {
        req.items
            .iter()
            .map(|i| i.product_id)
            .filter(|id| *id > 0)
            .collect()
    };
    let payer = svc.payer_of(user.id).await?;
    let channels = svc.order_channels(payer, amount, &product_ids).await?;
    ok(serde_json::to_value(channels).map_err(Error::from)?)
}

async fn list(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Query(q): Params,
) -> ApiResult<Paged<OrderSummary>> {
    let page = page_of(&q);
    let result = s
        .svc
        .order
        .service
        .list_orders(
            &Viewer::User(user.id),
            &tenant.0,
            &text(&q, "status"),
            &text(&q, "order_no"),
            page,
        )
        .await?;
    Ok(Paged(
        result.items.iter().map(OrderSummary::from).collect(),
        Pagination::new(page, result.total),
    ))
}

async fn stats(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Query(q): Params,
) -> ApiResult<Data<Value>> {
    let by_status = s
        .svc
        .order
        .service
        .order_stats(user.id, &tenant.0, &text(&q, "order_no"))
        .await?;
    let total: i64 = by_status.values().sum();
    ok(json!({ "total": total, "by_status": by_status }))
}

async fn detail(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Path(order_no): Path<String>,
) -> ApiResult<Data<Value>> {
    if order_no.trim().is_empty() {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID).into());
    }
    let svc = &s.svc.order.service;
    let order = svc
        .get_order(&Viewer::User(user.id), &tenant.0, &order_no)
        .await?;
    ok(detail_json(svc, &order, true).await?)
}

async fn download_fulfillment(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Path(order_no): Path<String>,
) -> Result<Response, crate::response::ApiError> {
    let order = s
        .svc
        .order
        .service
        .get_any_order(&Viewer::User(user.id), &tenant.0, &order_no)
        .await?;
    let payload = OrderService::fulfillment_payload(&order)?;
    Ok(download(&order.order_no, payload))
}

async fn cancel(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Path(order_no): Path<String>,
) -> ApiResult<Data<Value>> {
    if order_no.trim().is_empty() {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID).into());
    }
    let order = s
        .svc
        .order
        .service
        .cancel_order(user.id, &tenant.0, &order_no)
        .await?;
    ok(
        serde_json::to_value(zs_domain::order::view::OrderDetail::new(&order, false))
            .map_err(Error::from)?,
    )
}
