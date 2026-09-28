//! Rate limiting helpers on top of [`zs_app::identity::rate_limit::RateLimiter`]
//! (in-process; limited requests get HTTP 429 like the original).

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use zs_app::identity::rate_limit::RateLimiter;
use zs_domain::Error;

use crate::client::{Cidr, resolve_ip};
use crate::response::ApiError;

/// Message key of generic limits.
pub const MSG_RATE_LIMITED: &str = "error.rate_limited";
/// Message key of login limits.
pub const MSG_LOGIN_TOO_MANY: &str = "error.login_too_many";

/// Counts one request for `key`; `Err` is a ready 429 response carrying the wait seconds.
pub fn check(limiter: &RateLimiter, key: &str, msg_key: &'static str) -> Result<(), ApiError> {
    limiter.hit(key).map_err(|wait| {
        ApiError::with_http_status(
            Error::too_many(msg_key).arg(wait),
            StatusCode::TOO_MANY_REQUESTS,
        )
    })
}

/// Key `"{value}|{ip}"` for a JSON field (lower-cased, trimmed), or `ip` when empty
/// (original `KeyByIPAndJSONField`).
pub fn key_by_ip_and_field(ip: &str, value: &str) -> String {
    let value = value.trim().to_lowercase();
    if value.is_empty() {
        ip.to_owned()
    } else {
        format!("{value}|{ip}")
    }
}

/// Key `"{ip}|{header}"` with the header truncated to 128 bytes (original
/// `KeyByIPAndHeader`: limits applied before authentication must include the IP).
pub fn key_by_ip_and_header(ip: &str, header: &str) -> String {
    let mut end = header.len().min(128);
    while !header.is_char_boundary(end) {
        end -= 1;
    }
    let header = &header[..end];
    if header.is_empty() {
        ip.to_owned()
    } else {
        format!("{ip}|{header}")
    }
}

/// State of [`by_ip`]: the limiter, the message key and the trusted proxies.
#[derive(Debug, Clone)]
pub struct IpLimit {
    pub limiter: RateLimiter,
    pub msg_key: &'static str,
    pub trusted_proxies: std::sync::Arc<Vec<Cidr>>,
}

/// Middleware limiting by client IP; apply with
/// `axum::middleware::from_fn_with_state(IpLimit { … }, by_ip)`.
pub async fn by_ip(State(limit): State<IpLimit>, req: Request, next: Next) -> Response {
    let peer = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip());
    let xff = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let ip = resolve_ip(peer, xff, &limit.trusted_proxies);
    match check(&limit.limiter, &ip, limit.msg_key) {
        Ok(()) => next.run(req).await,
        Err(e) => e.into_response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // RISK-06: the email part of the key is normalised.
    #[test]
    fn keys_are_normalized() {
        assert_eq!(
            key_by_ip_and_field("1.2.3.4", " Test@Example.com "),
            key_by_ip_and_field("1.2.3.4", "test@example.com")
        );
        assert_eq!(key_by_ip_and_field("1.2.3.4", " "), "1.2.3.4");
        let long = "k".repeat(300);
        assert_eq!(key_by_ip_and_header("ip", &long).len(), 3 + 128);
        assert_eq!(key_by_ip_and_header("ip", ""), "ip");
    }
}
