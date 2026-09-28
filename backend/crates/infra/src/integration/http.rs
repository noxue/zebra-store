//! SSRF-safe outbound HTTP for supplier / downstream URLs (UPS-01, UPS-10):
//! every resolved address must be public (checked in the DNS resolver, so DNS
//! rebinding to an internal address is refused at connect time), literal internal
//! addresses are refused before any I/O, redirects are never followed, requests time
//! out and response bodies are size-capped.

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

pub use crate::notify::safe_http::AddressPolicy;
use crate::notify::safe_http::is_public_ip;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};

/// Connect timeout (original dial timeout 10 s).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Error raised by the resolver for a refused address.
#[derive(Debug)]
pub(crate) struct ForbiddenAddress(pub String);

impl std::fmt::Display for ForbiddenAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "forbidden address: {}", self.0)
    }
}

impl std::error::Error for ForbiddenAddress {}

/// Resolver refusing names that resolve to any non-public address.
#[derive(Debug, Clone, Copy)]
struct PublicResolver;

impl Resolve for PublicResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let host = name.as_str().to_owned();
            let addrs: Vec<SocketAddr> =
                tokio::net::lookup_host((host.as_str(), 0)).await?.collect();
            if addrs.is_empty() || addrs.iter().any(|a| !is_public_ip(a.ip())) {
                return Err(
                    Box::new(ForbiddenAddress(host)) as Box<dyn std::error::Error + Send + Sync>
                );
            }
            Ok(Box::new(addrs.into_iter()) as Addrs)
        })
    }
}

/// True when `err` (or a cause) is a refused address.
pub(crate) fn is_forbidden(err: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = source {
        if e.downcast_ref::<ForbiddenAddress>().is_some() {
            return true;
        }
        source = e.source();
    }
    false
}

/// Builds a client for `policy` with the given total timeout.
pub(crate) fn build_client(policy: AddressPolicy, timeout: Duration) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(timeout);
    if policy == AddressPolicy::PublicOnly {
        builder = builder.dns_resolver(Arc::new(PublicResolver));
    }
    // Builder errors only come from TLS backend initialisation; fall back to defaults
    // (still no redirects) rather than panicking.
    builder.build().unwrap_or_else(|error| {
        tracing::error!(%error, "outbound http client build failed");
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap_or_default()
    })
}

/// Validates scheme / host and literal addresses before any I/O; returns the URL.
pub(crate) fn check_url(policy: AddressPolicy, raw: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| "invalid url".to_owned())?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err("scheme".into());
    }
    let host = url.host_str().ok_or_else(|| "host".to_owned())?;
    if policy == AddressPolicy::PublicOnly {
        let bare = host.trim_start_matches('[').trim_end_matches(']');
        let lower = bare.trim_end_matches('.').to_ascii_lowercase();
        if lower == "localhost" || lower.ends_with(".localhost") {
            return Err("localhost".into());
        }
        if let Ok(ip) = bare.parse::<IpAddr>()
            && !is_public_ip(ip)
        {
            return Err(ip.to_string());
        }
    }
    Ok(url)
}

/// Reads at most `limit` bytes of a response body; `Err` when larger.
pub(crate) async fn read_limited(
    mut res: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, String> {
    if res.content_length().is_some_and(|l| l > limit as u64) {
        return Err("response too large".into());
    }
    let mut out = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.without_url().to_string())? {
        if out.len() + chunk.len() > limit {
            return Err("response too large".into());
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // UPS-01: literal internal targets are refused before any I/O.
    #[test]
    fn ups01_literal_targets() {
        for bad in [
            "http://127.0.0.1:9/x",
            "http://10.0.0.1/",
            "http://169.254.169.254/latest",
            "http://100.64.1.1/",
            "http://[::1]/",
            "http://[::ffff:127.0.0.1]/",
            "http://localhost/",
            "http://a.localhost./",
            "ftp://example.com/",
        ] {
            assert!(check_url(AddressPolicy::PublicOnly, bad).is_err(), "{bad}");
        }
        assert!(check_url(AddressPolicy::PublicOnly, "https://8.8.8.8/x").is_ok());
        assert!(check_url(AddressPolicy::AllowPrivate, "http://127.0.0.1:9/x").is_ok());
    }

    // UPS-01: a name resolving to loopback is refused by the resolver (DNS rebinding).
    #[tokio::test]
    async fn ups01_resolved_loopback_is_refused() {
        use std::str::FromStr;
        let name = Name::from_str("localhost").expect("valid name");
        let err = PublicResolver.resolve(name).await.err().expect("refused");
        assert!(err.to_string().contains("forbidden address"), "{err}");
    }
}
