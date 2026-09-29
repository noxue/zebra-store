//! SSRF-safe HTTP client for third-party card converters.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use zs_domain::integration::card_converter::{
    ConverterHttp, ConverterType, ExchangeRequest, ExchangeResponse,
};
use zs_domain::{Error, Result};

use super::http::{AddressPolicy, build_no_redirect_client, check_url, read_limited};

const TIMEOUT: Duration = Duration::from_secs(15);
const RESPONSE_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct HttpCardConverter {
    http: reqwest::Client,
    policy: AddressPolicy,
}

impl HttpCardConverter {
    pub fn new(policy: AddressPolicy) -> Self {
        Self {
            http: build_no_redirect_client(policy, TIMEOUT),
            policy,
        }
    }

    fn endpoint(&self, base: &str, path: &str) -> Result<reqwest::Url> {
        let normalized = base.trim().trim_end_matches('/');
        let mut url = check_url(self.policy, normalized)
            .map_err(|_| Error::bad_request("error.card_converter_url_invalid"))?;
        if self.policy == AddressPolicy::PublicOnly && url.scheme() != "https" {
            return Err(Error::bad_request("error.card_converter_url_invalid"));
        }
        if !url.username().is_empty() || url.password().is_some() || url.query().is_some() {
            return Err(Error::bad_request("error.card_converter_url_invalid"));
        }
        let base_path = url.path().trim_end_matches('/');
        url.set_path(&format!("{base_path}{path}"));
        url.set_fragment(None);
        Ok(url)
    }

    async fn request_json<T: for<'de> Deserialize<'de>>(
        &self,
        method: reqwest::Method,
        url: reqwest::Url,
        token: &str,
        idempotency_key: Option<&str>,
        body: Option<Vec<u8>>,
    ) -> Result<T> {
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(key) = idempotency_key {
            request = request.header("Idempotency-Key", key);
        }
        if let Some(body) = body {
            request = request
                .header(reqwest::header::CONTENT_TYPE, "application/json")
                .body(body);
        }
        let response = request.send().await.map_err(|error| {
            Error::internal_msg(if error.is_timeout() {
                "card converter timed out"
            } else {
                "card converter request failed"
            })
        })?;
        if response.status().is_redirection() {
            return Err(Error::internal_msg("card converter redirect refused"));
        }
        if !response.status().is_success() {
            return Err(Error::internal_msg(format!(
                "card converter returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let bytes = read_limited(response, RESPONSE_LIMIT)
            .await
            .map_err(|_| Error::internal_msg("card converter response is too large"))?;
        serde_json::from_slice(&bytes)
            .map_err(|_| Error::internal_msg("card converter response is invalid JSON"))
    }
}

#[derive(Debug, Deserialize)]
struct TypesResponse {
    protocol_version: String,
    types: Vec<ConverterType>,
}

#[derive(Debug, Deserialize)]
struct HealthResponse {
    #[serde(default = "health_ok")]
    ok: bool,
}

fn health_ok() -> bool {
    true
}

#[async_trait]
impl ConverterHttp for HttpCardConverter {
    async fn types(&self, base: &str, token: &str) -> Result<Vec<ConverterType>> {
        let response: TypesResponse = self
            .request_json(
                reqwest::Method::GET,
                self.endpoint(base, "/integration/v1/types")?,
                token,
                None,
                None,
            )
            .await?;
        if response.protocol_version != "1"
            || response.types.is_empty()
            || response.types.len() > 500
            || response.types.iter().any(|item| {
                item.id.trim().is_empty()
                    || item.id.len() > 100
                    || item.name.trim().is_empty()
                    || item.name.len() > 200
                    || item.fields.len() > 64
                    || item.fields.iter().any(|field| {
                        field.key.is_empty()
                            || field.key.len() > 100
                            || !field
                                .key
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
                            || field.label.trim().is_empty()
                            || field.label.len() > 200
                            || !matches!(
                                field.kind.as_str(),
                                "text" | "number" | "boolean" | "select"
                            )
                            || field.options.len() > 100
                            || (field.kind == "select" && field.options.is_empty())
                    })
            })
        {
            return Err(Error::internal_msg("card converter type list is invalid"));
        }
        let mut ids = std::collections::HashSet::new();
        if response
            .types
            .iter()
            .any(|item| !ids.insert(item.id.as_str()))
        {
            return Err(Error::internal_msg(
                "card converter returned duplicate type ids",
            ));
        }
        for item in &response.types {
            let mut fields = std::collections::HashSet::new();
            if item
                .fields
                .iter()
                .any(|field| !fields.insert(field.key.as_str()))
            {
                return Err(Error::internal_msg(
                    "card converter returned duplicate field keys",
                ));
            }
        }
        Ok(response.types)
    }

    async fn health(&self, base: &str, token: &str) -> Result<()> {
        let url = self.endpoint(base, "/health")?;
        let response = self
            .http
            .get(url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| {
                Error::internal_msg(if error.is_timeout() {
                    "health check timed out"
                } else {
                    "health check request failed"
                })
            })?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            self.types(base, token).await?;
            return Ok(());
        }
        if !response.status().is_success() {
            return Err(Error::internal_msg(format!(
                "health check returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let bytes = read_limited(response, 4096)
            .await
            .map_err(|_| Error::internal_msg("health response is invalid"))?;
        let response: HealthResponse = serde_json::from_slice(&bytes)
            .map_err(|_| Error::internal_msg("health response is invalid"))?;
        if !response.ok {
            return Err(Error::internal_msg("health check reported unhealthy"));
        }
        Ok(())
    }

    async fn exchange(
        &self,
        base: &str,
        token: &str,
        request: &ExchangeRequest,
    ) -> Result<ExchangeResponse> {
        if request.protocol_version != "1"
            || request.items.is_empty()
            || request.items.len() > 100
            || request.order["quantity"].as_u64() != Some(request.items.len() as u64)
        {
            return Err(Error::bad_request("error.card_converter_request_invalid"));
        }
        let bytes = serde_json::to_vec(request)?;
        let response: ExchangeResponse = self
            .request_json(
                reqwest::Method::POST,
                self.endpoint(base, "/integration/v1/exchanges")?,
                token,
                Some(&request.idempotency_key),
                Some(bytes),
            )
            .await?;
        if response.protocol_version != "1" || response.items.len() != request.items.len() {
            return Err(Error::internal_msg(
                "card converter returned the wrong number of cards",
            ));
        }
        let mut by_index = std::collections::HashMap::new();
        for item in &response.items {
            if item.card.trim().is_empty()
                || item.card.len() > 4096
                || by_index.insert(item.index, item.card.as_str()).is_some()
            {
                return Err(Error::internal_msg("card converter returned invalid cards"));
            }
        }
        for input in &request.items {
            let output = by_index
                .get(&input.index)
                .ok_or_else(|| Error::internal_msg("card converter omitted a card"))?;
            if output.trim() == input.upstream_card.trim() {
                return Err(Error::internal_msg(
                    "card converter returned an unchanged card",
                ));
            }
        }
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn rejects_http_converter_urls_in_production_policy() {
        let client = HttpCardConverter::new(AddressPolicy::PublicOnly);
        assert!(client.endpoint("http://example.com", "/health").is_err());
        assert!(
            client
                .endpoint("https://user:pass@example.com", "/health")
                .is_err()
        );
        assert!(client.endpoint("https://localhost", "/health").is_err());
    }

    #[test]
    fn permits_http_only_for_explicit_private_test_policy() {
        let client = HttpCardConverter::new(AddressPolicy::AllowPrivate);
        assert!(client.endpoint("http://127.0.0.1:8080", "/health").is_ok());
    }

    #[test]
    fn bounds_exchange_card_count_and_requires_exact_order_quantity() {
        let request = ExchangeRequest {
            protocol_version: "1".into(),
            idempotency_key: "order-1".into(),
            type_id: "type-a".into(),
            order: serde_json::json!({"quantity":2}),
            site: Value::Null,
            product: Value::Null,
            sku: Value::Null,
            items: vec![
                zs_domain::integration::card_converter::ExchangeItem {
                    index: 0,
                    upstream_card: "a".into(),
                },
                zs_domain::integration::card_converter::ExchangeItem {
                    index: 1,
                    upstream_card: "b".into(),
                },
            ],
            extra: Value::Null,
        };
        assert_eq!(
            request.items.len(),
            request.order["quantity"].as_u64().unwrap() as usize
        );
    }
}
