//! The upstream API we serve (`/api/v1/upstream/*`, HMAC-signed, real HTTP status
//! codes, `{ok:false, error_code, error_message}` errors) and the supplier callback
//! we receive as a buyer (`POST /api/v1/upstream/callback`, `{ok, message}`).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequest, OriginalUri, Path, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::integration::credential::{AuthFailure, Caller, SignedRequest};
use zs_app::integration::inbound::InboundFailure;
use zs_app::integration::supplier::{Failure, OrderRequest};
use zs_domain::Id;
use zs_domain::catalog::product::JsonMap;
use zs_domain::integration::adapter::{InboundError, InboundRequest};
use zs_domain::integration::connection::PROTOCOL_DUJIAO_NEXT;
use zs_domain::integration::supplier::{
    MAX_BODY_BYTES, MAX_CALLBACK_BODY_BYTES, PRODUCTS_MAX_PAGE_SIZE,
};
use zs_shared::sign;

use super::{Params, param};
use crate::client::resolve_ip;
use crate::extract::{
    BindFailure, BindField, BindRules, Query, bind_value, req, req_min1, validator_text,
};
use crate::middleware::rate_limit::{self, MSG_RATE_LIMITED};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/upstream")
        .post("/ping", ping)
        .get("/categories", categories)
        .get("/products", products)
        .get("/products/{id}", product)
        .post("/orders", create_order)
        .get("/orders/{id}", get_order)
        .post("/orders/{id}/cancel", cancel_order)
        .post("/callback", callback)
}

/// `{ok:false, error_code, error_message}` with a real HTTP status.
fn fail(status: u16, code: &str, message: &str) -> Response {
    let status = StatusCode::from_u16(status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    (
        status,
        Json(json!({"ok": false, "error_code": code, "error_message": message})),
    )
        .into_response()
}

fn failure(f: Failure) -> Response {
    fail(f.status, f.code, &f.message)
}

fn internal() -> Response {
    fail(500, "internal_error", "internal error")
}

pub(super) fn header(headers: &HeaderMap, name: &str) -> String {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

pub(super) fn client_ip(req: &Request, s: &AppState) -> String {
    let peer = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip());
    let xff = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    resolve_ip(peer, xff, &s.trusted_proxies)
}

/// An authenticated upstream API request (original `UpstreamAPIAuthMiddleware`,
/// preceded by the `IP|API key` rate limit, UPS-10).
#[derive(Debug)]
pub struct Signed {
    pub caller: Caller,
    pub body: Bytes,
    pub client_ip: String,
}

impl FromRequest<AppState> for Signed {
    type Rejection = Response;

    async fn from_request(req: Request, s: &AppState) -> Result<Self, Self::Rejection> {
        let ip = client_ip(&req, s);
        let headers = req.headers().clone();
        let api_key = header(&headers, sign::HEADER_API_KEY);
        let limit_key = rate_limit::key_by_ip_and_header(&ip, &api_key);
        if let Err(e) =
            rate_limit::check(&s.svc.integration.api_limiter, &limit_key, MSG_RATE_LIMITED)
        {
            return Err(e.into_response());
        }
        let path = req
            .extensions()
            .get::<OriginalUri>()
            .map_or_else(|| req.uri().path().to_owned(), |u| u.0.path().to_owned());
        let method = req.method().as_str().to_owned();
        let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await else {
            return Err(fail(400, "bad_request", "failed to read request body"));
        };
        let timestamp = header(&headers, sign::HEADER_TIMESTAMP);
        let signature = header(&headers, sign::HEADER_SIGNATURE);
        let auth = s
            .svc
            .integration
            .credentials
            .authenticate(SignedRequest {
                api_key: &api_key,
                timestamp: &timestamp,
                signature: &signature,
                method: &method,
                path: &path,
                body: &body,
            })
            .await;
        match auth {
            Ok(caller) => Ok(Self {
                caller,
                body,
                client_ip: ip,
            }),
            Err(f) => Err(match f {
                AuthFailure::MissingHeaders => fail(
                    401,
                    "missing_auth_headers",
                    "missing authentication headers",
                ),
                AuthFailure::InvalidTimestamp => {
                    fail(401, "invalid_timestamp", "invalid timestamp")
                }
                AuthFailure::TimestampExpired => {
                    fail(401, "timestamp_expired", "timestamp expired")
                }
                AuthFailure::InvalidKey => {
                    fail(403, "invalid_api_key", "api key is invalid or disabled")
                }
                AuthFailure::UserDisabled => fail(403, "user_disabled", "user account is disabled"),
                AuthFailure::InvalidSignature => {
                    fail(401, "invalid_signature", "signature verification failed")
                }
                AuthFailure::Internal => internal(),
            }),
        }
    }
}

async fn ping(State(s): State<AppState>, req: Signed) -> Response {
    match s.svc.integration.supplier.ping(&req.caller).await {
        Ok(v) => Json(v).into_response(),
        Err(error) => {
            tracing::error!(%error, "upstream ping failed");
            internal()
        }
    }
}

async fn categories(State(s): State<AppState>, _req: Signed) -> Response {
    match s.svc.integration.supplier.categories().await {
        Ok(list) => {
            let items: Vec<Value> = list
                .into_iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "parent_id": c.parent_id,
                        "slug": c.slug,
                        "name": c.name,
                        "icon": c.icon,
                        "sort_order": c.sort_order,
                    })
                })
                .collect();
            Json(json!({"ok": true, "categories": items})).into_response()
        }
        Err(error) => {
            tracing::error!(%error, "upstream list categories failed");
            fail(500, "internal_error", "failed to list categories")
        }
    }
}

async fn products(State(s): State<AppState>, Query(q): Query<Params>, req: Signed) -> Response {
    let get = |k: &str| param(&q, k).to_owned();
    let page = get("page")
        .parse::<i64>()
        .ok()
        .filter(|p| *p > 0)
        .unwrap_or(1);
    let page_size = get("page_size")
        .parse::<i64>()
        .ok()
        .filter(|p| *p > 0)
        .unwrap_or(PRODUCTS_MAX_PAGE_SIZE);
    let include_inactive = get("include_inactive") == "true";
    // An unparseable `updated_after` is ignored (full listing), like the original.
    let updated_after = DateTime::parse_from_rfc3339(&get("updated_after"))
        .ok()
        .map(|t| t.with_timezone(&Utc));
    match s
        .svc
        .integration
        .supplier
        .products(
            &req.caller,
            page,
            page_size,
            updated_after,
            include_inactive,
        )
        .await
    {
        Ok(l) => Json(json!({
            "ok": true,
            "items": l.items,
            "total": l.total,
            "page": l.page,
            "page_size": l.page_size,
            "includes_inactive": l.includes_inactive,
        }))
        .into_response(),
        Err(error) => {
            tracing::error!(%error, "upstream list products failed");
            fail(500, "internal_error", "failed to list products")
        }
    }
}

/// Delisted products answer 200 with `is_active=false`; deleted ones 404
/// `product_not_found` (UPS-14).
async fn product(State(s): State<AppState>, Path(id): Path<String>, req: Signed) -> Response {
    let Some(id) = id.trim().parse::<Id>().ok().filter(|v| *v > 0) else {
        return fail(404, "product_not_found", "product not found");
    };
    match s.svc.integration.supplier.product(&req.caller, id).await {
        Ok(Some(p)) => Json(json!({"ok": true, "product": p})).into_response(),
        Ok(None) => fail(404, "product_not_found", "product not found"),
        Err(error) => {
            tracing::error!(%error, id, "upstream get product failed");
            fail(500, "internal_error", "failed to get product")
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CreateOrderBody {
    sku_id: Id,
    quantity: i32,
    manual_form_data: Option<JsonMap>,
    downstream_order_no: String,
    trace_id: String,
    callback_url: String,
}

impl BindRules for CreateOrderBody {
    const FIELDS: &'static [BindField] =
        &[req("sku_id", "SKUID"), req_min1("quantity", "Quantity")];
}

async fn create_order(State(s): State<AppState>, req: Signed) -> Response {
    let body: CreateOrderBody = match bind_value(&req.body) {
        Ok(b) => b,
        Err(BindFailure::Fields(args)) => {
            let text = validator_text("createOrderRequest", &args);
            return fail(400, "bad_request", &format!("invalid request body: {text}"));
        }
        Err(BindFailure::Invalid) => {
            let detail = serde_json::from_slice::<CreateOrderBody>(&req.body)
                .err()
                .map_or_else(|| "invalid json".to_owned(), |e| e.to_string());
            return fail(
                400,
                "bad_request",
                &format!("invalid request body: {detail}"),
            );
        }
    };
    let order = OrderRequest {
        sku_id: body.sku_id,
        quantity: body.quantity,
        manual_form_data: body.manual_form_data,
        downstream_order_no: body.downstream_order_no,
        trace_id: body.trace_id,
        callback_url: body.callback_url,
        client_ip: req.client_ip.clone(),
    };
    match s
        .svc
        .integration
        .supplier
        .create_order(&req.caller, &order)
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(f) => failure(f),
    }
}

fn order_id(raw: &str) -> Option<Id> {
    raw.trim().parse::<Id>().ok().filter(|v| *v > 0)
}

async fn get_order(State(s): State<AppState>, Path(id): Path<String>, req: Signed) -> Response {
    let Some(id) = order_id(&id) else {
        return fail(400, "bad_request", "invalid order id");
    };
    match s.svc.integration.supplier.get_order(&req.caller, id).await {
        Ok(v) => Json(v).into_response(),
        Err(f) => failure(f),
    }
}

async fn cancel_order(State(s): State<AppState>, Path(id): Path<String>, req: Signed) -> Response {
    let Some(id) = order_id(&id) else {
        return fail(400, "bad_request", "invalid order id");
    };
    match s
        .svc
        .integration
        .supplier
        .cancel_order(&req.caller, id)
        .await
    {
        Ok(v) => Json(v).into_response(),
        Err(f) => failure(f),
    }
}

/// Supplier callback (we are the buyer). Always HTTP 200 with `{ok, message}`; the
/// body is capped at 1 MB (UPS-02) and the route is rate limited per IP.
pub async fn callback(State(s): State<AppState>, req: Request) -> Response {
    let ip = client_ip(&req, &s);
    if let Err(e) = rate_limit::check(&s.svc.integration.callback_limiter, &ip, MSG_RATE_LIMITED) {
        return e.into_response();
    }
    let headers = req.headers().clone();
    let path = req.uri().path().to_owned();
    let query = req.uri().query().unwrap_or_default().to_owned();
    let reply =
        |ok: bool, message: &str| Json(json!({"ok": ok, "message": message})).into_response();
    let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_CALLBACK_BODY_BYTES).await else {
        return reply(false, "failed to read request body");
    };
    let lookup = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let inbound = InboundRequest {
        method: "POST",
        path: &path,
        query: &query,
        headers: &lookup,
        body: &body,
    };
    let r = s
        .svc
        .integration
        .inbound
        .handle(PROTOCOL_DUJIAO_NEXT, &inbound)
        .await;
    match r {
        Ok(_) => reply(true, "received"),
        Err(f) => reply(false, callback_message(f)),
    }
}

/// Messages of the original callback handler.
fn callback_message(f: InboundFailure) -> &'static str {
    match f {
        InboundFailure::Adapter(InboundError::MissingHeaders) => "missing authentication headers",
        InboundFailure::Adapter(InboundError::InvalidTimestamp) => "invalid timestamp",
        InboundFailure::Adapter(InboundError::TimestampExpired) => "timestamp expired",
        InboundFailure::Adapter(InboundError::InvalidSignature) => "signature verification failed",
        InboundFailure::Adapter(InboundError::InvalidBody) => "invalid request body",
        InboundFailure::Adapter(InboundError::MissingFields) => "missing required fields",
        InboundFailure::InvalidKey => "invalid api key",
        InboundFailure::NotFound => "procurement order not found",
        InboundFailure::Processing => "callback processing failed",
        InboundFailure::UnknownProtocol | InboundFailure::Internal => "internal error",
    }
}
