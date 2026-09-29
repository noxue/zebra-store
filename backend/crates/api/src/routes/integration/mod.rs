//! `integration` endpoints: API credentials (user + admin), site connections,
//! product mappings, procurement orders, reconciliation (compliance-gated) and the
//! upstream API served under `/api/v1/upstream`.

mod admin;
mod card_converter;
mod credential;
mod mapping;
mod procurement;
mod provide;
mod reconciliation;
pub mod upstream;
pub mod zs;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use zs_domain::integration::connection::Decimal;
use zs_domain::{Error, Id};
use zs_shared::page::PageRequest;

use super::{RouteSet, Routes};
use crate::extract::{BindField, BindRules, req_min1};

/// Routes of the integration group.
pub fn routes() -> RouteSet {
    RouteSet {
        user: credential::user().merge(provide::user()),
        admin: Routes::new("/admin")
            .merge(credential::admin())
            .merge(admin::connections())
            .merge(mapping::admin())
            .merge(procurement::admin())
            .merge(reconciliation::admin())
            .merge(card_converter::admin()),
        upstream: upstream::routes(),
        user_open: zs::routes(),
        root: provide::root(),
        ..RouteSet::default()
    }
}

/// Raw query parameters (lenient parsing like the original `c.Query`).
pub(crate) type Params = HashMap<String, String>;

fn param<'a>(q: &'a Params, key: &str) -> &'a str {
    q.get(key).map(|v| v.trim()).unwrap_or_default()
}

/// Positive integer parameter (invalid / missing → 0, like `ParseQueryUint(…, false)`).
fn id_param(q: &Params, key: &str) -> Id {
    param(q, key)
        .parse::<Id>()
        .ok()
        .filter(|v| *v > 0)
        .unwrap_or(0)
}

/// `page` / `page_size` with the given default and maximum page size.
fn page_of(q: &Params, page_key: &str, size_key: &str, default: u64, max: u64) -> PageRequest {
    let page = param(q, page_key)
        .parse::<u64>()
        .ok()
        .filter(|p| *p > 0)
        .unwrap_or(1);
    let size = param(q, size_key)
        .parse::<u64>()
        .ok()
        .filter(|p| *p > 0)
        .unwrap_or(default)
        .min(max);
    PageRequest {
        page,
        page_size: size,
    }
}

/// Standard admin pagination (default 20, max 200).
fn page(q: &Params) -> PageRequest {
    page_of(q, "page", "page_size", 20, zs_shared::page::MAX_PAGE_SIZE)
}

/// Optional RFC3339 time; an unparseable value is a 400 (UPS-22).
fn time_param(q: &Params, key: &str) -> Result<Option<DateTime<Utc>>, Error> {
    let raw = param(q, key);
    if raw.is_empty() {
        return Ok(None);
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|t| Some(t.with_timezone(&Utc)))
        .map_err(|_| Error::invalid())
}

/// Accepts a decimal as number or string; blank / null → `None`.
fn de_opt_decimal<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Decimal>, D::Error> {
    match Value::deserialize(d)? {
        Value::Null => Ok(None),
        Value::Number(n) => n
            .to_string()
            .parse::<Decimal>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        Value::String(s) if s.trim().is_empty() => Ok(None),
        Value::String(s) => s
            .trim()
            .parse::<Decimal>()
            .map(Some)
            .map_err(serde::de::Error::custom),
        _ => Err(serde::de::Error::custom("invalid decimal")),
    }
}

/// `{"ids": [..]}` batch body (at least one id).
#[derive(Debug, Deserialize)]
struct IdsRequest {
    #[serde(default)]
    ids: Vec<Id>,
    #[serde(default)]
    is_active: bool,
}

impl BindRules for IdsRequest {
    const FIELDS: &'static [BindField] = &[req_min1("ids", "IDs")];
}

impl IdsRequest {
    fn ids(&self) -> Result<&[Id], Error> {
        if self.ids.is_empty() {
            Err(Error::invalid())
        } else {
            Ok(&self.ids)
        }
    }
}

/// `{"is_active": bool}` body.
#[derive(Debug, Deserialize)]
struct ActiveRequest {
    #[serde(default)]
    is_active: bool,
}
