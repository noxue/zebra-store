//! Guest storefront endpoints (`/api/v1/guest/*`): credentials only via
//! `Authorization: Guest …` (ORD-02); reads and writes are rate limited per IP.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::response::Response;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::order::checkout::{CheckoutRequest, GuestInfo};
use zs_app::order::payment::PayRequest;
use zs_app::order::{CAPTCHA_SCENE_GUEST_CREATE_ORDER, OrderService};
use zs_domain::identity::captcha::CaptchaPayload;
use zs_domain::order::model::keys;
use zs_domain::order::view::OrderSummary;
use zs_domain::{Error, Id};
use zs_shared::page::{PageRequest, Pagination};

use super::common::{
    GuestRead, GuestViewer, GuestWrite, ItemBody, OrderError, TenantCtx, checkout_error, download,
    form_data, items_of, request_scheme,
};
use super::user::{Params, detail_json, page_of, pay_created, text};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Query, req};
use crate::i18n;
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/guest")
        .post("/orders/preview", preview)
        .get("/orders", list)
        .get("/orders/{order_no}", detail)
        .get(
            "/orders/{order_no}/fulfillment/download",
            download_fulfillment,
        )
        .post("/orders", create)
        .post("/orders/create-and-pay", create_and_pay)
}

#[derive(Debug, Default, Deserialize)]
struct GuestOrderBody {
    #[serde(default)]
    email: String,
    #[serde(default)]
    order_password: String,
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
    captcha_payload: CaptchaPayload,
    #[serde(default)]
    channel_id: Id,
}

impl BindRules for GuestOrderBody {
    const FIELDS: &'static [BindField] = &[
        req("email", "Email"),
        req("order_password", "OrderPassword"),
        req("items", "Items"),
    ];
}

fn checkout(tenant: TenantCtx, ip: &str, locale: &str, req: GuestOrderBody) -> CheckoutRequest {
    CheckoutRequest {
        user_id: 0,
        guest: Some(GuestInfo {
            email: req.email,
            password: req.order_password,
            locale: locale.to_owned(),
        }),
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

/// `binding:"required"` of the original request (empty email / password / items).
fn require_body(req: &GuestOrderBody) -> Result<(), Error> {
    if req.email.is_empty() || req.order_password.is_empty() || req.items.is_empty() {
        return Err(Error::invalid());
    }
    Ok(())
}

async fn preview(
    _: GuestRead,
    State(s): State<AppState>,
    tenant: TenantCtx,
    Client(client): Client,
    uri: Uri,
    headers: HeaderMap,
    Bind(req): Bind<GuestOrderBody>,
) -> Result<Data<Value>, OrderError> {
    require_body(&req)?;
    let locale = i18n::resolve_locale(uri.query(), &headers);
    let preview = s
        .svc
        .order
        .service
        .preview(&checkout(tenant, &client.ip, locale, req))
        .await
        .map_err(|e| checkout_error(e, &uri, &headers))?;
    Ok(Data(serde_json::to_value(preview).map_err(Error::from)?))
}

async fn verify_captcha(s: &AppState, payload: &CaptchaPayload, ip: &str) -> Result<(), Error> {
    s.svc
        .identity
        .captcha
        .verify(CAPTCHA_SCENE_GUEST_CREATE_ORDER, payload, ip)
        .await
}

async fn create(
    _: GuestWrite,
    State(s): State<AppState>,
    tenant: TenantCtx,
    Client(client): Client,
    uri: Uri,
    headers: HeaderMap,
    Bind(req): Bind<GuestOrderBody>,
) -> Result<Data<Value>, OrderError> {
    require_body(&req)?;
    verify_captcha(&s, &req.captcha_payload, &client.ip).await?;
    let locale = i18n::resolve_locale(uri.query(), &headers);
    let svc = &s.svc.order.service;
    let order = svc
        .create_order(&checkout(tenant, &client.ip, locale, req))
        .await
        .map_err(|e| checkout_error(e, &uri, &headers))?;
    Ok(Data(detail_json(svc, &order, false).await?))
}

async fn create_and_pay(
    _: GuestWrite,
    State(s): State<AppState>,
    tenant: TenantCtx,
    Client(client): Client,
    uri: Uri,
    headers: HeaderMap,
    Bind(req): Bind<GuestOrderBody>,
) -> Result<Data<Value>, OrderError> {
    require_body(&req)?;
    verify_captcha(&s, &req.captcha_payload, &client.ip).await?;
    let locale = i18n::resolve_locale(uri.query(), &headers);
    let svc = &s.svc.order.service;
    let channel_id = req.channel_id;
    let tenant_value = tenant.0.clone();
    let order = svc
        .create_order(&checkout(tenant, &client.ip, locale, req))
        .await
        .map_err(|e| checkout_error(e, &uri, &headers))?;
    if channel_id == 0 {
        let order_json = detail_json(svc, &order, false).await?;
        return Ok(Data(
            json!({"order": order_json, "order_no": order.order_no}),
        ));
    }
    let pay = PayRequest {
        order_id: order.id,
        channel_id,
        use_balance: false,
        client_ip: client.ip.clone(),
        tenant: tenant_value,
        scheme: request_scheme(&headers),
    };
    Ok(Data(pay_created(svc, &order, pay, &uri, &headers).await?))
}

async fn list(
    _: GuestRead,
    State(s): State<AppState>,
    GuestViewer(viewer): GuestViewer,
    tenant: TenantCtx,
    Query(q): Params,
) -> ApiResult<Paged<OrderSummary>> {
    let svc = &s.svc.order.service;
    let order_no = text(&q, "order_no");
    if !order_no.is_empty() {
        let one = PageRequest {
            page: 1,
            page_size: 1,
        };
        return match svc.get_order(&viewer, &tenant.0, &order_no).await {
            Ok(order) => Ok(Paged(
                vec![OrderSummary::from(&order)],
                Pagination {
                    total_page: 1,
                    ..Pagination::new(one, 1)
                },
            )),
            Err(e) if e.is_not_found() => Ok(Paged(
                Vec::new(),
                Pagination {
                    total_page: 1,
                    ..Pagination::new(one, 0)
                },
            )),
            Err(e) => Err(e.or_internal(keys::ORDER_FETCH_FAILED).into()),
        };
    }
    let page = page_of(&q);
    let result = svc.list_orders(&viewer, &tenant.0, "", "", page).await?;
    Ok(Paged(
        result.items.iter().map(OrderSummary::from).collect(),
        Pagination::new(page, result.total),
    ))
}

async fn detail(
    _: GuestRead,
    State(s): State<AppState>,
    GuestViewer(viewer): GuestViewer,
    tenant: TenantCtx,
    Path(order_no): Path<String>,
) -> ApiResult<Data<Value>> {
    if order_no.trim().is_empty() {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID).into());
    }
    let svc = &s.svc.order.service;
    let order = svc.get_order(&viewer, &tenant.0, &order_no).await?;
    ok(detail_json(svc, &order, true).await?)
}

async fn download_fulfillment(
    _: GuestRead,
    State(s): State<AppState>,
    GuestViewer(viewer): GuestViewer,
    tenant: TenantCtx,
    Path(order_no): Path<String>,
) -> Result<Response, ApiError> {
    let order = s
        .svc
        .order
        .service
        .get_any_order(&viewer, &tenant.0, &order_no)
        .await?;
    let payload = OrderService::fulfillment_payload(&order)?;
    Ok(download(&order.order_no, payload))
}
