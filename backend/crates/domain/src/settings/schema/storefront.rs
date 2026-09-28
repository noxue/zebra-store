//! `dashboard_config` and `home_announcement` (port of `schema/storefront/*`).

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use zs_shared::i18n::LOCALES;

use super::value::{as_obj, localized_field, parse_bool, parse_int, read_bool, text};

// ---------------------------------------------------------------------------
// dashboard_config
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DashboardAlert {
    pub low_stock_threshold: i64,
    pub out_of_stock_products_threshold: i64,
    pub pending_payment_orders_threshold: i64,
    pub payments_failed_threshold: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DashboardRanking {
    pub top_products_limit: i64,
    pub top_channels_limit: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct DashboardAccounting {
    pub refund_reverses_cost: bool,
}

/// Dashboard thresholds and ranking sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct DashboardSetting {
    pub alert: DashboardAlert,
    pub ranking: DashboardRanking,
    pub accounting: DashboardAccounting,
}

impl Default for DashboardSetting {
    fn default() -> Self {
        Self {
            alert: DashboardAlert {
                low_stock_threshold: 5,
                out_of_stock_products_threshold: 1,
                pending_payment_orders_threshold: 20,
                payments_failed_threshold: 10,
            },
            ranking: DashboardRanking {
                top_products_limit: 5,
                top_channels_limit: 5,
            },
            accounting: DashboardAccounting::default(),
        }
    }
}

fn in_range_or(value: i64, min: i64, max: i64, default: i64) -> i64 {
    if (min..=max).contains(&value) {
        value
    } else {
        default
    }
}

impl DashboardSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let a = &mut self.alert;
        a.low_stock_threshold = in_range_or(a.low_stock_threshold, 1, 500, 5);
        a.out_of_stock_products_threshold =
            in_range_or(a.out_of_stock_products_threshold, 1, 10000, 1);
        a.pending_payment_orders_threshold =
            in_range_or(a.pending_payment_orders_threshold, 1, 100_000, 20);
        a.payments_failed_threshold = in_range_or(a.payments_failed_threshold, 1, 100_000, 10);
        let r = &mut self.ranking;
        r.top_products_limit = in_range_or(r.top_products_limit, 1, 20, 5);
        r.top_channels_limit = in_range_or(r.top_channels_limit, 1, 20, 5);
        self
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut r = fallback;
        let o = raw.and_then(Value::as_object);
        if let Some(a) = as_obj(o.and_then(|m| m.get("alert"))) {
            for (k, f) in [
                ("low_stock_threshold", &mut r.alert.low_stock_threshold),
                (
                    "out_of_stock_products_threshold",
                    &mut r.alert.out_of_stock_products_threshold,
                ),
                (
                    "pending_payment_orders_threshold",
                    &mut r.alert.pending_payment_orders_threshold,
                ),
                (
                    "payments_failed_threshold",
                    &mut r.alert.payments_failed_threshold,
                ),
            ] {
                if let Some(v) = parse_int(a.get(k)) {
                    *f = v;
                }
            }
        }
        if let Some(rk) = as_obj(o.and_then(|m| m.get("ranking"))) {
            if let Some(v) = parse_int(rk.get("top_products_limit")) {
                r.ranking.top_products_limit = v;
            }
            if let Some(v) = parse_int(rk.get("top_channels_limit")) {
                r.ranking.top_channels_limit = v;
            }
        }
        if let Some(ac) = as_obj(o.and_then(|m| m.get("accounting"))) {
            r.accounting.refund_reverses_cost = read_bool(
                ac,
                "refund_reverses_cost",
                r.accounting.refund_reverses_cost,
            );
        }
        r.normalized()
    }

    pub fn encode(&self) -> Value {
        json!(self.normalized())
    }
}

// ---------------------------------------------------------------------------
// home_announcement
// ---------------------------------------------------------------------------

const ANNOUNCEMENT_TYPES: [&str; 3] = ["normal", "info", "warning"];

fn rfc3339_or_empty(raw: Option<&Value>) -> String {
    let t = text(raw);
    if t.is_empty() || DateTime::parse_from_rfc3339(&t).is_err() {
        String::new()
    } else {
        t
    }
}

/// Refuses an announcement whose end is before its start (live QA I-20: it was
/// accepted and the announcement silently never showed).
pub fn validate_announcement(value: &Value) -> crate::Result<()> {
    let n = normalize_announcement(value);
    let at = |k: &str| {
        n.get(k)
            .and_then(Value::as_str)
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
    };
    match (at("start_at"), at("end_at")) {
        (Some(start), Some(end)) if end < start => Err(crate::Error::invalid()),
        _ => Ok(()),
    }
}

/// Normalizes the home announcement so invalid values never reach the database.
pub fn normalize_announcement(value: &Value) -> Value {
    let o = value.as_object();
    let get = |k: &str| o.and_then(|m| m.get(k));
    let kind = text(get("type"));
    let kind = if ANNOUNCEMENT_TYPES.contains(&kind.as_str()) {
        kind
    } else {
        "normal".to_owned()
    };
    json!({
        "enabled": parse_bool(get("enabled")),
        "type": kind,
        "title": localized_field(get("title")),
        "content": localized_field(get("content")),
        "start_at": rfc3339_or_empty(get("start_at")),
        "end_at": rfc3339_or_empty(get("end_at")),
    })
}

/// 32-bit FNV-1a, used for the announcement content fingerprint.
fn fnv1a32(chunks: &[&[u8]]) -> u32 {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET;
    for chunk in chunks {
        for b in *chunk {
            hash ^= u32::from(*b);
            hash = hash.wrapping_mul(PRIME);
        }
    }
    hash
}

/// 8-hex-digit fingerprint of type + localized title/content (schedule changes do not affect it).
pub fn announcement_version(kind: &str, title: &Value, content: &Value) -> String {
    let mut parts: Vec<Vec<u8>> = vec![kind.as_bytes().to_vec()];
    for lang in LOCALES {
        parts.push(vec![0]);
        parts.push(
            title
                .get(lang)
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes()
                .to_vec(),
        );
        parts.push(vec![0]);
        parts.push(
            content
                .get(lang)
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes()
                .to_vec(),
        );
    }
    let refs: Vec<&[u8]> = parts.iter().map(Vec::as_slice).collect();
    format!("{:08x}", fnv1a32(&refs))
}

/// The announcement to display at `now`, if any: enabled, in schedule, with content.
pub fn active_announcement(value: &Value, now: DateTime<Utc>) -> Option<Value> {
    let a = normalize_announcement(value);
    if !a["enabled"].as_bool().unwrap_or(false) {
        return None;
    }
    let parse = |k: &str| {
        a[k].as_str()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
    };
    if parse("start_at").is_some_and(|start| now < start) {
        return None;
    }
    if parse("end_at").is_some_and(|end| now > end) {
        return None;
    }
    let content = &a["content"];
    let has_content = LOCALES.iter().any(|l| {
        content
            .get(*l)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    });
    if !has_content {
        return None;
    }
    let kind = a["type"].as_str().unwrap_or("normal");
    Some(json!({
        "type": kind,
        "title": a["title"],
        "content": content,
        "version": announcement_version(kind, &a["title"], content),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn dashboard_decodes_and_resets() {
        let d = DashboardSetting::decode(
            Some(
                &json!({"alert": {"low_stock_threshold": "900"}, "ranking": {"top_products_limit": 3}, "accounting": {"refund_reverses_cost": "true"}}),
            ),
            DashboardSetting::default(),
        );
        assert_eq!(d.alert.low_stock_threshold, 5);
        assert_eq!(d.ranking.top_products_limit, 3);
        assert!(d.accounting.refund_reverses_cost);
    }

    #[test]
    fn announcement_normalize_and_activity() {
        let raw = json!({
            "enabled": true, "type": "danger",
            "content": {"zh-CN": " 维护 "}, "start_at": "not-a-time",
            "end_at": "2026-01-02T00:00:00Z",
        });
        let n = normalize_announcement(&raw);
        assert_eq!(n["type"], "normal");
        assert_eq!(n["start_at"], "");
        assert_eq!(n["content"]["zh-CN"], "维护");
        let before = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let after = Utc.with_ymd_and_hms(2026, 1, 3, 0, 0, 0).unwrap();
        let active = active_announcement(&raw, before).unwrap();
        assert_eq!(active["version"].as_str().unwrap().len(), 8);
        assert!(active.get("enabled").is_none());
        assert!(active_announcement(&raw, after).is_none());
        let empty = json!({"enabled": true, "content": {"zh-CN": "  "}});
        assert!(active_announcement(&empty, before).is_none());
    }

    // SET-02: an expired announcement is not served; schedule edits keep the version.
    #[test]
    fn set_02_expired_hidden_and_schedule_does_not_change_version() {
        let now = Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap();
        let base = json!({"enabled": true, "type": "info", "title": {"zh-CN": "t"},
            "content": {"zh-CN": "<img src=x onerror=alert(1)>"}});
        let mut expired = base.clone();
        expired["end_at"] = json!("2026-05-01T00:00:00Z");
        assert!(active_announcement(&expired, now).is_none());
        let mut a = base.clone();
        a["start_at"] = json!("2026-01-01T00:00:00Z");
        let mut b = base;
        b["start_at"] = json!("2026-02-01T00:00:00+08:00");
        let va = active_announcement(&a, now).unwrap()["version"].clone();
        let vb = active_announcement(&b, now).unwrap()["version"].clone();
        assert_eq!(va, vb);
    }

    #[test]
    fn version_tracks_content_only() {
        let t = json!({"zh-CN": "a"});
        let v1 = announcement_version("info", &t, &json!({"zh-CN": "x"}));
        let v2 = announcement_version("info", &t, &json!({"zh-CN": "y"}));
        assert_ne!(v1, v2);
        // FNV-1a of the empty string is the offset basis.
        assert_eq!(format!("{:08x}", fnv1a32(&[])), "811c9dc5");
    }
}
