//! Client information (IP honouring trusted proxies, user agent, request id).

use std::net::{IpAddr, SocketAddr};

use axum::extract::{ConnectInfo, FromRef, FromRequestParts};
use axum::http::request::Parts;
use zs_app::identity::admin_auth::ClientInfo;

use crate::middleware::request_id::RequestId;
use crate::response::ApiError;
use crate::state::AppState;

/// A parsed CIDR block such as `127.0.0.1/32`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cidr {
    addr: IpAddr,
    prefix: u8,
}

impl Cidr {
    /// Parses `ip` or `ip/prefix`.
    pub fn parse(s: &str) -> Option<Self> {
        let (ip, prefix) = match s.trim().split_once('/') {
            Some((ip, p)) => (ip.parse::<IpAddr>().ok()?, p.parse::<u8>().ok()?),
            None => {
                let ip = s.trim().parse::<IpAddr>().ok()?;
                (ip, if ip.is_ipv4() { 32 } else { 128 })
            }
        };
        let max = if ip.is_ipv4() { 32 } else { 128 };
        (prefix <= max).then_some(Self { addr: ip, prefix })
    }

    pub fn contains(&self, ip: IpAddr) -> bool {
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

/// Resolves the client IP like Gin: when the peer is a trusted proxy, walk
/// `X-Forwarded-For` from the right and return the first untrusted address.
pub fn resolve_ip(peer: Option<IpAddr>, forwarded: Option<&str>, trusted: &[Cidr]) -> String {
    let Some(peer) = peer else {
        return String::new();
    };
    let is_trusted = |ip: IpAddr| trusted.iter().any(|c| c.contains(ip));
    if !is_trusted(peer) {
        return peer.to_string();
    }
    if let Some(xff) = forwarded {
        for hop in xff.split(',').rev() {
            match hop.trim().parse::<IpAddr>() {
                Ok(ip) if !is_trusted(ip) => return ip.to_string(),
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
    peer.to_string()
}

/// Extractor yielding [`ClientInfo`].
#[derive(Debug, Clone)]
pub struct Client(pub ClientInfo);

impl<S> FromRequestParts<S> for Client
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        let peer = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|c| c.0.ip());
        let xff = parts
            .headers
            .get("x-forwarded-for")
            .and_then(|v| v.to_str().ok());
        let header = |n: &str| {
            parts
                .headers
                .get(n)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
                .to_owned()
        };
        Ok(Self(ClientInfo {
            ip: resolve_ip(peer, xff, &app.trusted_proxies),
            user_agent: header("user-agent"),
            request_id: parts
                .extensions
                .get::<RequestId>()
                .map(|r| r.0.clone())
                .unwrap_or_default(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cidr_contains() {
        let c = Cidr::parse("10.0.0.0/8").unwrap();
        assert!(c.contains("10.1.2.3".parse().unwrap()));
        assert!(!c.contains("11.0.0.1".parse().unwrap()));
        assert!(
            Cidr::parse("::1/128")
                .unwrap()
                .contains("::1".parse().unwrap())
        );
        assert!(Cidr::parse("bad").is_none());
    }

    #[test]
    fn resolves_forwarded_only_from_trusted_peer() {
        let trusted = vec![Cidr::parse("127.0.0.1/32").unwrap()];
        let local = Some("127.0.0.1".parse().unwrap());
        assert_eq!(
            resolve_ip(local, Some("1.2.3.4, 127.0.0.1"), &trusted),
            "1.2.3.4"
        );
        let remote = Some("8.8.8.8".parse().unwrap());
        assert_eq!(resolve_ip(remote, Some("1.2.3.4"), &trusted), "8.8.8.8");
    }
}
