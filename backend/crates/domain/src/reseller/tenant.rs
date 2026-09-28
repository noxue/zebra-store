//! Request tenant: the main shop or a reseller site (port of `contract/tenant.go`).

use serde::Serialize;

use crate::Id;

/// Reason stored on an unavailable tenant (unknown / inactive domain).
pub const UNAVAILABLE_NOT_FOUND: &str = "not_found";

/// Which shop a storefront request belongs to (RSL-06).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ResellerTenant {
    pub host: String,
    pub is_main: bool,
    pub reseller_id: Option<Id>,
    pub reseller_user_id: Id,
    pub primary_domain: String,
    pub unavailable: bool,
    pub unavailable_reason: String,
}

impl ResellerTenant {
    /// The main shop.
    pub fn main(host: &str) -> Self {
        Self {
            host: normalize_host(host),
            is_main: true,
            ..Self::default()
        }
    }

    /// A live reseller site.
    pub fn reseller(
        host: &str,
        reseller_id: Id,
        reseller_user_id: Id,
        primary_domain: &str,
    ) -> Self {
        Self {
            host: normalize_host(host),
            is_main: false,
            reseller_id: Some(reseller_id),
            reseller_user_id,
            primary_domain: normalize_host(primary_domain),
            ..Self::default()
        }
    }

    /// An unknown or inactive reseller host.
    pub fn unavailable(host: &str, reason: &str) -> Self {
        Self {
            host: normalize_host(host),
            unavailable: true,
            unavailable_reason: reason.trim().to_owned(),
            ..Self::default()
        }
    }

    /// A usable reseller site request.
    pub fn is_reseller(&self) -> bool {
        self.reseller_id.is_some() && !self.is_main && !self.unavailable
    }

    /// Domain recorded on order snapshots (`SnapshotDomain`).
    pub fn snapshot_domain(&self) -> String {
        if self.primary_domain.trim().is_empty() {
            normalize_host(&self.host)
        } else {
            normalize_host(&self.primary_domain)
        }
    }

    /// The content group's `/public/config` tenant.
    pub fn to_content(&self) -> crate::content::public::Tenant {
        crate::content::public::Tenant {
            reseller_id: if self.is_reseller() {
                self.reseller_id
            } else {
                None
            },
            host: self.host.clone(),
        }
    }
}

/// Lower-cases and strips port, trailing dot and IPv6 brackets (`NormalizeHost`).
pub fn normalize_host(raw: &str) -> String {
    let mut host = raw.trim().to_lowercase();
    if host.is_empty() {
        return host;
    }
    if host.starts_with('[') {
        // "[::1]:8080" → "::1"; a bare "[::1]" is trimmed below.
        if let Some(end) = host.find(']') {
            let rest = &host[end + 1..];
            if rest.starts_with(':') {
                host = host[1..end].to_owned();
            }
        }
    } else if host.matches(':').count() == 1 {
        // Go's SplitHostPort only accepts a single colon without brackets.
        if let Some((h, _)) = host.split_once(':') {
            host = h.to_owned();
        }
    }
    if let Some(stripped) = host.strip_suffix('.') {
        host = stripped.to_owned();
    }
    host.trim_matches(|c| c == '[' || c == ']').to_owned()
}

/// Host of a request: `X-Forwarded-Host` (first value) only when trusted (`ResolveRequestHost`).
pub fn request_host(
    host: Option<&str>,
    forwarded_host: Option<&str>,
    trust_forwarded: bool,
) -> String {
    let mut raw = host.unwrap_or_default();
    if trust_forwarded
        && let Some(forwarded) = forwarded_host.map(str::trim).filter(|f| !f.is_empty())
    {
        raw = forwarded.split(',').next().unwrap_or_default();
    }
    normalize_host(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    // RSL-06: host normalisation.
    #[test]
    fn rsl06_normalizes_hosts() {
        let cases = [
            ("Shop.Example.COM", "shop.example.com"),
            ("shop.example.com:443", "shop.example.com"),
            ("shop.example.com.", "shop.example.com"),
            ("  LOCALHOST:5173  ", "localhost"),
            ("[::1]:8080", "::1"),
            ("Shop.EXAMPLE.com.:8080", "shop.example.com"),
            ("", ""),
        ];
        for (input, want) in cases {
            assert_eq!(normalize_host(input), want, "input {input:?}");
        }
    }

    // RSL-06: forged X-Forwarded-Host is ignored unless trusted.
    #[test]
    fn rsl06_forwarded_host_only_when_trusted() {
        let host = Some("internal.example.test");
        let fwd = Some("Shop.Example.test, other.test");
        assert_eq!(request_host(host, fwd, false), "internal.example.test");
        assert_eq!(request_host(host, fwd, true), "shop.example.test");
        assert_eq!(
            request_host(host, Some("  "), true),
            "internal.example.test"
        );
    }

    #[test]
    fn tenant_constructors() {
        let main = ResellerTenant::main("Main.Example.test");
        assert!(main.is_main && main.reseller_id.is_none() && !main.is_reseller());
        let t = ResellerTenant::reseller("shop.example.test", 42, 100, "Primary.example.test");
        assert!(t.is_reseller());
        assert_eq!(t.reseller_user_id, 100);
        assert_eq!(t.snapshot_domain(), "primary.example.test");
        assert_eq!(t.to_content().reseller_id, Some(42));
        let u = ResellerTenant::unavailable("x.test", UNAVAILABLE_NOT_FOUND);
        assert!(u.unavailable && !u.is_reseller());
        assert_eq!(u.to_content().reseller_id, None);
    }
}
