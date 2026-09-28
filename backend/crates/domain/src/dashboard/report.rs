//! Report windows (port of `reporting/application/resolver.go`) and the number
//! formatting shared by every dashboard response.

use chrono::{DateTime, Days, NaiveDate, SecondsFormat, TimeZone, Utc};
use chrono_tz::Tz;
use rust_decimal::{Decimal, RoundingStrategy};

use crate::{Error, Result};

/// Longest accepted `custom` range (original `CustomMaxDays`).
pub const CUSTOM_MAX_DAYS: i64 = 90;
/// Range used when the query omits `range`.
pub const DEFAULT_RANGE: &str = "7d";

/// Raw report query (`range`, `from`, `to`, `tz`, `force_refresh`).
#[derive(Debug, Clone, Default)]
pub struct ReportQuery {
    pub range: String,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub timezone: String,
    pub force_refresh: bool,
}

/// A normalized half-open interval `[start, end)` in a display time zone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub range: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub tz: Tz,
    /// Time-zone name echoed in responses.
    pub timezone: String,
}

impl Window {
    /// `from` of the responses: the start in the display zone (RFC 3339).
    pub fn from_label(&self) -> String {
        rfc3339_in(self.start, self.tz)
    }

    /// `to` of the responses: the last included second in the display zone.
    pub fn to_label(&self) -> String {
        rfc3339_in(self.end - chrono::Duration::seconds(1), self.tz)
    }

    /// Local calendar day (`YYYY-MM-DD`) of an instant in the display zone.
    pub fn day_of(&self, at: DateTime<Utc>) -> String {
        at.with_timezone(&self.tz).format("%Y-%m-%d").to_string()
    }

    /// Every local day touched by the window, in order (trend buckets).
    pub fn days(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut day = self.start.with_timezone(&self.tz).date_naive();
        while local_midnight(self.tz, day) < self.end {
            out.push(day.format("%Y-%m-%d").to_string());
            match day.checked_add_days(Days::new(1)) {
                Some(next) => day = next,
                None => break,
            }
        }
        out
    }
}

fn rfc3339_in(at: DateTime<Utc>, tz: Tz) -> String {
    at.with_timezone(&tz)
        .to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Midnight of `day` in `tz` as an instant; a skipped midnight (DST gap)
/// resolves to the first valid local time after it.
fn local_midnight(tz: Tz, day: NaiveDate) -> DateTime<Utc> {
    let naive = day.and_time(chrono::NaiveTime::MIN);
    (0..=3)
        .find_map(|hours| {
            tz.from_local_datetime(&(naive + chrono::Duration::hours(hours)))
                .earliest()
        })
        .map_or_else(|| naive.and_utc(), |t| t.with_timezone(&Utc))
}

/// Parses an IANA zone name; empty or unknown names fall back to UTC
/// (the original falls back to the server zone).
pub fn parse_zone(raw: &str) -> (Tz, String) {
    let name = raw.trim();
    match name.parse::<Tz>() {
        Ok(tz) if !name.is_empty() => (tz, name.to_owned()),
        _ => (Tz::UTC, "UTC".to_owned()),
    }
}

/// Resolves `q` against `now` (original `reporting.Resolve`).
///
/// Invalid ranges yield `error.bad_request`.
pub fn resolve(q: &ReportQuery, now: DateTime<Utc>) -> Result<Window> {
    let mut range = q.range.trim().to_lowercase();
    if range.is_empty() {
        range = DEFAULT_RANGE.to_owned();
    }
    let (tz, timezone) = parse_zone(&q.timezone);
    let today = now.with_timezone(&tz).date_naive();
    let days_back = |n: u64| {
        today
            .checked_sub_days(Days::new(n))
            .map(|d| local_midnight(tz, d))
            .ok_or_else(Error::invalid)
    };
    let tomorrow = today
        .checked_add_days(Days::new(1))
        .map(|d| local_midnight(tz, d))
        .ok_or_else(Error::invalid)?;
    let (start, end) = match range.as_str() {
        "today" => (days_back(0)?, tomorrow),
        "7d" => (days_back(6)?, tomorrow),
        "30d" => (days_back(29)?, tomorrow),
        "custom" => {
            let (Some(from), Some(to)) = (q.from, q.to) else {
                return Err(Error::invalid());
            };
            if to < from || to - from > chrono::Duration::days(CUSTOM_MAX_DAYS) {
                return Err(Error::invalid());
            }
            (from, to + chrono::Duration::seconds(1))
        }
        _ => return Err(Error::invalid()),
    };
    if end <= start {
        return Err(Error::invalid());
    }
    Ok(Window {
        range,
        start,
        end,
        tz,
        timezone,
    })
}

/// Formats a money value like the original `%.2f`.
pub fn money(value: Decimal) -> String {
    two_places(value)
}

/// Formats a percentage like the original `%.2f`.
pub fn percent(value: Decimal) -> String {
    two_places(value)
}

/// `part / whole * 100`, zero when `whole` is not positive.
pub fn rate(part: Decimal, whole: Decimal) -> Decimal {
    if whole > Decimal::ZERO {
        part * Decimal::ONE_HUNDRED / whole
    } else {
        Decimal::ZERO
    }
}

fn two_places(value: Decimal) -> String {
    let mut v = value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero);
    if v.is_zero() {
        v = Decimal::ZERO;
    }
    v.rescale(2);
    v.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn query(range: &str, tz: &str) -> ReportQuery {
        ReportQuery {
            range: range.into(),
            timezone: tz.into(),
            ..ReportQuery::default()
        }
    }

    #[test]
    fn seven_days_in_shanghai() {
        // 2026-09-23T17:30Z is already 09-24 01:30 in Shanghai.
        let w = resolve(&query("", "Asia/Shanghai"), at("2026-09-23T17:30:00Z")).unwrap();
        assert_eq!(w.range, "7d");
        assert_eq!(w.timezone, "Asia/Shanghai");
        assert_eq!(w.start, at("2026-09-17T16:00:00Z"));
        assert_eq!(w.end, at("2026-09-24T16:00:00Z"));
        assert_eq!(w.from_label(), "2026-09-18T00:00:00+08:00");
        assert_eq!(w.to_label(), "2026-09-24T23:59:59+08:00");
        let days = w.days();
        assert_eq!(days.len(), 7);
        assert_eq!(days[0], "2026-09-18");
        assert_eq!(days[6], "2026-09-24");
    }

    #[test]
    fn today_and_thirty_days() {
        let now = at("2026-09-24T10:00:00Z");
        let w = resolve(&query("TODAY", "UTC"), now).unwrap();
        assert_eq!(
            (w.start, w.end),
            (at("2026-09-24T00:00:00Z"), at("2026-09-25T00:00:00Z"))
        );
        assert_eq!(w.days(), vec!["2026-09-24".to_owned()]);
        let w = resolve(&query("30d", "UTC"), now).unwrap();
        assert_eq!(w.start, at("2026-08-26T00:00:00Z"));
        assert_eq!(w.days().len(), 30);
    }

    // DB-05 ③: a malicious zone name is never used; it falls back to UTC.
    #[test]
    fn invalid_zone_falls_back_to_utc() {
        let w = resolve(&query("today", "UTC'); drop"), at("2026-09-24T10:00:00Z")).unwrap();
        assert_eq!(w.timezone, "UTC");
        assert_eq!(w.tz, Tz::UTC);
    }

    // DB-05 ①: UTC 2026-03-30 17:00 belongs to 03-31 in Shanghai.
    #[test]
    fn day_bucket_uses_display_zone() {
        let w = resolve(&query("7d", "Asia/Shanghai"), at("2026-03-31T02:00:00Z")).unwrap();
        assert_eq!(w.day_of(at("2026-03-30T17:00:00Z")), "2026-03-31");
        assert_eq!(w.day_of(at("2026-03-30T15:59:59Z")), "2026-03-30");
    }

    #[test]
    fn dst_zone_keeps_local_midnights() {
        // America/New_York leaves DST on 2026-11-01.
        let w = resolve(&query("7d", "America/New_York"), at("2026-11-03T12:00:00Z")).unwrap();
        assert_eq!(w.start, at("2026-10-28T04:00:00Z"));
        assert_eq!(w.end, at("2026-11-04T05:00:00Z"));
        assert_eq!(w.days().len(), 7);
    }

    #[test]
    fn custom_ranges() {
        let now = at("2026-09-24T10:00:00Z");
        let mut q = query("custom", "UTC");
        assert!(resolve(&q, now).is_err(), "from/to required");
        q.from = Some(at("2026-09-01T00:00:00Z"));
        q.to = Some(at("2026-09-02T23:59:59Z"));
        let w = resolve(&q, now).unwrap();
        assert_eq!(w.end, at("2026-09-03T00:00:00Z"));
        assert_eq!(w.to_label(), "2026-09-02T23:59:59Z");
        assert_eq!(
            w.days(),
            vec!["2026-09-01".to_owned(), "2026-09-02".to_owned()]
        );
        q.to = Some(at("2026-08-31T00:00:00Z"));
        assert!(resolve(&q, now).is_err(), "to before from");
        q.to = Some(at("2026-12-01T00:00:00Z"));
        assert!(resolve(&q, now).is_err(), "longer than 90 days");
        assert!(resolve(&query("1y", "UTC"), now).is_err());
    }

    #[test]
    fn formatting() {
        assert_eq!(money(Decimal::new(1234, 1)), "123.40");
        assert_eq!(money(Decimal::new(-1, 3)), "0.00");
        assert_eq!(money(Decimal::new(12345, 3)), "12.35");
        assert_eq!(percent(rate(Decimal::from(1), Decimal::from(3))), "33.33");
        assert_eq!(percent(rate(Decimal::from(2), Decimal::from(3))), "66.67");
        assert_eq!(percent(rate(Decimal::ONE, Decimal::ZERO)), "0.00");
    }
}
