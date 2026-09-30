//! Storefront payment endpoints of orders (order group): `POST /payments`,
//! `POST /payments/:id/capture`, `GET /payments/latest` and their guest variants.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::order::payment::{LatestPaymentView, PayRequest, PaymentView};
use zs_app::order::query::Viewer;
use zs_domain::payment::errors::keys as pay_keys;
use zs_domain::{Error, Id};

use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Query, req};
use crate::middleware::auth::CurrentUser;
use crate::response::{ApiError, ApiResult, Data, ok};
use crate::routes::Routes;
use crate::routes::order::common::{GuestRead, GuestViewer, GuestWrite, TenantCtx, request_scheme};
use crate::state::AppState;

pub(super) fn user_routes() -> Routes {
    Routes::new("")
        .post("/payments", create)
        .post("/payments/{id}/capture", capture)
        .get("/payments/latest", latest)
}

pub(super) fn guest_routes() -> Routes {
    Routes::new("/guest")
        .post("/payments", guest_create)
        .post("/payments/{id}/capture", guest_capture)
        .get("/payments/latest", guest_latest)
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    #[serde(default)]
    order_no: String,
    #[serde(default)]
    channel_id: Id,
    #[serde(default)]
    channel_type: String,
    #[serde(default)]
    use_balance: bool,
}

impl BindRules for CreateBody {
    const FIELDS: &'static [BindField] = &[req("order_no", "OrderNo")];
}

/// Guest variant: the original `CreateGuestPaymentRequest` also requires `channel_id`.
#[derive(Debug, Deserialize)]
#[serde(transparent)]
struct GuestCreateBody(CreateBody);

impl BindRules for GuestCreateBody {
    const FIELDS: &'static [BindField] =
        &[req("order_no", "OrderNo"), req("channel_id", "ChannelID")];
}

#[derive(Debug, Deserialize)]
struct LatestQuery {
    #[serde(default)]
    order_no: String,
}

fn payment_id(raw: &str) -> Result<Id, Error> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(pay_keys::PAYMENT_INVALID))
}

async fn pay(
    s: &AppState,
    viewer: Viewer,
    tenant: TenantCtx,
    ip: &str,
    headers: &HeaderMap,
    req: CreateBody,
    allow_balance: bool,
) -> ApiResult<Data<PaymentView>> {
    if req.order_no.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    let svc = &s.svc.order.service;
    let order = svc.get_order(&viewer, &tenant.0, &req.order_no).await?;
    let view = svc
        .pay(&PayRequest {
            order_id: order.id,
            channel_id: req.channel_id,
            channel_type: req.channel_type.clone(),
            use_balance: allow_balance && req.use_balance,
            client_ip: ip.to_owned(),
            tenant: tenant.0,
            scheme: request_scheme(headers),
        })
        .await?;
    ok(view)
}

async fn create(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Client(client): Client,
    headers: HeaderMap,
    Bind(req): Bind<CreateBody>,
) -> ApiResult<Data<PaymentView>> {
    pay(
        &s,
        Viewer::User(user.id),
        tenant,
        &client.ip,
        &headers,
        req,
        true,
    )
    .await
}

async fn guest_create(
    _: GuestWrite,
    State(s): State<AppState>,
    viewer: Result<GuestViewer, ApiError>,
    tenant: TenantCtx,
    Client(client): Client,
    headers: HeaderMap,
    Bind(GuestCreateBody(req)): Bind<GuestCreateBody>,
) -> ApiResult<Data<PaymentView>> {
    // The original binds the body before reading the guest credentials.
    let GuestViewer(viewer) = viewer?;
    if req.channel_id <= 0 {
        return Err(Error::invalid().into());
    }
    pay(&s, viewer, tenant, &client.ip, &headers, req, false).await
}

async fn capture_for(
    s: &AppState,
    viewer: Viewer,
    tenant: TenantCtx,
    id: &str,
) -> ApiResult<Data<Value>> {
    let id = payment_id(id)?;
    let svc = &s.svc.order.service;
    let payment = svc.owned_payment(&viewer, &tenant.0, id).await?;
    let updated = svc
        .capture(payment)
        .await
        .map_err(|e| e.or_internal(pay_keys::PAYMENT_CALLBACK_FAILED))?;
    ok(json!({"payment_id": updated.id, "status": updated.status}))
}

async fn capture(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Data<Value>> {
    capture_for(&s, Viewer::User(user.id), tenant, &id).await
}

async fn guest_capture(
    _: GuestWrite,
    State(s): State<AppState>,
    GuestViewer(viewer): GuestViewer,
    tenant: TenantCtx,
    Path(id): Path<String>,
) -> ApiResult<Data<Value>> {
    capture_for(&s, viewer, tenant, &id).await
}

async fn latest_for(
    s: &AppState,
    viewer: Viewer,
    tenant: TenantCtx,
    order_no: &str,
) -> ApiResult<Data<LatestPaymentView>> {
    if order_no.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    let svc = &s.svc.order.service;
    let order = svc.get_order(&viewer, &tenant.0, order_no).await?;
    ok(svc.latest_payment(&order).await?)
}

async fn latest(
    State(s): State<AppState>,
    CurrentUser(user): CurrentUser,
    tenant: TenantCtx,
    Query(q): Query<LatestQuery>,
) -> ApiResult<Data<LatestPaymentView>> {
    latest_for(&s, Viewer::User(user.id), tenant, &q.order_no).await
}

async fn guest_latest(
    _: GuestRead,
    State(s): State<AppState>,
    GuestViewer(viewer): GuestViewer,
    tenant: TenantCtx,
    Query(q): Query<LatestQuery>,
) -> ApiResult<Data<LatestPaymentView>> {
    latest_for(&s, viewer, tenant, &q.order_no).await
}
