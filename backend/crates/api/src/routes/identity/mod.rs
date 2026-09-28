//! Identity routes: admin login/2FA/RBAC, storefront auth and `/me`, captcha,
//! compliance and audit logs.

mod admin;
mod authz;
mod oauth;
mod user;

use axum::body::Bytes;
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use zs_domain::identity::mailer::BrandScope;
use zs_domain::{Error, Id};
use zs_shared::page::PageRequest;

pub use zs_app::identity::rfc3339;
pub use zs_domain::identity::captcha::CaptchaPayload;

use super::RouteSet;
use crate::response::ApiError;

/// Routes of the identity group.
pub fn routes() -> RouteSet {
    RouteSet {
        public: user::public(),
        auth: user::auth().merge(oauth::auth()),
        user: user::me().merge(oauth::me()),
        admin_open: admin::open(),
        admin: admin::authenticated().merge(authz::routes()),
        ..RouteSet::default()
    }
}

/// Email brand scope of a storefront request (original `mailbrand.scopeFromContext`):
/// a reseller site brands mails with its own host, the main shop with its brand.
fn brand_scope(tenant: Option<&crate::middleware::tenant::Tenant>) -> BrandScope {
    match tenant.filter(|t| t.is_reseller()) {
        Some(t) => BrandScope {
            reseller_id: t.reseller_id,
            host: if t.host.trim().is_empty() {
                t.primary_domain.clone()
            } else {
                t.host.clone()
            },
        },
        None => BrandScope::default(),
    }
}

/// Parses a JSON body; malformed input yields `error.bad_request`.
fn parse_json<T: DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|e| {
        tracing::debug!(error = %e, "invalid json body");
        Error::invalid().into()
    })
}

/// Reads a string field of a JSON body without failing (rate-limit keys).
fn json_field(body: &Bytes, field: &str) -> String {
    serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get(field).and_then(|f| f.as_str()).map(str::to_owned))
        .unwrap_or_default()
}

/// Positive numeric path/query id, or `err_key`.
fn parse_id(raw: &str, err_key: &'static str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(err_key).into())
}

/// Optional positive id query parameter (`error.bad_request` when malformed).
fn opt_id(raw: Option<&String>) -> Result<Option<Id>, ApiError> {
    match raw.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => parse_id(s, "error.bad_request").map(Some),
    }
}

/// Optional RFC 3339 query parameter (`error.bad_request` when malformed).
fn opt_time(raw: Option<&String>) -> Result<Option<DateTime<Utc>>, ApiError> {
    match raw.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => DateTime::parse_from_rfc3339(s)
            .map(|t| Some(t.with_timezone(&Utc)))
            .map_err(|_| Error::invalid().into()),
    }
}

/// `page` / `page_size` query parameters (lenient, like `ginutil.ParsePagination`).
fn page_of(q: &std::collections::HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

/// Go `strconv.ParseBool` semantics; empty means `false`.
fn parse_bool(raw: Option<&String>) -> Result<bool, ApiError> {
    match raw.map(|s| s.trim()).unwrap_or("") {
        "" => Ok(false),
        "1" | "t" | "T" | "TRUE" | "true" | "True" => Ok(true),
        "0" | "f" | "F" | "FALSE" | "false" | "False" => Ok(false),
        _ => Err(Error::invalid().into()),
    }
}

/// Text query parameter, trimmed.
fn text(q: &std::collections::HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}
