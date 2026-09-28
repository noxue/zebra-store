//! PayPal Orders v2 gateway with server-side webhook verification on the raw event bytes
//! (PAY-28..PAY-31).

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{encode_pairs, path_escape};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, GatewayQueryResult, Headers, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::PaymentStatus;

use super::common::{
    ExchangeRate, GatewayEnv, callback_amount, decode_object, first_non_empty, go_decimal_string,
    go_json_map, go_json_ordered, is_request_uri, parse_config, read_path,
};
use super::http::HttpRequest;

const DEFAULT_BASE_URL: &str = "https://api-m.sandbox.paypal.com";

/// PayPal channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub client_id: String,
    pub client_secret: String,
    pub base_url: String,
    pub return_url: String,
    pub cancel_url: String,
    pub webhook_id: String,
    pub brand_name: String,
    pub locale: String,
    pub landing_page: String,
    pub user_action: String,
    pub shipping_preference: String,
    pub target_currency: String,
    pub exchange_rate: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "paypal")?;
        for f in [
            &mut c.client_id,
            &mut c.client_secret,
            &mut c.return_url,
            &mut c.cancel_url,
            &mut c.webhook_id,
            &mut c.brand_name,
            &mut c.locale,
            &mut c.landing_page,
            &mut c.user_action,
            &mut c.shipping_preference,
            &mut c.exchange_rate,
        ] {
            *f = f.trim().to_owned();
        }
        c.base_url = c.base_url.trim().trim_end_matches('/').to_owned();
        if c.base_url.is_empty() {
            c.base_url = DEFAULT_BASE_URL.to_owned();
        }
        if c.user_action.is_empty() {
            c.user_action = "PAY_NOW".to_owned();
        }
        if c.shipping_preference.is_empty() {
            c.shipping_preference = "NO_SHIPPING".to_owned();
        }
        c.target_currency = c.target_currency.trim().to_ascii_uppercase();
        Ok(c)
    }

    /// `ValidateConfig`: `webhook_id` is optional on save (PAY-31) but required to verify.
    pub fn validate(&self) -> Result<(), GatewayError> {
        let invalid = |m: &str| Err(GatewayError::config(format!("paypal config invalid: {m}")));
        for (name, value) in [
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
            ("base_url", &self.base_url),
            ("return_url", &self.return_url),
            ("cancel_url", &self.cancel_url),
        ] {
            if value.is_empty() {
                return invalid(&format!("{name} is required"));
            }
        }
        for (name, value) in [
            ("base_url", &self.base_url),
            ("return_url", &self.return_url),
            ("cancel_url", &self.cancel_url),
        ] {
            if !is_request_uri(value) {
                return invalid(&format!("{name} is invalid"));
            }
        }
        Ok(())
    }
}

/// `marshalWebhookVerifyRequest`: metadata object with the raw event bytes embedded verbatim (PAY-28).
pub fn verify_request_body(meta: &[(&str, &str)], raw_event: &[u8]) -> Vec<u8> {
    let object: Map<String, Value> = meta
        .iter()
        .map(|(k, v)| ((*k).to_owned(), Value::String((*v).to_owned())))
        .collect();
    let encoded = go_json_ordered(&Value::Object(object));
    let mut body = encoded.trim_end_matches('}').as_bytes().to_vec();
    body.extend_from_slice(b",\"webhook_event\":");
    body.extend_from_slice(raw_event);
    body.push(b'}');
    body
}

/// Event type / resource status → payment status (`ToPaymentStatus`).
pub fn to_payment_status(event_type: &str, resource_status: &str) -> Option<PaymentStatus> {
    match event_type.trim().to_ascii_uppercase().as_str() {
        "PAYMENT.CAPTURE.COMPLETED" | "CHECKOUT.ORDER.COMPLETED" => {
            return Some(PaymentStatus::Success);
        }
        "PAYMENT.CAPTURE.DENIED"
        | "PAYMENT.CAPTURE.DECLINED"
        | "PAYMENT.CAPTURE.FAILED"
        | "CHECKOUT.ORDER.DENIED" => {
            return Some(PaymentStatus::Failed);
        }
        "PAYMENT.CAPTURE.PENDING" | "CHECKOUT.ORDER.APPROVED" => {
            return Some(PaymentStatus::Pending);
        }
        _ => {}
    }
    match resource_status.trim().to_ascii_uppercase().as_str() {
        "COMPLETED" => Some(PaymentStatus::Success),
        "DENIED" | "DECLINED" | "FAILED" | "VOIDED" => Some(PaymentStatus::Failed),
        "PENDING" | "APPROVED" | "CREATED" | "SAVED" => Some(PaymentStatus::Pending),
        _ => None,
    }
}

/// `CaptureAmount`: resource.amount → purchase_units[0].amount → first capture (PAY-30).
pub fn capture_amount(resource: &Value) -> (String, String) {
    let read = |base: &[&str]| {
        let path = |leaf: &'static str| {
            let mut p = base.to_vec();
            p.push(leaf);
            read_path(resource, &p).trim().to_owned()
        };
        (path("value"), path("currency_code"))
    };
    for base in [&["amount"][..], &["purchase_units", "0", "amount"][..]] {
        let (value, currency) = read(base);
        if !value.is_empty() && !currency.is_empty() {
            return (value, currency);
        }
    }
    read(&["purchase_units", "0", "payments", "captures", "0", "amount"])
}

fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// The PayPal adapter.
#[derive(Debug, Clone)]
pub struct PaypalGateway {
    env: GatewayEnv,
}

impl PaypalGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn checked(raw: &ChannelConfig) -> Result<Config, GatewayError> {
        let c = Config::parse(raw)?;
        c.validate()?;
        Ok(c)
    }

    async fn access_token(&self, cfg: &Config) -> Result<String, GatewayError> {
        let auth = B64.encode(format!("{}:{}", cfg.client_id, cfg.client_secret));
        let req = HttpRequest::new("POST", format!("{}/v1/oauth2/token", cfg.base_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Authorization", format!("Basic {auth}"))
            .body(encode_pairs(&[(
                "grant_type".into(),
                "client_credentials".into(),
            )]));
        let resp = self.env.http.send(req).await.map_err(|_| {
            GatewayError::AuthFailed("paypal auth failed: request token failed".into())
        })?;
        if !resp.is_success() {
            return Err(GatewayError::AuthFailed(format!(
                "paypal auth failed: token status {}",
                resp.status
            )));
        }
        let parsed = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::AuthFailed("paypal auth failed: decode token response failed".into())
        })?;
        let token = parsed
            .get("access_token")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        if token.is_empty() {
            return Err(GatewayError::AuthFailed(
                "paypal auth failed: access_token is empty".into(),
            ));
        }
        Ok(token)
    }

    async fn json_request(
        &self,
        cfg: &Config,
        method: &str,
        path: &str,
        token: &str,
        body: Vec<u8>,
    ) -> Result<(u16, Vec<u8>), GatewayError> {
        let req = HttpRequest::new(method, format!("{}{path}", cfg.base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {token}"))
            .body(body);
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|_| GatewayError::request("paypal request failed: http request failed"))?;
        Ok((resp.status, resp.body))
    }

    /// `VerifyWebhookSignature`: always required (PAY-30), raw bytes embedded (PAY-28).
    async fn verify_webhook(
        &self,
        cfg: &Config,
        headers: &Headers,
        raw_event: &[u8],
    ) -> Result<(), GatewayError> {
        if cfg.webhook_id.is_empty() {
            return Err(GatewayError::config(
                "paypal config invalid: webhook_id is required",
            ));
        }
        if raw_event.is_empty()
            || serde_json::from_slice::<serde::de::IgnoredAny>(raw_event).is_err()
        {
            return Err(GatewayError::signature(
                "paypal webhook verify failed: webhook event is invalid JSON",
            ));
        }
        let fields = [
            ("transmission_id", headers.get("Paypal-Transmission-Id")),
            ("transmission_time", headers.get("Paypal-Transmission-Time")),
            ("cert_url", headers.get("Paypal-Cert-Url")),
            ("auth_algo", headers.get("Paypal-Auth-Algo")),
            ("transmission_sig", headers.get("Paypal-Transmission-Sig")),
        ];
        if let Some((name, _)) = fields.iter().find(|(_, v)| v.is_empty()) {
            return Err(GatewayError::signature(format!(
                "paypal webhook verify failed: missing {name}"
            )));
        }
        let mut meta: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
        meta.push(("webhook_id", cfg.webhook_id.as_str()));
        let body = verify_request_body(&meta, raw_event);
        let token = self.access_token(cfg).await?;
        let (status, resp) = self
            .json_request(
                cfg,
                "POST",
                "/v1/notifications/verify-webhook-signature",
                &token,
                body,
            )
            .await?;
        if !(200..300).contains(&status) {
            return Err(GatewayError::signature(format!(
                "paypal webhook verify failed: verify status {status}"
            )));
        }
        let parsed = decode_object(&resp).ok_or_else(|| {
            GatewayError::signature("paypal webhook verify failed: decode verify response failed")
        })?;
        let verdict = parsed
            .get("verification_status")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !verdict.trim().eq_ignore_ascii_case("SUCCESS") {
            return Err(GatewayError::signature(
                "paypal webhook verify failed: verify result is not success",
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl PaymentGateway for PaypalGateway {
    fn key(&self) -> &'static str {
        "official:paypal"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            query: true,
            webhook: true,
            ..GatewayCapabilities::default()
        }
    }

    fn validate_config(&self, config: &ChannelConfig, _mode: &str) -> Result<(), GatewayError> {
        Self::checked(config).map(|_| ())
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let cfg = Self::checked(config)?;
        let original_amount = go_decimal_string(input.amount.decimal());
        let exchange = ExchangeRate::new(&cfg.target_currency, &cfg.exchange_rate);
        let (pay_amount, pay_currency) = exchange.convert(&original_amount, &input.currency, 2)?;
        let return_url = match input.return_url.trim() {
            "" => cfg.return_url.clone(),
            v => v.to_owned(),
        };
        let return_url = append_query_params(&return_url, &input.return_url_query);
        let cancel_url = match input.cancel_url.trim() {
            "" => cfg.cancel_url.clone(),
            v => v.to_owned(),
        };
        if input.order_no.trim().is_empty()
            || pay_amount.trim().is_empty()
            || pay_currency.trim().is_empty()
        {
            return Err(GatewayError::config(
                "paypal config invalid: order input is invalid",
            ));
        }
        let token = self.access_token(&cfg).await?;
        let mut context = Map::new();
        context.insert("return_url".into(), Value::String(return_url));
        context.insert("cancel_url".into(), Value::String(cancel_url));
        context.insert("user_action".into(), Value::String(cfg.user_action.clone()));
        context.insert(
            "shipping_preference".into(),
            Value::String(cfg.shipping_preference.clone()),
        );
        for (k, v) in [
            ("brand_name", &cfg.brand_name),
            ("locale", &cfg.locale),
            ("landing_page", &cfg.landing_page),
        ] {
            if !v.is_empty() {
                context.insert(k.into(), Value::String(v.clone()));
            }
        }
        let payload = serde_json::json!({
            "intent": "CAPTURE",
            "purchase_units": [{
                "invoice_id": input.order_no,
                "amount": {"currency_code": pay_currency.trim().to_ascii_uppercase(), "value": pay_amount.trim()},
                "description": input.subject.trim(),
            }],
            "application_context": Value::Object(context),
        });
        let (status, body) = self
            .json_request(
                &cfg,
                "POST",
                "/v2/checkout/orders",
                &token,
                go_json_map(&payload).into_bytes(),
            )
            .await?;
        if !(200..300).contains(&status) {
            return Err(GatewayError::response(format!(
                "paypal response invalid: create order status {status}"
            )));
        }
        let raw = decode_object(&body).ok_or_else(|| {
            GatewayError::response("paypal response invalid: decode response failed")
        })?;
        let order_id = raw
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let approve = raw
            .get("links")
            .and_then(Value::as_array)
            .and_then(|links| {
                links.iter().filter_map(Value::as_object).find_map(|l| {
                    let rel = l.get("rel").and_then(Value::as_str).unwrap_or_default();
                    let href = l
                        .get("href")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .trim();
                    (rel.trim().eq_ignore_ascii_case("approve") && !href.is_empty())
                        .then(|| href.to_owned())
                })
            })
            .unwrap_or_default();
        if order_id.is_empty() || approve.is_empty() {
            return Err(GatewayError::response(
                "paypal response invalid: missing order id or approve url",
            ));
        }
        let mut result_payload = raw;
        if exchange.needs_conversion() {
            exchange.audit(&mut result_payload, &original_amount, &input.currency);
        }
        Ok(GatewayCreateResult {
            provider_ref: order_id,
            redirect_url: approve,
            qr_code_url: String::new(),
            payload: result_payload,
            display_channel_type: String::new(),
            amount_sent: pay_amount,
            currency_sent: pay_currency,
        })
    }

    /// Capture (`CaptureOrder`) — PayPal's active "query" step.
    async fn query_payment(
        &self,
        config: &ChannelConfig,
        provider_ref: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        let cfg = Self::checked(config)?;
        let order_id = provider_ref.trim();
        if order_id.is_empty() {
            return Err(GatewayError::config(
                "paypal config invalid: order id is empty",
            ));
        }
        let token = self.access_token(&cfg).await?;
        let path = format!("/v2/checkout/orders/{}/capture", path_escape(order_id));
        let (status, body) = self
            .json_request(&cfg, "POST", &path, &token, b"{}".to_vec())
            .await?;
        if !(200..300).contains(&status) {
            return Err(GatewayError::response(format!(
                "paypal response invalid: capture status {status}"
            )));
        }
        let raw = decode_object(&body).ok_or_else(|| {
            GatewayError::response("paypal response invalid: decode response failed")
        })?;
        let value = Value::Object(raw.clone());
        let mut state = read_path(&value, &["status"]).trim().to_owned();
        let capture = ["purchase_units", "0", "payments", "captures", "0"];
        let with = |extra: &[&str]| {
            let mut p = capture.to_vec();
            p.extend_from_slice(extra);
            read_path(&value, &p).trim().to_owned()
        };
        let capture_status = with(&["status"]);
        if !capture_status.is_empty() {
            state = capture_status;
        }
        if state.is_empty() {
            return Err(GatewayError::response(
                "paypal response invalid: missing capture status",
            ));
        }
        let order_ref = first_non_empty(&[&read_path(&value, &["id"]), order_id]);
        Ok(GatewayQueryResult {
            provider_ref: order_ref,
            status: to_payment_status("", &state),
            amount: callback_amount(&with(&["amount", "value"])),
            currency: with(&["amount", "currency_code"]).to_ascii_uppercase(),
            paid_at: parse_time(&with(&["create_time"])),
            payload: raw,
        })
    }

    async fn parse_webhook(
        &self,
        config: &ChannelConfig,
        headers: &Headers,
        body: &[u8],
        _now: DateTime<Utc>,
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Self::checked(config)?;
        let event: Value = serde_json::from_slice(body)
            .map_err(|e| GatewayError::response(format!("webhook body not valid JSON: {e}")))?;
        self.verify_webhook(&cfg, headers, body).await?;
        let event_type = read_path(&event, &["event_type"]).trim().to_owned();
        if event_type.is_empty() {
            return Err(GatewayError::response(
                "paypal response invalid: event_type is missing",
            ));
        }
        let resource = match event.get("resource") {
            Some(Value::Object(r)) => Value::Object(r.clone()),
            _ => Value::Object(Map::new()),
        };
        let status = to_payment_status(&event_type, &read_path(&resource, &["status"]));
        let (value, currency) = capture_amount(&resource);
        let related_order = {
            let supp = read_path(
                &resource,
                &["supplementary_data", "related_ids", "order_id"],
            )
            .trim()
            .to_owned();
            if !supp.is_empty() {
                supp
            } else if event_type
                .to_ascii_uppercase()
                .starts_with("CHECKOUT.ORDER")
                && !read_path(&resource, &["id"]).trim().is_empty()
            {
                read_path(&resource, &["id"]).trim().to_owned()
            } else {
                read_path(&resource, &["order_id"]).trim().to_owned()
            }
        };
        let paid_at = ["create_time", "update_time"]
            .iter()
            .find_map(|k| parse_time(&read_path(&resource, &[k])))
            .or_else(|| parse_time(&read_path(&event, &["create_time"])));
        let mut payload = Map::new();
        payload.insert("event".into(), event.clone());
        Ok(GatewayCallbackResult {
            order_no: read_path(&resource, &["purchase_units", "0", "invoice_id"])
                .trim()
                .to_owned(),
            provider_ref: related_order,
            status,
            amount: callback_amount(&value),
            currency: currency.trim().to_ascii_uppercase(),
            paid_at,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::http::{HttpResponse, MockTransport};
    use crate::payment::test_support::{env_with, obj};
    use serde_json::json;
    use zs_shared::money::Amount;

    /// PAY-28: Go vector — metadata marshalled in struct order, raw event bytes embedded verbatim.
    #[test]
    fn pay_28_verify_body_matches_go() {
        let meta = [
            ("transmission_id", "tid"),
            ("transmission_time", "2026-09-24T10:00:00Z"),
            ("cert_url", "https://api.paypal.com/cert?a=1&b=<2>"),
            ("auth_algo", "SHA256withRSA"),
            ("transmission_sig", "sig=="),
            ("webhook_id", "WH1"),
        ];
        let raw = r#"{"id":"E1","amount":10.50,"name":"测试<"}"#;
        assert_eq!(
            String::from_utf8_lossy(&verify_request_body(&meta, raw.as_bytes())),
            r#"{"transmission_id":"tid","transmission_time":"2026-09-24T10:00:00Z","cert_url":"https://api.paypal.com/cert?a=1\u0026b=\u003c2\u003e","auth_algo":"SHA256withRSA","transmission_sig":"sig==","webhook_id":"WH1","webhook_event":{"id":"E1","amount":10.50,"name":"测试<"}}"#
        );
    }

    fn cfg(webhook_id: &str) -> ChannelConfig {
        obj(
            json!({"client_id": "cid", "client_secret": "sec", "base_url": "https://pp.example.com",
            "return_url": "https://shop/pay", "cancel_url": "https://shop/cancel", "webhook_id": webhook_id}),
        )
    }

    fn pp_headers() -> Headers {
        Headers::from([
            ("Paypal-Transmission-Id", "tid"),
            ("Paypal-Transmission-Time", "2026-09-24T10:00:00Z"),
            ("Paypal-Cert-Url", "https://api.paypal.com/cert"),
            ("Paypal-Auth-Algo", "SHA256withRSA"),
            ("Paypal-Transmission-Sig", "sig=="),
        ])
    }

    fn mock(verdict: &'static str) -> MockTransport {
        MockTransport::new(move |req| {
            if req.url.ends_with("/v1/oauth2/token") {
                Ok(HttpResponse::new(200, r#"{"access_token":"tok"}"#))
            } else {
                Ok(HttpResponse::new(
                    200,
                    format!(r#"{{"verification_status":"{verdict}"}}"#),
                ))
            }
        })
    }

    const EVENT: &str = r#"{"id":"WH-1","event_type":"PAYMENT.CAPTURE.COMPLETED","create_time":"2026-09-24T10:00:00Z","resource":{"id":"CAP1","status":"COMPLETED","amount":{"value":"10.50","currency_code":"usd"},"supplementary_data":{"related_ids":{"order_id":"ORDER1"}}}}"#;

    /// PAY-28 / PAY-30: verified capture event; raw bytes forwarded; verification failures rejected.
    #[tokio::test]
    async fn pay_30_webhook_verification() {
        let ok = mock("SUCCESS");
        let gw = PaypalGateway::new(env_with(ok.clone()));
        let res = gw
            .parse_webhook(&cfg("WH1"), &pp_headers(), EVENT.as_bytes(), Utc::now())
            .await
            .unwrap_or_default();
        assert_eq!(res.status, Some(PaymentStatus::Success));
        assert_eq!(res.amount, Amount::from_cents(1050));
        assert_eq!(res.currency, "USD");
        assert_eq!(res.provider_ref, "ORDER1");
        let verify = &ok.requests()[1];
        assert!(
            String::from_utf8_lossy(&verify.body)
                .ends_with(&format!(",\"webhook_event\":{EVENT}}}"))
        );
        assert_eq!(verify.header_value("Authorization"), Some("Bearer tok"));
        let gw = PaypalGateway::new(env_with(mock("FAILURE")));
        assert!(matches!(
            gw.parse_webhook(&cfg("WH1"), &pp_headers(), EVENT.as_bytes(), Utc::now())
                .await,
            Err(GatewayError::SignatureInvalid(_))
        ));
        let quiet = mock("SUCCESS");
        let gw = PaypalGateway::new(env_with(quiet.clone()));
        let missing = Headers::from([("Paypal-Transmission-Id", "tid")]);
        assert!(
            gw.parse_webhook(&cfg("WH1"), &missing, EVENT.as_bytes(), Utc::now())
                .await
                .is_err()
        );
        assert!(
            quiet.requests().is_empty(),
            "missing headers must not reach PayPal"
        );
    }

    /// PAY-31: saving without webhook_id is allowed; webhooks are then rejected as config errors.
    #[tokio::test]
    async fn pay_31_webhook_id_required_only_for_verification() {
        let gw = PaypalGateway::new(env_with(mock("SUCCESS")));
        assert!(gw.validate_config(&cfg(""), "redirect").is_ok());
        assert!(matches!(
            gw.parse_webhook(&cfg(""), &pp_headers(), EVENT.as_bytes(), Utc::now())
                .await,
            Err(GatewayError::ConfigInvalid(_))
        ));
    }

    /// PAY-30: amount extraction fallbacks and status mapping.
    #[test]
    fn pay_30_amount_fallbacks() {
        let r = json!({"purchase_units": [{"amount": {"value": "88.66", "currency_code": "USD"}}]});
        assert_eq!(capture_amount(&r), ("88.66".into(), "USD".into()));
        let r = json!({"purchase_units": [{"payments": {"captures": [{"amount": {"value": "1", "currency_code": "EUR"}}]}}]});
        assert_eq!(capture_amount(&r), ("1".into(), "EUR".into()));
        assert_eq!(
            capture_amount(&json!({"status": "COMPLETED"})),
            (String::new(), String::new())
        );
        assert_eq!(
            to_payment_status("CHECKOUT.ORDER.APPROVED", ""),
            Some(PaymentStatus::Pending)
        );
        assert_eq!(
            to_payment_status("X", "voided"),
            Some(PaymentStatus::Failed)
        );
        assert_eq!(to_payment_status("X", "?"), None);
    }

    /// PAY-29: Go vector for the create payload plus conversion (72 CNY × 0.1389 → 10 USD).
    #[tokio::test]
    async fn pay_29_create_order_with_conversion() {
        let m = MockTransport::new(|req| {
            if req.url.ends_with("/v1/oauth2/token") {
                Ok(HttpResponse::new(200, r#"{"access_token":"tok"}"#))
            } else {
                Ok(HttpResponse::new(
                    201,
                    r#"{"id":"ORDER1","status":"CREATED","links":[{"rel":"approve","href":"https://pp/approve"}]}"#,
                ))
            }
        });
        let gw = PaypalGateway::new(env_with(m.clone()));
        let mut c = cfg("WH1");
        c.insert("target_currency".into(), json!("usd"));
        c.insert("exchange_rate".into(), json!("0.1389"));
        c.insert("brand_name".into(), json!("Shop&Co"));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            subject: "DJ1".into(),
            amount: Amount::from(72),
            currency: "CNY".into(),
            return_url: "https://r/pay?x=1&y=2".into(),
            cancel_url: "https://r/cancel".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&c, &input).await.unwrap_or_default();
        assert_eq!(
            (res.amount_sent.as_str(), res.currency_sent.as_str()),
            ("10", "USD")
        );
        assert_eq!(res.redirect_url, "https://pp/approve");
        assert_eq!(
            String::from_utf8_lossy(&m.requests()[1].body),
            r#"{"application_context":{"brand_name":"Shop\u0026Co","cancel_url":"https://r/cancel","return_url":"https://r/pay?x=1\u0026y=2","shipping_preference":"NO_SHIPPING","user_action":"PAY_NOW"},"intent":"CAPTURE","purchase_units":[{"amount":{"currency_code":"USD","value":"10"},"description":"DJ1","invoice_id":"DJP1"}]}"#
        );
    }
}
