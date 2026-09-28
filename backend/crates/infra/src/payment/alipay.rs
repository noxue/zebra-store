//! Official Alipay gateway (RSA2/RSA, cert SN pass-through, precreate/wap/page) — PAY-01, PAY-22, PAY-34.

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{FormMap, encode_pairs, form_raw, form_to_json, form_value};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{InteractionMode, PaymentStatus};

use super::common::{
    ExchangeRate, GatewayEnv, RsaHash, callback_amount, go_decimal_string, go_json_map,
    is_request_uri, parse_config, parse_decimal, parse_rsa_private_key, parse_rsa_public_key,
    rsa_sign, rsa_verify,
};
use super::http::HttpRequest;

const SIGN_TYPE_RSA2: &str = "RSA2";
const SIGN_TYPE_RSA: &str = "RSA";
const GATEWAY_DEFAULT: &str = "https://openapi.alipay.com/gateway.do";
const RESP_CODE_SUCCESS: &str = "10000";
/// Alipay timestamps and `gmt_payment` are China Standard Time (UTC+8).
const ALIPAY_OFFSET_SECS: i32 = 8 * 3600;

/// Alipay channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub app_id: String,
    pub private_key: String,
    pub alipay_public_key: String,
    pub gateway_url: String,
    pub notify_url: String,
    pub return_url: String,
    pub sign_type: String,
    pub app_cert_sn: String,
    pub alipay_root_cert_sn: String,
    pub target_currency: String,
    pub exchange_rate: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "alipay")?;
        c.app_id = c.app_id.trim().to_owned();
        c.private_key = c.private_key.trim().to_owned();
        c.alipay_public_key = c.alipay_public_key.trim().to_owned();
        c.gateway_url = c.gateway_url.trim().to_owned();
        c.notify_url = c.notify_url.trim().to_owned();
        c.return_url = c.return_url.trim().to_owned();
        c.sign_type = c.sign_type.trim().to_ascii_uppercase();
        c.app_cert_sn = c.app_cert_sn.trim().to_owned();
        c.alipay_root_cert_sn = c.alipay_root_cert_sn.trim().to_owned();
        if c.sign_type.is_empty() {
            c.sign_type = SIGN_TYPE_RSA2.to_owned();
        }
        if c.gateway_url.is_empty() {
            c.gateway_url = GATEWAY_DEFAULT.to_owned();
        }
        Ok(c)
    }

    fn exchange(&self) -> ExchangeRate {
        ExchangeRate::new(&self.target_currency, &self.exchange_rate)
    }

    /// `ValidateConfig(cfg, interactionMode)`.
    pub fn validate(&self, mode: &str) -> Result<(), GatewayError> {
        let invalid = |m: &str| Err(GatewayError::config(format!("alipay config invalid: {m}")));
        for (name, value) in [
            ("app_id", &self.app_id),
            ("private_key", &self.private_key),
            ("alipay_public_key", &self.alipay_public_key),
            ("gateway_url", &self.gateway_url),
            ("notify_url", &self.notify_url),
        ] {
            if value.is_empty() {
                return invalid(&format!("{name} is required"));
            }
        }
        if !is_request_uri(&self.gateway_url) {
            return invalid("gateway_url is invalid");
        }
        if !is_request_uri(&self.notify_url) {
            return invalid("notify_url is invalid");
        }
        if !self.return_url.is_empty() && !is_request_uri(&self.return_url) {
            return invalid("return_url is invalid");
        }
        let mode = InteractionMode::parse(mode);
        if !matches!(
            mode,
            Some(InteractionMode::Qr | InteractionMode::Wap | InteractionMode::Page)
        ) {
            return invalid("interaction_mode is not supported");
        }
        if matches!(mode, Some(InteractionMode::Wap | InteractionMode::Page))
            && self.return_url.is_empty()
        {
            return invalid("return_url is required for this mode");
        }
        if self.sign_type != SIGN_TYPE_RSA2 && self.sign_type != SIGN_TYPE_RSA {
            return invalid("sign_type is invalid");
        }
        Ok(())
    }
}

/// `buildSignContent`: sorted `k=v` of non-empty params except `sign`.
pub fn sign_content(params: &[(String, String)]) -> String {
    let mut items: Vec<&(String, String)> = params
        .iter()
        .filter(|(k, v)| !k.trim().is_empty() && k.trim() != "sign" && !v.trim().is_empty())
        .collect();
    items.sort_by(|a, b| a.0.cmp(&b.0));
    items
        .iter()
        .map(|(k, v)| format!("{}={v}", k.trim()))
        .collect::<Vec<_>>()
        .join("&")
}

fn hash_for(sign_type: &str) -> RsaHash {
    if sign_type.eq_ignore_ascii_case(SIGN_TYPE_RSA) {
        RsaHash::Sha1
    } else {
        RsaHash::Sha256
    }
}

/// Signs the content with the merchant private key (`signContent`).
pub fn sign(content: &str, private_key: &str, sign_type: &str) -> Result<String, GatewayError> {
    let content = content.trim();
    if content.is_empty() {
        return Err(GatewayError::config(
            "alipay sign generate failed: empty sign content",
        ));
    }
    let key = parse_rsa_private_key(private_key).ok_or_else(|| {
        GatewayError::config("alipay sign generate failed: parse private key failed")
    })?;
    rsa_sign(&key, hash_for(sign_type), content.as_bytes())
        .ok_or_else(|| GatewayError::config("alipay sign generate failed"))
}

/// `buildGatewayPayURL`: gateway URL with the sorted, encoded params as query.
pub fn gateway_url_with(gateway: &str, params: &[(String, String)]) -> String {
    let pairs: Vec<(String, String)> = params
        .iter()
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .filter(|(k, v)| !k.is_empty() && !v.is_empty())
        .collect();
    let base = gateway.trim();
    if base.is_empty() {
        return String::new();
    }
    let base = base.split('?').next().unwrap_or(base);
    format!("{base}?{}", encode_pairs(&pairs))
}

/// Verifies an async notification: signature with the configured sign type only (PAY-01),
/// then `app_id` ownership (PAY-34).
pub fn verify_notification(cfg: &Config, form: &FormMap) -> Result<(), GatewayError> {
    if form.is_empty() {
        return Err(GatewayError::signature(
            "alipay signature invalid: callback form is empty",
        ));
    }
    let sign = form_raw(form, "sign").trim().to_owned();
    if sign.is_empty() {
        return Err(GatewayError::signature(
            "alipay signature invalid: sign is required",
        ));
    }
    let sign_type = match cfg.sign_type.trim().to_ascii_uppercase() {
        t if t.is_empty() => SIGN_TYPE_RSA2.to_owned(),
        t => t,
    };
    if sign_type != SIGN_TYPE_RSA2 && sign_type != SIGN_TYPE_RSA {
        return Err(GatewayError::signature(
            "alipay signature invalid: sign_type is invalid",
        ));
    }
    let params: Vec<(String, String)> = form
        .iter()
        .filter(|(k, _)| {
            let k = k.trim();
            !k.is_empty() && !k.eq_ignore_ascii_case("sign") && !k.eq_ignore_ascii_case("sign_type")
        })
        .filter_map(|(k, v)| {
            v.first()
                .filter(|s| !s.is_empty())
                .map(|s| (k.trim().to_owned(), s.clone()))
        })
        .collect();
    let content = sign_content(&params);
    if content.is_empty() {
        return Err(GatewayError::signature(
            "alipay signature invalid: sign content is empty",
        ));
    }
    let key = parse_rsa_public_key(&cfg.alipay_public_key).ok_or_else(|| {
        GatewayError::signature("alipay signature invalid: parse public key failed")
    })?;
    if !rsa_verify(&key, hash_for(&sign_type), content.as_bytes(), &sign) {
        return Err(GatewayError::signature(
            "alipay signature invalid: verify failed",
        ));
    }
    let mut app_id = form_raw(form, "app_id").trim().to_owned();
    if app_id.is_empty() {
        app_id = form_raw(form, "appid").trim().to_owned();
    }
    if app_id.is_empty() {
        return Err(GatewayError::signature(
            "alipay signature invalid: app_id is required",
        ));
    }
    if !app_id.eq_ignore_ascii_case(cfg.app_id.trim()) {
        return Err(GatewayError::signature(
            "alipay signature invalid: app_id mismatch",
        ));
    }
    Ok(())
}

fn china_offset() -> Option<FixedOffset> {
    FixedOffset::east_opt(ALIPAY_OFFSET_SECS)
}

/// `gmt_payment` (`2006-01-02 15:04:05`, China time) → UTC.
pub fn parse_gmt_payment(raw: &str) -> Option<DateTime<Utc>> {
    let naive = NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S").ok()?;
    naive
        .and_local_timezone(china_offset()?)
        .single()
        .map(|t| t.with_timezone(&Utc))
}

/// The official Alipay adapter.
#[derive(Debug, Clone)]
pub struct AlipayGateway {
    env: GatewayEnv,
}

impl AlipayGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    async fn precreate(
        &self,
        cfg: &Config,
        params: &[(String, String)],
    ) -> Result<(String, String, Map<String, Value>), GatewayError> {
        let (biz, protocol): (Vec<_>, Vec<_>) = params
            .iter()
            .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
            .filter(|(k, v)| !k.is_empty() && !v.is_empty())
            .partition(|(k, _)| k == "biz_content");
        // PAY-22: protocol params (charset first of all) go in the URL, only biz_content in the body.
        let req = HttpRequest::new("POST", gateway_url_with(&cfg.gateway_url, &protocol))
            .header(
                "Content-Type",
                "application/x-www-form-urlencoded; charset=utf-8",
            )
            .header("Accept", "application/json")
            .body(encode_pairs(&biz));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|_| GatewayError::request("alipay request failed: http request failed"))?;
        if !resp.is_success() {
            return Err(GatewayError::response(format!(
                "alipay response invalid: status {}",
                resp.status
            )));
        }
        let raw = super::common::decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("alipay response invalid: decode response failed")
        })?;
        let node = raw
            .get("alipay_trade_precreate_response")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| {
                GatewayError::response(
                    "alipay response invalid: alipay_trade_precreate_response not found",
                )
            })?;
        let read = |k: &str| match node.get(k) {
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        };
        let code = read("code");
        if code != RESP_CODE_SUCCESS {
            let mut msg = read("sub_msg");
            if msg.is_empty() {
                msg = read("msg");
            }
            if msg.is_empty() {
                msg = format!("code={code}");
            }
            return Err(GatewayError::response(format!(
                "alipay response invalid: {msg}"
            )));
        }
        let qr = read("qr_code");
        if qr.is_empty() {
            return Err(GatewayError::response(
                "alipay response invalid: qr_code is empty",
            ));
        }
        Ok((qr, read("trade_no"), raw))
    }
}

#[async_trait]
impl PaymentGateway for AlipayGateway {
    fn key(&self) -> &'static str {
        "official:alipay"
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
        interaction_mode: &str,
    ) -> Result<(), GatewayError> {
        let mode = match interaction_mode.trim() {
            "" => "qr",
            m => m,
        };
        Config::parse(config)?.validate(mode)
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let mode_text = input
            .interaction_mode
            .map(InteractionMode::as_str)
            .unwrap_or_default();
        let cfg = Config::parse(config)?;
        cfg.validate(mode_text)?;
        let original_amount = go_decimal_string(input.amount.decimal());
        let exchange = cfg.exchange();
        let (pay_amount, pay_currency) = exchange.convert(&original_amount, &input.currency, 2)?;
        let return_url = match input.return_url.trim() {
            "" => cfg.return_url.clone(),
            v => v.to_owned(),
        };
        let return_url = append_query_params(&return_url, &input.return_url_query);
        let order_no = input.order_no.trim().to_owned();
        let amount = parse_decimal(&pay_amount)
            .filter(|a| a.is_sign_positive() && !a.is_zero())
            .ok_or_else(|| GatewayError::config("alipay config invalid: amount is invalid"))?;
        if order_no.is_empty() {
            return Err(GatewayError::config(
                "alipay config invalid: order_no/amount is required",
            ));
        }
        let total_amount = zs_shared::money::Amount::new(amount).to_string();
        let subject = match input.subject.trim() {
            "" => order_no.clone(),
            s => s.to_owned(),
        };
        let (method, product_code) = match input.interaction_mode {
            Some(InteractionMode::Qr) => ("alipay.trade.precreate", "FACE_TO_FACE_PAYMENT"),
            Some(InteractionMode::Wap) => ("alipay.trade.wap.pay", "QUICK_WAP_WAY"),
            Some(InteractionMode::Page) => ("alipay.trade.page.pay", "FAST_INSTANT_TRADE_PAY"),
            _ => {
                return Err(GatewayError::config(
                    "alipay config invalid: interaction_mode is not supported",
                ));
            }
        };
        let mut biz = Map::new();
        biz.insert("out_trade_no".into(), Value::String(order_no.clone()));
        biz.insert("total_amount".into(), Value::String(total_amount));
        biz.insert("subject".into(), Value::String(subject));
        biz.insert("product_code".into(), Value::String(product_code.into()));
        let notify_url = match input.notify_url.trim() {
            "" => cfg.notify_url.clone(),
            v => v.to_owned(),
        };
        let timestamp = china_offset()
            .map(|o| {
                self.env
                    .clock
                    .now()
                    .with_timezone(&o)
                    .format("%Y-%m-%d %H:%M:%S")
                    .to_string()
            })
            .unwrap_or_default();
        let mut params: Vec<(String, String)> = vec![
            ("app_id".into(), cfg.app_id.clone()),
            ("method".into(), method.into()),
            ("format".into(), "JSON".into()),
            ("charset".into(), "utf-8".into()),
            ("sign_type".into(), cfg.sign_type.clone()),
            ("timestamp".into(), timestamp),
            ("version".into(), "1.0".into()),
            ("notify_url".into(), notify_url),
            ("biz_content".into(), go_json_map(&Value::Object(biz))),
        ];
        if !return_url.is_empty() {
            params.push(("return_url".into(), return_url));
        }
        if !cfg.app_cert_sn.is_empty() {
            params.push(("app_cert_sn".into(), cfg.app_cert_sn.clone()));
        }
        if !cfg.alipay_root_cert_sn.is_empty() {
            params.push((
                "alipay_root_cert_sn".into(),
                cfg.alipay_root_cert_sn.clone(),
            ));
        }
        let signature = sign(&sign_content(&params), &cfg.private_key, &cfg.sign_type)?;
        params.push(("sign".into(), signature));
        let (redirect_url, qr_code_url, provider_ref, mut payload) =
            if input.interaction_mode == Some(InteractionMode::Qr) {
                let (qr, trade_no, raw) = self.precreate(&cfg, &params).await?;
                (String::new(), qr, trade_no, raw)
            } else {
                let pay_url = gateway_url_with(&cfg.gateway_url, &params);
                let mut raw = Map::new();
                raw.insert("pay_url".into(), Value::String(pay_url.clone()));
                raw.insert("method".into(), Value::String(method.into()));
                raw.insert("out_trade_no".into(), Value::String(order_no.clone()));
                (pay_url, String::new(), String::new(), raw)
            };
        if exchange.needs_conversion() {
            exchange.audit(&mut payload, &original_amount, &input.currency);
        }
        Ok(GatewayCreateResult {
            provider_ref,
            redirect_url,
            qr_code_url,
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
        verify_notification(&cfg, form)?;
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
            amount: callback_amount(&form_value(form, "total_amount")),
            currency: "CNY".to_owned(),
            paid_at: parse_gmt_payment(&form_value(form, "gmt_payment")),
            payload: form_to_json(form),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::http::{HttpResponse, MockTransport};
    use crate::payment::test_support::{PRIV_PEM, PUB_PEM, env_with, obj};
    use serde_json::json;
    use zs_domain::payment::form::pairs_to_form;
    use zs_shared::money::Amount;

    fn p(items: &[(&str, &str)]) -> Vec<(String, String)> {
        items
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    const GO_BIZ: &str = r#"{"out_trade_no":"DJP1","product_code":"QUICK_WAP_WAY","subject":"DJ1 \u003c测试\u003e\u0026","total_amount":"72.43"}"#;

    fn vector_params() -> Vec<(String, String)> {
        p(&[
            ("app_id", "2021000000000001"),
            ("method", "alipay.trade.wap.pay"),
            ("format", "JSON"),
            ("charset", "utf-8"),
            ("sign_type", "RSA2"),
            ("timestamp", "2026-09-24 18:00:00"),
            ("version", "1.0"),
            ("notify_url", "https://shop/api/v1/payments/callback"),
            ("biz_content", GO_BIZ),
            (
                "return_url",
                "https://shop/pay?alipay_return=1&biz_type=order&order_no=DJ1",
            ),
            ("app_cert_sn", "abc123"),
        ])
    }

    const GO_RSA2: &str = "c46IyZmjce6dbzQgRs/7O8L0PSEJr0QL632tKyIKVvYRJ+JJMvnWl3FBSnlP4LMUFRjuQ/JWm+KcE04G8gd9xvDzo1+Ne4yj0vMWzcB4JtR9YXpvyKY7S/eA4M8lpNn1badu0VWs86frg34sfiufuOs8f/am+dK8ZL4xI2C61FKu9sUOv0Ckse1h3BOgH8FChD8ql9LdvP2NfAvvciVtukl2yohMTuCA2A5FACmmuXyAIOJ+HrjCljpAZiCxvYQZiPnKM6xGCDwydSrb3Sb7mp2/wlWi680vNZWX4n+OBsjGFT95cPUl8NBOFLQYeY7EdioMe+NgyiLH67YbcM3euA==";
    const GO_RSA1: &str = "AXKruJZE8NMYbccYpI6i9UYYP1YKuo546tAYLy85n62gC47LTB+pjGJWTn/MXDOZYsYVn3Irnd4OmSAHoYYNOlONDmog9lCLi6HuDXnzDW7Io/PQzzw6VqICgOK6aZNk2ywHe8p6vFFmE+n18ybMs1m8amFb6JBdkX2kJLzauOsQkhxky7TkWgEOu01pZfwndpm6WmeiuLQs27cgwoHpndBPFdxn8I4+oSIEiUDf/LdpOZWCzwdiDPQttpgiSKXxj+Uwi6CRFOxV8NbD8af1bHMsHF9NlZeMC5AUEgcAM5Vq09Rvewu6GYZeKr7FoebvBE8lcpv5KBG2/+7tVrOApA==";

    /// Go vectors: biz_content JSON, sign content, RSA2 / RSA(SHA1) signatures and pay URL.
    #[test]
    fn sign_vectors_match_go() {
        let biz = json!({"out_trade_no": "DJP1", "total_amount": "72.43", "subject": "DJ1 <测试>&", "product_code": "QUICK_WAP_WAY"});
        assert_eq!(go_json_map(&biz), GO_BIZ);
        let content = sign_content(&vector_params());
        assert_eq!(
            content,
            format!(
                "app_cert_sn=abc123&app_id=2021000000000001&biz_content={GO_BIZ}&charset=utf-8&format=JSON&method=alipay.trade.wap.pay&notify_url=https://shop/api/v1/payments/callback&return_url=https://shop/pay?alipay_return=1&biz_type=order&order_no=DJ1&sign_type=RSA2&timestamp=2026-09-24 18:00:00&version=1.0"
            )
        );
        assert_eq!(
            sign(&content, PRIV_PEM, "RSA2").unwrap_or_default(),
            GO_RSA2
        );
        assert_eq!(sign(&content, PRIV_PEM, "RSA").unwrap_or_default(), GO_RSA1);
        let mut params = vector_params();
        params.push(("sign".into(), GO_RSA2.into()));
        assert_eq!(
            gateway_url_with("https://openapi.alipay.com/gateway.do", &params),
            "https://openapi.alipay.com/gateway.do?app_cert_sn=abc123&app_id=2021000000000001&biz_content=%7B%22out_trade_no%22%3A%22DJP1%22%2C%22product_code%22%3A%22QUICK_WAP_WAY%22%2C%22subject%22%3A%22DJ1+%5Cu003c%E6%B5%8B%E8%AF%95%5Cu003e%5Cu0026%22%2C%22total_amount%22%3A%2272.43%22%7D&charset=utf-8&format=JSON&method=alipay.trade.wap.pay&notify_url=https%3A%2F%2Fshop%2Fapi%2Fv1%2Fpayments%2Fcallback&return_url=https%3A%2F%2Fshop%2Fpay%3Falipay_return%3D1%26biz_type%3Dorder%26order_no%3DDJ1&sign=c46IyZmjce6dbzQgRs%2F7O8L0PSEJr0QL632tKyIKVvYRJ%2BJJMvnWl3FBSnlP4LMUFRjuQ%2FJWm%2BKcE04G8gd9xvDzo1%2BNe4yj0vMWzcB4JtR9YXpvyKY7S%2FeA4M8lpNn1badu0VWs86frg34sfiufuOs8f%2Fam%2BdK8ZL4xI2C61FKu9sUOv0Ckse1h3BOgH8FChD8ql9LdvP2NfAvvciVtukl2yohMTuCA2A5FACmmuXyAIOJ%2BHrjCljpAZiCxvYQZiPnKM6xGCDwydSrb3Sb7mp2%2FwlWi680vNZWX4n%2BOBsjGFT95cPUl8NBOFLQYeY7EdioMe%2BNgyiLH67YbcM3euA%3D%3D&sign_type=RSA2&timestamp=2026-09-24+18%3A00%3A00&version=1.0"
        );
    }

    const GO_CB_SIGN: &str = "naqN3ED9wWU9ZlzJGvNduWqI5MPZ7+ZvszHMUNuYW4cZ9jsBziOGG70VP9uCjhayM9KPQ+YNazboEVH66M4ZRicyMVQkTqQAA7U+dyEvzdj/qH01kb5GP47r9hW7jMLX7Yp4HF1lJmWKgQpnjM7wVzvL5Oz+hM9qVjXIY8QO8fcxgLI/Y8sRa4vGFwr9cYSZDjJSGze/WLagOOzbAbWPMaQ5Et6/1vPVRYZrTTZNmRWVx2oRNtA7Oi6XDAx5xtfNo9TnrkV4lxcpL5s9lfRsJIZGHM7y+jL7w/TmSRb7+FhH5vBqJY1d3zWnRgV6oWwuOvGoL5tGkphyq2ckL/aeFw==";

    pub(crate) fn callback_form(app_id: &str, sign_type: &str, sign: &str) -> FormMap {
        pairs_to_form(p(&[
            ("app_id", app_id),
            ("out_trade_no", "DJP1"),
            ("trade_no", "2026092422001"),
            ("trade_status", "TRADE_SUCCESS"),
            ("total_amount", "72.43"),
            ("gmt_payment", "2026-09-24 18:01:02"),
            ("notify_id", "n1"),
            ("sign_type", sign_type),
            ("subject", "DJ1 测试"),
            ("sign", sign),
        ]))
    }

    fn cfg() -> ChannelConfig {
        obj(
            json!({"app_id": "2021000000000001", "private_key": PRIV_PEM, "alipay_public_key": PUB_PEM,
            "notify_url": "https://shop/api/v1/payments/callback", "return_url": "https://shop/pay"}),
        )
    }

    /// PAY-34 / PAY-01: Go-signed notification verifies; app_id and sign_type rules.
    #[test]
    fn pay_34_callback_ownership() {
        let gw = AlipayGateway::new(env_with(MockTransport::fixed(500, "")));
        let ok = gw
            .verify_callback(
                &cfg(),
                &callback_form("2021000000000001", "RSA2", GO_CB_SIGN),
                b"",
            )
            .unwrap_or_default();
        assert_eq!(ok.status, Some(PaymentStatus::Success));
        assert_eq!(ok.amount, Amount::from_cents(7243));
        assert_eq!(
            ok.paid_at.map(|t| t.to_rfc3339()),
            Some("2026-09-24T10:01:02+00:00".into())
        );
        // A request claiming sign_type=RSA is still verified as RSA2 (the configured type).
        let rsa1 = sign(
            "app_id=2021000000000001&gmt_payment=2026-09-24 18:01:02&notify_id=n1&out_trade_no=DJP1&subject=DJ1 测试&total_amount=72.43&trade_no=2026092422001&trade_status=TRADE_SUCCESS",
            PRIV_PEM,
            "RSA",
        )
        .unwrap_or_default();
        assert!(
            gw.verify_callback(
                &cfg(),
                &callback_form("2021000000000001", "RSA", &rsa1),
                b""
            )
            .is_err()
        );
        let other_app = gw.verify_callback(
            &cfg(),
            &callback_form("2026999999999999", "RSA2", GO_CB_SIGN),
            b"",
        );
        assert!(matches!(other_app, Err(GatewayError::SignatureInvalid(_))));
        let mut no_app = callback_form("2021000000000001", "RSA2", GO_CB_SIGN);
        no_app.remove("app_id");
        assert!(gw.verify_callback(&cfg(), &no_app, b"").is_err());
    }

    /// PAY-22: precreate puts protocol params in the URL and only biz_content in the body.
    #[tokio::test]
    async fn pay_22_precreate_request_layout() {
        let mock = MockTransport::new(|_| {
            Ok(HttpResponse::new(
                200,
                r#"{"alipay_trade_precreate_response":{"code":"10000","msg":"Success","out_trade_no":"DJP1","qr_code":"https://qr.alipay.com/x"},"sign":"s"}"#,
            ))
        });
        let gw = AlipayGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            subject: "DJ1".into(),
            amount: Amount::from_cents(7243),
            currency: "CNY".into(),
            interaction_mode: Some(InteractionMode::Qr),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&cfg(), &input).await.unwrap_or_default();
        assert_eq!(res.qr_code_url, "https://qr.alipay.com/x");
        let req = &mock.requests()[0];
        assert!(
            req.url.contains("charset=utf-8")
                && req.url.contains("sign=")
                && !req.url.contains("biz_content")
        );
        assert!(String::from_utf8_lossy(&req.body).starts_with("biz_content="));
        assert_eq!(
            req.header_value("Content-Type"),
            Some("application/x-www-form-urlencoded; charset=utf-8")
        );
        assert!(req.url.contains("timestamp=2026-06-29+04%3A53%3A20"));
    }

    /// PAY-10: conversion to CNY with audit fields; WAP mode returns a signed pay URL.
    #[tokio::test]
    async fn pay_10_conversion_and_wap() {
        let gw = AlipayGateway::new(env_with(MockTransport::fixed(500, "")));
        let mut c = cfg();
        c.insert("target_currency".into(), json!("cny"));
        c.insert("exchange_rate".into(), json!("7.2"));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from(10),
            currency: "USD".into(),
            interaction_mode: Some(InteractionMode::Wap),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&c, &input).await.unwrap_or_default();
        assert_eq!(
            (res.amount_sent.as_str(), res.currency_sent.as_str()),
            ("72", "CNY")
        );
        assert_eq!(res.payload["original_amount"], "10");
        assert!(res.redirect_url.contains("total_amount%22%3A%2272.00%22"));
        let mut bad = cfg();
        bad.insert("target_currency".into(), json!("CNY"));
        bad.insert("exchange_rate".into(), json!("0"));
        assert!(matches!(
            gw.create_payment(&bad, &input).await,
            Err(GatewayError::ConfigInvalid(_))
        ));
        assert!(gw.validate_config(&cfg(), "redirect").is_err());
        let mut no_return = cfg();
        no_return.remove("return_url");
        assert!(gw.validate_config(&no_return, "").is_ok());
        assert!(gw.validate_config(&no_return, "page").is_err());
    }
}
