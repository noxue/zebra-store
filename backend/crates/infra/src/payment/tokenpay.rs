//! TokenPay gateway: MD5 over the raw callback JSON (numbers keep their text), fiat
//! amount/currency semantics (PAY-46, PAY-47).

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::FormMap;
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{PaymentStatus, SITE_CURRENCY_DEFAULT};

use super::common::{
    GatewayEnv, callback_amount, decode_object, go_decimal_string, go_json_map, md5_hex,
    parse_config, signature_eq,
};
use super::http::HttpRequest;
use super::raw_json::RawJson;

const CREATE_ORDER_PATH: &str = "/CreateOrder";
/// Offset used for zone-less `PayTime` values (the gateway reports China time).
const PAY_TIME_OFFSET_SECS: i32 = 8 * 3600;

/// TokenPay channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gateway_url: String,
    pub notify_secret: String,
    /// Crypto currency code (e.g. `USDT_TRC20`), passed through verbatim (PAY-47).
    pub currency: String,
    pub notify_url: String,
    pub redirect_url: String,
    /// Fiat currency of `ActualAmount`.
    pub base_currency: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "tokenpay")?;
        c.gateway_url = c.gateway_url.trim().trim_end_matches('/').to_owned();
        c.notify_secret = c.notify_secret.trim().to_owned();
        c.currency = c.currency.trim().to_owned();
        c.notify_url = c.notify_url.trim().to_owned();
        c.redirect_url = c.redirect_url.trim().to_owned();
        c.base_currency = c.base_currency.trim().to_ascii_uppercase();
        if c.base_currency.is_empty() {
            c.base_currency = SITE_CURRENCY_DEFAULT.to_owned();
        }
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        for (name, value) in [
            ("gateway_url", &self.gateway_url),
            ("notify_secret", &self.notify_secret),
            ("currency", &self.currency),
        ] {
            if value.is_empty() {
                return Err(GatewayError::config(format!(
                    "tokenpay config invalid: {name} is required"
                )));
            }
        }
        Ok(())
    }
}

/// A sign value as decoded by Go with `UseNumber`.
fn sign_text(v: &RawJson) -> Option<String> {
    match v {
        RawJson::Null => None,
        RawJson::String(s) => (!s.trim().is_empty()).then(|| s.trim().to_owned()),
        RawJson::Number(n) => (!n.trim().is_empty()).then(|| n.trim().to_owned()),
        RawJson::Bool(b) => Some(b.to_string()),
        nested => Some(nested.go_marshal()),
    }
}

/// `SignPayload`: sorted `k=v` of non-empty values (keys ≠ `Signature`, any case) + secret, MD5.
pub fn sign_members(members: &[(String, RawJson)], secret: &str) -> String {
    let mut map: std::collections::BTreeMap<&str, &RawJson> = std::collections::BTreeMap::new();
    for (k, v) in members {
        map.insert(k, v);
    }
    let base = map
        .into_iter()
        .filter(|(k, _)| !k.trim().eq_ignore_ascii_case("signature"))
        .filter_map(|(k, v)| sign_text(v).map(|t| format!("{k}={t}")))
        .collect::<Vec<_>>()
        .join("&");
    md5_hex(format!("{base}{}", secret.trim()).as_bytes())
}

fn pick_string(obj: &RawJson, keys: &[&str]) -> String {
    for key in keys {
        if let Some(v) = obj.get(key) {
            return match v {
                RawJson::String(s) => s.clone(),
                RawJson::Number(n) => n.clone(),
                RawJson::Null => continue,
                RawJson::Bool(b) => b.to_string(),
                other => other.go_marshal(),
            };
        }
    }
    String::new()
}

fn pick_int(obj: &RawJson, keys: &[&str]) -> i64 {
    for key in keys {
        match obj.get(key) {
            Some(RawJson::Number(n)) => {
                if let Ok(i) = n.parse::<i64>() {
                    return i;
                }
            }
            Some(RawJson::String(s)) => {
                if let Ok(i) = s.trim().parse::<i64>() {
                    return i;
                }
            }
            _ => {}
        }
    }
    0
}

/// `ParsePaidAt`: `2006-01-02 15:04:05` (China time) or RFC 3339.
pub fn parse_paid_at(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let offset = FixedOffset::east_opt(PAY_TIME_OFFSET_SECS)?;
    if let Ok(naive) = NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S") {
        return naive
            .and_local_timezone(offset)
            .single()
            .map(|t| t.with_timezone(&Utc));
    }
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// The TokenPay adapter.
#[derive(Debug, Clone)]
pub struct TokenpayGateway {
    env: GatewayEnv,
}

impl TokenpayGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn checked(raw: &ChannelConfig) -> Result<Config, GatewayError> {
        let c = Config::parse(raw)?;
        c.validate()?;
        Ok(c)
    }
}

fn info_string(raw: &Map<String, Value>, key: &str) -> String {
    match raw.get("info") {
        Some(Value::Object(info)) => match info.get(key) {
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        },
        _ => String::new(),
    }
}

#[async_trait]
impl PaymentGateway for TokenpayGateway {
    fn key(&self) -> &'static str {
        "tokenpay:"
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
        Self::checked(config).map(|_| ())
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let cfg = Self::checked(config)?;
        let redirect_url = match input.return_url.trim() {
            "" => cfg.redirect_url.clone(),
            v => v.to_owned(),
        };
        let redirect_url = append_query_params(&redirect_url, &input.return_url_query);
        let actual_amount = go_decimal_string(input.amount.decimal());
        if input.order_no.trim().is_empty()
            || input.order_user_key.trim().is_empty()
            || actual_amount.is_empty()
        {
            return Err(GatewayError::config("tokenpay config invalid"));
        }
        // PAY-46: `Currency` is the crypto code from the channel config, never the order's fiat.
        let notify_url = match input.notify_url.trim() {
            "" => cfg.notify_url.clone(),
            v => v.to_owned(),
        };
        let mut members: Vec<(String, RawJson)> = vec![
            (
                "OutOrderId".into(),
                RawJson::String(input.order_no.trim().to_owned()),
            ),
            (
                "OrderUserKey".into(),
                RawJson::String(input.order_user_key.trim().to_owned()),
            ),
            ("ActualAmount".into(), RawJson::String(actual_amount)),
            ("Currency".into(), RawJson::String(cfg.currency.clone())),
        ];
        if !notify_url.is_empty() {
            members.push(("NotifyUrl".into(), RawJson::String(notify_url)));
        }
        if !redirect_url.is_empty() {
            members.push(("RedirectUrl".into(), RawJson::String(redirect_url)));
        }
        let signature = sign_members(&members, &cfg.notify_secret);
        let mut body = Map::new();
        for (k, v) in &members {
            if let RawJson::String(s) = v {
                body.insert(k.clone(), Value::String(s.clone()));
            }
        }
        body.insert("Signature".into(), Value::String(signature));
        let req = HttpRequest::new("POST", format!("{}{CREATE_ORDER_PATH}", cfg.gateway_url))
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .body(go_json_map(&Value::Object(body)));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("tokenpay request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::request(format!(
                "tokenpay request failed: http status {}",
                resp.status
            )));
        }
        let raw = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("tokenpay response invalid: decode response failed")
        })?;
        if raw.get("success").and_then(Value::as_bool) != Some(true) {
            let msg = raw
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default();
            return Err(GatewayError::response(format!(
                "tokenpay response invalid: {msg}"
            )));
        }
        let mut pay_url = match raw.get("data") {
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        if pay_url.is_empty() {
            pay_url = info_string(&raw, "PaymentUrl");
        }
        let qr_link = info_string(&raw, "QrCodeLink");
        let qr_code = if qr_link.is_empty() {
            info_string(&raw, "QrCodeBase64")
        } else {
            qr_link
        };
        Ok(GatewayCreateResult {
            provider_ref: info_string(&raw, "Id"),
            redirect_url: pay_url,
            qr_code_url: qr_code,
            payload: raw,
            ..GatewayCreateResult::default()
        })
    }

    fn verify_callback(
        &self,
        config: &ChannelConfig,
        _form: &FormMap,
        body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Config::parse(config)?;
        let data = RawJson::parse(body)
            .filter(|v| v.members().is_some_and(|m| !m.is_empty()))
            .ok_or_else(|| {
                GatewayError::response("tokenpay response invalid: decode callback failed")
            })?;
        let members = data.members().unwrap_or_default();
        let signature = pick_string(&data, &["Signature", "signature"])
            .trim()
            .to_owned();
        if cfg.notify_secret.is_empty() {
            return Err(GatewayError::config("tokenpay config invalid"));
        }
        if !signature_eq(&sign_members(members, &cfg.notify_secret), &signature) {
            return Err(GatewayError::signature("tokenpay signature invalid"));
        }
        let status = match pick_int(&data, &["Status", "status"]) {
            1 => PaymentStatus::Success,
            2 => PaymentStatus::Expired,
            _ => PaymentStatus::Pending,
        };
        // PAY-46: ActualAmount/BaseCurrency are the fiat values matching the payment row.
        let actual_amount = pick_string(&data, &["ActualAmount", "actual_amount"])
            .trim()
            .to_owned();
        let base_currency = pick_string(&data, &["BaseCurrency", "base_currency"])
            .trim()
            .to_ascii_uppercase();
        let currency = if base_currency.is_empty() {
            cfg.base_currency.clone()
        } else {
            base_currency.clone()
        };
        let raw_value: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
        let text = |keys: &[&str]| Value::String(pick_string(&data, keys).trim().to_owned());
        let mut payload = Map::new();
        payload.insert("Raw".into(), raw_value);
        payload.insert("Signature".into(), Value::String(signature));
        payload.insert("TokenOrderID".into(), text(&["Id", "id"]));
        payload.insert("OutOrderID".into(), text(&["OutOrderId", "out_order_id"]));
        payload.insert(
            "OrderUserKey".into(),
            text(&["OrderUserKey", "order_user_key"]),
        );
        payload.insert(
            "Status".into(),
            Value::from(pick_int(&data, &["Status", "status"])),
        );
        payload.insert("ActualAmount".into(), Value::String(actual_amount.clone()));
        payload.insert("Amount".into(), text(&["Amount", "amount"]));
        payload.insert("BaseCurrency".into(), Value::String(base_currency));
        payload.insert("Currency".into(), text(&["Currency", "currency"]));
        payload.insert("PayTime".into(), text(&["PayTime", "pay_time"]));
        payload.insert(
            "PassThroughInfo".into(),
            text(&["PassThroughInfo", "pass_through_info"]),
        );
        Ok(GatewayCallbackResult {
            order_no: pick_string(&data, &["OutOrderId", "out_order_id"])
                .trim()
                .to_owned(),
            provider_ref: pick_string(&data, &["Id", "id"]).trim().to_owned(),
            status: Some(status),
            amount: callback_amount(&actual_amount),
            currency,
            paid_at: parse_paid_at(&pick_string(&data, &["PayTime", "pay_time"])),
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

    const GO_CALLBACK: &str = r#"{"Id":"TP1","OutOrderId":"DJP1","OrderUserKey":"7","Status":1,"ActualAmount":10.50,"Amount":"1.38","BaseCurrency":"cny","Currency":"USDT_TRC20","PayTime":"2026-09-24 10:00:00","PassThroughInfo":"","Extra":{"b":2,"a":"x<y"},"Flag":true,"Nothing":null,"Signature":"abc"}"#;

    /// Go `SignPayload` vectors (raw number text, nested objects marshalled, nulls skipped).
    #[test]
    fn sign_matches_go() {
        let data = RawJson::parse(GO_CALLBACK.as_bytes()).unwrap_or(RawJson::Null);
        assert_eq!(
            sign_members(data.members().unwrap_or_default(), "secret"),
            "23ba2ae4a4de7578c55df385dd46ac19"
        );
        let create: Vec<(String, RawJson)> = [
            ("OutOrderId", "DJP1"),
            ("OrderUserKey", "a@b.com"),
            ("ActualAmount", "10.5"),
            ("Currency", "USDT_TRC20"),
            ("NotifyUrl", "https://n/cb"),
            ("RedirectUrl", "https://r/pay?x=1&y=2"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), RawJson::String((*v).to_owned())))
        .collect();
        assert_eq!(
            sign_members(&create, "secret"),
            "e80c63b7b3dd5e15ad1274d831e8d84c"
        );
    }

    fn cfg() -> ChannelConfig {
        obj(
            json!({"gateway_url": "https://tp.example.com", "notify_secret": "secret", "currency": "USDT_TRC20"}),
        )
    }

    fn signed_callback(actual: &str, base_currency: &str) -> String {
        let mut v = json!({"Id": "TP1", "OutOrderId": "DJP1", "OrderUserKey": "7", "Status": 1,
            "ActualAmount": actual, "Amount": "1.38", "BaseCurrency": base_currency, "Currency": "USDT_TRC20",
            "PayTime": "2026-09-24 10:00:00"});
        let text = v.to_string();
        let data = RawJson::parse(text.as_bytes()).unwrap_or(RawJson::Null);
        let sign = sign_members(data.members().unwrap_or_default(), "secret");
        v["Signature"] = json!(sign);
        v.to_string()
    }

    /// PAY-46: callbacks are compared on the fiat ActualAmount/BaseCurrency.
    #[test]
    fn pay_46_callback_uses_fiat_fields() {
        let gw = TokenpayGateway::new(env_with(MockTransport::fixed(500, "")));
        let res = gw
            .verify_callback(
                &cfg(),
                &FormMap::new(),
                signed_callback("10.00", "cny").as_bytes(),
            )
            .unwrap_or_default();
        assert_eq!(res.amount, Amount::from(10));
        assert_eq!(res.currency, "CNY");
        assert_eq!(res.status, Some(PaymentStatus::Success));
        assert_eq!(res.order_no, "DJP1");
        assert_eq!(res.provider_ref, "TP1");
        assert_eq!(
            res.paid_at.map(|t| t.to_rfc3339()),
            Some("2026-09-24T02:00:00+00:00".into())
        );
        let mut tampered: Value =
            serde_json::from_str(&signed_callback("10.00", "cny")).unwrap_or_default();
        tampered["ActualAmount"] = json!("9.99");
        let r = gw.verify_callback(&cfg(), &FormMap::new(), tampered.to_string().as_bytes());
        assert!(matches!(r, Err(GatewayError::SignatureInvalid(_))));
    }

    /// PAY-46 / PAY-47: create request sends the channel's crypto code verbatim and fiat amount.
    #[tokio::test]
    async fn pay_47_create_passes_currency_verbatim() {
        let mock = MockTransport::fixed(
            200,
            r#"{"success":true,"data":"https://tp/pay/1","info":{"Id":"TP1","QrCodeLink":"https://tp/qr/1"}}"#,
        );
        let gw = TokenpayGateway::new(env_with(mock.clone()));
        let mut c = cfg();
        c.insert("currency".into(), json!("usdt_trc20"));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            order_user_key: "7".into(),
            amount: Amount::from(10),
            currency: "CNY".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&c, &input).await.unwrap_or_default();
        assert_eq!(res.provider_ref, "TP1");
        assert_eq!(res.redirect_url, "https://tp/pay/1");
        assert_eq!(res.qr_code_url, "https://tp/qr/1");
        let body: Value = serde_json::from_slice(&mock.requests()[0].body).unwrap_or_default();
        assert_eq!(body["Currency"], "usdt_trc20");
        assert_eq!(body["ActualAmount"], "10");
        assert_eq!(mock.requests()[0].url, "https://tp.example.com/CreateOrder");
    }
}
