//! Gateway callbacks: `GET|POST /payments/callback`, `POST /payments/webhook/{dujiaopay,paypal,stripe}`
//! and the admin-configured custom paths (`callback_routes_config`).
//!
//! Bodies are capped at 1 MiB and processing runs in a detached task so a client
//! disconnect cannot abort verification half-way (PAY-04, PAY-13).

use std::sync::Arc;

use axum::body::Body as AxumBody;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use zs_app::payment::CallbackService;
use zs_app::payment::callback::{CallbackReply, CallbackRequest, WebhookKind};
use zs_domain::Error;
use zs_domain::payment::errors::keys;
use zs_domain::payment::gateway::Headers;
use zs_domain::payment::routes::CallbackKind;

use crate::client::Client;
use crate::middleware::rate_limit::{MSG_RATE_LIMITED, check};
use crate::response::{ApiError, Data};
use crate::routes::Routes;
use crate::state::AppState;

/// Largest accepted callback body (`maxCallbackBodyBytes`, 1 MiB).
const MAX_BODY_BYTES: usize = 1 << 20;

/// Default callback routes (mounted at `/api/v1`, no auth).
pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/payments/callback", shared_default)
        .post("/payments/callback", shared_default)
        .post("/payments/webhook/dujiaopay", dujiaopay_default)
        .post("/payments/webhook/paypal", paypal_default)
        .post("/payments/webhook/stripe", stripe_default)
}

/// Catch-all for custom callback paths under `/api/` (static routes always win).
pub(super) fn custom_routes() -> Routes {
    Routes::new("")
        .get("/api/{*rest}", custom)
        .post("/api/{*rest}", custom)
}

fn not_found() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

/// Reads the request into a [`CallbackRequest`]; `None` when the body exceeds 1 MiB.
async fn read_request(req: Request, client_ip: String) -> Option<CallbackRequest> {
    let (parts, body) = req.into_parts();
    let body = axum::body::to_bytes(body, MAX_BODY_BYTES).await.ok();
    let mut headers = Headers::new();
    for (name, value) in &parts.headers {
        if let Ok(v) = value.to_str() {
            headers.insert(name.as_str(), v);
        }
    }
    let content_type = headers.get("content-type");
    Some(CallbackRequest {
        method: parts.method.as_str().to_owned(),
        path: parts.uri.path().to_owned(),
        raw_query: parts.uri.query().unwrap_or_default().to_owned(),
        content_type,
        client_ip,
        headers,
        body: body?.to_vec(),
    })
}

fn render_reply(reply: CallbackReply) -> Response {
    let mut resp = (
        StatusCode::from_u16(reply.status).unwrap_or(StatusCode::OK),
        reply.body,
    )
        .into_response();
    if let Ok(ct) = HeaderValue::from_str(reply.content_type) {
        resp.headers_mut().insert(header::CONTENT_TYPE, ct);
    }
    resp
}

async fn run_shared(svc: Arc<CallbackService>, req: Request, client_ip: String) -> Response {
    let Some(creq) = read_request(req, client_ip).await else {
        // Unreadable (oversized) bodies match no provider format.
        return not_found();
    };
    match tokio::spawn(async move { svc.handle_shared(&creq).await }).await {
        Ok(reply) => render_reply(reply),
        Err(e) => {
            tracing::error!(error = %e, "payment_callback_task_failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

async fn run_webhook(
    svc: Arc<CallbackService>,
    kind: WebhookKind,
    req: Request,
    client_ip: String,
) -> Response {
    let Some(creq) = read_request(req, client_ip).await else {
        return ApiError::from(Error::invalid()).into_response();
    };
    let joined = tokio::spawn(async move { svc.handle_webhook(kind, &creq).await }).await;
    match joined {
        Ok(Ok(ack)) => Data(ack).into_response(),
        Ok(Err(e)) => ApiError::from(e).into_response(),
        Err(e) => {
            tracing::error!(error = %e, "payment_webhook_task_failed");
            ApiError::from(
                Error::internal_msg(e.to_string()).or_internal(keys::PAYMENT_CALLBACK_FAILED),
            )
            .into_response()
        }
    }
}

/// Applies the callback rate limit (default paths only, like the original route group).
fn limited(s: &AppState, ip: &str) -> Option<Response> {
    check(&s.svc.payment.callback_limiter, ip, MSG_RATE_LIMITED)
        .err()
        .map(IntoResponse::into_response)
}

async fn default_path(s: AppState, kind: CallbackKind, ip: String, req: Request) -> Response {
    if let Some(resp) = limited(&s, &ip) {
        return resp;
    }
    let svc = s.svc.payment.callbacks.clone();
    // A configured custom path hides the default one.
    if svc.default_path_hidden(kind).await {
        return not_found();
    }
    dispatch(svc, kind, req, ip).await
}

async fn dispatch(
    svc: Arc<CallbackService>,
    kind: CallbackKind,
    req: Request,
    ip: String,
) -> Response {
    match kind {
        CallbackKind::Shared => run_shared(svc, req, ip).await,
        CallbackKind::DujiaoPay => run_webhook(svc, WebhookKind::DujiaoPay, req, ip).await,
        CallbackKind::Paypal => run_webhook(svc, WebhookKind::Paypal, req, ip).await,
        CallbackKind::Stripe => run_webhook(svc, WebhookKind::Stripe, req, ip).await,
    }
}

async fn shared_default(State(s): State<AppState>, Client(c): Client, req: Request) -> Response {
    default_path(s, CallbackKind::Shared, c.ip, req).await
}

async fn dujiaopay_default(State(s): State<AppState>, Client(c): Client, req: Request) -> Response {
    default_path(s, CallbackKind::DujiaoPay, c.ip, req).await
}

async fn paypal_default(State(s): State<AppState>, Client(c): Client, req: Request) -> Response {
    default_path(s, CallbackKind::Paypal, c.ip, req).await
}

async fn stripe_default(State(s): State<AppState>, Client(c): Client, req: Request) -> Response {
    default_path(s, CallbackKind::Stripe, c.ip, req).await
}

/// Custom callback paths configured by the admin; anything else under `/api/` stays a 404.
async fn custom(State(s): State<AppState>, Client(c): Client, req: Request<AxumBody>) -> Response {
    let svc = s.svc.payment.callbacks.clone();
    let routes = svc.routes().await;
    if !routes.has_custom_routes() {
        return not_found();
    }
    let is_post = req.method() == axum::http::Method::POST;
    match routes.match_custom(req.uri().path(), is_post) {
        Some(kind) => dispatch(svc, kind, req, c.ip).await,
        None => not_found(),
    }
}
