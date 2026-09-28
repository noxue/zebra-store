//! Shared pieces of the order endpoints: tenant / guest extractors, guest rate limits,
//! the risk rate-limit response, request DTOs and file downloads.

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;
use zs_app::order::checkout::ItemRequest;
use zs_app::order::query::Viewer;
use zs_domain::order::guest::{MAX_AUTHORIZATION_BYTES, split_credentials};
use zs_domain::order::model::{JsonMap, keys};
use zs_domain::{Error, Id};

use crate::client::Client;
use crate::i18n;
use crate::middleware::rate_limit::{MSG_RATE_LIMITED, check};
use crate::middleware::tenant::{Tenant, tenant_of};
use crate::response::ApiError;
use crate::state::AppState;

/// The request's reseller tenant (main shop when the middleware did not run).
#[derive(Debug, Clone)]
pub struct TenantCtx(pub Tenant);

impl<S: Send + Sync> FromRequestParts<S> for TenantCtx {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self(tenant_of(parts)))
    }
}

/// Decodes unpadded base64url (`base64.RawURLEncoding`).
pub fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn value(c: u8) -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    if bytes.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= value(*c)? << (18 - 6 * i);
        }
        let n = chunk.len();
        let decoded = acc.to_be_bytes();
        out.extend_from_slice(&decoded[1..n]);
        // Non-canonical trailing bits are rejected like Go's strict decoder.
        let unused = match n {
            2 => acc & 0xFFFF,
            3 => acc & 0xFF,
            _ => 0,
        };
        if unused != 0 {
            return None;
        }
    }
    Some(out)
}

/// Guest credentials from `Authorization: Guest <base64url(email "\n" password)>`
/// (ORD-02); query or body credentials are never read.
pub fn guest_credentials(headers: &HeaderMap) -> Option<(String, String)> {
    const SCHEME: &str = "Guest ";
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?.trim();
    if value.len() <= SCHEME.len() || value.len() > MAX_AUTHORIZATION_BYTES {
        return None;
    }
    if !value[..SCHEME.len()].eq_ignore_ascii_case(SCHEME) {
        return None;
    }
    let raw = base64url_decode(value[SCHEME.len()..].trim())?;
    split_credentials(&String::from_utf8(raw).ok()?)
}

/// Extractor of the guest viewer; missing credentials answer `error.guest_email_required`.
#[derive(Debug, Clone)]
pub struct GuestViewer(pub Viewer);

impl<S: Send + Sync> FromRequestParts<S> for GuestViewer {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let (email, password) = guest_credentials(&parts.headers)
            .ok_or_else(|| Error::bad_request(keys::GUEST_EMAIL_REQUIRED))?;
        Ok(Self(Viewer::Guest { email, password }))
    }
}

/// Guest read limit (120 / min per IP).
pub struct GuestRead;
/// Guest write limit (20 / min per IP, 300 s block).
pub struct GuestWrite;

impl<S> FromRequestParts<S> for GuestRead
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Client(client) = Client::from_request_parts(parts, state).await?;
        let app = AppState::from_ref(state);
        check(
            &app.svc.order.guest_read_limiter,
            &client.ip,
            MSG_RATE_LIMITED,
        )?;
        Ok(Self)
    }
}

impl<S> FromRequestParts<S> for GuestWrite
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Client(client) = Client::from_request_parts(parts, state).await?;
        let app = AppState::from_ref(state);
        check(
            &app.svc.order.guest_write_limiter,
            &client.ip,
            MSG_RATE_LIMITED,
        )?;
        Ok(Self)
    }
}

impl std::fmt::Debug for GuestRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GuestRead")
    }
}

impl std::fmt::Debug for GuestWrite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GuestWrite")
    }
}

/// Handler error that also renders the order rate limit with `Retry-After` (RISK-01).
#[derive(Debug)]
pub enum OrderError {
    Api(ApiError),
    RateLimited {
        message: String,
        retry_after: String,
    },
}

impl From<ApiError> for OrderError {
    fn from(e: ApiError) -> Self {
        Self::Api(e)
    }
}

impl From<Error> for OrderError {
    fn from(e: Error) -> Self {
        Self::Api(e.into())
    }
}

impl IntoResponse for OrderError {
    fn into_response(self) -> Response {
        match self {
            Self::Api(e) => e.into_response(),
            Self::RateLimited {
                message,
                retry_after,
            } => {
                let mut resp = (
                    StatusCode::OK,
                    axum::Json(json!({"status_code": 429, "msg": message, "data": {}})),
                )
                    .into_response();
                if let Ok(v) = HeaderValue::from_str(&retry_after) {
                    resp.headers_mut().insert(header::RETRY_AFTER, v);
                }
                resp
            }
        }
    }
}

/// Maps a checkout error; the order rate limit becomes `"<msg> (<n>s)"` + `Retry-After`.
pub fn checkout_error(e: Error, uri: &Uri, headers: &HeaderMap) -> OrderError {
    if e.key() == keys::RISK_ORDER_RATE_LIMITED
        && let Some(secs) = e
            .args()
            .first()
            .filter(|s| s.parse::<i64>().is_ok_and(|n| n > 0))
    {
        let locale = i18n::resolve_locale(uri.query(), headers);
        return OrderError::RateLimited {
            message: format!(
                "{} ({secs}s)",
                i18n::translate(locale, keys::RISK_ORDER_RATE_LIMITED)
            ),
            retry_after: secs.clone(),
        };
    }
    OrderError::Api(e.into())
}

/// Request scheme for tenant return URLs (`X-Forwarded-Proto`, else `http`).
pub fn request_scheme(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(',').next())
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| v == "http" || v == "https")
        .unwrap_or_else(|| "http".to_owned())
}

/// An order line of a request body (`OrderItemRequest`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ItemBody {
    #[serde(default)]
    pub product_id: Id,
    #[serde(default)]
    pub sku_id: Id,
    #[serde(default)]
    pub quantity: i32,
}

pub fn items_of(items: &[ItemBody]) -> Vec<ItemRequest> {
    items
        .iter()
        .map(|i| ItemRequest {
            product_id: i.product_id,
            sku_id: i.sku_id,
            quantity: i.quantity,
        })
        .collect()
}

/// `manual_form_data` accepts any object (non-object values are ignored).
pub fn form_data(raw: Option<serde_json::Value>) -> JsonMap {
    match raw {
        Some(serde_json::Value::Object(m)) => m,
        _ => JsonMap::new(),
    }
}

/// A plain-text attachment (`fulfillment-<order_no>.txt`); the file name keeps only safe
/// characters (DLV-05).
pub fn download(order_no: &str, payload: String) -> Response {
    let safe: String = order_no
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let mut resp = (StatusCode::OK, payload).into_response();
    let h = resp.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    if let Ok(v) =
        HeaderValue::from_str(&format!("attachment; filename=\"fulfillment-{safe}.txt\""))
    {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_raw_url_base64() {
        assert_eq!(
            base64url_decode("YUBiLmNvbQphYmNkZWY").as_deref(),
            Some(&b"a@b.com\nabcdef"[..])
        );
        assert_eq!(base64url_decode("_-8"), Some(vec![0xff, 0xef]));
        assert_eq!(base64url_decode("YQ=="), None);
        assert_eq!(base64url_decode("Y"), None);
    }

    /// ORD-02: only the `Guest` authorization header carries credentials.
    #[test]
    fn ord_02_guest_header() {
        let mut h = HeaderMap::new();
        assert_eq!(guest_credentials(&h), None);
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Guest QUBCLkNPTQphYmNkZWY"),
        );
        assert_eq!(
            guest_credentials(&h),
            Some(("a@b.com".into(), "abcdef".into()))
        );
        h.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer QUBCLkNPTQphYmNkZWY"),
        );
        assert_eq!(guest_credentials(&h), None);
    }
}
