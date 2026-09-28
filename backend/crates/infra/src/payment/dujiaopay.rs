//! DujiaoPay gateway: HMAC-signed API requests and webhooks (PAY-48, PAY-49).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{encode_pairs, parse_query};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, Headers, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{
    InteractionMode, PAYLOAD_FIAT_CURRENCY_SENT, PaymentStatus, SITE_CURRENCY_DEFAULT, order_mode,
    provider,
};
use zs_shared::money::Amount;

use super::common::{
    GatewayEnv, bytes_eq, first_non_empty, go_decimal_string, go_json_map, hmac_sha256,
    parse_config, parse_decimal, read_string,
};
use super::http::HttpRequest;

const CREATE_ORDER_PATH: &str = "/v1/orders";
/// Accepted webhook clock skew (5 minutes).
const WEBHOOK_TOLERANCE_SECS: i64 = 300;
const NONCE_BYTES: usize = 16;

/// Built-in `token_id` → chain table (`supportedTokenChains`).
const TOKEN_CHAINS: [(&str, &str); 19] = [
    ("tron-trx", "tron"),
    ("tron-usdt", "tron"),
    ("ethereum-eth", "ethereum"),
    ("ethereum-usdt", "ethereum"),
    ("ethereum-usdc", "ethereum"),
    ("bsc-bnb", "bsc"),
    ("bsc-usdt", "bsc"),
    ("bsc-usdc", "bsc"),
    ("polygon-usdc", "polygon"),
    ("polygon-usdt0", "polygon"),
    ("base-usdc", "base"),
    ("arbitrum-usdc", "arbitrum"),
    ("arbitrum-usdt0", "arbitrum"),
    ("plasma-usdt0", "plasma"),
    ("x-layer-usdt0", "x-layer"),
    ("solana-usdc", "solana"),
    ("solana-usdt", "solana"),
    ("aptos-usdc", "aptos"),
    ("aptos-usdt", "aptos"),
];

/// `ResolveChain`: built-in table, else the `<chain>-<token>` naming convention.
pub fn resolve_chain(token_id: &str) -> String {
    let token_id = token_id.trim().to_ascii_lowercase();
    if let Some((_, chain)) = TOKEN_CHAINS.iter().find(|(t, _)| *t == token_id) {
        return (*chain).to_owned();
    }
    match token_id.rfind('-') {
        Some(idx) if idx > 0 && idx < token_id.len() - 1 => token_id[..idx].to_owned(),
        _ => String::new(),
    }
}

/// DujiaoPay channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api_base_url: String,
    pub api_key_id: String,
    pub api_secret: String,
    pub webhook_secret: String,
    pub order_mode: String,
    pub chain: String,
    pub token_id: String,
    pub allowed_methods: String,
    pub fiat_currency: String,
    pub success_url: String,
    pub cancel_url: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "dujiaopay")?;
        c.normalize();
        Ok(c)
    }

    fn normalize(&mut self) {
        self.api_base_url = self.api_base_url.trim().trim_end_matches('/').to_owned();
        self.api_key_id = self.api_key_id.trim().to_owned();
        self.api_secret = self.api_secret.trim().to_owned();
        self.webhook_secret = self.webhook_secret.trim().to_owned();
        self.order_mode = self.order_mode.trim().to_ascii_lowercase();
        self.chain = self.chain.trim().to_ascii_lowercase();
        self.token_id = self.token_id.trim().to_ascii_lowercase();
        self.allowed_methods = self
            .allowed_methods
            .trim()
            .replace(' ', "")
            .to_ascii_lowercase();
        self.fiat_currency = self.fiat_currency.trim().to_ascii_uppercase();
        self.success_url = self.success_url.trim().to_owned();
        self.cancel_url = self.cancel_url.trim().to_owned();
        if self.order_mode.is_empty() {
            self.order_mode = order_mode::TRANSACTION.to_owned();
        }
        if self.order_mode == order_mode::CASHIER {
            self.chain.clear();
            self.token_id.clear();
        } else {
            self.allowed_methods.clear();
        }
        if self.fiat_currency.is_empty() {
            self.fiat_currency = SITE_CURRENCY_DEFAULT.to_owned();
        }
        if self.chain.is_empty() && !self.token_id.is_empty() {
            self.chain = resolve_chain(&self.token_id);
        }
    }

    fn is_cashier(&self) -> bool {
        self.order_mode == order_mode::CASHIER
    }

    /// De-duplicated cashier `allowed_methods` in configured order.
    pub fn allowed_method_list(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for item in self.allowed_methods.split(',') {
            let t = item.trim().to_ascii_lowercase();
            if !t.is_empty() && !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    pub fn validate(&mut self) -> Result<(), GatewayError> {
        self.normalize();
        for (name, value) in [
            ("api_base_url", &self.api_base_url),
            ("api_key_id", &self.api_key_id),
            ("api_secret", &self.api_secret),
            ("webhook_secret", &self.webhook_secret),
            ("fiat_currency", &self.fiat_currency),
        ] {
            if value.is_empty() {
                return Err(GatewayError::config(format!(
                    "dujiaopay config invalid: {name} is required"
                )));
            }
        }
        if self.order_mode != order_mode::TRANSACTION && !self.is_cashier() {
            return Err(GatewayError::config(
                "dujiaopay config invalid: order_mode is invalid",
            ));
        }
        if self.is_cashier() {
            for token in self.allowed_method_list() {
                if resolve_chain(&token).is_empty() {
                    return Err(GatewayError::UnsupportedChannel(format!(
                        "dujiaopay token unsupported: {token}"
                    )));
                }
            }
            return Ok(());
        }
        if self.token_id.is_empty() {
            return Err(GatewayError::config(
                "dujiaopay config invalid: token_id is required",
            ));
        }
        if self.chain.is_empty() {
            return Err(GatewayError::UnsupportedChannel(format!(
                "dujiaopay token unsupported: {}",
                self.token_id
            )));
        }
        Ok(())
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "mirrors the original SignHeaders signature"
)]
/// `SignHeaders`: canonical `METHOD\npath\nquery\nsha256(body)\ntimestamp\nnonce`, HMAC hex.
pub fn sign_headers(
    secret: &str,
    key_id: &str,
    method: &str,
    path: &str,
    raw_query: &str,
    body: &[u8],
    unix: i64,
    nonce: &str,
) -> Vec<(String, String)> {
    let path = path.trim();
    let path = if path.is_empty() {
        "/".to_owned()
    } else if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    let raw_query = raw_query.trim();
    let query = if raw_query.is_empty() {
        String::new()
    } else {
        match parse_query(raw_query) {
            (pairs, None) => encode_pairs(&pairs),
            (_, Some(_)) => raw_query.to_owned(),
        }
    };
    let canonical = [
        method.trim().to_ascii_uppercase(),
        path,
        query,
        hex::encode(Sha256::digest(body)),
        unix.to_string(),
        nonce.trim().to_owned(),
    ]
    .join("\n");
    vec![
        ("DJP-Key-ID".into(), key_id.trim().to_owned()),
        ("DJP-Timestamp".into(), unix.to_string()),
        ("DJP-Nonce".into(), nonce.trim().to_owned()),
        (
            "DJP-Signature".into(),
            hex::encode(hmac_sha256(secret.as_bytes(), canonical.as_bytes())),
        ),
    ]
}

fn check_channel_type_for_mode(cfg: &Config, channel: &str) -> Result<(), GatewayError> {
    if channel.is_empty() {
        return Ok(());
    }
    if cfg.is_cashier() {
        if channel != provider::DUJIAOPAY {
            return Err(GatewayError::UnsupportedChannel(format!(
                "dujiaopay cashier channel_type {channel}"
            )));
        }
        return Ok(());
    }
    if channel == provider::DUJIAOPAY {
        return Err(GatewayError::UnsupportedChannel(
            "dujiaopay transaction channel_type requires token_id".into(),
        ));
    }
    Ok(())
}

fn envelope_str(map: &Map<String, Value>, key: &str) -> Result<String, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.trim().to_owned()),
        Some(_) => Err(GatewayError::response(
            "dujiaopay response invalid: decode webhook failed",
        )),
    }
}

fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// The DujiaoPay adapter.
#[derive(Debug, Clone)]
pub struct DujiaoPayGateway {
    env: GatewayEnv,
}

impl DujiaoPayGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn parse_for(raw: &ChannelConfig, channel: &str) -> Result<Config, GatewayError> {
        let mut cfg = Config::parse(raw)?;
        let channel = channel.trim().to_ascii_lowercase();
        if cfg.token_id.is_empty() && !channel.is_empty() && channel != provider::DUJIAOPAY {
            cfg.token_id = channel;
        }
        if cfg.chain.is_empty() && !cfg.token_id.is_empty() {
            cfg.chain = resolve_chain(&cfg.token_id);
        }
        cfg.validate()?;
        Ok(cfg)
    }
}

#[async_trait]
impl PaymentGateway for DujiaoPayGateway {
    fn key(&self) -> &'static str {
        "dujiaopay:"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            webhook: true,
            ..GatewayCapabilities::default()
        }
    }

    fn validate_config(
        &self,
        config: &ChannelConfig,
        channel_type: &str,
    ) -> Result<(), GatewayError> {
        let channel = channel_type.trim().to_ascii_lowercase();
        let cfg = Self::parse_for(config, &channel)?;
        check_channel_type_for_mode(&cfg, &channel)
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let channel = input.channel_type.trim().to_ascii_lowercase();
        let mut cfg = Self::parse_for(config, &channel)?;
        check_channel_type_for_mode(&cfg, &channel)?;
        let qr = input.interaction_mode == Some(InteractionMode::Qr);
        if cfg.is_cashier() && qr {
            return Err(GatewayError::config(
                "dujiaopay cashier order mode only supports redirect interaction_mode",
            ));
        }
        let raw_fiat = config
            .get("fiat_currency")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if raw_fiat.trim().is_empty() && !input.currency.trim().is_empty() {
            cfg.fiat_currency = input.currency.trim().to_ascii_uppercase();
        }
        let success_url = first_non_empty(&[&input.return_url, &cfg.success_url]);
        let success_url = append_query_params(&success_url, &input.return_url_query);
        let cancel_url = match input.cancel_url.trim() {
            "" => cfg.cancel_url.clone(),
            v => v.to_owned(),
        };
        let mut metadata = Map::new();
        if input.payment_id > 0 {
            metadata.insert("payment_id".into(), Value::from(input.payment_id));
        }
        if input.order_id > 0 {
            metadata.insert("order_id".into(), Value::from(input.order_id));
        }
        if !input.subject.is_empty() {
            metadata.insert("subject".into(), Value::String(input.subject.clone()));
        }
        cfg.validate()?;
        let merchant_order_id = input.order_no.trim().to_owned();
        let fiat_amount = go_decimal_string(input.amount.decimal());
        if merchant_order_id.is_empty() || fiat_amount.is_empty() {
            return Err(GatewayError::config(
                "dujiaopay config invalid: merchant_order_id and fiat_amount are required",
            ));
        }
        let mut payload = Map::new();
        payload.insert(
            "fiat_currency".into(),
            Value::String(cfg.fiat_currency.clone()),
        );
        payload.insert("fiat_amount".into(), Value::String(fiat_amount.clone()));
        payload.insert(
            "merchant_order_id".into(),
            Value::String(merchant_order_id.clone()),
        );
        if cfg.is_cashier() {
            let methods = cfg.allowed_method_list();
            if !methods.is_empty() {
                let allowed: Vec<Value> = methods
                    .iter()
                    .map(|t| serde_json::json!({"chain": resolve_chain(t), "token_id": t}))
                    .collect();
                payload.insert("allowed_methods".into(), Value::Array(allowed));
            }
        } else {
            payload.insert("chain".into(), Value::String(cfg.chain.clone()));
            payload.insert("token_id".into(), Value::String(cfg.token_id.clone()));
        }
        if !success_url.is_empty() {
            payload.insert("success_url".into(), Value::String(success_url));
        }
        if !cancel_url.is_empty() {
            payload.insert("cancel_url".into(), Value::String(cancel_url));
        }
        if !metadata.is_empty() {
            payload.insert("metadata".into(), Value::Object(metadata));
        }
        let body = go_json_map(&Value::Object(payload));
        let nonce = self.env.random_hex(NONCE_BYTES);
        let mut req = HttpRequest::new("POST", format!("{}{CREATE_ORDER_PATH}", cfg.api_base_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .header("Idempotency-Key", merchant_order_id.clone());
        for (k, v) in sign_headers(
            &cfg.api_secret,
            &cfg.api_key_id,
            "POST",
            CREATE_ORDER_PATH,
            "",
            body.as_bytes(),
            self.env.clock.now().timestamp(),
            &nonce,
        ) {
            req = req.header(&k, v);
        }
        let resp = self
            .env
            .http
            .send(req.body(body))
            .await
            .map_err(|e| GatewayError::request(format!("dujiaopay request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::request(format!(
                "dujiaopay request failed: status={} body={}",
                resp.status,
                String::from_utf8_lossy(&resp.body).trim()
            )));
        }
        let raw = super::common::decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("dujiaopay response invalid: decode response failed")
        })?;
        let source = match raw.get("data") {
            Some(Value::Object(d)) => d.clone(),
            _ => raw.clone(),
        };
        let field = |k: &str| read_string(&source, k);
        let (order_id, checkout_url) = (field("order_id"), field("checkout_url"));
        if order_id.is_empty() || checkout_url.is_empty() {
            return Err(GatewayError::response(
                "dujiaopay response invalid: missing order_id/checkout_url",
            ));
        }
        let qr_code_url = if qr {
            first_non_empty(&[&field("pay_address"), &checkout_url])
        } else {
            checkout_url.clone()
        };
        let mut result_payload = raw.clone();
        for key in [
            "order_id",
            "chain",
            "token_id",
            "pay_address",
            "payable_amount",
            "checkout_url",
        ] {
            let v = field(key);
            if !v.trim().is_empty() {
                result_payload.insert(key.into(), Value::String(v.trim().to_owned()));
            }
        }
        result_payload.insert(
            PAYLOAD_FIAT_CURRENCY_SENT.into(),
            Value::String(cfg.fiat_currency.clone()),
        );
        Ok(GatewayCreateResult {
            provider_ref: order_id,
            redirect_url: checkout_url,
            qr_code_url,
            payload: result_payload,
            display_channel_type: String::new(),
            amount_sent: fiat_amount,
            currency_sent: cfg.fiat_currency,
        })
    }

    async fn parse_webhook(
        &self,
        config: &ChannelConfig,
        headers: &Headers,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Config::parse(config)?;
        if cfg.webhook_secret.is_empty() {
            return Err(GatewayError::config(
                "dujiaopay config invalid: webhook_secret is required",
            ));
        }
        let timestamp = headers.get("DJP-Webhook-Timestamp");
        let signature = headers.get("DJP-Webhook-Signature");
        if timestamp.is_empty() || signature.is_empty() {
            return Err(GatewayError::signature("missing webhook signature headers"));
        }
        let ts: i64 = timestamp
            .parse()
            .map_err(|_| GatewayError::signature("invalid webhook timestamp"))?;
        if (now.timestamp() - ts).abs() > WEBHOOK_TOLERANCE_SECS {
            return Err(GatewayError::signature(
                "webhook timestamp outside tolerance",
            ));
        }
        let mut signed = timestamp.clone().into_bytes();
        signed.push(b'.');
        signed.extend_from_slice(body);
        let want = hmac_sha256(cfg.webhook_secret.as_bytes(), &signed);
        let got = hex::decode(signature.trim().trim_start_matches("sha256=")).unwrap_or_default();
        if !bytes_eq(&got, &want) {
            return Err(GatewayError::signature("webhook signature mismatch"));
        }
        let raw = super::common::decode_object(body).ok_or_else(|| {
            GatewayError::response("dujiaopay response invalid: decode webhook failed")
        })?;
        let data = match raw.get("data") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(d)) => d.clone(),
            Some(_) => {
                return Err(GatewayError::response(
                    "dujiaopay response invalid: decode webhook failed",
                ));
            }
        };
        let mut event_id = envelope_str(&raw, "event_id")?;
        let event_type = envelope_str(&raw, "event_type")?;
        envelope_str(&raw, "event_version")?;
        let created_at = parse_time(&envelope_str(&raw, "created_at")?);
        let order_id = envelope_str(&data, "order_id")?;
        let merchant_order_id = envelope_str(&data, "merchant_order_id")?;
        let fiat_currency = envelope_str(&data, "fiat_currency")?.to_ascii_uppercase();
        let fiat_amount = envelope_str(&data, "fiat_amount")?;
        let tx_id = envelope_str(&data, "tx_id")?;
        let tx_hash = envelope_str(&data, "tx_hash")?;
        let mut paid_at = parse_time(&envelope_str(&data, "paid_at")?);
        if paid_at.is_none() && event_type == "order.paid" {
            paid_at = created_at;
        }
        if event_id.is_empty() {
            event_id = headers.get("DJP-Webhook-ID");
        }
        let status = match event_type.as_str() {
            "order.paid" => Some(PaymentStatus::Success),
            "order.expired" => Some(PaymentStatus::Expired),
            "order.canceled" => Some(PaymentStatus::Failed),
            _ => None,
        };
        if status.is_some()
            && (event_id.is_empty() || order_id.is_empty() || merchant_order_id.is_empty())
        {
            return Err(GatewayError::response(
                "missing event_id/order_id/merchant_order_id",
            ));
        }
        let transaction_id = if tx_id.is_empty() { tx_hash } else { tx_id };
        let mut payload = raw;
        if !transaction_id.is_empty() {
            payload.insert("tx_id".into(), Value::String(transaction_id.clone()));
            payload.insert("tx_hash".into(), Value::String(transaction_id));
        }
        let amount = if fiat_amount.is_empty() {
            Amount::ZERO
        } else {
            Amount::new(
                parse_decimal(&fiat_amount)
                    .ok_or_else(|| GatewayError::response("invalid fiat_amount"))?,
            )
        };
        Ok(GatewayCallbackResult {
            order_no: merchant_order_id,
            provider_ref: order_id,
            status,
            amount,
            currency: fiat_currency,
            paid_at,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::http::MockTransport;
    use crate::payment::test_support::{NOW_UNIX, env_with, obj};
    use chrono::TimeZone;
    use serde_json::json;

    /// Go `SignHeaders` vectors.
    #[test]
    fn pay_48_sign_headers_match_go() {
        let body = r#"{"chain":"tron","fiat_amount":"9.9","fiat_currency":"CNY","merchant_order_id":"DJP1","metadata":{"order_id":3,"payment_id":5,"subject":"DJ1 订单"},"success_url":"https://r/pay?a=1\u0026b=\u003c2\u003e","token_id":"tron-usdt"}"#;
        let h = sign_headers(
            "secret",
            "kid",
            "POST",
            "/v1/orders",
            "",
            body.as_bytes(),
            NOW_UNIX,
            "nonce123",
        );
        assert_eq!(
            h[3].1,
            "0c706b4a680561ef7a10f591d20b46787ee343f947a91f353650363a1133760c"
        );
        let h2 = sign_headers(
            "secret",
            "kid",
            "get",
            "v1/orders",
            "b=2&a=1 2",
            b"",
            NOW_UNIX,
            "n",
        );
        assert_eq!(
            h2[3].1,
            "c6b512b09613484853b91cf69268be7ac213a3344785aef6fa103352479067b2"
        );
    }

    fn cfg(extra: Value) -> ChannelConfig {
        let mut c = obj(
            json!({"api_base_url": "https://djp.example.com/", "api_key_id": "kid", "api_secret": "secret",
            "webhook_secret": "whsec"}),
        );
        c.extend(obj(extra));
        c
    }

    /// Go vector: create body is the sorted, HTML-escaped `json.Marshal` of the payload map.
    #[tokio::test]
    async fn create_body_matches_go() {
        let mock = MockTransport::fixed(
            200,
            r#"{"data":{"order_id":"djp_1","checkout_url":"https://djp/c/1","pay_address":"TAddr","payable_amount":"1.38"}}"#,
        );
        let gw = DujiaoPayGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            payment_id: 5,
            order_id: 3,
            order_no: "DJP1".into(),
            subject: "DJ1 订单".into(),
            amount: Amount::from_cents(990),
            currency: "CNY".into(),
            return_url: "https://r/pay?a=1&b=<2>".into(),
            channel_type: "tron-usdt".into(),
            interaction_mode: Some(InteractionMode::Qr),
            ..GatewayCreateInput::default()
        };
        let res = gw
            .create_payment(&cfg(json!({})), &input)
            .await
            .unwrap_or_default();
        let req = &mock.requests()[0];
        assert_eq!(
            String::from_utf8_lossy(&req.body),
            r#"{"chain":"tron","fiat_amount":"9.9","fiat_currency":"CNY","merchant_order_id":"DJP1","metadata":{"order_id":3,"payment_id":5,"subject":"DJ1 订单"},"success_url":"https://r/pay?a=1\u0026b=\u003c2\u003e","token_id":"tron-usdt"}"#
        );
        assert_eq!(req.header_value("Idempotency-Key"), Some("DJP1"));
        assert_eq!(req.header_value("DJP-Timestamp"), Some("1782680000"));
        assert_eq!(res.qr_code_url, "TAddr");
        assert_eq!(res.provider_ref, "djp_1");
        assert_eq!(res.payload[PAYLOAD_FIAT_CURRENCY_SENT], "CNY");
    }

    /// PAY-49: cashier mode rules and de-duplicated allowed methods.
    #[tokio::test]
    async fn pay_49_cashier_mode() {
        let mock = MockTransport::fixed(
            200,
            r#"{"order_id":"djp_2","checkout_url":"https://djp/c/2"}"#,
        );
        let gw = DujiaoPayGateway::new(env_with(mock.clone()));
        let cashier = cfg(
            json!({"order_mode": "cashier", "allowed_methods": "tron-usdt, base-usdc,tron-usdt", "fiat_currency": "USD"}),
        );
        let input = GatewayCreateInput {
            order_no: "DJP2".into(),
            amount: Amount::from(10),
            channel_type: "dujiaopay".into(),
            ..GatewayCreateInput::default()
        };
        gw.create_payment(&cashier, &input)
            .await
            .unwrap_or_default();
        let body: Value = serde_json::from_slice(&mock.requests()[0].body).unwrap_or_default();
        assert_eq!(
            body["allowed_methods"],
            json!([{"chain": "tron", "token_id": "tron-usdt"}, {"chain": "base", "token_id": "base-usdc"}])
        );
        assert!(body.get("chain").is_none());
        let qr = GatewayCreateInput {
            interaction_mode: Some(InteractionMode::Qr),
            ..input.clone()
        };
        assert!(matches!(
            gw.create_payment(&cashier, &qr).await,
            Err(GatewayError::ConfigInvalid(_))
        ));
        assert!(matches!(
            gw.validate_config(&cashier, "tron-usdt"),
            Err(GatewayError::UnsupportedChannel(_))
        ));
        assert!(matches!(
            gw.validate_config(&cfg(json!({"token_id": "tron-usdt"})), "dujiaopay"),
            Err(GatewayError::UnsupportedChannel(_))
        ));
        assert!(
            gw.validate_config(
                &cfg(json!({"order_mode": "cashier", "allowed_methods": "bogus"})),
                "dujiaopay"
            )
            .is_err()
        );
        assert_eq!(resolve_chain("x-layer-usdt0"), "x-layer");
        assert_eq!(resolve_chain("foo-bar-baz"), "foo-bar");
        assert_eq!(resolve_chain("usdt"), "");
    }

    fn webhook(ts: i64, body: &str, secret: &str, prefix: bool) -> Headers {
        let mut msg = ts.to_string().into_bytes();
        msg.push(b'.');
        msg.extend_from_slice(body.as_bytes());
        let sig = hex::encode(hmac_sha256(secret.as_bytes(), &msg));
        let sig = if prefix { format!("sha256={sig}") } else { sig };
        Headers::from([
            ("DJP-Webhook-Timestamp", ts.to_string().as_str()),
            ("DJP-Webhook-Signature", sig.as_str()),
        ])
    }

    /// PAY-48: signature, prefix, tolerance and tamper checks.
    #[tokio::test]
    async fn pay_48_webhook_verification() {
        let gw = DujiaoPayGateway::new(env_with(MockTransport::fixed(500, "")));
        let now = Utc.timestamp_opt(NOW_UNIX, 0).single().unwrap_or_default();
        let body = r#"{"event_id":"e1","event_type":"order.paid","created_at":"2026-09-24T10:00:00Z","data":{"order_id":"djp_1","merchant_order_id":"DJP1","fiat_currency":"cny","fiat_amount":"9.90","tx_hash":"0xabc"}}"#;
        let ok = gw
            .parse_webhook(
                &cfg(json!({})),
                &webhook(NOW_UNIX, body, "whsec", true),
                body.as_bytes(),
                now,
            )
            .await
            .unwrap_or_default();
        assert_eq!(ok.status, Some(PaymentStatus::Success));
        assert_eq!(ok.amount, Amount::from_cents(990));
        assert_eq!(ok.currency, "CNY");
        assert_eq!(ok.payload["tx_id"], "0xabc");
        assert_eq!(
            ok.paid_at.map(|t| t.to_rfc3339()),
            Some("2026-09-24T10:00:00+00:00".into())
        );
        assert!(
            gw.parse_webhook(
                &cfg(json!({})),
                &webhook(NOW_UNIX, body, "whsec", false),
                body.as_bytes(),
                now
            )
            .await
            .is_ok()
        );
        let stale = gw
            .parse_webhook(
                &cfg(json!({})),
                &webhook(NOW_UNIX - 301, body, "whsec", false),
                body.as_bytes(),
                now,
            )
            .await;
        assert!(matches!(stale, Err(GatewayError::SignatureInvalid(_))));
        let tampered = body.replace("9.90", "9.91");
        let r = gw
            .parse_webhook(
                &cfg(json!({})),
                &webhook(NOW_UNIX, body, "whsec", false),
                tampered.as_bytes(),
                now,
            )
            .await;
        assert!(matches!(r, Err(GatewayError::SignatureInvalid(_))));
        let go_sig = Headers::from([
            ("DJP-Webhook-Timestamp", "1782680000"),
            (
                "DJP-Webhook-Signature",
                "63c85416db72d4aa9e80b6d9e0825d9606da37c475bfda84e403d2d4efbd0dbf",
            ),
        ]);
        let unknown = gw
            .parse_webhook(&cfg(json!({})), &go_sig, br#"{"event_id":"e1"}"#, now)
            .await;
        assert_eq!(unknown.map(|r| r.status), Ok(None));
    }
}
