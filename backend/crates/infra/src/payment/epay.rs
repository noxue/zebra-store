//! 易支付 (epay) v1 (MD5) / v2 (RSA-SHA256) gateway (`gateway/epay`, `adapters/epay`).

use std::collections::BTreeMap;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{FormMap, encode_pairs, form_raw, form_to_json, form_value};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{InteractionMode, PaymentStatus, channel_type};

use super::common::{
    ExchangeRate, GatewayEnv, RsaHash, callback_amount, go_decimal_string, md5_hex, parse_config,
    parse_rsa_private_key, parse_rsa_public_key, rsa_sign, rsa_verify, signature_eq,
};
use super::http::HttpRequest;

const VERSION_V2: &str = "v2";
const SIGN_TYPE_RSA: &str = "RSA";
const SIGN_TYPE_MD5: &str = "MD5";
const API_PATH_V2: &str = "/api/pay/create";
const API_PATH_V1: &str = "/mapi.php";
const SUBMIT_PATH_V2: &str = "/api/pay/submit";
const SUBMIT_PATH_V1: &str = "/submit.php";
const PAY_TYPE_QRCODE: &str = "qrcode";
const ACCEPT_LANGUAGE: &str = "zh-CN,zh;q=0.9,en;q=0.8";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

/// Epay channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub gateway_url: String,
    pub epay_version: String,
    pub merchant_id: String,
    pub merchant_key: String,
    pub private_key: String,
    #[serde(rename = "platform_public_key")]
    pub public_key: String,
    pub sign_type: String,
    pub api_path: String,
    pub notify_url: String,
    pub return_url: String,
    pub method: String,
    pub device: String,
    pub target_currency: String,
    pub exchange_rate: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "epay")?;
        c.epay_version = c.epay_version.trim().to_ascii_lowercase();
        c.sign_type = c.sign_type.trim().to_owned();
        let v2 = c.epay_version == VERSION_V2;
        if c.sign_type.is_empty() {
            c.sign_type = if v2 { SIGN_TYPE_RSA } else { SIGN_TYPE_MD5 }.to_owned();
        }
        if c.api_path.is_empty() {
            c.api_path = if v2 { API_PATH_V2 } else { API_PATH_V1 }.to_owned();
        }
        if c.method.is_empty() {
            c.method = "web".to_owned();
        }
        if c.device.is_empty() {
            c.device = "pc".to_owned();
        }
        Ok(c)
    }

    fn is_v2(&self) -> bool {
        self.epay_version == VERSION_V2
    }

    fn exchange(&self) -> ExchangeRate {
        ExchangeRate::new(&self.target_currency, &self.exchange_rate)
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        let required = |v: &str, name: &str| {
            if v.trim().is_empty() {
                Err(GatewayError::config(format!(
                    "epay config invalid: {name} is required"
                )))
            } else {
                Ok(())
            }
        };
        required(&self.gateway_url, "gateway_url")?;
        required(&self.merchant_id, "merchant_id")?;
        required(&self.notify_url, "notify_url")?;
        required(&self.return_url, "return_url")?;
        if self.is_v2() {
            required(&self.private_key, "private_key")?;
            required(&self.public_key, "platform_public_key")?;
        } else {
            required(&self.merchant_key, "merchant_key")?;
        }
        Ok(())
    }
}

/// Maps a channel type to the epay `type` parameter.
pub fn resolve_pay_type(channel: &str) -> Option<&'static str> {
    match channel.trim().to_ascii_lowercase().as_str() {
        channel_type::WECHAT | channel_type::WXPAY => Some(channel_type::WXPAY),
        channel_type::ALIPAY => Some(channel_type::ALIPAY),
        channel_type::QQPAY => Some(channel_type::QQPAY),
        _ => None,
    }
}

/// `buildSignContent`: sorted `k=v` of non-empty params except `sign`/`sign_type`.
pub fn sign_content(params: &BTreeMap<String, String>) -> String {
    params
        .iter()
        .filter(|(k, v)| !v.is_empty() && k.as_str() != "sign" && k.as_str() != "sign_type")
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// MD5 signature: `md5(content + key)` lower hex.
pub fn sign_md5(content: &str, key: &str) -> String {
    md5_hex(format!("{content}{key}").as_bytes())
}

fn sign_rsa(content: &str, private_key: &str) -> Result<String, GatewayError> {
    let key = parse_rsa_private_key(private_key)
        .ok_or_else(|| GatewayError::config("epay signature generate failed"))?;
    rsa_sign(&key, RsaHash::Sha256, content.as_bytes())
        .ok_or_else(|| GatewayError::config("epay signature generate failed"))
}

fn endpoint(gateway_url: &str, path: &str) -> String {
    let base = gateway_url.trim().trim_end_matches('/');
    let path = path.trim();
    if path.is_empty() {
        return base.to_owned();
    }
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

/// Verifies callback signature (`VerifyCallback`) and merchant ownership (`VerifyCallbackOwnership`).
pub fn verify_callback_form(cfg: &Config, form: &FormMap) -> Result<(), GatewayError> {
    let sign = form_raw(form, "sign").trim().to_owned();
    if sign.is_empty() {
        return Err(GatewayError::signature("epay signature invalid"));
    }
    let params: BTreeMap<String, String> = form
        .iter()
        .filter_map(|(k, v)| v.first().map(|first| (k.clone(), first.clone())))
        .collect();
    let content = sign_content(&params);
    if cfg.is_v2() {
        let key = parse_rsa_public_key(&cfg.public_key)
            .ok_or_else(|| GatewayError::signature("epay signature invalid"))?;
        if !rsa_verify(&key, RsaHash::Sha256, content.as_bytes(), &sign) {
            return Err(GatewayError::signature("epay signature invalid"));
        }
    } else {
        // PAY-01: an empty key would let anyone forge a valid MD5 signature.
        if cfg.merchant_key.trim().is_empty() {
            return Err(GatewayError::config("epay config invalid"));
        }
        if !signature_eq(&sign_md5(&content, &cfg.merchant_key), &sign) {
            return Err(GatewayError::signature("epay signature invalid"));
        }
    }
    // PAY-35: the callback must belong to this channel's merchant.
    let pid = form_raw(form, "pid").trim().to_owned();
    if pid.is_empty() || pid != cfg.merchant_id.trim() {
        return Err(GatewayError::signature(
            "epay signature invalid: merchant mismatch",
        ));
    }
    Ok(())
}

/// Unwraps a response that is a JSON string containing JSON (`normalizeResponseBody`, PAY-36).
pub fn normalize_response_body(body: &[u8]) -> Result<Vec<u8>, GatewayError> {
    let text = String::from_utf8_lossy(body);
    let trimmed = text.trim();
    if !trimmed.starts_with('"') {
        return Ok(trimmed.as_bytes().to_vec());
    }
    let inner: String = serde_json::from_str(trimmed)
        .map_err(|_| GatewayError::response("epay response invalid"))?;
    Ok(inner.trim().as_bytes().to_vec())
}

fn typed_str(map: &Map<String, Value>, key: &str) -> Result<String, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(s)) => Ok(s.clone()),
        Some(_) => Err(GatewayError::response("epay response invalid")),
    }
}

fn typed_int(map: &Map<String, Value>, key: &str) -> Result<i64, GatewayError> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(0),
        Some(Value::Number(n)) => n
            .as_i64()
            .ok_or_else(|| GatewayError::response("epay response invalid")),
        Some(_) => Err(GatewayError::response("epay response invalid")),
    }
}

/// The epay adapter.
#[derive(Debug, Clone)]
pub struct EpayGateway {
    env: GatewayEnv,
}

struct Native {
    order_no: String,
    amount: String,
    subject: String,
    pay_type: &'static str,
    client_ip: String,
    notify_url: String,
    return_url: String,
}

struct NativeResult {
    pay_url: String,
    qr_code: String,
    trade_no: String,
    raw: Map<String, Value>,
}

impl EpayGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn checked_config(raw: &ChannelConfig) -> Result<Config, GatewayError> {
        let cfg = Config::parse(raw)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn signed_params(
        &self,
        cfg: &Config,
        mut params: BTreeMap<String, String>,
    ) -> Result<BTreeMap<String, String>, GatewayError> {
        let content = sign_content(&params);
        let sign = if cfg.is_v2() {
            if cfg.private_key.is_empty() {
                return Err(GatewayError::config("epay config invalid"));
            }
            sign_rsa(&content, &cfg.private_key)?
        } else {
            if cfg.merchant_key.is_empty() {
                return Err(GatewayError::config("epay config invalid"));
            }
            sign_md5(&content, &cfg.merchant_key)
        };
        params.insert("sign".into(), sign);
        params.insert("sign_type".into(), cfg.sign_type.clone());
        Ok(params)
    }

    fn base_params(&self, cfg: &Config, n: &Native) -> BTreeMap<String, String> {
        let mut p = BTreeMap::new();
        p.insert("pid".to_owned(), cfg.merchant_id.clone());
        p.insert("type".to_owned(), n.pay_type.to_owned());
        p.insert("out_trade_no".to_owned(), n.order_no.clone());
        p.insert("notify_url".to_owned(), n.notify_url.clone());
        p.insert("return_url".to_owned(), n.return_url.clone());
        p.insert("name".to_owned(), n.subject.clone());
        p.insert("money".to_owned(), n.amount.clone());
        p
    }

    /// `BuildRedirectURL`: a locally signed page-jump URL (no HTTP request).
    fn build_redirect(&self, cfg: &Config, n: &Native) -> Result<NativeResult, GatewayError> {
        let mut params = self.base_params(cfg, n);
        let submit = if cfg.is_v2() {
            params.insert(
                "timestamp".into(),
                self.env.clock.now().timestamp().to_string(),
            );
            SUBMIT_PATH_V2
        } else {
            SUBMIT_PATH_V1
        };
        let params = self.signed_params(cfg, params)?;
        let endpoint = endpoint(&cfg.gateway_url, submit);
        let pairs: Vec<(String, String)> = params
            .iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let encoded = encode_pairs(&pairs);
        let pay_url = if encoded.is_empty() {
            endpoint.clone()
        } else {
            format!("{endpoint}?{encoded}")
        };
        let raw_params: Map<String, Value> = params
            .iter()
            .map(|(k, v)| (k.clone(), Value::String(v.clone())))
            .collect();
        let mut raw = Map::new();
        raw.insert("mode".into(), Value::String("redirect".into()));
        raw.insert("endpoint".into(), Value::String(endpoint));
        raw.insert("params".into(), Value::Object(raw_params));
        Ok(NativeResult {
            pay_url,
            qr_code: String::new(),
            trade_no: String::new(),
            raw,
        })
    }

    /// `CreatePayment`: server-to-server order creation (`mapi.php` / `api/pay/create`).
    async fn create_api(&self, cfg: &Config, n: &Native) -> Result<NativeResult, GatewayError> {
        if n.client_ip.is_empty() {
            return Err(GatewayError::config(
                "epay config invalid: client ip is required",
            ));
        }
        let mut params = self.base_params(cfg, n);
        params.insert("clientip".into(), n.client_ip.clone());
        if cfg.is_v2() {
            params.insert("method".into(), cfg.method.clone());
            params.insert(
                "timestamp".into(),
                self.env.clock.now().timestamp().to_string(),
            );
        } else {
            params.insert("device".into(), cfg.device.clone());
        }
        let params = self.signed_params(cfg, params)?;
        let pairs: Vec<(String, String)> = params
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let req = HttpRequest::new("POST", endpoint(&cfg.gateway_url, &cfg.api_path))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Accept", "application/json, text/plain, */*")
            .header("Accept-Encoding", "identity")
            .header("Accept-Language", ACCEPT_LANGUAGE)
            .header("User-Agent", USER_AGENT)
            .body(encode_pairs(&pairs));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("epay request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::request("epay request failed"));
        }
        let body = normalize_response_body(&resp.body)?;
        let raw = match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(m)) => m,
            Ok(_) | Err(_) => return Err(GatewayError::response("epay response invalid")),
        };
        let code = typed_int(&raw, "code")?;
        let msg = typed_str(&raw, "msg")?;
        let trade_no = typed_str(&raw, "trade_no")?.trim().to_owned();
        if cfg.is_v2() {
            let pay_type = typed_str(&raw, "pay_type")?.trim().to_owned();
            let pay_info = typed_str(&raw, "pay_info")?.trim().to_owned();
            if code != 0 {
                return Err(GatewayError::response(format!(
                    "epay response invalid: {msg}"
                )));
            }
            let (pay_url, qr_code) = if pay_type.eq_ignore_ascii_case(PAY_TYPE_QRCODE) {
                (String::new(), pay_info)
            } else {
                (pay_info, String::new())
            };
            Ok(NativeResult {
                pay_url,
                qr_code,
                trade_no,
                raw,
            })
        } else {
            let pay_url = typed_str(&raw, "payurl")?.trim().to_owned();
            let qr_code = typed_str(&raw, "qrcode")?.trim().to_owned();
            let scheme = typed_str(&raw, "urlscheme")?;
            if code != 1 {
                return Err(GatewayError::response(format!(
                    "epay response invalid: {msg}"
                )));
            }
            let pay_url = if pay_url.is_empty() && !scheme.is_empty() {
                scheme.trim().to_owned()
            } else {
                pay_url
            };
            Ok(NativeResult {
                pay_url,
                qr_code,
                trade_no,
                raw,
            })
        }
    }
}

#[async_trait]
impl PaymentGateway for EpayGateway {
    fn key(&self) -> &'static str {
        "epay:"
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
        if !channel_type.is_empty() && resolve_pay_type(channel_type).is_none() {
            return Err(GatewayError::UnsupportedChannel(format!(
                "epay channel_type {channel_type}"
            )));
        }
        Self::checked_config(config).map(|_| ())
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let pay_type = resolve_pay_type(&input.channel_type).ok_or_else(|| {
            GatewayError::UnsupportedChannel(format!("epay channel_type {}", input.channel_type))
        })?;
        let cfg = Self::checked_config(config)?;
        let notify_url = match input.notify_url.trim() {
            "" => cfg.notify_url.trim().to_owned(),
            v => v.to_owned(),
        };
        let return_url = match input.return_url.trim() {
            "" => cfg.return_url.trim().to_owned(),
            v => v.to_owned(),
        };
        let return_url = append_query_params(&return_url, &input.return_url_query);
        let original_amount = go_decimal_string(input.amount.decimal());
        let exchange = cfg.exchange();
        let (pay_amount, pay_currency) = exchange.convert(&original_amount, &input.currency, 2)?;
        if input.order_no.is_empty()
            || pay_amount.is_empty()
            || notify_url.is_empty()
            || return_url.is_empty()
        {
            return Err(GatewayError::config("epay config invalid"));
        }
        let native = Native {
            order_no: input.order_no.clone(),
            amount: pay_amount.clone(),
            subject: if input.subject.is_empty() {
                input.order_no.clone()
            } else {
                input.subject.clone()
            },
            pay_type,
            client_ip: input.client_ip.clone(),
            notify_url,
            return_url,
        };
        let result = match input.interaction_mode {
            Some(InteractionMode::Redirect) => self.build_redirect(&cfg, &native)?,
            None | Some(InteractionMode::Qr) => self.create_api(&cfg, &native).await?,
            Some(other) => {
                return Err(GatewayError::config(format!(
                    "epay interaction_mode {other}"
                )));
            }
        };
        let mut payload = result.raw;
        if exchange.needs_conversion() {
            exchange.audit(&mut payload, &original_amount, &input.currency);
        }
        Ok(GatewayCreateResult {
            provider_ref: result.trade_no,
            redirect_url: result.pay_url,
            qr_code_url: result.qr_code,
            payload,
            display_channel_type: String::new(),
            amount_sent: pay_amount,
            currency_sent: pay_currency,
        })
    }

    fn verify_callback(
        &self,
        config: &ChannelConfig,
        form: &FormMap,
        _body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Config::parse(config)?;
        verify_callback_form(&cfg, form)?;
        let trade_status = form_value(form, "trade_status");
        let status = if trade_status == "TRADE_SUCCESS" || trade_status == "TRADE_FINISHED" {
            PaymentStatus::Success
        } else {
            PaymentStatus::Pending
        };
        Ok(GatewayCallbackResult {
            order_no: form_value(form, "out_trade_no"),
            provider_ref: form_value(form, "trade_no"),
            status: Some(status),
            amount: callback_amount(&form_value(form, "money")),
            currency: "CNY".to_owned(),
            paid_at: None,
            payload: form_to_json(form),
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::payment::http::{HttpResponse, MockTransport};
    use crate::payment::test_support::{PRIV_PEM, PUB_PEM, env_with, obj};
    use serde_json::json;
    use zs_domain::payment::form::pairs_to_form;
    use zs_shared::money::Amount;

    fn params(items: &[(&str, &str)]) -> BTreeMap<String, String> {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn vector_params() -> BTreeMap<String, String> {
        params(&[
            ("pid", "1001"),
            ("type", "alipay"),
            ("out_trade_no", "DJP20260924000001123456"),
            (
                "notify_url",
                "https://shop.example.com/api/v1/payments/callback",
            ),
            (
                "return_url",
                "https://shop.example.com/pay?biz_type=order&epay_return=1&order_no=DJ1",
            ),
            ("name", "DJ1 测试"),
            ("money", "9.9"),
            ("clientip", "127.0.0.1"),
            ("device", "pc"),
            ("sign", "x"),
            ("sign_type", "MD5"),
            ("empty", ""),
        ])
    }

    /// Go vectors: `buildSignContent`, `signMD5`, `signRSA` (PKCS#1 v1.5 is deterministic).
    #[test]
    fn sign_vectors_match_go() {
        let content = sign_content(&vector_params());
        assert_eq!(
            content,
            "clientip=127.0.0.1&device=pc&money=9.9&name=DJ1 测试&notify_url=https://shop.example.com/api/v1/payments/callback&out_trade_no=DJP20260924000001123456&pid=1001&return_url=https://shop.example.com/pay?biz_type=order&epay_return=1&order_no=DJ1&type=alipay"
        );
        assert_eq!(
            sign_md5(&content, "key123"),
            "a5752727f1f078297e215130e241a24f"
        );
        let rsa = sign_rsa(&content, PRIV_PEM).unwrap_or_default();
        assert_eq!(rsa, GO_EPAY_RSA);
        let pubkey = parse_rsa_public_key(PUB_PEM);
        assert!(pubkey.is_some_and(|k| rsa_verify(&k, RsaHash::Sha256, content.as_bytes(), &rsa)));
    }

    const GO_EPAY_RSA: &str = "hoooqzaZpUWLLG6CBSdoyMXVdhcO/Zs1XwPx+/klcWeBVRqyNYw2rHNL58bQ7ZqHJCElQ7rrtji1+Zv8TcVjRq7B2FNgYl2otX06NRWCDwn9eQMBihDmw0PMdhK/5pCN5pSfCXi+YY06upcssGxbfU6nw4xqJM7MIRAiULlFRt1JJbPxa70IrW54uH6ryPxulZ66pyELoiCf5AvSa49Pht0JslWpjvWPvqvIkOPpm/19A80uYmbiuuzjmkH2GQQGC2sWgOL1TqBhoMUc5vrcbcmrHd2dbxiQL0q7UOvGgpgKHCpY8mROHuAk/H0qHVsjmM5wX9lW3Vm3K2G1mjAfxw==";

    fn v1_config() -> ChannelConfig {
        obj(json!({
            "gateway_url": "https://pay.example.com/", "merchant_id": "1001", "merchant_key": "key123",
            "notify_url": "https://n.example.com/cb", "return_url": "https://r.example.com/pay"
        }))
    }

    /// PAY-37: redirect mode builds `/submit.php` URL identical to Go `BuildRedirectURL`.
    #[tokio::test]
    async fn pay_37_redirect_v1_matches_go() {
        let gw = EpayGateway::new(env_with(MockTransport::fixed(500, "")));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from_cents(990),
            subject: "DJ1 测试 &+".into(),
            channel_type: "wechat".into(),
            notify_url: "https://n.example.com/cb".into(),
            return_url: "https://r.example.com/pay?a=1&b=x y".into(),
            interaction_mode: Some(InteractionMode::Redirect),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&v1_config(), &input).await;
        let res = res.unwrap_or_default();
        // Go appends no query params here; our input.return_url_query is empty, but the
        // return URL is re-encoded by AppendQueryParams only when params are present.
        assert_eq!(
            res.redirect_url,
            "https://pay.example.com/submit.php?money=9.9&name=DJ1+%E6%B5%8B%E8%AF%95+%26%2B&notify_url=https%3A%2F%2Fn.example.com%2Fcb&out_trade_no=DJP1&pid=1001&return_url=https%3A%2F%2Fr.example.com%2Fpay%3Fa%3D1%26b%3Dx+y&sign=44ef84a19ddfa6550710569f7128e646&sign_type=MD5&type=wxpay"
        );
        assert_eq!(res.payload["mode"], "redirect");
        assert_eq!(res.amount_sent, "9.9");
    }

    /// PAY-36: v1 API creation posts the Go-identical form and unwraps double-encoded JSON.
    #[tokio::test]
    async fn pay_36_create_v1_double_encoded_response() {
        let mock = MockTransport::new(|_| {
            Ok(HttpResponse::new(
                200,
                r#""{\"code\":1,\"trade_no\":\"T1\",\"payurl\":\"https://x\"}""#,
            ))
        });
        let gw = EpayGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from_cents(990),
            subject: "DJ1".into(),
            channel_type: "qqpay".into(),
            client_ip: "1.2.3.4".into(),
            notify_url: "https://n.example.com/cb".into(),
            return_url: "https://r.example.com/pay".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw
            .create_payment(&v1_config(), &input)
            .await
            .unwrap_or_default();
        assert_eq!(res.provider_ref, "T1");
        assert_eq!(res.redirect_url, "https://x");
        let reqs = mock.requests();
        assert_eq!(reqs[0].url, "https://pay.example.com/mapi.php");
        assert_eq!(reqs[0].header_value("Accept-Encoding"), Some("identity"));
        assert_eq!(
            String::from_utf8_lossy(&reqs[0].body),
            "clientip=1.2.3.4&device=pc&money=9.9&name=DJ1&notify_url=https%3A%2F%2Fn.example.com%2Fcb&out_trade_no=DJP1&pid=1001&return_url=https%3A%2F%2Fr.example.com%2Fpay&sign=853a871baa025eb4fe831c6669579b74&sign_type=MD5&type=qqpay"
        );
    }

    /// PAY-36: v2 double-encoded QR response.
    #[tokio::test]
    async fn pay_36_create_v2_qrcode() {
        let mock = MockTransport::fixed(
            200,
            r#""{\"code\":0,\"trade_no\":\"T2\",\"pay_type\":\"qrcode\",\"pay_info\":\"weixin://x\"}""#,
        );
        let gw = EpayGateway::new(env_with(mock.clone()));
        let cfg = obj(json!({
            "gateway_url": "https://pay.example.com", "merchant_id": "1001", "epay_version": "V2",
            "private_key": PRIV_PEM, "platform_public_key": PUB_PEM,
            "notify_url": "https://n/cb", "return_url": "https://r/pay"
        }));
        let input = GatewayCreateInput {
            order_no: "DJP2".into(),
            amount: Amount::from(10),
            channel_type: "alipay".into(),
            client_ip: "1.2.3.4".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&cfg, &input).await.unwrap_or_default();
        assert_eq!(res.qr_code_url, "weixin://x");
        assert_eq!(res.provider_ref, "T2");
        let body = String::from_utf8_lossy(&mock.requests()[0].body).into_owned();
        assert!(
            body.contains("method=web")
                && body.contains("timestamp=1782680000")
                && body.contains("sign_type=RSA")
        );
        assert_eq!(
            mock.requests()[0].url,
            "https://pay.example.com/api/pay/create"
        );
    }

    fn signed_form(pid: &str, key: &str) -> FormMap {
        let mut p = params(&[
            ("pid", pid),
            ("trade_no", "T1"),
            ("out_trade_no", "DJP1"),
            ("type", "alipay"),
            ("name", "DJ1"),
            ("money", "9.90"),
            ("trade_status", "TRADE_SUCCESS"),
            ("sign_type", "MD5"),
        ]);
        let sign = sign_md5(&sign_content(&p), key);
        p.insert("sign".into(), sign.to_ascii_uppercase());
        pairs_to_form(p.into_iter().collect())
    }

    /// PAY-35 / PAY-01: pid ownership, empty-key rejection, case-insensitive signatures.
    #[test]
    fn pay_35_callback_ownership_and_keys() {
        let gw = EpayGateway::new(env_with(MockTransport::fixed(500, "")));
        let ok = gw.verify_callback(&v1_config(), &signed_form("1001", "key123"), b"");
        let ok = ok.unwrap_or_default();
        assert_eq!(ok.status, Some(PaymentStatus::Success));
        assert_eq!(ok.amount, Amount::from_cents(990));
        assert_eq!(ok.order_no, "DJP1");
        let wrong_pid = gw.verify_callback(&v1_config(), &signed_form("1002", "key123"), b"");
        assert!(matches!(wrong_pid, Err(GatewayError::SignatureInvalid(_))));
        let mut missing_pid = signed_form("1001", "key123");
        missing_pid.remove("pid");
        assert!(gw.verify_callback(&v1_config(), &missing_pid, b"").is_err());
        let mut empty_key = v1_config();
        empty_key.insert("merchant_key".into(), json!(" "));
        let forged = gw.verify_callback(&empty_key, &signed_form("1001", ""), b"");
        assert!(matches!(forged, Err(GatewayError::ConfigInvalid(_))));
        let tampered = {
            let mut f = signed_form("1001", "key123");
            f.insert("money".into(), vec!["0.01".into()]);
            f
        };
        assert!(gw.verify_callback(&v1_config(), &tampered, b"").is_err());
    }

    /// PAY-35 (v2): RSA-signed callbacks with pid checks.
    #[test]
    fn pay_35_callback_v2_rsa() {
        let gw = EpayGateway::new(env_with(MockTransport::fixed(500, "")));
        let cfg = obj(
            json!({"epay_version": "v2", "merchant_id": "1001", "platform_public_key": PUB_PEM}),
        );
        let make = |pid: &str| {
            let mut p = params(&[
                ("pid", pid),
                ("out_trade_no", "DJP1"),
                ("trade_status", "TRADE_SUCCESS"),
                ("money", "1"),
            ]);
            let sign = sign_rsa(&sign_content(&p), PRIV_PEM).unwrap_or_default();
            p.insert("sign".into(), sign);
            pairs_to_form(p.into_iter().collect())
        };
        assert!(gw.verify_callback(&cfg, &make("1001"), b"").is_ok());
        assert!(gw.verify_callback(&cfg, &make("1002"), b"").is_err());
    }

    #[test]
    fn validates_config_and_channel_types() {
        let gw = EpayGateway::new(env_with(MockTransport::fixed(500, "")));
        assert!(gw.validate_config(&v1_config(), "wechat").is_ok());
        assert!(matches!(
            gw.validate_config(&v1_config(), "paypal"),
            Err(GatewayError::UnsupportedChannel(_))
        ));
        assert!(
            gw.validate_config(&obj(json!({"gateway_url": "x"})), "")
                .is_err()
        );
        assert!(
            gw.validate_config(&obj(json!({"gateway_url": 1})), "")
                .is_err()
        );
    }
}
