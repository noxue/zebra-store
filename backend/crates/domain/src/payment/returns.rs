//! Synchronous return URLs (PAY-06, PAY-16, PAY-20) and gateway order numbers (PAY-09).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::form::{encode_pairs, parse_query};
use super::types::provider;

/// Prefix of every gateway order number (`serial.Generate("DJP")`).
pub const GATEWAY_ORDER_NO_PREFIX: &str = "DJP";

/// Default storefront path a tenant return URL falls back to.
pub const DEFAULT_RETURN_PATH: &str = "/pay";

/// Generates an unguessable per-attempt gateway order number (PAY-09): never derived from ids.
pub fn new_gateway_order_no(now: DateTime<Utc>) -> String {
    zs_shared::serial::generate(GATEWAY_ORDER_NO_PREFIX, now)
}

/// Reuses the payment's gateway order number or generates a new one (`resolveGatewayOrderNo`).
pub fn resolve_gateway_order_no(existing: &str, now: DateTime<Utc>) -> String {
    let existing = existing.trim();
    if existing.is_empty() {
        new_gateway_order_no(now)
    } else {
        existing.to_owned()
    }
}

/// `common.AppendQueryParams`: merges non-empty params into the URL query (Go re-encodes sorted).
pub fn append_query_params(raw_url: &str, params: &BTreeMap<String, String>) -> String {
    let raw_url = raw_url.trim();
    if raw_url.is_empty() {
        return String::new();
    }
    if params.is_empty() {
        return raw_url.to_owned();
    }
    let (without_fragment, fragment) = match raw_url.split_once('#') {
        Some((a, b)) => (a, Some(b)),
        None => (raw_url, None),
    };
    let (base, raw_query) = match without_fragment.split_once('?') {
        Some((a, b)) => (a, b),
        None => (without_fragment, ""),
    };
    let (mut pairs, _) = parse_query(raw_query);
    for (key, value) in params {
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        pairs.retain(|(k, _)| k != key);
        pairs.push((key.to_owned(), value.to_owned()));
    }
    let mut out = base.to_owned();
    let encoded = encode_pairs(&pairs);
    if !encoded.is_empty() {
        out.push('?');
        out.push_str(&encoded);
    }
    if let Some(fragment) = fragment.filter(|f| !f.is_empty()) {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// Business context of a return link (`buildPaymentReturnQuery` inputs).
#[derive(Debug, Clone, Default)]
pub struct ReturnContext {
    /// `order` (default) or `recharge`.
    pub biz_type: String,
    /// Order number or recharge number.
    pub business_no: String,
    /// Guest order (no user); ignored for recharges.
    pub guest: bool,
}

/// Marker appended to return links: `<provider>_return`, or `<channel>_return` for official gateways.
pub fn return_marker(provider_type: &str, channel_type: &str) -> String {
    let provider_type = provider_type.trim().to_ascii_lowercase();
    if provider_type == provider::OFFICIAL {
        format!("{}_return", channel_type.trim().to_ascii_lowercase())
    } else {
        format!("{provider_type}_return")
    }
}

/// Query parameters appended to every gateway return URL (PAY-16/PAY-20).
pub fn build_return_query(
    ctx: &ReturnContext,
    marker: &str,
    session_id: &str,
) -> BTreeMap<String, String> {
    let mut params = BTreeMap::new();
    let biz_type = match ctx.biz_type.trim().to_ascii_lowercase() {
        t if t.is_empty() => "order".to_owned(),
        t => t,
    };
    let business_no = ctx.business_no.trim();
    params.insert("biz_type".to_owned(), biz_type.clone());
    if biz_type == "recharge" {
        if !business_no.is_empty() {
            params.insert("recharge_no".to_owned(), business_no.to_owned());
        }
    } else {
        if !business_no.is_empty() {
            params.insert("order_no".to_owned(), business_no.to_owned());
        }
        if ctx.guest {
            params.insert("guest".to_owned(), "1".to_owned());
        }
    }
    let marker = marker.trim();
    if !marker.is_empty() {
        params.insert(marker.to_owned(), "1".to_owned());
    }
    let session_id = session_id.trim();
    if !session_id.is_empty() {
        params.insert("session_id".to_owned(), session_id.to_owned());
    }
    params
}

/// Path (+query) of the channel's configured return page for tenant rewriting (`tenantReturnPath`).
pub fn tenant_return_path(config: &serde_json::Map<String, serde_json::Value>) -> String {
    for key in ["return_url", "success_url"] {
        let Some(raw) = config.get(key).and_then(|v| v.as_str()).map(str::trim) else {
            continue;
        };
        if raw.is_empty() {
            continue;
        }
        let Some(after_scheme) = raw.split_once("://").map(|(_, rest)| rest) else {
            continue;
        };
        let after_host = after_scheme
            .find(['/', '?', '#'])
            .map(|i| &after_scheme[i..]);
        let Some(rest) = after_host else { continue };
        let rest = rest.split('#').next().unwrap_or_default();
        let (path, query) = match rest.split_once('?') {
            Some((p, q)) => (p, q),
            None => (rest, ""),
        };
        if path.is_empty() || path == "/" {
            continue;
        }
        return if query.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{query}")
        };
    }
    DEFAULT_RETURN_PATH.to_owned()
}

/// A verified reseller tenant (only ever from the tenant resolver, never a raw Host header).
#[derive(Debug, Clone, Default)]
pub struct TenantSite {
    pub is_main: bool,
    pub unavailable: bool,
    pub host: String,
    pub primary_domain: String,
}

/// Rewrites the synchronous return URL onto a reseller domain (PAY-06); empty = use channel config.
pub fn tenant_return_url(
    tenant: Option<&TenantSite>,
    request_scheme: &str,
    config: &serde_json::Map<String, serde_json::Value>,
) -> String {
    let Some(tenant) = tenant else {
        return String::new();
    };
    if tenant.is_main || tenant.unavailable {
        return String::new();
    }
    let host = match tenant.host.trim() {
        "" => tenant.primary_domain.trim(),
        h => h,
    };
    if host.is_empty() {
        return String::new();
    }
    let scheme = match request_scheme.trim().to_ascii_lowercase().as_str() {
        "http" => "http",
        _ => "https",
    };
    format!("{scheme}://{host}{}", tenant_return_path(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn params(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// PAY-16: vectors from Go `common.AppendQueryParams`.
    #[test]
    fn pay_16_append_query_matches_go() {
        assert_eq!(
            append_query_params(
                "https://shop/pay?x=1",
                &params(&[("order_no", "A1"), ("bepusdt_return", "1")])
            ),
            "https://shop/pay?bepusdt_return=1&order_no=A1&x=1"
        );
        assert_eq!(
            append_query_params(
                "https://shop/pay?z=1&a=2#frag",
                &params(&[("k", "a b&c/~*'()!"), ("e", " ")])
            ),
            "https://shop/pay?a=2&k=a+b%26c%2F~%2A%27%28%29%21&z=1#frag"
        );
        assert_eq!(
            append_query_params("/pay", &params(&[("q", "中文")])),
            "/pay?q=%E4%B8%AD%E6%96%87"
        );
        assert_eq!(append_query_params("  ", &params(&[("a", "1")])), "");
    }

    /// PAY-20: order vs recharge return parameters.
    #[test]
    fn pay_20_return_query_distinguishes_business() {
        let guest = build_return_query(
            &ReturnContext {
                biz_type: String::new(),
                business_no: "DJ1".into(),
                guest: true,
            },
            "epay_return",
            "",
        );
        assert_eq!(
            guest,
            params(&[
                ("biz_type", "order"),
                ("order_no", "DJ1"),
                ("guest", "1"),
                ("epay_return", "1")
            ])
        );
        let recharge = build_return_query(
            &ReturnContext {
                biz_type: "recharge".into(),
                business_no: "WR1".into(),
                guest: true,
            },
            "alipay_return",
            "",
        );
        assert_eq!(
            recharge,
            params(&[
                ("biz_type", "recharge"),
                ("recharge_no", "WR1"),
                ("alipay_return", "1")
            ])
        );
        assert_eq!(return_marker("official", "Wechat"), "wechat_return");
        assert_eq!(return_marker("epusdt", "usdt"), "epusdt_return");
    }

    /// PAY-06: tenant return URL rewriting.
    #[test]
    fn pay_06_tenant_return_url() {
        let cfg = json!({"return_url": "https://main.example.com/pay"});
        let cfg = cfg.as_object().cloned().unwrap_or_default();
        assert_eq!(tenant_return_url(None, "https", &cfg), "");
        let main = TenantSite {
            is_main: true,
            ..TenantSite::default()
        };
        assert_eq!(tenant_return_url(Some(&main), "https", &cfg), "");
        let shop = TenantSite {
            host: "shop.example.com".into(),
            ..TenantSite::default()
        };
        assert_eq!(
            tenant_return_url(Some(&shop), "https", &cfg),
            "https://shop.example.com/pay"
        );
        let primary = TenantSite {
            primary_domain: "p.example.com".into(),
            ..TenantSite::default()
        };
        assert_eq!(
            tenant_return_url(Some(&primary), "http", &cfg),
            "http://p.example.com/pay"
        );
        assert_eq!(
            tenant_return_url(Some(&shop), "ftp", &cfg),
            "https://shop.example.com/pay"
        );
        let unavailable = TenantSite {
            unavailable: true,
            ..shop.clone()
        };
        assert_eq!(tenant_return_url(Some(&unavailable), "https", &cfg), "");
        let obj = |v: serde_json::Value| v.as_object().cloned().unwrap_or_default();
        assert_eq!(
            tenant_return_path(&obj(
                json!({"return_url": "https://main/checkout/result?from=gateway"})
            )),
            "/checkout/result?from=gateway"
        );
        assert_eq!(
            tenant_return_path(&obj(json!({"success_url": "https://main/done"}))),
            "/done"
        );
        assert_eq!(
            tenant_return_path(&obj(
                json!({"return_url": "https://main/a", "success_url": "https://main/b"})
            )),
            "/a"
        );
        assert_eq!(
            tenant_return_path(&obj(json!({"return_url": "https://main/"}))),
            "/pay"
        );
    }

    /// PAY-09: gateway order numbers are random DJP serials and reused when present.
    #[test]
    fn pay_09_gateway_order_no() {
        let now = Utc::now();
        let a = new_gateway_order_no(now);
        let b = new_gateway_order_no(now);
        assert!(a.starts_with("DJP") && b.starts_with("DJP"));
        assert_eq!(a.len(), 3 + 14 + 6);
        assert_eq!(resolve_gateway_order_no("CUSTOM-1", now), "CUSTOM-1");
        assert!(resolve_gateway_order_no(" ", now).starts_with("DJP"));
    }
}
