//! OKPay gateway: HMAC-SHA256 upper-hex signatures with `id`/`timestamp`/`nonce`,
//! JSON-only callbacks flattened with dotted keys (PAY-38, PAY-39, PAY-40).

use async_trait::async_trait;
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{FormMap, encode_pairs};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{PaymentStatus, channel_type};

use super::common::{
    GatewayEnv, callback_amount, decode_object, go_decimal_string, hmac_sha256, parse_config,
    read_string, signature_eq,
};
use super::http::HttpRequest;
use super::raw_json::RawJson;

const DEFAULT_GATEWAY_URL: &str = "https://api.okaypay.me/shop";
const PAY_LINK_PATH: &str = "/payLink";
/// Random bytes of the request nonce (hex encoded).
const NONCE_BYTES: usize = 16;
/// Crypto amounts carry eight decimals.
const AMOUNT_SCALE: u32 = 8;

/// OKPay channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gateway_url: String,
    pub merchant_id: String,
    pub merchant_token: String,
    pub return_url: String,
    pub callback_url: String,
    pub display_name: String,
    pub exchange_rate: String,
    pub coin: String,
    pub status: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "okpay")?;
        c.gateway_url = c.gateway_url.trim().trim_end_matches('/').to_owned();
        c.merchant_id = c.merchant_id.trim().to_owned();
        c.merchant_token = c.merchant_token.trim().to_owned();
        c.return_url = c.return_url.trim().to_owned();
        c.callback_url = c.callback_url.trim().to_owned();
        c.display_name = c.display_name.trim().to_owned();
        c.exchange_rate = c.exchange_rate.trim().to_owned();
        c.coin = c.coin.trim().to_ascii_uppercase();
        c.status = c.status.trim().to_owned();
        if c.gateway_url.is_empty() {
            c.gateway_url = DEFAULT_GATEWAY_URL.to_owned();
        }
        if c.exchange_rate.is_empty() {
            c.exchange_rate = "1".to_owned();
        }
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        for (name, value) in [
            ("merchant_id", &self.merchant_id),
            ("merchant_token", &self.merchant_token),
            ("return_url", &self.return_url),
            ("callback_url", &self.callback_url),
            ("gateway_url", &self.gateway_url),
        ] {
            if value.is_empty() {
                return Err(GatewayError::config(format!(
                    "okpay config invalid: {name} is required"
                )));
            }
        }
        parse_rate(&self.exchange_rate)?;
        if !self.coin.is_empty() && !is_supported_coin(&self.coin) {
            return Err(GatewayError::config(format!(
                "okpay config invalid: unsupported coin {}",
                self.coin
            )));
        }
        Ok(())
    }
}

fn is_supported_coin(coin: &str) -> bool {
    matches!(coin.trim().to_ascii_uppercase().as_str(), "USDT" | "TRX")
}

fn is_supported_channel(channel: &str) -> bool {
    matches!(
        channel.trim().to_ascii_lowercase().as_str(),
        channel_type::USDT | channel_type::TRX
    )
}

fn resolve_coin(channel: &str) -> String {
    match channel.trim().to_ascii_lowercase().as_str() {
        channel_type::USDT => "USDT".to_owned(),
        channel_type::TRX => "TRX".to_owned(),
        _ => String::new(),
    }
}

/// `ParseExchangeRate`: empty = 1, must be > 0.
pub fn parse_rate(raw: &str) -> Result<Decimal, GatewayError> {
    let raw = if raw.trim().is_empty() {
        "1"
    } else {
        raw.trim()
    };
    let rate: Decimal = raw
        .parse()
        .map_err(|_| GatewayError::config("okpay config invalid: exchange_rate invalid"))?;
    if rate <= Decimal::ZERO {
        return Err(GatewayError::config(
            "okpay config invalid: exchange_rate must be greater than 0",
        ));
    }
    Ok(rate)
}

/// `ConvertAmountByRate`: amount × rate rounded to 8 decimals.
pub fn convert_amount(base: &str, rate: &str) -> Result<Decimal, GatewayError> {
    let amount: Decimal = base
        .trim()
        .parse()
        .map_err(|_| GatewayError::config("okpay config invalid: invalid amount"))?;
    Ok((amount * parse_rate(rate)?)
        .round_dp_with_strategy(AMOUNT_SCALE, RoundingStrategy::MidpointAwayFromZero))
}

/// Fixed 8-decimal text (`StringFixed(8)`).
fn fixed8(d: Decimal) -> String {
    let mut d = d;
    d.rescale(AMOUNT_SCALE);
    d.to_string()
}

/// `buildSignature`: sorted `k=v` (non-empty, trimmed, no URL encoding), HMAC-SHA256 upper hex.
pub fn build_signature(pairs: &[(String, String)], token: &str) -> String {
    let base = pairs
        .iter()
        .filter_map(|(k, v)| {
            let (k, v) = (k.trim(), v.trim());
            (!k.is_empty() && !v.is_empty()).then(|| format!("{k}={v}"))
        })
        .collect::<Vec<_>>()
        .join("&");
    hex::encode_upper(hmac_sha256(token.trim().as_bytes(), base.as_bytes()))
}

/// Flattens the callback JSON into ordered `data.key` / `list[0].x` pairs (`parseOrderedJSON`).
pub fn flatten(value: &RawJson) -> Option<Vec<(String, String)>> {
    fn walk(value: &RawJson, prefix: &str, out: &mut Vec<(String, String)>) {
        match value {
            RawJson::Object(members) => {
                for (k, v) in members {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    walk_member(v, &key, out);
                }
            }
            RawJson::Array(items) => {
                for (i, v) in items.iter().enumerate() {
                    walk_member(v, &format!("{prefix}[{i}]"), out);
                }
            }
            _ => {}
        }
    }
    fn walk_member(v: &RawJson, key: &str, out: &mut Vec<(String, String)>) {
        match v {
            RawJson::Object(_) | RawJson::Array(_) => walk(v, key, out),
            RawJson::String(s) => out.push((key.trim().to_owned(), s.trim().to_owned())),
            scalar => out.push((
                key.trim().to_owned(),
                scalar.scalar_text().unwrap_or_default(),
            )),
        }
    }
    let RawJson::Object(_) = value else {
        return None;
    };
    let mut out = Vec::new();
    walk(value, "", &mut out);
    Some(out)
}

/// A parsed OKPay callback.
#[derive(Debug, Clone, Default)]
pub struct Callback {
    pub pairs: Vec<(String, String)>,
    pub raw: std::collections::BTreeMap<String, String>,
}

impl Callback {
    fn get(&self, key: &str) -> String {
        self.raw
            .get(key)
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    }
}

/// `ParseCallback`: JSON only (form-encoded bodies are rejected, PAY-38); `sign` required.
pub fn parse_callback(body: &[u8]) -> Result<Callback, GatewayError> {
    let text = String::from_utf8_lossy(body);
    if !text.trim().starts_with('{') {
        return Err(GatewayError::response("okpay response invalid"));
    }
    let value =
        RawJson::parse(body).ok_or_else(|| GatewayError::response("okpay response invalid"))?;
    let pairs = flatten(&value).ok_or_else(|| GatewayError::response("okpay response invalid"))?;
    if pairs.is_empty() {
        return Err(GatewayError::response("okpay response invalid"));
    }
    let raw = pairs.iter().cloned().collect();
    let cb = Callback { pairs, raw };
    if cb.get("sign").is_empty() {
        return Err(GatewayError::response("okpay response invalid"));
    }
    Ok(cb)
}

/// `VerifyCallback`: merchant id (when configured) must match exactly; HMAC must match.
pub fn verify_callback(cfg: &Config, cb: &Callback) -> Result<(), GatewayError> {
    if cfg.merchant_token.is_empty() {
        return Err(GatewayError::config("okpay config invalid"));
    }
    // PAY-01: a missing merchant id is a mismatch, not a skip.
    if !cfg.merchant_id.is_empty() && cb.get("id") != cfg.merchant_id {
        return Err(GatewayError::signature("okpay signature invalid"));
    }
    let sign = cb.get("sign");
    if sign.is_empty() {
        return Err(GatewayError::signature("okpay signature invalid"));
    }
    let mut pairs: Vec<(String, String)> = cb
        .pairs
        .iter()
        .filter(|(k, _)| !k.trim().eq_ignore_ascii_case("sign"))
        .cloned()
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    if !signature_eq(&build_signature(&pairs, &cfg.merchant_token), &sign) {
        return Err(GatewayError::signature("okpay signature invalid"));
    }
    Ok(())
}

/// The OKPay adapter.
#[derive(Debug, Clone)]
pub struct OkpayGateway {
    env: GatewayEnv,
}

impl OkpayGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn parse_for(raw: &ChannelConfig, channel: &str) -> Result<Config, GatewayError> {
        let mut cfg = Config::parse(raw)?;
        if cfg.coin.is_empty() && !channel.is_empty() {
            cfg.coin = resolve_coin(channel);
        }
        cfg.validate()?;
        Ok(cfg)
    }

    /// `SignPayload`: adds `id`, `timestamp` and `nonce`, signs, drops empty values.
    fn sign_payload(&self, payload: &[(String, String)], cfg: &Config) -> Vec<(String, String)> {
        let mut values: Vec<(String, String)> = payload
            .iter()
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .collect();
        values.push(("id".into(), cfg.merchant_id.trim().to_owned()));
        values.push((
            "timestamp".into(),
            self.env.clock.now().timestamp().to_string(),
        ));
        values.push(("nonce".into(), self.env.random_hex(NONCE_BYTES)));
        values.sort_by(|a, b| a.0.cmp(&b.0));
        let sign = build_signature(&values, &cfg.merchant_token);
        let mut out: Vec<(String, String)> = values
            .into_iter()
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .collect();
        out.push(("sign".into(), sign));
        out
    }
}

#[async_trait]
impl PaymentGateway for OkpayGateway {
    fn key(&self) -> &'static str {
        "okpay:"
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
        if !channel_type.is_empty() && !is_supported_channel(channel_type) {
            return Err(GatewayError::UnsupportedChannel(format!(
                "okpay channel_type {channel_type}"
            )));
        }
        Self::parse_for(config, channel_type).map(|_| ())
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        if !input.channel_type.is_empty() && !is_supported_channel(&input.channel_type) {
            return Err(GatewayError::UnsupportedChannel(format!(
                "okpay channel_type {}",
                input.channel_type
            )));
        }
        let cfg = Self::parse_for(config, &input.channel_type)?;
        let original_amount = go_decimal_string(input.amount.decimal());
        let return_url = match input.return_url.trim() {
            "" => cfg.return_url.clone(),
            v => v.to_owned(),
        };
        let return_url = append_query_params(&return_url, &input.return_url_query);
        if input.order_no.trim().is_empty() || original_amount.is_empty() {
            return Err(GatewayError::config("okpay config invalid"));
        }
        if !is_supported_coin(&cfg.coin) {
            return Err(GatewayError::config(
                "okpay config invalid: coin is required",
            ));
        }
        if return_url.is_empty() || cfg.callback_url.is_empty() {
            return Err(GatewayError::config("okpay config invalid"));
        }
        let converted = convert_amount(&original_amount, &cfg.exchange_rate)?;
        let mut payload: Vec<(String, String)> = vec![
            ("unique_id".into(), input.order_no.trim().to_owned()),
            ("amount".into(), fixed8(converted)),
            ("return_url".into(), return_url),
            ("callback_url".into(), cfg.callback_url.clone()),
            ("coin".into(), cfg.coin.clone()),
        ];
        let name = if input.subject.trim().is_empty() {
            cfg.display_name.clone()
        } else {
            input.subject.trim().to_owned()
        };
        if !name.is_empty() {
            payload.push(("name".into(), name));
        }
        if !cfg.status.is_empty() {
            payload.push(("status".into(), cfg.status.clone()));
        }
        let signed = self.sign_payload(&payload, &cfg);
        let req = HttpRequest::new("POST", format!("{}{PAY_LINK_PATH}", cfg.gateway_url))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(encode_pairs(&signed));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("okpay request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::request(format!(
                "okpay request failed: unexpected status {}",
                resp.status
            )));
        }
        let raw = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("okpay response invalid: decode response failed")
        })?;
        let data = match raw.get("data") {
            Some(Value::Object(d)) => d.clone(),
            Some(Value::Array(items)) => items
                .first()
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            _ => Map::new(),
        };
        let order_id = read_string(&data, "order_id");
        let pay_url = read_string(&data, "pay_url");
        if order_id.is_empty() || pay_url.is_empty() {
            return Err(GatewayError::response(
                "okpay response invalid: missing order_id/pay_url",
            ));
        }
        // PAY-39/PAY-40: persist exactly what was requested so callbacks compare against the snapshot.
        let mut amount_sent = original_amount.clone();
        let currency_sent = if cfg.coin.is_empty() {
            input.currency.trim().to_ascii_uppercase()
        } else {
            cfg.coin.clone()
        };
        let mut result_payload = raw.clone();
        if !matches!(cfg.exchange_rate.as_str(), "" | "1" | "1.0") {
            amount_sent = fixed8(converted);
            result_payload.insert(
                "exchange_rate".into(),
                Value::String(cfg.exchange_rate.clone()),
            );
            result_payload.insert("original_amount".into(), Value::String(original_amount));
            result_payload.insert(
                "original_currency".into(),
                Value::String(input.currency.clone()),
            );
        }
        Ok(GatewayCreateResult {
            provider_ref: order_id,
            redirect_url: pay_url.clone(),
            qr_code_url: pay_url,
            payload: result_payload,
            display_channel_type: String::new(),
            amount_sent,
            currency_sent,
        })
    }

    fn verify_callback(
        &self,
        config: &ChannelConfig,
        _form: &FormMap,
        body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Config::parse(config)?;
        let cb = parse_callback(body)?;
        verify_callback(&cfg, &cb)?;
        let status = if !cb.get("status").eq_ignore_ascii_case("success") {
            PaymentStatus::Failed
        } else {
            match cb.get("data.status").as_str() {
                "1" => PaymentStatus::Success,
                "2" => PaymentStatus::Failed,
                _ => PaymentStatus::Pending,
            }
        };
        let coin = cb.get("data.coin").to_ascii_uppercase();
        let currency = if coin.is_empty() {
            cfg.coin.clone()
        } else {
            coin.clone()
        };
        let mut payload = Map::new();
        payload.insert(
            "RawPairs".into(),
            Value::Array(
                cb.pairs
                    .iter()
                    .map(|(k, v)| serde_json::json!({"Key": k, "Value": v}))
                    .collect(),
            ),
        );
        payload.insert(
            "Raw".into(),
            Value::Object(
                cb.raw
                    .iter()
                    .map(|(k, v)| (k.clone(), Value::String(v.clone())))
                    .collect(),
            ),
        );
        for (field, key) in [
            ("MerchantID", "id"),
            ("Code", "code"),
            ("RequestStatus", "status"),
            ("Sign", "sign"),
            ("OrderID", "data.order_id"),
            ("UniqueID", "data.unique_id"),
            ("PayUserID", "data.pay_user_id"),
            ("Amount", "data.amount"),
            ("PaymentStatus", "data.status"),
            ("Type", "data.type"),
        ] {
            payload.insert(field.into(), Value::String(cb.get(key)));
        }
        payload.insert("Coin".into(), Value::String(coin));
        Ok(GatewayCallbackResult {
            order_no: cb.get("data.unique_id"),
            provider_ref: cb.get("data.order_id"),
            status: Some(status),
            amount: callback_amount(&cb.get("data.amount")),
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

    const TOKEN: &str = "TESTtoken123456789abcdefghijABCD";

    fn pairs(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    /// PAY-38: official OKPay test vectors A, B and C.
    #[test]
    fn pay_38_official_vectors() {
        let a = pairs(&[
            ("amount", "100.5"),
            ("coin", "USDT"),
            ("id", "10001"),
            ("nonce", "a1b2c3d4e5"),
            ("timestamp", "1782680000"),
            ("unique_id", "ORDER-20260628-001"),
        ]);
        assert_eq!(
            build_signature(&a, TOKEN),
            "7444ADFD8E4F4DA09D752DDF9345E0EE56DC25090FCFAF675DD042830E5E3F79"
        );
        let c = pairs(&[
            ("a", "0"),
            ("b", "0"),
            ("e", "false"),
            ("f", "hello"),
            ("id", "7"),
            ("nest.x", "1"),
            ("nest.y", "2"),
        ]);
        assert_eq!(
            build_signature(&c, TOKEN),
            "8BC0AF979075038025DDD51B6F4A2E6CF3FF9B5B5371EB2268D303F89883E92A"
        );
    }

    const VECTOR_B: &str = r#"{"status":"success","code":200,"data":{"order_id":"abc123def456","unique_id":"ORDER-20260628-001","pay_user_id":123456789,"amount":"100.5","coin":"USDT","status":1,"type":"deposit"},"id":10001,"sign":"64B09C8847849FA6921D8FFBDF8E406D4A8EA623E53970712350F61783403F7D"}"#;

    fn cfg() -> ChannelConfig {
        obj(
            json!({"merchant_id": "10001", "merchant_token": TOKEN, "return_url": "https://r/pay",
            "callback_url": "https://shop/api/v1/payments/callback", "coin": "usdt"}),
        )
    }

    /// PAY-38 / PAY-39: vector B callback verifies; bad signatures, form bodies and merchant
    /// mismatches are rejected.
    #[test]
    fn pay_39_callback_verification() {
        let gw = OkpayGateway::new(env_with(MockTransport::fixed(500, "")));
        let ok = gw
            .verify_callback(&cfg(), &FormMap::new(), VECTOR_B.as_bytes())
            .unwrap_or_default();
        assert_eq!(ok.status, Some(PaymentStatus::Success));
        assert_eq!(ok.order_no, "ORDER-20260628-001");
        assert_eq!(ok.provider_ref, "abc123def456");
        assert_eq!(ok.amount, Amount::from_cents(10050));
        assert_eq!(ok.currency, "USDT");
        let bad = VECTOR_B.replace("\"sign\":\"", "\"sign\":\"BAD");
        assert!(
            gw.verify_callback(&cfg(), &FormMap::new(), bad.as_bytes())
                .is_err()
        );
        let form = b"code=200&data.order_id=abc123&status=success&sign=DEADBEEF";
        assert!(matches!(
            gw.verify_callback(&cfg(), &FormMap::new(), form),
            Err(GatewayError::ResponseInvalid(_))
        ));
        let no_id = VECTOR_B.replace(",\"id\":10001", "");
        assert!(matches!(
            gw.verify_callback(&cfg(), &FormMap::new(), no_id.as_bytes()),
            Err(GatewayError::SignatureInvalid(_))
        ));
    }

    /// Go vector: nested arrays flatten to `data.list[0].a`, raw number text is kept.
    #[test]
    fn pay_38_flatten_matches_go() {
        let body = r#"{"code":200,"data":{"list":[{"a":1},{"b":"x"}],"order_id":"O1","unique_id":"DJP1","amount":10.50,"coin":"usdt","status":1,"pay_user_id":null,"ok":false},"id":"10001","status":"success","sign":"S"}"#;
        let cb = parse_callback(body.as_bytes()).unwrap_or_default();
        assert_eq!(
            cb.pairs,
            pairs(&[
                ("code", "200"),
                ("data.list[0].a", "1"),
                ("data.list[1].b", "x"),
                ("data.order_id", "O1"),
                ("data.unique_id", "DJP1"),
                ("data.amount", "10.50"),
                ("data.coin", "usdt"),
                ("data.status", "1"),
                ("data.pay_user_id", ""),
                ("data.ok", "false"),
                ("id", "10001"),
                ("status", "success"),
                ("sign", "S"),
            ])
        );
        // The Go vector signs the stripped pairs in document order.
        let stripped: Vec<(String, String)> =
            cb.pairs.into_iter().filter(|(k, _)| k != "sign").collect();
        assert_eq!(
            build_signature(&stripped, TOKEN),
            "F339AF8364040DB395C2DDC972039F026345720B590C6D8D57A7DA0997AE77F3"
        );
    }

    /// PAY-39 / PAY-40 / PAY-08: USD order + coin USDT + rate 1 → currency USDT, amount unchanged;
    /// rate 7 → 616 USDT snapshot with audit fields.
    #[tokio::test]
    async fn pay_40_amount_snapshot() {
        let mock = MockTransport::fixed(
            200,
            r#"{"code":200,"data":{"order_id":"OK1","pay_url":"https://ok/pay/OK1"}}"#,
        );
        let gw = OkpayGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from(88),
            currency: "USD".into(),
            channel_type: "usdt".into(),
            subject: "DJ1".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&cfg(), &input).await.unwrap_or_default();
        assert_eq!(res.currency_sent, "USDT");
        assert_eq!(res.amount_sent, "88");
        assert!(!res.payload.contains_key("exchange_rate"));
        let body = String::from_utf8_lossy(&mock.requests()[0].body).into_owned();
        assert!(
            body.contains("amount=88.00000000")
                && body.contains("id=10001")
                && body.contains("timestamp=1782680000")
        );
        assert!(body.contains(&format!("nonce={}", "ab".repeat(16))));
        let mut rate7 = cfg();
        rate7.insert("exchange_rate".into(), json!("7"));
        let res = gw.create_payment(&rate7, &input).await.unwrap_or_default();
        assert_eq!(res.amount_sent, "616.00000000");
        assert_eq!(res.payload["original_amount"], "88");
        assert_eq!(res.payload["exchange_rate"], "7");
    }
}
