//! `marketing` endpoints: coupons, promotions, member levels, gift cards.

mod coupon;
mod gift_card;
mod member_level;
mod promotion;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use zs_domain::{Error, Id};
use zs_shared::page::PageRequest;

use super::RouteSet;

/// Routes of the `marketing` group.
pub fn routes() -> RouteSet {
    RouteSet {
        public: member_level::public(),
        admin: coupon::admin()
            .merge(promotion::admin())
            .merge(member_level::admin())
            .merge(gift_card::admin()),
        ..RouteSet::default()
    }
}

/// Lenient `page` / `page_size` query (non-numeric values fall back to defaults like `Atoi`).
#[derive(Debug, Default, Deserialize)]
struct PageQuery {
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    page_size: Option<String>,
}

impl PageQuery {
    fn request(&self) -> PageRequest {
        self.request_with_default(None)
    }

    /// Uses `default_size` when `page_size` is absent (e.g. member levels default to 50).
    fn request_with_default(&self, default_size: Option<u64>) -> PageRequest {
        let parse = |v: &Option<String>| v.as_deref().and_then(|s| s.trim().parse::<u64>().ok());
        let size = match &self.page_size {
            None => default_size,
            some => parse(some),
        };
        PageRequest::new(parse(&self.page), size)
    }
}

/// Optional RFC 3339 time (`""` → none), as the original `ginutil.ParseTimeNullable`.
fn parse_time(raw: &str) -> Result<Option<DateTime<Utc>>, Error> {
    if raw.is_empty() {
        return Ok(None);
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|t| Some(t.with_timezone(&Utc)))
        .map_err(|_| Error::invalid())
}

/// Optional id query value; `zero_invalid` rejects `0` (original `ginutil.ParseQueryUint`).
fn parse_query_id(raw: Option<&str>, zero_invalid: bool) -> Result<Id, Error> {
    let raw = raw.map(str::trim).unwrap_or_default();
    if raw.is_empty() {
        return Ok(0);
    }
    let v = raw.parse::<u64>().map_err(|_| Error::invalid())?;
    if zero_invalid && v == 0 {
        return Err(Error::invalid());
    }
    Id::try_from(v).map_err(|_| Error::invalid())
}

/// Optional boolean query value (Go `strconv.ParseBool`); absent/blank → none.
fn parse_bool(raw: Option<&str>) -> Result<Option<bool>, Error> {
    match raw.map(str::trim) {
        None | Some("") => Ok(None),
        Some("1" | "t" | "T" | "TRUE" | "true" | "True") => Ok(Some(true)),
        Some("0" | "f" | "F" | "FALSE" | "false" | "False") => Ok(Some(false)),
        Some(_) => Err(Error::invalid()),
    }
}
