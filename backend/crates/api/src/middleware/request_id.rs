//! `X-Request-ID` propagation.

use axum::extract::Request;
use axum::http::HeaderValue;
use axum::middleware::Next;
use axum::response::Response;

/// Header carrying the request id.
pub const HEADER: &str = "x-request-id";

/// The request id, available as a request extension.
#[derive(Debug, Clone)]
pub struct RequestId(pub String);

/// Reuses an incoming `X-Request-ID` (if sane) or generates a UUID, and echoes it back.
pub async fn layer(mut req: Request, next: Next) -> Response {
    let id = req
        .headers()
        .get(HEADER)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 128)
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    req.extensions_mut().insert(RequestId(id.clone()));
    let mut res = next.run(req).await;
    if let Ok(value) = HeaderValue::from_str(&id) {
        res.headers_mut().insert(HEADER, value);
    }
    res
}
