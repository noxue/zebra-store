//! SSRF-safe HTTP client for third-party card converters.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use zs_domain::integration::card_converter::{
    ConverterHttp, ConverterRefundRef, ConverterRefundRequest, ConverterRefundStatus,
    ExchangeRequest, ExchangeResponse,
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
        body: Option<Vec<u8>>,
    ) -> Result<T> {
        let mut request = self
            .http
            .request(method, url)
            .bearer_auth(token)
            .header(reqwest::header::ACCEPT, "application/json");
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
struct HealthResponse {
    #[serde(default = "health_ok")]
    ok: bool,
}

fn health_ok() -> bool {
    true
}

#[async_trait]
impl ConverterHttp for HttpCardConverter {
    async fn prepare_refund(
        &self,
        base: &str,
        token: &str,
        request: &ConverterRefundRequest,
    ) -> Result<ConverterRefundStatus> {
        if request.order_no.trim().is_empty()
            || request.refund_no.trim().is_empty()
            || request.cards.is_empty()
            || request.cards.len() > 100
            || request
                .cards
                .iter()
                .any(|card| card.trim().is_empty() || card.len() > 4096)
        {
            return Err(Error::bad_request("error.card_converter_request_invalid"));
        }
        self.request_json(
            reqwest::Method::POST,
            self.endpoint(base, "/integration/v1/refunds/prepare")?,
            token,
            Some(serde_json::to_vec(request)?),
        )
        .await
    }

    async fn confirm_refund(
        &self,
        base: &str,
        token: &str,
        reference: &ConverterRefundRef,
    ) -> Result<ConverterRefundStatus> {
        self.refund_action(base, token, "confirm", reference).await
    }

    async fn cancel_refund(
        &self,
        base: &str,
        token: &str,
        reference: &ConverterRefundRef,
    ) -> Result<ConverterRefundStatus> {
        self.refund_action(base, token, "cancel", reference).await
    }

    async fn refund_status(
        &self,
        base: &str,
        token: &str,
        reference: &ConverterRefundRef,
    ) -> Result<ConverterRefundStatus> {
        self.refund_action(base, token, "status", reference).await
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
            return Err(Error::internal_msg(
                "card converter health endpoint is missing",
            ));
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
        if request.items.is_empty() || request.items.len() > 100 {
            return Err(Error::bad_request("error.card_converter_request_invalid"));
        }
        let bytes = serde_json::to_vec(request)?;
        let response: ExchangeResponse = self
            .request_json(
                reqwest::Method::POST,
                self.endpoint(base, "/integration/v1/exchanges")?,
                token,
                Some(bytes),
            )
            .await?;
        if response.items.len() != request.items.len() {
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

impl HttpCardConverter {
    async fn refund_action(
        &self,
        base: &str,
        token: &str,
        action: &str,
        reference: &ConverterRefundRef,
    ) -> Result<ConverterRefundStatus> {
        if reference.order_no.trim().is_empty() || reference.refund_no.trim().is_empty() {
            return Err(Error::bad_request("error.card_converter_request_invalid"));
        }
        self.request_json(
            reqwest::Method::POST,
            self.endpoint(base, &format!("/integration/v1/refunds/{action}"))?,
            token,
            Some(serde_json::to_vec(reference)?),
        )
        .await
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
    fn bounds_exchange_card_count_and_keeps_context_out_of_the_wire_body() {
        let request = ExchangeRequest {
            order_no: "ZS1".into(),
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
        let body = serde_json::to_value(&request).expect("valid request");
        assert_eq!(body["items"].as_array().map(Vec::len), Some(2));
        assert_eq!(body.as_object().map(|body| body.len()), Some(3));
        assert_eq!(body["order_no"], "ZS1");
        assert!(body.get("idempotency_key").is_none());
        assert!(body.get("order").is_none());
        assert!(body.get("site").is_none());
        assert!(body.get("product").is_none());
        assert!(body.get("sku").is_none());
    }
}
