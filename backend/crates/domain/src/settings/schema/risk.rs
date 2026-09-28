//! `order_risk_control_config` (port of `schema/security/order_risk_control.go`).

use std::net::IpAddr;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

const VERSION: i64 = 2;

/// Order rate limit policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRateLimit {
    pub enabled: bool,
    pub window_seconds: i64,
    pub max_requests: i64,
    pub block_seconds: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskCommonPolicy {
    pub ip_blacklist: Vec<String>,
}

/// Guest (IP-keyed) policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskGuestPolicy {
    pub enabled: bool,
    pub max_pending_orders_per_ip: i64,
    pub max_quantity_per_product_per_order: i64,
    pub max_pending_quantity_per_ip_product: i64,
    pub payment_expire_minutes: i64,
    pub rate_limit: OrderRateLimit,
}

/// Member (user-keyed) policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RiskMemberPolicy {
    pub enabled: bool,
    pub max_pending_orders_per_user: i64,
    pub max_pending_orders_per_ip: i64,
    pub max_quantity_per_product_per_order: i64,
    pub rate_limit: OrderRateLimit,
}

/// Order risk control settings; `0` always disables the matching limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrderRiskSetting {
    pub version: i64,
    pub enabled: bool,
    pub common: RiskCommonPolicy,
    pub guest: RiskGuestPolicy,
    pub member: RiskMemberPolicy,
}

impl Default for OrderRiskSetting {
    /// Recommended values for new installs; the master switch stays off.
    fn default() -> Self {
        Self {
            version: VERSION,
            enabled: false,
            common: RiskCommonPolicy::default(),
            guest: RiskGuestPolicy {
                enabled: true,
                max_pending_orders_per_ip: 2,
                max_quantity_per_product_per_order: 1,
                max_pending_quantity_per_ip_product: 2,
                payment_expire_minutes: 10,
                rate_limit: OrderRateLimit {
                    enabled: true,
                    window_seconds: 60,
                    max_requests: 3,
                    block_seconds: 120,
                },
            },
            member: RiskMemberPolicy {
                enabled: true,
                max_pending_orders_per_user: 5,
                max_pending_orders_per_ip: 0,
                max_quantity_per_product_per_order: 0,
                rate_limit: OrderRateLimit {
                    enabled: false,
                    window_seconds: 60,
                    max_requests: 10,
                    block_seconds: 120,
                },
            },
        }
    }
}

fn limit(value: i64, max: i64, fallback: i64) -> i64 {
    if (0..=max).contains(&value) {
        value
    } else {
        fallback
    }
}

fn rate(mut r: OrderRateLimit, fallback: OrderRateLimit) -> OrderRateLimit {
    if !(10..=3600).contains(&r.window_seconds) {
        r.window_seconds = fallback.window_seconds;
    }
    if !(1..=100).contains(&r.max_requests) {
        r.max_requests = fallback.max_requests;
    }
    if !(0..=86400).contains(&r.block_seconds) {
        r.block_seconds = fallback.block_seconds;
    }
    r
}

/// True for an IP address or a CIDR block.
pub fn is_ip_or_cidr(value: &str) -> bool {
    if value.parse::<IpAddr>().is_ok() {
        return true;
    }
    let Some((ip, prefix)) = value.split_once('/') else {
        return false;
    };
    let Ok(ip) = ip.parse::<IpAddr>() else {
        return false;
    };
    let max = if ip.is_ipv4() { 32 } else { 128 };
    !prefix.is_empty()
        && prefix.chars().all(|c| c.is_ascii_digit())
        && prefix.parse::<u32>().is_ok_and(|p| p <= max)
}

/// Deep-merges `overlay` into `base` (null values are ignored, like Go's unmarshal).
fn merge(base: &mut Value, overlay: &Value) {
    match (base, overlay) {
        (Value::Object(b), Value::Object(o)) => {
            for (k, v) in o {
                if v.is_null() {
                    continue;
                }
                match b.get_mut(k) {
                    Some(existing) => merge(existing, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, o) => *b = o.clone(),
    }
}

impl OrderRiskSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let d = Self::default();
        self.version = VERSION;
        let g = &mut self.guest;
        g.max_pending_orders_per_ip = limit(
            g.max_pending_orders_per_ip,
            100,
            d.guest.max_pending_orders_per_ip,
        );
        g.max_quantity_per_product_per_order = limit(
            g.max_quantity_per_product_per_order,
            100_000,
            d.guest.max_quantity_per_product_per_order,
        );
        g.max_pending_quantity_per_ip_product = limit(
            g.max_pending_quantity_per_ip_product,
            100_000,
            d.guest.max_pending_quantity_per_ip_product,
        );
        g.payment_expire_minutes = limit(
            g.payment_expire_minutes,
            10080,
            d.guest.payment_expire_minutes,
        );
        g.rate_limit = rate(g.rate_limit, d.guest.rate_limit);
        let m = &mut self.member;
        m.max_pending_orders_per_user = limit(
            m.max_pending_orders_per_user,
            100,
            d.member.max_pending_orders_per_user,
        );
        m.max_pending_orders_per_ip = limit(
            m.max_pending_orders_per_ip,
            100,
            d.member.max_pending_orders_per_ip,
        );
        m.max_quantity_per_product_per_order = limit(
            m.max_quantity_per_product_per_order,
            100_000,
            d.member.max_quantity_per_product_per_order,
        );
        m.rate_limit = rate(m.rate_limit, d.member.rate_limit);
        let mut ips: Vec<String> = Vec::new();
        for entry in self.common.ip_blacklist.iter().map(|s| s.trim()) {
            if !entry.is_empty() && is_ip_or_cidr(entry) && !ips.iter().any(|e| e == entry) {
                ips.push(entry.to_owned());
            }
        }
        self.common.ip_blacklist = ips;
        self
    }

    /// Decodes the nested v2 format, migrating the legacy flat format.
    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let Some(o) = raw.and_then(Value::as_object) else {
            return fallback.normalized();
        };
        if o.contains_key("guest") {
            let mut base = serde_json::to_value(&fallback).unwrap_or(Value::Object(Map::new()));
            merge(&mut base, &Value::Object(o.clone()));
            return serde_json::from_value::<Self>(base)
                .map(Self::normalized)
                .unwrap_or_else(|_| fallback.normalized());
        }
        Self::decode_legacy(o, fallback)
    }

    fn decode_legacy(o: &Map<String, Value>, fallback: Self) -> Self {
        let mut r = fallback;
        // Legacy defaults first, so sparse legacy JSON never picks up v2 recommendations.
        r.guest.max_pending_orders_per_ip = 5;
        r.member.max_pending_orders_per_user = 3;
        r.member.max_pending_orders_per_ip = 5;
        r.guest.max_quantity_per_product_per_order = 0;
        r.guest.max_pending_quantity_per_ip_product = 0;
        r.guest.payment_expire_minutes = 0;
        r.member.max_quantity_per_product_per_order = 0;
        let mut legacy = OrderRateLimit {
            enabled: false,
            window_seconds: 60,
            max_requests: 5,
            block_seconds: 120,
        };
        r.guest.rate_limit = legacy;
        r.member.rate_limit = legacy;
        let int = |v: Option<&Value>| v.and_then(Value::as_i64);
        if let Some(b) = o.get("enabled").and_then(Value::as_bool) {
            r.enabled = b;
        }
        if let Some(v) = int(o.get("max_pending_orders_per_user")) {
            r.member.max_pending_orders_per_user = v;
        }
        if let Some(v) = int(o.get("max_pending_orders_per_ip")) {
            r.guest.max_pending_orders_per_ip = v;
            r.member.max_pending_orders_per_ip = v;
        }
        if let Some(rl) = o.get("order_rate_limit").and_then(Value::as_object) {
            if let Some(b) = rl.get("enabled").and_then(Value::as_bool) {
                legacy.enabled = b;
            }
            if let Some(v) = int(rl.get("window_seconds")) {
                legacy.window_seconds = v;
            }
            if let Some(v) = int(rl.get("max_requests")) {
                legacy.max_requests = v;
            }
            if let Some(v) = int(rl.get("block_seconds")) {
                legacy.block_seconds = v;
            }
            r.guest.rate_limit = legacy;
            r.member.rate_limit = legacy;
        }
        if let Some(list) = o.get("ip_blacklist").and_then(Value::as_array) {
            r.common.ip_blacklist = list
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
        }
        r.normalized()
    }

    pub fn encode(&self) -> Value {
        serde_json::to_value(self.clone().normalized()).unwrap_or(Value::Object(Map::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn nested_decode_overlays_and_clamps() {
        let s = OrderRiskSetting::decode(
            Some(&json!({
                "enabled": true,
                "guest": {"max_pending_orders_per_ip": 500, "rate_limit": {"window_seconds": 5}},
                "common": {"ip_blacklist": ["1.2.3.4", "10.0.0.0/8", "bad", "1.2.3.4", "::1/200"]},
            })),
            OrderRiskSetting::default(),
        );
        assert!(s.enabled);
        assert_eq!(s.guest.max_pending_orders_per_ip, 2);
        assert_eq!(s.guest.rate_limit.window_seconds, 60);
        assert_eq!(s.guest.max_quantity_per_product_per_order, 1);
        assert_eq!(s.common.ip_blacklist, vec!["1.2.3.4", "10.0.0.0/8"]);
    }

    #[test]
    fn legacy_flat_format_migrates() {
        let s = OrderRiskSetting::decode(
            Some(
                &json!({"enabled": true, "max_pending_orders_per_ip": 7, "order_rate_limit": {"enabled": true}}),
            ),
            OrderRiskSetting::default(),
        );
        assert_eq!(s.guest.max_pending_orders_per_ip, 7);
        assert_eq!(s.member.max_pending_orders_per_ip, 7);
        assert_eq!(s.member.max_pending_orders_per_user, 3);
        assert!(s.guest.rate_limit.enabled);
        assert_eq!(s.guest.rate_limit.max_requests, 5);
        assert_eq!(s.guest.payment_expire_minutes, 0);
        assert_eq!(s.version, 2);
    }

    #[test]
    fn cidr_validation() {
        assert!(is_ip_or_cidr("2001:db8::/32"));
        assert!(!is_ip_or_cidr("1.2.3.4/33"));
        assert!(!is_ip_or_cidr("1.2.3.4/"));
    }
}
