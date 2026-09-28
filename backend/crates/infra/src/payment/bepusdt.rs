//! BEpusdt gateway: MD5 signatures over `%v`-formatted params, transaction/cashier modes (PAY-44).

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
use zs_domain::payment::types::{
    InteractionMode, PaymentStatus, SITE_CURRENCY_DEFAULT, order_mode, provider,
};

use super::common::{
    GatewayEnv, decode_object, go_decimal_string, go_float_v, go_json_map, md5_hex, parse_config,
    signature_eq,
};
use super::epusdt::{SignValue, amount_from_float, loose_float, sign_base};
use super::http::HttpRequest;

const CREATE_TRANSACTION_PATH: &str = "/api/v1/order/create-transaction";
const CREATE_ORDER_PATH: &str = "/api/v1/order/create-order";
const DEFAULT_TRADE_TYPE: &str = "usdt.trc20";
const STATUS_SUCCESS: i64 = 2;
const STATUS_EXPIRED: i64 = 3;

/// `Sign`: `md5(sorted k=%v … + auth_token)` lower hex.
pub fn sign(params: &[(&str, SignValue)], auth_token: &str) -> String {
    let base = sign_base(params, |v| match v {
        SignValue::Str(s) => s.clone(),
        SignValue::Float(f) => go_float_v(*f),
        SignValue::Int(i) => i.to_string(),
    });
    md5_hex(format!("{base}{auth_token}").as_bytes())
}

/// Legacy channel type → trade type (`ResolveTradeType`).
pub fn resolve_trade_type(channel: &str) -> Option<&'static str> {
    match channel.trim().to_ascii_lowercase().as_str() {
        "usdt" | "usdt-trc20" => Some("usdt.trc20"),
        "usdc-trc20" => Some("usdc.trc20"),
        "trx" => Some("tron.trx"),
        _ => None,
    }
}

fn is_legacy_channel(channel: &str) -> bool {
    resolve_trade_type(channel).is_some()
}

/// BEpusdt channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gateway_url: String,
    pub auth_token: String,
    pub trade_type: String,
    pub order_mode: String,
    pub currencies: String,
    pub fiat: String,
    pub notify_url: String,
    pub return_url: String,
    pub address: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "bepusdt")?;
        c.gateway_url = c.gateway_url.trim().trim_end_matches('/').to_owned();
        c.auth_token = c.auth_token.trim().to_owned();
        c.trade_type = c.trade_type.trim().to_owned();
        c.order_mode = c.order_mode.trim().to_ascii_lowercase();
        c.currencies = c.currencies.trim().replace(' ', "").to_ascii_uppercase();
        c.fiat = c.fiat.trim().to_owned();
        c.notify_url = c.notify_url.trim().to_owned();
        c.return_url = c.return_url.trim().to_owned();
        c.address = c.address.trim().to_owned();
        if c.order_mode.is_empty() {
            c.order_mode = order_mode::TRANSACTION.to_owned();
        }
        if c.order_mode == order_mode::CASHIER {
            c.trade_type.clear();
        } else if c.trade_type.is_empty() {
            c.trade_type = DEFAULT_TRADE_TYPE.to_owned();
        }
        if c.order_mode != order_mode::CASHIER {
            c.currencies.clear();
        }
        if c.fiat.is_empty() {
            c.fiat = SITE_CURRENCY_DEFAULT.to_owned();
        }
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        for (name, value) in [
            ("gateway_url", &self.gateway_url),
            ("auth_token", &self.auth_token),
            ("notify_url", &self.notify_url),
            ("return_url", &self.return_url),
        ] {
            if value.is_empty() {
                return Err(GatewayError::config(format!(
                    "bepusdt config invalid: {name} is required"
                )));
            }
        }
        if self.order_mode != order_mode::TRANSACTION && self.order_mode != order_mode::CASHIER {
            return Err(GatewayError::config(
                "bepusdt config invalid: order_mode is invalid",
            ));
        }
        Ok(())
    }

    fn checked(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let c = Self::parse(raw)?;
        c.validate()?;
        Ok(c)
    }

    fn is_cashier(&self) -> bool {
        self.order_mode == order_mode::CASHIER
    }
}

/// `resolveBepusdtTradeLabels`: `usdt.arbitrum` → (`arbitrum`, `arbitrum-usdt`), `tron.trx` → (`tron`, `tron-trx`).
pub fn trade_labels(trade_type: &str) -> (String, String) {
    let normalized = trade_type.trim().to_ascii_lowercase();
    let parts: Vec<&str> = normalized.split('.').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return (String::new(), String::new());
    }
    let network = |n: &str| match n {
        "trc20" => "tron".to_owned(),
        "erc20" | "eth" => "ethereum".to_owned(),
        "bep20" => "bsc".to_owned(),
        other => other.to_owned(),
    };
    if matches!(parts[0], "usdt" | "usdc") {
        let net = network(parts[1]);
        let token_id = format!("{net}-{}", parts[0]);
        (net, token_id)
    } else {
        let net = network(parts[0]);
        let token_id = format!("{net}-{}", parts[1]);
        (net, token_id)
    }
}

fn check_channel_type(cfg: &Config, channel: &str) -> Result<(), GatewayError> {
    let channel = channel.trim().to_ascii_lowercase();
    if channel.is_empty() {
        return Ok(());
    }
    if cfg.is_cashier() {
        if channel != provider::BEPUSDT {
            return Err(GatewayError::UnsupportedChannel(format!(
                "bepusdt cashier channel_type {channel}"
            )));
        }
        return Ok(());
    }
    if channel == provider::BEPUSDT {
        if cfg.trade_type.is_empty() {
            return Err(GatewayError::config("bepusdt trade_type is required"));
        }
        return Ok(());
    }
    if !is_legacy_channel(&channel) {
        return Err(GatewayError::UnsupportedChannel(format!(
            "bepusdt channel_type {channel}"
        )));
    }
    Ok(())
}

fn typed_str(map: &Map<String, Value>, key: &str) -> Result<String, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(GatewayError::response(format!(
            "bepusdt response invalid: {key}"
        ))),
    }
}

fn typed_int(map: &Map<String, Value>, key: &str) -> Result<i64, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| GatewayError::response(format!("bepusdt response invalid: {key}"))),
        Some(_) => Err(GatewayError::response(format!(
            "bepusdt response invalid: {key}"
        ))),
    }
}

/// The BEpusdt adapter.
#[derive(Debug, Clone)]
pub struct BepusdtGateway {
    env: GatewayEnv,
}

impl BepusdtGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }
}

#[async_trait]
impl PaymentGateway for BepusdtGateway {
    fn key(&self) -> &'static str {
        "bepusdt:"
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
        channel_type: &str,
    ) -> Result<(), GatewayError> {
        let normalized = channel_type.trim().to_ascii_lowercase();
        if !normalized.is_empty()
            && normalized != provider::BEPUSDT
            && !is_legacy_channel(&normalized)
        {
            return Err(GatewayError::UnsupportedChannel(format!(
                "bepusdt channel_type {channel_type}"
            )));
        }
        let cfg = Config::checked(config)?;
        check_channel_type(&cfg, &normalized)
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let cfg = Config::checked(config)?;
        check_channel_type(&cfg, &input.channel_type)?;
        let return_url = match input.return_url.trim() {
            "" => cfg.return_url.clone(),
            v => v.to_owned(),
        };
        let return_url = append_query_params(&return_url, &input.return_url_query);
        let mode = input.interaction_mode;
        if cfg.is_cashier() && mode == Some(InteractionMode::Qr) {
            return Err(GatewayError::config(
                "bepusdt cashier order mode only supports redirect interaction_mode",
            ));
        }
        let amount_text = go_decimal_string(input.amount.decimal());
        if input.order_no.is_empty() || amount_text.is_empty() {
            return Err(GatewayError::config("bepusdt config invalid"));
        }
        let amount: f64 = amount_text
            .parse()
            .map_err(|_| GatewayError::config("bepusdt config invalid: invalid amount"))?;
        let notify_url = if input.notify_url.is_empty() {
            cfg.notify_url.clone()
        } else {
            input.notify_url.clone()
        };
        let mut params: Vec<(&str, SignValue)> = vec![
            ("order_id", SignValue::Str(input.order_no.clone())),
            ("amount", SignValue::Float(amount)),
            ("notify_url", SignValue::Str(notify_url)),
            ("redirect_url", SignValue::Str(return_url)),
            ("fiat", SignValue::Str(cfg.fiat.clone())),
        ];
        if !cfg.is_cashier() {
            params.push(("trade_type", SignValue::Str(cfg.trade_type.clone())));
        }
        if !input.subject.is_empty() {
            params.push(("name", SignValue::Str(input.subject.clone())));
        }
        if cfg.is_cashier() {
            if !cfg.currencies.is_empty() {
                params.push(("currencies", SignValue::Str(cfg.currencies.clone())));
            }
        } else if !cfg.address.is_empty() {
            params.push(("address", SignValue::Str(cfg.address.clone())));
        }
        let signature = sign(&params, &cfg.auth_token);
        let mut body: Map<String, Value> = params
            .iter()
            .map(|(k, v)| {
                let json = match v {
                    SignValue::Str(s) => Value::String(s.clone()),
                    SignValue::Float(f) => super::common::json_float(*f),
                    SignValue::Int(i) => Value::from(*i),
                };
                ((*k).to_owned(), json)
            })
            .collect();
        body.insert("signature".into(), Value::String(signature));
        let path = if cfg.is_cashier() {
            CREATE_ORDER_PATH
        } else {
            CREATE_TRANSACTION_PATH
        };
        let req = HttpRequest::new("POST", format!("{}{path}", cfg.gateway_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(go_json_map(&Value::Object(body)));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("bepusdt request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::request(format!(
                "bepusdt request failed: http status {}",
                resp.status
            )));
        }
        let raw = decode_object(&resp.body)
            .ok_or_else(|| GatewayError::response("bepusdt response invalid"))?;
        let status_code = typed_int(&raw, "status_code")?;
        let message = typed_str(&raw, "message")?;
        let data = match raw.get("data") {
            Some(Value::Object(d)) => d.clone(),
            None | Some(Value::Null) => Map::new(),
            Some(_) => return Err(GatewayError::response("bepusdt response invalid")),
        };
        let trade_id = typed_str(&data, "trade_id")?;
        let actual_amount = if cfg.is_cashier() {
            String::new()
        } else {
            typed_str(&data, "actual_amount")?
        };
        let token = if cfg.is_cashier() {
            String::new()
        } else {
            typed_str(&data, "token")?
        };
        let payment_url = typed_str(&data, "payment_url")?;
        typed_str(&data, "amount")?;
        typed_int(&data, "expiration_time")?;
        if status_code != 200 {
            return Err(GatewayError::response(format!(
                "bepusdt response invalid: {message}"
            )));
        }
        let (mut redirect_url, mut qr_code_url) =
            (payment_url.trim().to_owned(), payment_url.trim().to_owned());
        match mode {
            Some(InteractionMode::Qr) => {
                qr_code_url = token.trim().to_owned();
                redirect_url.clear();
                if qr_code_url.is_empty() {
                    return Err(GatewayError::response("bepusdt token is empty"));
                }
            }
            None | Some(InteractionMode::Redirect) => {}
            Some(other) => {
                return Err(GatewayError::config(format!(
                    "bepusdt interaction_mode {other}"
                )));
            }
        }
        let mut payload = raw.clone();
        let mut data_out = match payload.get("data") {
            Some(Value::Object(d)) => d.clone(),
            _ => Map::new(),
        };
        let (chain, token_id) = trade_labels(&cfg.trade_type);
        for (key, value) in [
            ("order_mode", cfg.order_mode.as_str()),
            ("trade_type", cfg.trade_type.as_str()),
            ("token", token.as_str()),
            ("actual_amount", actual_amount.as_str()),
            ("payment_url", payment_url.as_str()),
            ("chain", chain.as_str()),
            ("token_id", token_id.as_str()),
        ] {
            if !value.trim().is_empty() {
                data_out.insert(key.to_owned(), Value::String(value.trim().to_owned()));
            }
        }
        payload.insert("data".into(), Value::Object(data_out));
        Ok(GatewayCreateResult {
            provider_ref: trade_id,
            redirect_url,
            qr_code_url,
            payload,
            display_channel_type: if cfg.is_cashier() {
                String::new()
            } else {
                cfg.trade_type.clone()
            },
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
            return Err(GatewayError::response("bepusdt response invalid"));
        }
        let data = decode_object(body)
            .ok_or_else(|| GatewayError::response("bepusdt response invalid"))?;
        let trade_id = typed_str(&data, "trade_id")?;
        let order_id = typed_str(&data, "order_id")?;
        let token = typed_str(&data, "token")?;
        let block_tx = typed_str(&data, "block_transaction_id")?;
        let signature = typed_str(&data, "signature")?;
        let status = typed_int(&data, "status")?;
        // PAY-43: expired/unpaid notifications are signed too and must never count as paid.
        if status != STATUS_SUCCESS {
            return Err(GatewayError::response(
                "bepusdt response invalid: status is not success",
            ));
        }
        let amount = loose_float(data.get("amount"));
        let params = [
            ("trade_id", SignValue::Str(trade_id.clone())),
            ("order_id", SignValue::Str(order_id.clone())),
            ("amount", SignValue::Float(amount)),
            (
                "actual_amount",
                SignValue::Float(loose_float(data.get("actual_amount"))),
            ),
            ("token", SignValue::Str(token.clone())),
            ("block_transaction_id", SignValue::Str(block_tx.clone())),
            ("status", SignValue::Int(status)),
        ];
        if cfg.auth_token.is_empty() {
            return Err(GatewayError::config("bepusdt config invalid"));
        }
        if !signature_eq(&sign(&params, &cfg.auth_token), &signature) {
            return Err(GatewayError::signature("bepusdt signature invalid"));
        }
        let mapped = match status {
            STATUS_SUCCESS => PaymentStatus::Success,
            STATUS_EXPIRED => PaymentStatus::Expired,
            _ => PaymentStatus::Pending,
        };
        let mut payload = Map::new();
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
        payload.insert("token".into(), Value::String(token));
        payload.insert("block_transaction_id".into(), Value::String(block_tx));
        payload.insert("signature".into(), Value::String(signature));
        payload.insert("status".into(), Value::from(status));
        let currency = match cfg.fiat.trim().to_ascii_uppercase() {
            c if c.is_empty() => SITE_CURRENCY_DEFAULT.to_owned(),
            c => c,
        };
        Ok(GatewayCallbackResult {
            order_no: order_id,
            provider_ref: trade_id,
            status: Some(mapped),
            amount: amount_from_float(amount),
            currency,
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
    use zs_shared::money::Amount;

    fn params(amount: f64) -> Vec<(&'static str, SignValue)> {
        vec![
            ("order_id", SignValue::Str("DJP1".into())),
            ("amount", SignValue::Float(amount)),
            ("notify_url", SignValue::Str("https://n/cb".into())),
            ("redirect_url", SignValue::Str("https://r/pay".into())),
            ("trade_type", SignValue::Str("usdt.trc20".into())),
            ("fiat", SignValue::Str("CNY".into())),
            ("name", SignValue::Str("DJ1".into())),
        ]
    }

    /// Go `bepusdt.Sign` vectors (`%v` float formatting incl. exponents).
    #[test]
    fn sign_matches_go() {
        assert_eq!(
            sign(&params(100.0), "tok"),
            "969011e5908e29591154c44f3fc00d0d"
        );
        assert_eq!(
            sign(&params(0.00001), "tok"),
            "c96ab9b46b969746cd956f19541d4cb3"
        );
        assert_eq!(
            sign(&params(1e21), "tok"),
            "abafa1197eb2b5e7f5d7d18ca659f33f"
        );
        let mut d = params(123_456_789.25);
        d.push(("status", SignValue::Int(2)));
        assert_eq!(sign(&d, "tok"), "568db9ac60d8f803601e4a5a1b85fd46");
    }

    fn cfg(extra: Value) -> ChannelConfig {
        let mut c = obj(
            json!({"gateway_url": "https://bep.example.com", "auth_token": "tok",
            "notify_url": "https://n/cb", "return_url": "https://r/pay"}),
        );
        c.extend(obj(extra));
        c
    }

    fn cb(status: i64, sign: &str) -> String {
        json!({"trade_id": "T1", "order_id": "DJP1", "amount": 10.5, "actual_amount": "1.46", "token": "TAddr",
            "block_transaction_id": "0xa", "signature": sign, "status": status})
        .to_string()
    }

    /// PAY-43: only status 2 is processed; Go vector signature verifies.
    #[test]
    fn pay_43_callback() {
        let gw = BepusdtGateway::new(env_with(MockTransport::fixed(500, "")));
        let ok = gw
            .verify_callback(
                &cfg(json!({})),
                &FormMap::new(),
                cb(2, "eeea5f82486e7d22c3b6483d0771e3dc").as_bytes(),
            )
            .unwrap_or_default();
        assert_eq!(ok.status, Some(PaymentStatus::Success));
        assert_eq!(ok.amount, Amount::from_cents(1050));
        assert_eq!(ok.currency, "CNY");
        for status in [1, 3] {
            let r =
                gw.verify_callback(&cfg(json!({})), &FormMap::new(), cb(status, "x").as_bytes());
            assert!(matches!(r, Err(GatewayError::ResponseInvalid(_))));
        }
        let mut empty = cfg(json!({}));
        empty.insert("auth_token".into(), json!(""));
        let forged = gw.verify_callback(
            &empty,
            &FormMap::new(),
            cb(2, "eeea5f82486e7d22c3b6483d0771e3dc").as_bytes(),
        );
        assert!(forged.is_err());
    }

    /// PAY-44: trade labels, QR uses the receiving address, cashier/QR and channel-type rules.
    #[tokio::test]
    async fn pay_44_modes_and_labels() {
        assert_eq!(
            trade_labels("usdt.arbitrum"),
            ("arbitrum".into(), "arbitrum-usdt".into())
        );
        assert_eq!(trade_labels("tron.trx"), ("tron".into(), "tron-trx".into()));
        assert_eq!(
            trade_labels("usdc.erc20"),
            ("ethereum".into(), "ethereum-usdc".into())
        );
        let mock = MockTransport::fixed(
            200,
            r#"{"status_code":200,"message":"ok","data":{"trade_id":"T9","order_id":"DJP1","amount":"10","actual_amount":"1.4","token":"TAddr","expiration_time":600,"payment_url":"https://bep/pay/T9"}}"#,
        );
        let gw = BepusdtGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from(10),
            channel_type: "bepusdt".into(),
            interaction_mode: Some(InteractionMode::Qr),
            ..GatewayCreateInput::default()
        };
        let res = gw
            .create_payment(&cfg(json!({"trade_type": "usdt.arbitrum"})), &input)
            .await
            .unwrap_or_default();
        assert_eq!(res.qr_code_url, "TAddr");
        assert_eq!(res.redirect_url, "");
        assert_eq!(res.display_channel_type, "usdt.arbitrum");
        assert_eq!(res.payload["data"]["chain"], "arbitrum");
        assert_eq!(res.payload["data"]["token_id"], "arbitrum-usdt");
        assert_eq!(
            mock.requests()[0].url,
            "https://bep.example.com/api/v1/order/create-transaction"
        );
        let cashier = cfg(json!({"order_mode": "cashier", "trade_type": "usdt.trc20"}));
        assert!(matches!(
            gw.create_payment(&cashier, &input).await,
            Err(GatewayError::ConfigInvalid(_))
        ));
        assert!(matches!(
            gw.validate_config(&cashier, "usdt-trc20"),
            Err(GatewayError::UnsupportedChannel(_))
        ));
        assert!(gw.validate_config(&cashier, "bepusdt").is_ok());
        assert!(gw.validate_config(&cfg(json!({})), "usdc-trc20").is_ok());
        assert!(matches!(
            gw.validate_config(&cfg(json!({})), "foo"),
            Err(GatewayError::UnsupportedChannel(_))
        ));
    }
}
