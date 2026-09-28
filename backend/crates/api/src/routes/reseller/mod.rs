//! `reseller` endpoints: the user console `/api/v1/reseller/*` (main shop only) and
//! the admin `/api/v1/admin/resellers/*` pages.

mod admin;
mod dto;
mod user;

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveTime, TimeDelta, Utc};
use serde::Deserialize;
use zs_domain::reseller::keys;
use zs_domain::reseller::pricing::SettingInput;
use zs_domain::{Error, ErrorKind, Id};
use zs_shared::money::Amount;
use zs_shared::page::PageRequest;

use super::RouteSet;
use crate::response::ApiError;

/// Routes of the `reseller` group.
pub fn routes() -> RouteSet {
    RouteSet {
        user: user::routes(),
        admin: admin::routes(),
        ..RouteSet::default()
    }
}

/// Raw query string, parsed leniently like gin's `c.Query`.
#[derive(Debug, Default, Deserialize)]
#[serde(transparent)]
struct Params(HashMap<String, String>);

impl Params {
    fn str(&self, key: &str) -> String {
        self.0
            .get(key)
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    }

    /// `ginutil.ParsePagination`: invalid values fall back to defaults.
    fn page(&self) -> PageRequest {
        PageRequest::new(
            self.str("page").parse().ok(),
            self.str("page_size").parse().ok(),
        )
    }

    /// Optional positive id; malformed values are ignored (admin filters).
    fn id(&self, key: &str) -> Option<Id> {
        self.str(key).parse::<Id>().ok().filter(|v| *v > 0)
    }

    /// Optional id where malformed input is `error.bad_request`.
    fn strict_id(&self, key: &str) -> Result<Option<Id>, ApiError> {
        let raw = self.str(key);
        if raw.is_empty() {
            return Ok(None);
        }
        raw.parse::<Id>()
            .map(|v| (v > 0).then_some(v))
            .map_err(|_| Error::invalid().into())
    }

    /// RFC 3339 time; anything else is ignored (admin `parseTimePointer`).
    fn time(&self, key: &str) -> Option<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(&self.str(key))
            .ok()
            .map(|t| t.with_timezone(&Utc))
    }

    /// RFC 3339 or `YYYY-MM-DD` (end of day for `to` bounds); malformed → bad request.
    fn day_time(&self, key: &str, end_of_day: bool) -> Result<Option<DateTime<Utc>>, ApiError> {
        let raw = self.str(key);
        if raw.is_empty() {
            return Ok(None);
        }
        if let Ok(t) = DateTime::parse_from_rfc3339(&raw) {
            return Ok(Some(t.with_timezone(&Utc)));
        }
        let day = NaiveDate::parse_from_str(&raw, "%Y-%m-%d")
            .map_err(|_| ApiError::from(Error::invalid()))?;
        let mut t = day.and_time(NaiveTime::MIN).and_utc();
        if end_of_day {
            t += TimeDelta::days(1) - TimeDelta::nanoseconds(1);
        }
        Ok(Some(t))
    }
}

/// Parses an optional decimal string (`""` = 0, two decimals like every amount /
/// percentage column); malformed → bad request.
fn decimal(raw: &str) -> Result<Amount, ApiError> {
    raw.trim()
        .parse::<Amount>()
        .map_err(|_| Error::invalid().into())
}

/// A decimal given as JSON string or number (the original binds strings).
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Num {
    Text(String),
    Number(serde_json::Number),
}

/// Value of an optional [`Num`] (missing / null = 0).
fn num(v: Option<&Num>) -> Result<Amount, ApiError> {
    match v {
        Some(Num::Text(s)) => decimal(s),
        Some(Num::Number(n)) => decimal(&n.to_string()),
        None => Ok(Amount::ZERO),
    }
}

/// One rule of `{settings: [...]}` (`ProductSettingRequest`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SettingRequest {
    sku_id: Id,
    is_listed: bool,
    pricing_mode: String,
    markup_percent: Option<Num>,
    fixed_markup_amount: Option<Num>,
    fixed_price_amount: Option<Num>,
    sort_order: i32,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SettingsRequest {
    settings: Vec<SettingRequest>,
}

impl SettingsRequest {
    fn inputs(&self) -> Result<Vec<SettingInput>, ApiError> {
        self.settings
            .iter()
            .map(|s| {
                Ok(SettingInput {
                    sku_id: s.sku_id,
                    is_listed: s.is_listed,
                    pricing_mode: s.pricing_mode.trim().to_owned(),
                    markup_percent: num(s.markup_percent.as_ref())?.decimal(),
                    fixed_markup_amount: num(s.fixed_markup_amount.as_ref())?.decimal(),
                    fixed_price_amount: num(s.fixed_price_amount.as_ref())?.decimal(),
                    sort_order: s.sort_order,
                })
            })
            .collect()
    }
}

/// Console surfaces (management / site / products / orders) answer an inactive
/// profile with `error.forbidden` (status 400), like the original handlers.
fn console(e: Error, fallback: &'static str) -> ApiError {
    if e.key() == keys::PROFILE_INACTIVE {
        return Error::bad_request(keys::FORBIDDEN).into();
    }
    e.or_internal(fallback).into()
}

/// Admin management errors (`respondAdminManagementError`): not found is 404 with
/// `error.bad_request`, validation errors collapse to `error.bad_request`.
fn admin_mgmt(e: Error) -> ApiError {
    match e.kind() {
        ErrorKind::NotFound => Error::not_found(keys::BAD_REQUEST).into(),
        _ if keys::ADMIN_BAD_REQUEST.contains(&e.key()) => Error::invalid().into(),
        _ => e.or_internal("error.save_failed").into(),
    }
}

/// Admin product-setting errors (`respondAdminProductSettingError`).
fn admin_products(e: Error, fallback: &'static str) -> ApiError {
    if e.key() == keys::PROFILE_INACTIVE {
        return Error::invalid().into();
    }
    e.or_internal(fallback).into()
}
