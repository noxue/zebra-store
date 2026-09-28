//! epusdt (GMPay) gateway: HMAC-SHA256 signatures (PAY-41), shared JSON callback.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::FormMap;
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{PaymentStatus, SITE_CURRENCY_DEFAULT, order_mode};
use zs_shared::money::Amount;

use super::common::{
    GatewayEnv, callback_amount, decode_object, go_decimal_string, go_float_f, go_json_map,
    hmac_sha256, json_float, parse_config, signature_eq,
};
use super::http::HttpRequest;

const CREATE_PATH: &str = "/payments/gmpay/v1/order/create-transaction";
const CHECKOUT_PREFIX: &str = "/pay/checkout-counter/";
/// Gateway status codes.
const STATUS_SUCCESS: i64 = 2;
const STATUS_EXPIRED: i64 = 3;
/// epusdt rejects merchant order numbers longer than this.
const MAX_ORDER_NO_LEN: usize = 32;
/// Minimum payable amount (exclusive).
const MIN_AMOUNT: f64 = 0.01;

/// A value participating in a signature (Go `interface{}` in the params map).
#[derive(Debug, Clone, PartialEq)]
pub enum SignValue {
    Str(String),
    Float(f64),
    Int(i64),
}

impl SignValue {
    fn is_empty(&self) -> bool {
        matches!(self, Self::Str(s) if s.trim().is_empty())
    }

    /// `formatSignValue`: floats via `FormatFloat('f', -1)`.
    fn format_f(&self) -> String {
        match self {
            Self::Str(s) => s.clone(),
            Self::Float(f) => go_float_f(*f),
            Self::Int(i) => i.to_string(),
        }
    }

    fn to_json(&self) -> Value {
        match self {
            Self::Str(s) => Value::String(s.clone()),
            Self::Float(f) => json_float(*f),
            Self::Int(i) => Value::from(*i),
        }
    }
}

/// Sorted `k=v` of non-empty params except `signature`.
pub fn sign_base(params: &[(&str, SignValue)], format: impl Fn(&SignValue) -> String) -> String {
    let mut keys: Vec<&(&str, SignValue)> = params
        .iter()
        .filter(|(k, v)| *k != "signature" && !v.is_empty())
        .collect();
    keys.sort_by(|a, b| a.0.cmp(b.0));
    keys.iter()
        .map(|(k, v)| format!("{k}={}", format(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// `Sign`: lower-hex HMAC-SHA256 keyed with `secret_key`.
pub fn sign(params: &[(&str, SignValue)], secret: &str) -> String {
    hex::encode(hmac_sha256(
        secret.as_bytes(),
        sign_base(params, SignValue::format_f).as_bytes(),
    ))
}

/// epusdt channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gateway_url: String,
    pub pid: String,
    pub secret_key: String,
    pub order_mode: String,
    pub token: String,
    pub network: String,
    pub currency: String,
    pub notify_url: String,
    pub return_url: String,
    pub payment_type: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "epusdt")?;
        c.gateway_url = c.gateway_url.trim().trim_end_matches('/').to_owned();
        c.pid = c.pid.trim().to_owned();
        c.secret_key = c.secret_key.trim().to_owned();
        c.order_mode = c.order_mode.trim().to_ascii_lowercase();
        if c.order_mode.is_empty() {
            c.order_mode = order_mode::TRANSACTION.to_owned();
        }
        c.token = c.token.trim().to_ascii_lowercase();
        c.network = c.network.trim().to_ascii_lowercase();
        c.currency = c.currency.trim().to_ascii_lowercase();
        c.notify_url = c.notify_url.trim().to_owned();
        c.return_url = c.return_url.trim().to_owned();
        c.payment_type = c.payment_type.trim().to_owned();
        if c.order_mode == order_mode::CASHIER {
            c.token.clear();
            c.network.clear();
            c.payment_type.clear();
        }
        if c.currency.is_empty() {
            c.currency = SITE_CURRENCY_DEFAULT.to_ascii_lowercase();
        }
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        for (name, value) in [
            ("gateway_url", &self.gateway_url),
            ("pid", &self.pid),
            ("secret_key", &self.secret_key),
            ("notify_url", &self.notify_url),
            ("return_url", &self.return_url),
        ] {
            if value.trim().is_empty() {
                return Err(GatewayError::config(format!(
                    "epusdt config invalid: {name} is required"
                )));
            }
        }
        if self.order_mode != order_mode::TRANSACTION && self.order_mode != order_mode::CASHIER {
            return Err(GatewayError::config(
                "epusdt config invalid: order_mode is invalid",
            ));
        }
        if self.order_mode == order_mode::TRANSACTION {
            if self.token.is_empty() {
                return Err(GatewayError::config(
                    "epusdt config invalid: token is required",
                ));
            }
            if self.network.is_empty() {
                return Err(GatewayError::config(
                    "epusdt config invalid: network is required",
                ));
            }
        }
        Ok(())
    }

    fn checked(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let c = Self::parse(raw)?;
        c.validate()?;
        Ok(c)
    }
}

fn typed_string(map: &Map<String, Value>, key: &str) -> Result<String, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(GatewayError::response(format!(
            "callback field {key} has wrong type"
        ))),
    }
}

fn typed_int(map: &Map<String, Value>, key: &str) -> Result<i64, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| GatewayError::response(format!("callback field {key} has wrong type"))),
        Some(_) => Err(GatewayError::response(format!(
            "callback field {key} has wrong type"
        ))),
    }
}

/// `toFloat`: JSON number or numeric string, else 0.
pub fn loose_float(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().unwrap_or_default(),
        Some(Value::String(s)) => s.trim().parse().unwrap_or_default(),
        _ => 0.0,
    }
}

/// Amount from a float the way `decimal.NewFromFloat` + `money.FromDecimal` do.
pub fn amount_from_float(f: f64) -> Amount {
    if f > 0.0 {
        callback_amount(&go_float_f(f))
    } else {
        Amount::ZERO
    }
}

/// The epusdt adapter.
#[derive(Debug, Clone)]
pub struct EpusdtGateway {
    env: GatewayEnv,
}

impl EpusdtGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }
}

fn display_channel_type(cfg: &Config) -> String {
    if cfg.order_mode == order_mode::CASHIER || cfg.token.is_empty() || cfg.network.is_empty() {
        String::new()
    } else {
        format!("{}.{}", cfg.token, cfg.network)
    }
}

/// Extracts `trade_id` from the top level or `data.trade_id` / `data.id`.
fn extract_trade_id(raw: &Map<String, Value>) -> String {
    if let Some(Value::String(s)) = raw.get("trade_id")
        && !s.is_empty()
    {
        return s.clone();
    }
    if let Some(Value::Object(data)) = raw.get("data") {
        for key in ["trade_id", "id"] {
            if let Some(Value::String(s)) = data.get(key)
                && !s.is_empty()
            {
                return s.clone();
            }
        }
    }
    String::new()
}

#[async_trait]
impl PaymentGateway for EpusdtGateway {
    fn key(&self) -> &'static str {
        "epusdt:"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            callback: true,
            ..GatewayCapabilities::default()
        }
    }

    fn validate_config(
        &self,
        config: &ChannelConfig,
        _channel_type: &str,
    ) -> Result<(), GatewayError> {
        Config::checked(config).map(|_| ())
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let cfg = Config::checked(config)?;
        let return_url = match input.return_url.trim() {
            "" => cfg.return_url.clone(),
            v => v.to_owned(),
        };
        let return_url = append_query_params(&return_url, &input.return_url_query);
        let order_no = input.order_no.trim();
        let amount_text = go_decimal_string(input.amount.decimal());
        if order_no.is_empty() || amount_text.is_empty() {
            return Err(GatewayError::config(
                "epusdt config invalid: order_no and amount are required",
            ));
        }
        if input.order_no.len() > MAX_ORDER_NO_LEN {
            return Err(GatewayError::config(
                "epusdt config invalid: order_no exceeds 32 chars",
            ));
        }
        let amount: f64 = amount_text.parse().map_err(|_| {
            GatewayError::config(format!(
                "epusdt config invalid: invalid amount {amount_text:?}"
            ))
        })?;
        if amount <= MIN_AMOUNT {
            return Err(GatewayError::config(
                "epusdt config invalid: amount must be greater than 0.01",
            ));
        }
        let notify_url = if input.notify_url.trim().is_empty() {
            cfg.notify_url.clone()
        } else {
            input.notify_url.clone()
        };
        let mut params: Vec<(&str, SignValue)> = vec![
            ("pid", SignValue::Str(cfg.pid.clone())),
            ("order_id", SignValue::Str(input.order_no.clone())),
            ("currency", SignValue::Str(cfg.currency.clone())),
            ("amount", SignValue::Float(amount)),
            ("notify_url", SignValue::Str(notify_url)),
            ("redirect_url", SignValue::Str(return_url)),
        ];
        if cfg.order_mode != order_mode::CASHIER {
            params.push(("token", SignValue::Str(cfg.token.clone())));
            params.push(("network", SignValue::Str(cfg.network.clone())));
        }
        if !input.subject.trim().is_empty() {
            params.push(("name", SignValue::Str(input.subject.clone())));
        }
        if !cfg.payment_type.is_empty() {
            params.push(("payment_type", SignValue::Str(cfg.payment_type.clone())));
        }
        let signature = sign(&params, &cfg.secret_key);
        let mut body: Map<String, Value> = params
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.to_json()))
            .collect();
        body.insert("signature".into(), Value::String(signature));
        let req = HttpRequest::new("POST", format!("{}{CREATE_PATH}", cfg.gateway_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(go_json_map(&Value::Object(body)));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("epusdt request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::request(format!(
                "epusdt request failed: http status {}",
                resp.status
            )));
        }
        let raw = decode_object(&resp.body)
            .ok_or_else(|| GatewayError::response("epusdt response invalid"))?;
        let trade_id = extract_trade_id(&raw);
        if trade_id.is_empty() {
            return Err(GatewayError::response(
                "epusdt response invalid: trade_id missing in response",
            ));
        }
        let payment_url = format!("{}{CHECKOUT_PREFIX}{trade_id}", cfg.gateway_url);
        Ok(GatewayCreateResult {
            provider_ref: trade_id,
            redirect_url: payment_url.clone(),
            qr_code_url: payment_url,
            payload: raw,
            display_channel_type: display_channel_type(&cfg),
            amount_sent: String::new(),
            currency_sent: String::new(),
        })
    }

    fn verify_callback(
        &self,
        config: &ChannelConfig,
        _form: &FormMap,
        body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Config::parse(config)?;
        if body.is_empty() {
            return Err(GatewayError::response("epusdt response invalid"));
        }
        let data =
            decode_object(body).ok_or_else(|| GatewayError::response("epusdt response invalid"))?;
        let pid = typed_string(&data, "pid")?;
        let trade_id = typed_string(&data, "trade_id")?;
        let order_id = typed_string(&data, "order_id")?;
        let receive_address = typed_string(&data, "receive_address")?;
        let token = typed_string(&data, "token")?;
        let block_tx = typed_string(&data, "block_transaction_id")?;
        let signature = typed_string(&data, "signature")?;
        let status = typed_int(&data, "status")?;
        // PAY-43: only the paid status may ever be processed.
        if status != STATUS_SUCCESS {
            return Err(GatewayError::response(format!(
                "epusdt response invalid: status={status}"
            )));
        }
        let amount = loose_float(data.get("amount"));
        let params = [
            ("pid", SignValue::Str(pid.clone())),
            ("trade_id", SignValue::Str(trade_id.clone())),
            ("order_id", SignValue::Str(order_id.clone())),
            ("amount", SignValue::Float(amount)),
            (
                "actual_amount",
                SignValue::Float(loose_float(data.get("actual_amount"))),
            ),
            ("receive_address", SignValue::Str(receive_address.clone())),
            ("token", SignValue::Str(token.clone())),
            ("block_transaction_id", SignValue::Str(block_tx.clone())),
            ("status", SignValue::Int(status)),
        ];
        if cfg.secret_key.is_empty() {
            return Err(GatewayError::config("epusdt config invalid"));
        }
        if !signature_eq(&sign(&params, &cfg.secret_key), &signature) {
            return Err(GatewayError::signature("epusdt signature invalid"));
        }
        let status = match status {
            STATUS_SUCCESS => PaymentStatus::Success,
            STATUS_EXPIRED => PaymentStatus::Expired,
            _ => PaymentStatus::Pending,
        };
        let mut payload = Map::new();
        payload.insert("pid".into(), Value::String(pid));
        payload.insert("trade_id".into(), Value::String(trade_id.clone()));
        payload.insert("order_id".into(), Value::String(order_id.clone()));
        payload.insert(
            "amount".into(),
            data.get("amount").cloned().unwrap_or(Value::Null),
        );
        payload.insert(
            "actual_amount".into(),
            data.get("actual_amount").cloned().unwrap_or(Value::Null),
        );
        payload.insert("receive_address".into(), Value::String(receive_address));
        payload.insert("token".into(), Value::String(token));
        payload.insert("block_transaction_id".into(), Value::String(block_tx));
        payload.insert("status".into(), Value::from(2));
        payload.insert("signature".into(), Value::String(signature));
        Ok(GatewayCallbackResult {
            order_no: order_id,
            provider_ref: trade_id,
            status: Some(status),
            amount: amount_from_float(amount),
            currency: cfg.currency.to_ascii_uppercase(),
            paid_at: None,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::http::MockTransport;
    use crate::payment::test_support::{env_with, obj};
    use serde_json::json;

    fn base(amount: SignValue) -> Vec<(&'static str, SignValue)> {
        vec![
            ("amount", amount),
            ("currency", SignValue::Str("cny".into())),
            ("network", SignValue::Str("tron".into())),
            (
                "notify_url",
                SignValue::Str("https://example.com/notify".into()),
            ),
            ("order_id", SignValue::Str("ORD-1".into())),
            ("pid", SignValue::Str("1000".into())),
            ("token", SignValue::Str("usdt".into())),
        ]
    }

    /// PAY-41: Go `epusdt.Sign` vectors (HMAC-SHA256, float formatting without trailing zeros).
    #[test]
    fn pay_41_sign_matches_go() {
        assert_eq!(
            sign(&base(SignValue::Float(100.0)), "sk-test"),
            "e9e9f2ccd6828a0f219873df7ebdf071069cbc63cbe255bb7895f54b15270b0c"
        );
        assert_eq!(
            sign_base(&base(SignValue::Float(100.0)), SignValue::format_f),
            "amount=100&currency=cny&network=tron&notify_url=https://example.com/notify&order_id=ORD-1&pid=1000&token=usdt"
        );
        assert_eq!(
            sign(&base(SignValue::Float(12.5)), "sk-test"),
            "9e020a4976d66e94dc6b6210e07a88580ab076a6c68b719d16a27d910ab04469"
        );
        // Go folds the constant 0.1 + 0.2 to exactly 0.3.
        let mut c = base(SignValue::Float(0.3));
        c.push(("status", SignValue::Int(2)));
        c.push(("empty", SignValue::Str(" ".into())));
        assert_eq!(
            sign(&c, "sk-test"),
            "eaf708e3fb9bab1b33e38bc2e71746adee06b18ffe9bff8fd4a56d8310e9093a"
        );
    }

    fn cfg() -> ChannelConfig {
        obj(
            json!({"gateway_url": "https://gm.example.com/", "pid": "1000", "secret_key": "sk-test",
            "token": "USDT", "network": "Tron", "notify_url": "https://n/cb", "return_url": "https://r/pay"}),
        )
    }

    /// Go vector: create request body (map keys sorted, HTML escaped, float amount).
    #[tokio::test]
    async fn create_body_matches_go() {
        let mock = MockTransport::fixed(200, r#"{"status_code":200,"data":{"trade_id":"T100"}}"#);
        let gw = EpusdtGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from_cents(1250),
            subject: "DJ1<&>".into(),
            notify_url: "https://n/cb".into(),
            return_url: "https://r/pay?order_no=DJ1&epusdt_return=1".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&cfg(), &input).await.unwrap_or_default();
        let reqs = mock.requests();
        assert_eq!(
            reqs[0].url,
            "https://gm.example.com/payments/gmpay/v1/order/create-transaction"
        );
        assert_eq!(
            String::from_utf8_lossy(&reqs[0].body),
            r#"{"amount":12.5,"currency":"cny","name":"DJ1\u003c\u0026\u003e","network":"tron","notify_url":"https://n/cb","order_id":"DJP1","pid":"1000","redirect_url":"https://r/pay?order_no=DJ1\u0026epusdt_return=1","signature":"9b3bfb5ba1ef723d383fc9a1887c46132b39f238481de0010f31d4ff5de8a6fe","token":"usdt"}"#
        );
        assert_eq!(
            res.redirect_url,
            "https://gm.example.com/pay/checkout-counter/T100"
        );
        assert_eq!(res.display_channel_type, "usdt.tron");
    }

    fn callback(status: i64, signature: &str) -> String {
        json!({"pid": "1000", "trade_id": "T100", "order_id": "DJP1", "amount": "12.5", "actual_amount": 1.7361,
            "receive_address": "TAddr", "token": "usdt", "block_transaction_id": "0xabc", "status": status,
            "signature": signature})
        .to_string()
    }

    const GO_CB_SIGN: &str = "a1fcb75a0c056bfa0b38c759aea4aed8831a18ed77f6b6c097b10f225b1da107";

    /// PAY-43 / PAY-01: signed success accepted; non-paid statuses and bad signatures rejected.
    #[test]
    fn pay_43_callback_statuses() {
        let gw = EpusdtGateway::new(env_with(MockTransport::fixed(500, "")));
        let ok = gw
            .verify_callback(
                &cfg(),
                &FormMap::new(),
                callback(2, &GO_CB_SIGN.to_uppercase()).as_bytes(),
            )
            .unwrap_or_default();
        assert_eq!(ok.status, Some(PaymentStatus::Success));
        assert_eq!(ok.amount, Amount::from_cents(1250));
        assert_eq!(ok.currency, "CNY");
        assert_eq!(ok.order_no, "DJP1");
        for status in [1, 3] {
            let r = gw.verify_callback(
                &cfg(),
                &FormMap::new(),
                callback(status, GO_CB_SIGN).as_bytes(),
            );
            assert!(matches!(r, Err(GatewayError::ResponseInvalid(_))));
        }
        let bad = gw.verify_callback(&cfg(), &FormMap::new(), callback(2, "00").as_bytes());
        assert!(matches!(bad, Err(GatewayError::SignatureInvalid(_))));
        let mut no_key = cfg();
        no_key.insert("secret_key".into(), json!(""));
        let forged =
            gw.verify_callback(&no_key, &FormMap::new(), callback(2, GO_CB_SIGN).as_bytes());
        assert!(matches!(forged, Err(GatewayError::ConfigInvalid(_))));
    }

    #[tokio::test]
    async fn rejects_small_amounts_and_long_order_numbers() {
        let gw = EpusdtGateway::new(env_with(MockTransport::fixed(500, "")));
        let small = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from_cents(1),
            ..GatewayCreateInput::default()
        };
        assert!(matches!(
            gw.create_payment(&cfg(), &small).await,
            Err(GatewayError::ConfigInvalid(_))
        ));
        let long = GatewayCreateInput {
            order_no: "D".repeat(33),
            amount: Amount::from(10),
            ..GatewayCreateInput::default()
        };
        assert!(matches!(
            gw.create_payment(&cfg(), &long).await,
            Err(GatewayError::ConfigInvalid(_))
        ));
        let fail = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from(10),
            ..GatewayCreateInput::default()
        };
        assert!(matches!(
            gw.create_payment(&cfg(), &fail).await,
            Err(GatewayError::RequestFailed(_))
        ));
    }
}
