//! `affiliate_config`, `upstream_sync_config` and `callback_routes_config`
//! (port of `schema/integration/*`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::value::{
    number, parse_bool, parse_float, parse_int, round2, string_list, truncate_chars,
};
use crate::Error;

// ---------------------------------------------------------------------------
// affiliate_config
// ---------------------------------------------------------------------------

const AFFILIATE_RATE_MAX: f64 = 100.0;
const AFFILIATE_CONFIRM_DAYS_MAX: i64 = 3650;
const AFFILIATE_CHANNELS_MAX: usize = 20;
const AFFILIATE_CHANNEL_MAX_CHARS: usize = 50;

/// Affiliate (推广返利) settings. Also the PUT request body.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AffiliateSetting {
    pub enabled: bool,
    pub commission_rate: f64,
    pub confirm_days: i64,
    pub min_withdraw_amount: f64,
    pub withdraw_channels: Vec<String>,
}

impl AffiliateSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.commission_rate = round2(self.commission_rate).clamp(0.0, AFFILIATE_RATE_MAX);
        self.confirm_days = self.confirm_days.clamp(0, AFFILIATE_CONFIRM_DAYS_MAX);
        self.min_withdraw_amount = round2(self.min_withdraw_amount).max(0.0);
        let mut channels: Vec<String> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for raw in &self.withdraw_channels {
            let value = truncate_chars(raw.trim(), AFFILIATE_CHANNEL_MAX_CHARS);
            if value.is_empty() {
                continue;
            }
            let key = value.to_lowercase();
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            channels.push(value);
            if channels.len() >= AFFILIATE_CHANNELS_MAX {
                break;
            }
        }
        self.withdraw_channels = channels;
        self
    }

    /// Validation after normalization (kept for contract parity; normalization clamps).
    pub fn validate(&self) -> crate::Result<()> {
        let n = self.clone().normalized();
        if !(0.0..=AFFILIATE_RATE_MAX).contains(&n.commission_rate)
            || !(0..=AFFILIATE_CONFIRM_DAYS_MAX).contains(&n.confirm_days)
            || n.min_withdraw_amount < 0.0
        {
            return Err(Error::invalid());
        }
        Ok(())
    }

    pub fn decode(raw: Option<&Value>) -> Self {
        let mut r = Self::default();
        if let Some(o) = raw.and_then(Value::as_object) {
            if let Some(v) = o.get("enabled") {
                r.enabled = parse_bool(Some(v));
            }
            if let Some(v) = parse_float(o.get("commission_rate")) {
                r.commission_rate = v;
            }
            if let Some(v) = parse_int(o.get("confirm_days")) {
                r.confirm_days = v;
            }
            if let Some(v) = parse_float(o.get("min_withdraw_amount")) {
                r.min_withdraw_amount = v;
            }
            if o.contains_key("withdraw_channels") {
                r.withdraw_channels = string_list(o.get("withdraw_channels")).unwrap_or_default();
            }
        }
        r.normalized()
    }

    pub fn encode(&self) -> Value {
        let n = self.clone().normalized();
        json!({
            "enabled": n.enabled,
            "commission_rate": number(n.commission_rate),
            "confirm_days": n.confirm_days,
            "min_withdraw_amount": number(n.min_withdraw_amount),
            "withdraw_channels": n.withdraw_channels,
        })
    }
}

// ---------------------------------------------------------------------------
// upstream_sync_config
// ---------------------------------------------------------------------------

/// Upstream catalogue synchronisation settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct UpstreamSyncSetting {
    pub interval_minutes: i64,
    pub pre_order_stock_check_enabled: bool,
    pub sync_page_size: i64,
    pub sync_max_pages: i64,
    pub sync_conn_concurrency: i64,
}

impl Default for UpstreamSyncSetting {
    fn default() -> Self {
        Self {
            interval_minutes: 5,
            pre_order_stock_check_enabled: true,
            sync_page_size: 50,
            sync_max_pages: 200,
            sync_conn_concurrency: 3,
        }
    }
}

fn clamp_or(value: i64, min: i64, max: i64, default: i64) -> i64 {
    if value < min { default } else { value.min(max) }
}

impl UpstreamSyncSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.interval_minutes = clamp_or(self.interval_minutes, 5, 1440, 5);
        self.sync_page_size = clamp_or(self.sync_page_size, 10, 200, 50);
        self.sync_max_pages = clamp_or(self.sync_max_pages, 10, 500, 200);
        self.sync_conn_concurrency = clamp_or(self.sync_conn_concurrency, 1, 10, 3);
        self
    }

    /// Fallback built from `queue.upstream_sync_interval` (e.g. `"10m"`).
    pub fn fallback(interval: &str) -> Self {
        let mut s = Self::default();
        if let Some(minutes) = parse_duration_minutes(interval).filter(|m| *m > 0) {
            s.interval_minutes = minutes;
        }
        s.normalized()
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut r = fallback.normalized();
        if let Some(o) = raw.and_then(Value::as_object) {
            if let Some(v) = parse_int(o.get("interval_minutes")) {
                r.interval_minutes = v;
            }
            if let Some(v) = o.get("pre_order_stock_check_enabled") {
                r.pre_order_stock_check_enabled = parse_bool(Some(v));
            }
            if let Some(v) = parse_int(o.get("sync_page_size")) {
                r.sync_page_size = v;
            }
            if let Some(v) = parse_int(o.get("sync_max_pages")) {
                r.sync_max_pages = v;
            }
            if let Some(v) = parse_int(o.get("sync_conn_concurrency")) {
                r.sync_conn_concurrency = v;
            }
        }
        r.normalized()
    }

    pub fn encode(&self) -> Value {
        json!(self.normalized())
    }
}

/// Parses Go-style durations (`90s`, `5m`, `1h30m`) into whole minutes.
fn parse_duration_minutes(raw: &str) -> Option<i64> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut seconds = 0.0_f64;
    let mut num = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            continue;
        }
        let mut unit = c.to_string();
        if c == 'm' && chars.peek() == Some(&'s') {
            unit.push('s');
            chars.next();
        }
        let n: f64 = num.parse().ok()?;
        num.clear();
        seconds += match unit.as_str() {
            "h" => n * 3600.0,
            "m" => n * 60.0,
            "s" => n,
            "ms" => n / 1000.0,
            _ => return None,
        };
    }
    if !num.is_empty() {
        return None;
    }
    Some((seconds / 60.0) as i64)
}

// ---------------------------------------------------------------------------
// callback_routes_config
// ---------------------------------------------------------------------------

/// Existing route prefixes a custom callback path may not collide with.
const RESERVED_PREFIXES: [&str; 7] = [
    "/api/v1/public/",
    "/api/v1/admin/",
    "/api/v1/auth/",
    "/api/v1/guest/",
    "/api/v1/channel/",
    "/api/v1/upstream/api/",
    "/api/v1/user/",
];

/// Custom payment/upstream callback paths (empty = default route).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CallbackRoutes {
    pub payment_callback: String,
    pub dujiaopay_webhook: String,
    pub paypal_webhook: String,
    pub stripe_webhook: String,
    pub upstream_callback: String,
}

/// Normalizes one custom callback path; unsafe paths become `""`.
pub fn normalize_callback_path(raw: &str) -> String {
    let mut path = raw.trim();
    if path.is_empty() {
        return String::new();
    }
    if let Some(idx) = path.find(['?', '#']) {
        path = &path[..idx];
    }
    let path = path.trim_end_matches('/');
    if !path.starts_with("/api/") {
        return String::new();
    }
    let with_slash = format!("{path}/");
    if RESERVED_PREFIXES
        .iter()
        .any(|p| with_slash.starts_with(p) || p.starts_with(&with_slash))
    {
        return String::new();
    }
    path.to_owned()
}

/// Error key of a refused custom callback path (live QA I-9).
pub const CALLBACK_ROUTE_INVALID: &str = "error.callback_route_invalid";

/// Built-in callback paths a custom path may not reuse: every one of them is a
/// registered route, so the custom path would be unreachable while its own
/// default is hidden.
const DEFAULT_CALLBACK_PATHS: [&str; 5] = [
    crate::payment::routes::DEFAULT_PAYMENT_CALLBACK,
    crate::payment::routes::DEFAULT_DUJIAOPAY_WEBHOOK,
    crate::payment::routes::DEFAULT_PAYPAL_WEBHOOK,
    crate::payment::routes::DEFAULT_STRIPE_WEBHOOK,
    crate::integration::downstream::CALLBACK_SIGN_PATH,
];

/// Field names of `callback_routes_config`, in de-duplication order.
const CALLBACK_FIELDS: [&str; 5] = [
    "payment_callback",
    "dujiaopay_webhook",
    "paypal_webhook",
    "stripe_webhook",
    "upstream_callback",
];

/// Refuses (instead of silently dropping) a write whose paths are unsafe, not
/// canonical (`.`/`..`/empty segments, odd characters), reuse a built-in
/// callback route or repeat each other (live QA I-9; the original only dropped
/// reserved prefixes and duplicates while reporting success).
pub fn validate_callback_routes(raw: &Value) -> crate::Result<()> {
    let invalid = || Err(crate::Error::bad_request(CALLBACK_ROUTE_INVALID));
    let Some(o) = raw.as_object() else {
        return invalid();
    };
    let mut seen: Vec<String> = Vec::new();
    for field in CALLBACK_FIELDS {
        let text = match o.get(field) {
            None | Some(Value::Null) => continue,
            Some(Value::String(s)) => s.trim(),
            Some(_) => return invalid(),
        };
        if text.is_empty() {
            continue;
        }
        let bare = text.split(['?', '#']).next().unwrap_or_default();
        let bare = bare.trim_end_matches('/');
        let canonical = bare.strip_prefix('/').is_some_and(|rest| {
            rest.split('/').all(|seg| {
                !seg.is_empty()
                    && seg != "."
                    && seg != ".."
                    && seg
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~'))
            })
        });
        let path = normalize_callback_path(text);
        if !canonical
            || path.is_empty()
            || DEFAULT_CALLBACK_PATHS.contains(&path.as_str())
            || seen.contains(&path)
        {
            return invalid();
        }
        seen.push(path);
    }
    Ok(())
}

impl CallbackRoutes {
    /// Decodes and normalizes each path (no de-duplication).
    pub fn decode(raw: Option<&Value>) -> Self {
        let o = raw.and_then(Value::as_object);
        let get = |k: &str| {
            normalize_callback_path(
                o.and_then(|m| m.get(k))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            )
        };
        Self {
            payment_callback: get("payment_callback"),
            dujiaopay_webhook: get("dujiaopay_webhook"),
            paypal_webhook: get("paypal_webhook"),
            stripe_webhook: get("stripe_webhook"),
            upstream_callback: get("upstream_callback"),
        }
    }

    /// Later duplicates of an earlier path are cleared.
    #[must_use]
    pub fn deduplicated(mut self) -> Self {
        let mut seen: Vec<String> = Vec::new();
        for field in [
            &mut self.payment_callback,
            &mut self.dujiaopay_webhook,
            &mut self.paypal_webhook,
            &mut self.stripe_webhook,
            &mut self.upstream_callback,
        ] {
            if field.is_empty() {
                continue;
            }
            if seen.contains(field) {
                field.clear();
            } else {
                seen.push(field.clone());
            }
        }
        self
    }

    pub fn has_custom_routes(&self) -> bool {
        [
            &self.payment_callback,
            &self.dujiaopay_webhook,
            &self.paypal_webhook,
            &self.stripe_webhook,
            &self.upstream_callback,
        ]
        .iter()
        .any(|s| !s.is_empty())
    }

    pub fn encode(&self) -> Value {
        json!(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // QA-A09 (live QA I-9): conflicting, reserved, traversal and duplicate paths
    // are refused on write instead of being dropped silently.
    #[test]
    fn qa_a09_callback_routes_validation() {
        assert!(
            validate_callback_routes(&json!({"payment_callback": "/api/pay/notify/?x=1"})).is_ok()
        );
        assert!(
            validate_callback_routes(&json!({"payment_callback": "", "stripe_webhook": null}))
                .is_ok()
        );
        for bad in [
            json!({"payment_callback": "/api/v1/payments/webhook/paypal"}),
            json!({"stripe_webhook": "/api/v1/payments/callback"}),
            json!({"paypal_webhook": "/api/v1/upstream/callback"}),
            json!({"payment_callback": "/api/v1/../admin/x"}),
            json!({"payment_callback": "/api/./x"}),
            json!({"payment_callback": "/api//x"}),
            json!({"payment_callback": "/api/v1/admin/x"}),
            json!({"payment_callback": "/hooks/x"}),
            json!({"payment_callback": "/api/a b"}),
            json!({"payment_callback": "/api/cb", "upstream_callback": "/api/cb/"}),
            json!({"payment_callback": 1}),
        ] {
            assert!(validate_callback_routes(&bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn affiliate_normalizes() {
        let s = AffiliateSetting::decode(Some(&json!({
            "enabled": "true",
            "commission_rate": "150.456",
            "confirm_days": -1,
            "min_withdraw_amount": 10.005,
            "withdraw_channels": [" Alipay ", "alipay", "", "USDT"],
        })));
        assert!(s.enabled);
        assert_eq!(s.commission_rate, 100.0);
        assert_eq!(s.confirm_days, 0);
        assert_eq!(s.withdraw_channels, vec!["Alipay", "USDT"]);
        assert_eq!(s.encode()["commission_rate"], json!(100));
    }

    #[test]
    fn upstream_sync_bounds_and_fallback() {
        let s = UpstreamSyncSetting::decode(
            Some(
                &json!({"interval_minutes": 1, "sync_page_size": 999, "sync_conn_concurrency": "0"}),
            ),
            UpstreamSyncSetting::default(),
        );
        assert_eq!(s.interval_minutes, 5);
        assert_eq!(s.sync_page_size, 200);
        assert_eq!(s.sync_conn_concurrency, 3);
        assert_eq!(UpstreamSyncSetting::fallback("30m").interval_minutes, 30);
        assert_eq!(UpstreamSyncSetting::fallback("1h30m").interval_minutes, 90);
        assert_eq!(UpstreamSyncSetting::fallback("bad").interval_minutes, 5);
    }

    // CNT-06: custom callback routes cannot shadow existing APIs and are de-duplicated.
    #[test]
    fn callback_routes_are_safe_and_unique() {
        assert_eq!(normalize_callback_path("/api/pay/cb/?x=1"), "/api/pay/cb");
        assert_eq!(normalize_callback_path("/pay/cb"), "");
        assert_eq!(normalize_callback_path("/api/v1/admin/x"), "");
        assert_eq!(normalize_callback_path("/api/v1/admin"), "");
        assert_eq!(normalize_callback_path("/api/v1"), "");
        assert_eq!(normalize_callback_path("/api/v1/upstream/api"), "");
        assert_eq!(
            normalize_callback_path("/api/v1/upstream/cb"),
            "/api/v1/upstream/cb"
        );
        let r = CallbackRoutes::decode(Some(&json!({
            "payment_callback": "/api/a",
            "paypal_webhook": "/api/a/",
            "stripe_webhook": "/api/b",
        })))
        .deduplicated();
        assert_eq!(r.payment_callback, "/api/a");
        assert_eq!(r.paypal_webhook, "");
        assert_eq!(r.stripe_webhook, "/api/b");
        assert!(r.has_custom_routes());
        assert!(!CallbackRoutes::default().has_custom_routes());
    }
}
