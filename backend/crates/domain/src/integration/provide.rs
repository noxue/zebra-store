//! Provider-side compatibility (other shop systems buying from us through *their* wire
//! protocol, e.g. acg-faka `/shared/*` or the mcy-shop OpenApi plugin): the
//! per-credential compat key those protocols sign with, and the pure rules shared by
//! every provider facade (IP allowlist, request-number syntax).
//!
//! The protocol-neutral use cases live in `zs_app::integration::provide`; see
//! `docs/protocol/third-party/provider-compat.md`.

use std::net::IpAddr;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{Id, Result};

/// Length of a generated compat `app_key` (uppercase letters + digits, ≈165 bits).
/// acg-faka accepts 1–64 non-blank characters (`Store.php:154-165`); its own keys
/// are 16 characters, ours are longer because the protocols sign with plain md5.
pub const APP_KEY_LEN: usize = 32;
/// Longest downstream request number (`downstream_order_refs.downstream_order_no`
/// is `varchar(64)`).
pub const MAX_REQUEST_NO_LEN: usize = 64;
/// Most entries of an IP allowlist.
pub const MAX_ALLOWLIST_ENTRIES: usize = 20;
/// Requests per minute per `protocol|IP|app_id` on the compat endpoints. acg-faka
/// calls `item`/`stock`/`valuation` on every buyer visit of an imported product, so
/// the budget is larger than the 60 / min of `/upstream/*`.
pub const COMPAT_REQUESTS_PER_MINUTE: i64 = 300;
/// Largest accepted compat request body (form fields only; acg-faka sends < 4 KiB).
pub const MAX_COMPAT_BODY_BYTES: usize = 1 << 20;
/// Stock reported for "unlimited" SKUs: acg-faka compares `num > stock` and adds
/// stocks as numbers, so `-1` cannot be used (faka-bridge uses the same value).
pub const UNLIMITED_STOCK: i64 = 999_999;

/// The compat key of an API credential (one per credential).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatKey {
    pub credential_id: Id,
    pub user_id: Id,
    /// The credential's `api_key` when the compat key was issued: a re-approval
    /// (which issues a new `api_key`) invalidates the compat key.
    pub bound_api_key: String,
    /// AES-GCM encrypted `app_key`.
    pub app_key: String,
    /// Owner's switch for the compat protocols.
    pub is_active: bool,
    /// Comma separated IPs / CIDRs; empty = any address.
    pub ip_allowlist: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Storage of compat keys plus the order-number lookup the facades need.
#[async_trait]
pub trait CompatKeyRepo: Send + Sync {
    async fn get(&self, credential_id: Id) -> Result<Option<CompatKey>>;
    /// Inserts or replaces the key of `key.credential_id`.
    async fn put(&self, key: &CompatKey, now: DateTime<Utc>) -> Result<()>;
    async fn touch(&self, credential_id: Id, at: DateTime<Utc>) -> Result<()>;
    /// Root order id of an order number.
    async fn order_id_by_no(&self, order_no: &str) -> Result<Option<Id>>;
}

/// A downstream request number (idempotency key): 1–64 visible characters.
pub fn valid_request_no(raw: &str) -> bool {
    !raw.is_empty()
        && raw.len() <= MAX_REQUEST_NO_LEN
        && raw.chars().all(|c| !c.is_control() && !c.is_whitespace())
}

/// One allowlist entry: an IP or `ip/prefix`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Net {
    addr: IpAddr,
    prefix: u8,
}

impl Net {
    fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim();
        let (addr, prefix) = match raw.split_once('/') {
            Some((ip, p)) => (ip.trim().parse::<IpAddr>().ok()?, p.trim().parse().ok()?),
            None => {
                let ip = raw.parse::<IpAddr>().ok()?;
                (ip, if ip.is_ipv4() { 32 } else { 128 })
            }
        };
        let max = if addr.is_ipv4() { 32 } else { 128 };
        (prefix <= max).then_some(Self { addr, prefix })
    }

    fn contains(self, ip: IpAddr) -> bool {
        let ip = match ip {
            IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(ip, IpAddr::V4),
            v4 @ IpAddr::V4(_) => v4,
        };
        match (self.addr, ip) {
            (IpAddr::V4(net), IpAddr::V4(ip)) => {
                let mask = u32::MAX
                    .checked_shl(32 - u32::from(self.prefix))
                    .unwrap_or(0);
                u32::from(net) & mask == u32::from(ip) & mask
            }
            (IpAddr::V6(net), IpAddr::V6(ip)) => {
                let mask = u128::MAX
                    .checked_shl(128 - u32::from(self.prefix))
                    .unwrap_or(0);
                u128::from(net) & mask == u128::from(ip) & mask
            }
            _ => false,
        }
    }
}

/// Normalizes an allowlist (comma / whitespace / newline separated): every entry must
/// be an IP or CIDR; returns the canonical comma-joined form.
pub fn normalize_allowlist(raw: &str) -> std::result::Result<String, String> {
    let entries: Vec<&str> = raw
        .split([',', '\n', '\r', ' ', '\t', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if entries.len() > MAX_ALLOWLIST_ENTRIES {
        return Err(format!("at most {MAX_ALLOWLIST_ENTRIES} entries"));
    }
    for e in &entries {
        if Net::parse(e).is_none() {
            return Err(format!("invalid IP or CIDR: {e}"));
        }
    }
    Ok(entries.join(","))
}

/// True when `ip` is allowed by `allowlist` (empty list = any address; an unknown
/// client address never matches a non-empty list).
pub fn ip_allowed(allowlist: &str, ip: &str) -> bool {
    let nets: Vec<Net> = allowlist
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(Net::parse)
        .collect();
    if nets.is_empty() {
        return true;
    }
    let Ok(ip) = ip.trim().parse::<IpAddr>() else {
        return false;
    };
    nets.iter().any(|n| n.contains(ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_numbers() {
        assert!(valid_request_no("123456789012345678"));
        assert!(valid_request_no(&"a".repeat(64)));
        assert!(!valid_request_no(""));
        assert!(!valid_request_no(&"a".repeat(65)));
        assert!(!valid_request_no("a b"));
        assert!(!valid_request_no("a\nb"));
    }

    #[test]
    fn allowlist_matching() {
        assert!(ip_allowed("", "8.8.8.8"));
        assert!(ip_allowed("", ""));
        let list = normalize_allowlist(" 1.2.3.4 ,10.0.0.0/8\n2001:db8::/32").unwrap();
        assert_eq!(list, "1.2.3.4,10.0.0.0/8,2001:db8::/32");
        assert!(ip_allowed(&list, "1.2.3.4"));
        assert!(!ip_allowed(&list, "1.2.3.5"));
        assert!(ip_allowed(&list, "10.200.1.1"));
        assert!(ip_allowed(&list, "::ffff:10.0.0.1"));
        assert!(ip_allowed(&list, "2001:db8::1"));
        assert!(!ip_allowed(&list, "2001:db9::1"));
        assert!(!ip_allowed(&list, ""));
        assert!(!ip_allowed(&list, "garbage"));
        assert!(normalize_allowlist("1.2.3.4/33").is_err());
        assert!(normalize_allowlist("example.com").is_err());
        assert_eq!(normalize_allowlist("  ").unwrap(), "");
    }
}
