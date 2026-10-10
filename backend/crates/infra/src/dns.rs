//! DNS adapter for custom reseller domain ownership checks.

use async_trait::async_trait;
use hickory_resolver::ResolveErrorKind;
use hickory_resolver::TokioResolver;
use hickory_resolver::proto::ProtoErrorKind;
use hickory_resolver::proto::rr::{RData, RecordType};
use zs_domain::reseller::ports::DomainDns;
use zs_domain::{Error, Result};

#[derive(Debug, Default)]
pub struct SystemDomainDns;

#[async_trait]
impl DomainDns for SystemDomainDns {
    async fn cname_target(&self, domain: &str) -> Result<Option<String>> {
        let resolver = TokioResolver::builder_tokio()
            .map_err(Error::internal)?
            .build();
        lookup_cname(&resolver, domain).await
    }
}

async fn lookup_cname(resolver: &TokioResolver, domain: &str) -> Result<Option<String>> {
    let name = format!("{domain}.");
    match resolver.lookup(name.as_str(), RecordType::CNAME).await {
        Ok(records) => Ok(records.iter().find_map(|record| match record {
            RData::CNAME(target) => Some(target.to_string()),
            _ => None,
        })),
        Err(err) => match err.kind() {
            ResolveErrorKind::Proto(proto)
                if matches!(proto.kind(), ProtoErrorKind::NoRecordsFound { .. }) =>
            {
                Ok(None)
            }
            _ => Err(Error::internal(err)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_resolver::config::{NameServerConfig, ResolverConfig};
    use hickory_resolver::name_server::TokioConnectionProvider;
    use hickory_resolver::proto::xfer::Protocol;
    use tokio::net::UdpSocket;

    fn encoded_name(name: &str) -> Vec<u8> {
        let mut bytes = Vec::new();
        for label in name.split('.') {
            bytes.push(label.len() as u8);
            bytes.extend_from_slice(label.as_bytes());
        }
        bytes.push(0);
        bytes
    }

    fn dns_answer(query: &[u8]) -> Option<Vec<u8>> {
        if query.len() < 17 {
            return None;
        }
        let mut end = 12;
        let mut labels = Vec::new();
        loop {
            let size = *query.get(end)? as usize;
            end += 1;
            if size == 0 {
                break;
            }
            labels.push(std::str::from_utf8(query.get(end..end + size)?).ok()?);
            end += size;
        }
        let question = query.get(12..end + 4)?;
        let domain = labels.join(".");
        let target = match domain.as_str() {
            "first.agent.test" | "second.agent.test" => Some("connect.example.test"),
            "wrong.agent.test" => Some("other.example.test"),
            _ => None,
        };
        let mut response = Vec::from(&query[..2]);
        response.extend_from_slice(&[0x85, 0x80, 0, 1, 0, u8::from(target.is_some()), 0, 0, 0, 0]);
        response.extend_from_slice(question);
        if let Some(target) = target {
            let cname = encoded_name(target);
            response.extend_from_slice(&[0xc0, 0x0c, 0, 5, 0, 1, 0, 0, 0, 60]);
            response.extend_from_slice(&(cname.len() as u16).to_be_bytes());
            response.extend_from_slice(&cname);
        }
        Some(response)
    }

    #[tokio::test]
    async fn local_dns_checks_multiple_cname_records() {
        let socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let mut buf = [0_u8; 2048];
            while let Ok((size, peer)) = socket.recv_from(&mut buf).await {
                if let Some(answer) = dns_answer(&buf[..size]) {
                    socket.send_to(&answer, peer).await.unwrap();
                }
            }
        });
        let mut config = ResolverConfig::new();
        let mut server_config = NameServerConfig::new(address, Protocol::Udp);
        server_config.trust_negative_responses = true;
        config.add_name_server(server_config);
        let resolver =
            TokioResolver::builder_with_config(config, TokioConnectionProvider::default()).build();
        for domain in ["first.agent.test", "second.agent.test"] {
            assert_eq!(
                lookup_cname(&resolver, domain).await.unwrap().as_deref(),
                Some("connect.example.test.")
            );
        }
        assert_eq!(
            lookup_cname(&resolver, "wrong.agent.test")
                .await
                .unwrap()
                .as_deref(),
            Some("other.example.test.")
        );
        assert_eq!(
            lookup_cname(&resolver, "missing.agent.test").await.unwrap(),
            None
        );
        server.abort();
    }
}
