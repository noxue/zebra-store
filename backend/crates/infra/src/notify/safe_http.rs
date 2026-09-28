//! SSRF-safe outbound HTTP for peer-configured URLs (UPS-01, UPS-10):
//! only public IPs are connected (checked after DNS resolution, so a domain
//! pointing at an internal address is refused too), redirects are never
//! followed and every request has a timeout.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use zs_domain::notify::ports::{CallbackPoster, KEY_FORBIDDEN_ADDRESS};
use zs_domain::{Error, Result};

/// Connect timeout (original dial timeout 10 s).
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Whole-request timeout (original client timeout 15 s).
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Which addresses may be contacted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressPolicy {
    /// Production: public unicast addresses only.
    PublicOnly,
    /// Integration tests against local mock servers (redirects are still refused).
    AllowPrivate,
}

/// True for globally routable unicast addresses.
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_public_v4(v4),
        IpAddr::V6(v6) => is_public_v6(v6),
    }
}

fn is_public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        || a == 0
        // 100.64.0.0/10 carrier-grade NAT
        || (a == 100 && (64..128).contains(&b))
        // 192.0.0.0/24 IETF protocol assignments
        || (a == 192 && b == 0 && c == 0)
        // 198.18.0.0/15 benchmarking
        || (a == 198 && (b == 18 || b == 19))
        // 240.0.0.0/4 reserved
        || a >= 240)
}

fn is_public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return is_public_v4(v4);
    }
    let seg = ip.segments();
    // IPv4-compatible (::a.b.c.d) and NAT64 (64:ff9b::/96) embed an IPv4 address.
    let embedded = Ipv4Addr::new(
        (seg[6] >> 8) as u8,
        (seg[6] & 0xff) as u8,
        (seg[7] >> 8) as u8,
        (seg[7] & 0xff) as u8,
    );
    if seg[..6] == [0, 0, 0, 0, 0, 0] && !ip.is_loopback() && !ip.is_unspecified() {
        return is_public_v4(embedded);
    }
    if seg[0] == 0x64 && seg[1] == 0xff9b && seg[2..6] == [0, 0, 0, 0] {
        return is_public_v4(embedded);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // fc00::/7 unique local
        || (seg[0] & 0xfe00) == 0xfc00
        // fe80::/10 link local
        || (seg[0] & 0xffc0) == 0xfe80
        // fec0::/10 deprecated site local
        || (seg[0] & 0xffc0) == 0xfec0
        // 2001:db8::/32 documentation
        || (seg[0] == 0x2001 && seg[1] == 0x0db8))
}

/// Error raised by the resolver for a refused address.
#[derive(Debug)]
struct ForbiddenAddress(String);

impl std::fmt::Display for ForbiddenAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{KEY_FORBIDDEN_ADDRESS}: {}", self.0)
    }
}

impl std::error::Error for ForbiddenAddress {}

/// DNS resolver that refuses names resolving to any non-public address.
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

fn forbidden(detail: &str) -> Error {
    Error::forbidden(KEY_FORBIDDEN_ADDRESS).arg(detail)
}

fn is_forbidden(err: &reqwest::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = source {
        if e.downcast_ref::<ForbiddenAddress>().is_some() {
            return true;
        }
        source = e.source();
    }
    false
}

/// HTTP client for peer-configured URLs.
#[derive(Debug, Clone)]
pub struct SafeHttp {
    client: reqwest::Client,
    policy: AddressPolicy,
}

impl SafeHttp {
    pub fn new(policy: AddressPolicy) -> Result<Self> {
        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT);
        if policy == AddressPolicy::PublicOnly {
            builder = builder.dns_resolver(Arc::new(PublicResolver));
        }
        Ok(Self {
            client: builder.build().map_err(Error::internal)?,
            policy,
        })
    }

    /// Validates scheme, host and (for IP literals) the address before any I/O.
    fn check_url(&self, raw: &str) -> Result<reqwest::Url> {
        let url = reqwest::Url::parse(raw.trim()).map_err(|_| forbidden("invalid url"))?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(forbidden("scheme"));
        }
        let host = url.host_str().ok_or_else(|| forbidden("host"))?;
        if self.policy == AddressPolicy::PublicOnly {
            if host.eq_ignore_ascii_case("localhost") {
                return Err(forbidden("localhost"));
            }
            let literal = host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<IpAddr>()
                .ok();
            if let Some(ip) = literal
                && !is_public_ip(ip)
            {
                return Err(forbidden(&ip.to_string()));
            }
        }
        Ok(url)
    }
}

#[async_trait]
impl CallbackPoster for SafeHttp {
    async fn post_json(&self, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<u16> {
        let url = self.check_url(url)?;
        let mut req = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_vec());
        for (k, v) in headers {
            req = req.header(k.as_str(), v.as_str());
        }
        match req.send().await {
            Ok(res) => Ok(res.status().as_u16()),
            Err(e) if is_forbidden(&e) => Err(forbidden("resolved to a non-public address")),
            Err(e) => Err(Error::internal(e.without_url())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    // UPS-01: address classification.
    #[test]
    fn ups01_classifies_addresses() {
        for bad in [
            "127.0.0.1",
            "10.0.0.1",
            "172.16.5.4",
            "192.168.1.1",
            "169.254.169.254",
            "100.64.1.1",
            "0.0.0.0",
            "255.255.255.255",
            "224.0.0.1",
            "::1",
            "::",
            "fc00::1",
            "fd12::1",
            "fe80::1",
            "::ffff:127.0.0.1",
            "::ffff:10.0.0.1",
            "64:ff9b::a00:1",
            "2001:db8::1",
        ] {
            assert!(!is_public_ip(ip(bad)), "{bad} must be refused");
        }
        for good in [
            "8.8.8.8",
            "1.1.1.1",
            "2606:4700:4700::1111",
            "::ffff:8.8.8.8",
        ] {
            assert!(is_public_ip(ip(good)), "{good} must be allowed");
        }
    }

    // UPS-01: literal and resolved internal targets fail with "forbidden address".
    #[tokio::test]
    async fn ups01_refuses_internal_targets() {
        let http = SafeHttp::new(AddressPolicy::PublicOnly).unwrap();
        for url in [
            "http://127.0.0.1:9/x",
            "http://10.0.0.1/x",
            "http://169.254.169.254/latest/meta-data",
            "http://100.64.1.1/",
            "http://[::1]/",
            "http://localhost/",
            "http://localhost.:8080/",
            "ftp://example.com/",
        ] {
            let err = http.post_json(url, &[], b"{}").await.unwrap_err();
            assert_eq!(err.kind(), zs_domain::ErrorKind::Forbidden, "{url}");
            assert_eq!(err.key(), KEY_FORBIDDEN_ADDRESS);
        }
    }

    /// Minimal HTTP server answering every request with `response`.
    async fn serve_once(response: &'static str) -> SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = sock.read(&mut buf).await;
                let _ = sock.write_all(response.as_bytes()).await;
                let _ = sock.shutdown().await;
            }
        });
        addr
    }

    // UPS-01: a 302 is returned as-is, never followed.
    #[tokio::test]
    async fn ups01_redirects_are_not_followed() {
        let addr = serve_once("HTTP/1.1 302 Found\r\nLocation: http://169.254.169.254/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await;
        let http = SafeHttp::new(AddressPolicy::AllowPrivate).unwrap();
        let status = http
            .post_json(&format!("http://{addr}/cb"), &[], b"{}")
            .await
            .unwrap();
        assert_eq!(status, 302);
    }
}
