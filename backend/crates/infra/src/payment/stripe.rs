//! Stripe Checkout gateway: session creation, active queries and `t=,v1=` webhook HMAC
//! verification (PAY-25, PAY-27).

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::Deserialize;
use serde_json::{Map, Value};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{encode_pairs, path_escape};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, GatewayQueryResult, Headers, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{InteractionMode, PaymentStatus};
use zs_shared::money::Amount;

use super::common::{
    ExchangeRate, GatewayEnv, callback_amount, decode_object, first_non_empty, go_decimal_string,
    hmac_sha256, is_request_uri, parse_config, signature_eq,
};
use super::http::HttpRequest;

const DEFAULT_API_BASE_URL: &str = "https://api.stripe.com";
/// Default webhook timestamp tolerance (seconds).
const DEFAULT_TOLERANCE_SECS: i64 = 300;
/// Currencies without minor units.
const ZERO_DECIMAL: [&str; 16] = [
    "BIF", "CLP", "DJF", "GNF", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "UGX", "VND", "VUV",
    "XAF", "XOF", "XPF",
];

/// Stripe channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub secret_key: String,
    pub publishable_key: String,
    pub webhook_secret: String,
    pub success_url: String,
    pub cancel_url: String,
    pub api_base_url: String,
    pub webhook_tolerance_seconds: i64,
    pub payment_method_types: Vec<String>,
    pub target_currency: String,
    pub exchange_rate: String,
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "stripe")?;
        for f in [
            &mut c.secret_key,
            &mut c.publishable_key,
            &mut c.webhook_secret,
            &mut c.success_url,
            &mut c.cancel_url,
        ] {
            *f = f.trim().to_owned();
        }
        c.api_base_url = c.api_base_url.trim().trim_end_matches('/').to_owned();
        if c.api_base_url.is_empty() {
            c.api_base_url = DEFAULT_API_BASE_URL.to_owned();
        }
        if c.webhook_tolerance_seconds <= 0 {
            c.webhook_tolerance_seconds = DEFAULT_TOLERANCE_SECS;
        }
        let mut types: Vec<String> = c
            .payment_method_types
            .iter()
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect();
        if types.is_empty() {
            types = vec!["card".to_owned()];
        }
        types.sort();
        c.payment_method_types = types;
        Ok(c)
    }

    pub fn validate(&self) -> Result<(), GatewayError> {
        let invalid = |m: &str| Err(GatewayError::config(format!("stripe config invalid: {m}")));
        for (name, value) in [
            ("secret_key", &self.secret_key),
            ("webhook_secret", &self.webhook_secret),
            ("success_url", &self.success_url),
            ("cancel_url", &self.cancel_url),
            ("api_base_url", &self.api_base_url),
        ] {
            if value.is_empty() {
                return invalid(&format!("{name} is required"));
            }
        }
        let sanitize = |u: &str| u.replace("{CHECKOUT_SESSION_ID}", "cs_test_placeholder");
        if !is_request_uri(&self.api_base_url) {
            return invalid("api_base_url is invalid");
        }
        if !is_request_uri(&sanitize(&self.success_url)) {
            return invalid("success_url is invalid");
        }
        if !is_request_uri(&sanitize(&self.cancel_url)) {
            return invalid("cancel_url is invalid");
        }
        if self.payment_method_types.is_empty() {
            return invalid("payment_method_types is empty");
        }
        Ok(())
    }
}

fn currency_scale(currency: &str) -> u32 {
    if ZERO_DECIMAL.contains(&currency.trim().to_ascii_uppercase().as_str()) {
        0
    } else {
        2
    }
}

/// `toMinorAmount`: amount in minor units (half-up rounding).
pub fn to_minor_amount(amount: &str, currency: &str) -> Result<i64, GatewayError> {
    let parsed: Decimal = amount
        .trim()
        .parse()
        .map_err(|_| GatewayError::config("stripe config invalid: amount is invalid"))?;
    if parsed <= Decimal::ZERO {
        return Err(GatewayError::config(
            "stripe config invalid: amount must be greater than zero",
        ));
    }
    let scale = currency_scale(currency);
    let minor = (parsed * Decimal::from(10_i64.pow(scale)))
        .round_dp_with_strategy(0, RoundingStrategy::MidpointAwayFromZero);
    i64::try_from(minor)
        .map_err(|_| GatewayError::config("stripe config invalid: amount precision is invalid"))
}

/// `fromMinorAmount`.
pub fn from_minor_amount(minor: i64, currency: &str) -> String {
    let scale = currency_scale(currency);
    Decimal::new(minor, scale).to_string()
}

/// `computeSignature`: hex HMAC-SHA256 of `"{t}.{body}"`.
pub fn compute_signature(secret: &str, timestamp: i64, body: &[u8]) -> String {
    let mut payload = format!("{timestamp}.").into_bytes();
    payload.extend_from_slice(body);
    hex::encode(hmac_sha256(secret.as_bytes(), &payload))
}

fn parse_signature_header(header: &str) -> Result<(i64, Vec<String>), GatewayError> {
    let mut timestamp = 0;
    let mut signatures = Vec::new();
    for part in header.split(',') {
        let Some((k, v)) = part.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "t" => {
                timestamp = v
                    .trim()
                    .parse::<i64>()
                    .ok()
                    .filter(|t| *t > 0)
                    .ok_or_else(|| {
                        GatewayError::signature("stripe signature invalid: invalid timestamp")
                    })?;
            }
            "v1" if !v.trim().is_empty() => signatures.push(v.trim().to_ascii_lowercase()),
            _ => {}
        }
    }
    if timestamp <= 0 {
        return Err(GatewayError::signature(
            "stripe signature invalid: timestamp is missing",
        ));
    }
    if signatures.is_empty() {
        return Err(GatewayError::signature(
            "stripe signature invalid: v1 signature is missing",
        ));
    }
    Ok((timestamp, signatures))
}

fn event_type_status(event_type: &str) -> Option<PaymentStatus> {
    match event_type.trim().to_ascii_lowercase().as_str() {
        "checkout.session.completed"
        | "checkout.session.async_payment_succeeded"
        | "payment_intent.succeeded" => Some(PaymentStatus::Success),
        "checkout.session.expired" => Some(PaymentStatus::Expired),
        "checkout.session.async_payment_failed"
        | "payment_intent.payment_failed"
        | "payment_intent.canceled" => Some(PaymentStatus::Failed),
        "payment_intent.processing" => Some(PaymentStatus::Pending),
        _ => None,
    }
}

/// `mapCheckoutSessionStatus`: only `payment_status = paid` (or complete + no payment required) succeeds.
pub fn checkout_session_status(payment_status: &str, session_status: &str) -> PaymentStatus {
    let (ps, ss) = (
        payment_status.trim().to_ascii_lowercase(),
        session_status.trim().to_ascii_lowercase(),
    );
    if ps == "paid" {
        PaymentStatus::Success
    } else if ss == "expired" {
        PaymentStatus::Expired
    } else if ss == "complete" && ps == "no_payment_required" {
        PaymentStatus::Success
    } else {
        PaymentStatus::Pending
    }
}

fn payment_intent_status(status: &str) -> PaymentStatus {
    match status.trim().to_ascii_lowercase().as_str() {
        "succeeded" => PaymentStatus::Success,
        "canceled" | "requires_payment_method" => PaymentStatus::Failed,
        _ => PaymentStatus::Pending,
    }
}

fn read_str(map: &Map<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.as_i64().map(|i| i.to_string()).unwrap_or_default(),
        _ => String::new(),
    }
}

fn read_i64(map: &Map<String, Value>, key: &str) -> i64 {
    match map.get(key) {
        Some(Value::Number(n)) => n.as_i64().unwrap_or_else(|| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Go converts float64 with int64(v)"
            )]
            let v = n.as_f64().unwrap_or_default() as i64;
            v
        }),
        Some(Value::String(s)) => s.trim().parse().unwrap_or_default(),
        _ => 0,
    }
}

fn payment_intent_id(map: &Map<String, Value>) -> String {
    match map.get("payment_intent") {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Object(o)) => read_str(o, "id"),
        _ => String::new(),
    }
}

fn unix_time(secs: i64) -> Option<DateTime<Utc>> {
    (secs > 0)
        .then(|| Utc.timestamp_opt(secs, 0).single())
        .flatten()
}

/// The Stripe adapter.
#[derive(Debug, Clone)]
pub struct StripeGateway {
    env: GatewayEnv,
}

struct Snapshot {
    session_id: String,
    intent_id: String,
    status: PaymentStatus,
    amount: String,
    currency: String,
    paid_at: Option<DateTime<Utc>>,
}

fn session_snapshot(raw: &Map<String, Value>) -> Snapshot {
    let currency = read_str(raw, "currency").to_ascii_uppercase();
    let minor = read_i64(raw, "amount_total");
    Snapshot {
        session_id: read_str(raw, "id"),
        intent_id: payment_intent_id(raw),
        status: checkout_session_status(&read_str(raw, "payment_status"), &read_str(raw, "status")),
        amount: if minor > 0 && !currency.is_empty() {
            from_minor_amount(minor, &currency)
        } else {
            String::new()
        },
        currency,
        paid_at: unix_time(read_i64(raw, "created")),
    }
}

fn intent_snapshot(raw: &Map<String, Value>) -> Snapshot {
    let currency = read_str(raw, "currency").to_ascii_uppercase();
    let mut minor = read_i64(raw, "amount_received");
    if minor <= 0 {
        minor = read_i64(raw, "amount");
    }
    Snapshot {
        session_id: String::new(),
        intent_id: read_str(raw, "id"),
        status: payment_intent_status(&read_str(raw, "status")),
        amount: if minor > 0 && !currency.is_empty() {
            from_minor_amount(minor, &currency)
        } else {
            String::new()
        },
        currency,
        paid_at: unix_time(read_i64(raw, "created")),
    }
}

impl StripeGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn checked(raw: &ChannelConfig) -> Result<Config, GatewayError> {
        let c = Config::parse(raw)?;
        c.validate()?;
        Ok(c)
    }

    async fn get(&self, cfg: &Config, path: &str) -> Result<Map<String, Value>, GatewayError> {
        let req = HttpRequest::new("GET", format!("{}{path}", cfg.api_base_url))
            .header("Authorization", format!("Bearer {}", cfg.secret_key));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("stripe request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::response(format!(
                "stripe response invalid: status {}",
                resp.status
            )));
        }
        decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("stripe response invalid: decode response failed")
        })
    }

    async fn query_session(
        &self,
        cfg: &Config,
        id: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        let raw = self
            .get(
                cfg,
                &format!(
                    "/v1/checkout/sessions/{}?expand[]=payment_intent",
                    path_escape(id.trim())
                ),
            )
            .await?;
        let s = session_snapshot(&raw);
        if s.session_id.is_empty() {
            return Err(GatewayError::response(
                "stripe response invalid: missing checkout session id",
            ));
        }
        Ok(query_result(s, id, raw))
    }

    async fn query_intent(
        &self,
        cfg: &Config,
        id: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        let raw = self
            .get(
                cfg,
                &format!("/v1/payment_intents/{}", path_escape(id.trim())),
            )
            .await?;
        let s = intent_snapshot(&raw);
        if s.intent_id.is_empty() {
            return Err(GatewayError::response(
                "stripe response invalid: missing payment intent id",
            ));
        }
        Ok(query_result(s, id, raw))
    }
}

fn query_result(s: Snapshot, provider_ref: &str, raw: Map<String, Value>) -> GatewayQueryResult {
    GatewayQueryResult {
        provider_ref: first_non_empty(&[&s.session_id, &s.intent_id, provider_ref]),
        status: Some(s.status),
        amount: callback_amount(&s.amount),
        currency: s.currency,
        paid_at: s.paid_at,
        payload: raw,
    }
}

#[async_trait]
impl PaymentGateway for StripeGateway {
    fn key(&self) -> &'static str {
        "official:stripe"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            query: true,
            webhook: true,
            ..GatewayCapabilities::default()
        }
    }

    fn validate_config(
        &self,
        config: &ChannelConfig,
        interaction_mode: &str,
    ) -> Result<(), GatewayError> {
        if !interaction_mode.is_empty()
            && InteractionMode::parse(interaction_mode) != Some(InteractionMode::Redirect)
        {
            return Err(GatewayError::config(
                "stripe only supports redirect interaction_mode",
            ));
        }
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
        let success_url = match input.return_url.trim() {
            "" => cfg.success_url.clone(),
            v => v.to_owned(),
        };
        let success_url = append_query_params(&success_url, &input.return_url_query);
        let cancel_url = match input.cancel_url.trim() {
            "" => cfg.cancel_url.clone(),
            v => v.to_owned(),
        };
        let order_no = input.order_no.trim().to_owned();
        if order_no.is_empty() {
            return Err(GatewayError::config(
                "stripe config invalid: order_no is required",
            ));
        }
        let currency = pay_currency.trim().to_ascii_uppercase();
        if currency.is_empty() {
            return Err(GatewayError::config(
                "stripe config invalid: currency is required",
            ));
        }
        let minor = to_minor_amount(&pay_amount, &currency)?;
        let subject = match input.subject.trim() {
            "" => order_no.clone(),
            s => s.to_owned(),
        };
        let mut form: Vec<(String, String)> = vec![
            ("mode".into(), "payment".into()),
            ("success_url".into(), success_url),
            ("cancel_url".into(), cancel_url),
            ("client_reference_id".into(), order_no.clone()),
            ("line_items[0][quantity]".into(), "1".into()),
            (
                "line_items[0][price_data][currency]".into(),
                currency.to_ascii_lowercase(),
            ),
            (
                "line_items[0][price_data][unit_amount]".into(),
                minor.to_string(),
            ),
            (
                "line_items[0][price_data][product_data][name]".into(),
                subject,
            ),
            ("metadata[order_no]".into(), order_no.clone()),
            ("payment_intent_data[metadata][order_no]".into(), order_no),
        ];
        if !input.email.trim().is_empty() {
            form.push(("customer_email".into(), input.email.trim().to_owned()));
        }
        for pm in &cfg.payment_method_types {
            form.push(("payment_method_types[]".into(), pm.clone()));
            // PAY-27: Stripe rejects web Checkout with WeChat Pay unless the client is declared.
            if pm == "wechat_pay" {
                form.push((
                    "payment_method_options[wechat_pay][client]".into(),
                    "web".into(),
                ));
            }
        }
        let req = HttpRequest::new("POST", format!("{}/v1/checkout/sessions", cfg.api_base_url))
            .header("Authorization", format!("Bearer {}", cfg.secret_key))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(encode_pairs(&form));
        let resp = self
            .env
            .http
            .send(req)
            .await
            .map_err(|e| GatewayError::request(format!("stripe request failed: {e}")))?;
        if !resp.is_success() {
            return Err(GatewayError::response(format!(
                "stripe response invalid: create checkout session status {}",
                resp.status
            )));
        }
        let raw = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("stripe response invalid: decode response failed")
        })?;
        let (session_id, url) = (read_str(&raw, "id"), read_str(&raw, "url"));
        if session_id.is_empty() || url.is_empty() {
            return Err(GatewayError::response(
                "stripe response invalid: missing session id or url",
            ));
        }
        let intent_id = payment_intent_id(&raw);
        let mut payload = raw;
        if exchange.needs_conversion() {
            exchange.audit(&mut payload, &original_amount, &input.currency);
        }
        Ok(GatewayCreateResult {
            provider_ref: first_non_empty(&[&session_id, &intent_id]),
            redirect_url: url,
            qr_code_url: String::new(),
            payload,
            display_channel_type: String::new(),
            amount_sent: pay_amount,
            currency_sent: pay_currency,
        })
    }

    async fn query_payment(
        &self,
        config: &ChannelConfig,
        provider_ref: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        let cfg = Self::checked(config)?;
        let id = provider_ref.trim();
        if id.is_empty() {
            return Err(GatewayError::config(
                "stripe config invalid: provider_ref is required",
            ));
        }
        if id.starts_with("cs_") {
            return self.query_session(&cfg, id).await;
        }
        if id.starts_with("pi_") {
            return self.query_intent(&cfg, id).await;
        }
        match self.query_session(&cfg, id).await {
            Ok(r) => Ok(r),
            Err(_) => self.query_intent(&cfg, id).await,
        }
    }

    async fn parse_webhook(
        &self,
        config: &ChannelConfig,
        headers: &Headers,
        body: &[u8],
        now: DateTime<Utc>,
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let cfg = Self::checked(config)?;
        if body.is_empty() {
            return Err(GatewayError::response(
                "stripe response invalid: body is empty",
            ));
        }
        let header = headers.get("Stripe-Signature");
        if header.is_empty() {
            return Err(GatewayError::signature(
                "stripe signature invalid: Stripe-Signature is required",
            ));
        }
        let (timestamp, signatures) = parse_signature_header(&header)?;
        if cfg.webhook_tolerance_seconds > 0
            && (now.timestamp() - timestamp).abs() > cfg.webhook_tolerance_seconds
        {
            return Err(GatewayError::signature(
                "stripe signature invalid: timestamp outside tolerance",
            ));
        }
        let expected = compute_signature(&cfg.webhook_secret, timestamp, body);
        if !signatures.iter().any(|s| signature_eq(&expected, s)) {
            return Err(GatewayError::signature(
                "stripe signature invalid: verify failed",
            ));
        }
        let event = decode_object(body).ok_or_else(|| {
            GatewayError::response("stripe response invalid: decode response failed")
        })?;
        let event_type = read_str(&event, "type");
        if event_type.is_empty() {
            return Err(GatewayError::response(
                "stripe response invalid: missing event type",
            ));
        }
        let object = event
            .get("data")
            .and_then(Value::as_object)
            .ok_or_else(|| GatewayError::response("stripe response invalid: missing data object"))?
            .get("object")
            .and_then(Value::as_object)
            .cloned()
            .ok_or_else(|| {
                GatewayError::response("stripe response invalid: missing event object")
            })?;
        let order_no = object
            .get("metadata")
            .and_then(Value::as_object)
            .map(|m| read_str(m, "order_no"))
            .unwrap_or_default();
        let by_type = event_type_status(&event_type);
        let (provider_ref, status, amount, currency, paid_at) = match read_str(&object, "object")
            .as_str()
        {
            "checkout.session" => {
                let s = session_snapshot(&object);
                // PAY-25: `completed` fires for delayed methods while still unpaid; trust payment_status.
                let status = match by_type {
                    Some(st) if !event_type.eq_ignore_ascii_case("checkout.session.completed") => {
                        Some(st)
                    }
                    _ => Some(s.status),
                };
                (
                    first_non_empty(&[&s.session_id, &s.intent_id]),
                    status,
                    s.amount,
                    s.currency,
                    s.paid_at,
                )
            }
            "payment_intent" => {
                let s = intent_snapshot(&object);
                (
                    s.intent_id,
                    by_type.or(Some(s.status)),
                    s.amount,
                    s.currency,
                    s.paid_at,
                )
            }
            _ => (String::new(), by_type, String::new(), String::new(), None),
        };
        let provider_ref = first_non_empty(&[&provider_ref, &read_str(&object, "id")]);
        Ok(GatewayCallbackResult {
            order_no,
            provider_ref,
            status,
            amount: if amount.is_empty() {
                Amount::ZERO
            } else {
                callback_amount(&amount)
            },
            currency,
            paid_at,
            payload: event,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::http::MockTransport;
    use crate::payment::test_support::{NOW_UNIX, env_with, obj};
    use serde_json::json;

    /// Go vectors: webhook signature and minor-unit conversion.
    #[test]
    fn signature_and_minor_units_match_go() {
        assert_eq!(
            compute_signature("whsec_test", NOW_UNIX, br#"{"id":"evt_1"}"#),
            "c36e51f8e20da5248776847162b4f42b8e2124aff739af65efe539650c565406"
        );
        assert_eq!(to_minor_amount("1000", "JPY").ok(), Some(1000));
        assert_eq!(to_minor_amount("12.345", "USD").ok(), Some(1235));
        assert_eq!(from_minor_amount(1234, "usd"), "12.34");
        assert_eq!(from_minor_amount(1000, "JPY"), "1000");
    }

    fn cfg() -> ChannelConfig {
        obj(
            json!({"secret_key": "sk_test", "webhook_secret": "whsec", "success_url": "https://r/pay?session_id={CHECKOUT_SESSION_ID}",
            "cancel_url": "https://r/cancel", "api_base_url": "https://stripe.example.com", "payment_method_types": ["wechat_pay", " Card "]}),
        )
    }

    /// PAY-27 / Go vector: checkout form (sorted, wechat_pay client option, customer email).
    #[tokio::test]
    async fn pay_27_checkout_form_matches_go() {
        let mock = MockTransport::fixed(
            200,
            r#"{"id":"cs_test_1","url":"https://checkout.stripe.com/c/pay/cs_test_1","status":"open","payment_intent":null}"#,
        );
        let gw = StripeGateway::new(env_with(mock.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            amount: Amount::from_cents(1234),
            currency: "usd".into(),
            subject: "DJ1 订单".into(),
            email: "a+b@x.com".into(),
            return_url: "https://r/pay?a=1&stripe_return=1".into(),
            ..GatewayCreateInput::default()
        };
        let res = gw.create_payment(&cfg(), &input).await.unwrap_or_default();
        assert_eq!(res.provider_ref, "cs_test_1");
        assert_eq!(
            String::from_utf8_lossy(&mock.requests()[0].body),
            "cancel_url=https%3A%2F%2Fr%2Fcancel&client_reference_id=DJP1&customer_email=a%2Bb%40x.com&line_items%5B0%5D%5Bprice_data%5D%5Bcurrency%5D=usd&line_items%5B0%5D%5Bprice_data%5D%5Bproduct_data%5D%5Bname%5D=DJ1+%E8%AE%A2%E5%8D%95&line_items%5B0%5D%5Bprice_data%5D%5Bunit_amount%5D=1234&line_items%5B0%5D%5Bquantity%5D=1&metadata%5Border_no%5D=DJP1&mode=payment&payment_intent_data%5Bmetadata%5D%5Border_no%5D=DJP1&payment_method_options%5Bwechat_pay%5D%5Bclient%5D=web&payment_method_types%5B%5D=card&payment_method_types%5B%5D=wechat_pay&success_url=https%3A%2F%2Fr%2Fpay%3Fa%3D1%26stripe_return%3D1"
        );
        let mut card_only = cfg();
        card_only.insert("payment_method_types".into(), json!(["card"]));
        let mock = MockTransport::fixed(200, r#"{"id":"cs_2","url":"https://c/2"}"#);
        StripeGateway::new(env_with(mock.clone()))
            .create_payment(&card_only, &input)
            .await
            .unwrap_or_default();
        assert!(!String::from_utf8_lossy(&mock.requests()[0].body).contains("wechat_pay"));
    }

    fn signed(body: &str, ts: i64) -> Headers {
        let sig = compute_signature("whsec", ts, body.as_bytes());
        Headers::from([(
            "Stripe-Signature",
            format!("t={ts},v1=deadbeef,v1={sig}").as_str(),
        )])
    }

    fn session_event(event_type: &str, payment_status: &str) -> String {
        json!({"id": "evt_1", "type": event_type, "data": {"object": {"object": "checkout.session", "id": "cs_1",
            "payment_status": payment_status, "status": "complete", "currency": "usd", "amount_total": 1050,
            "created": NOW_UNIX, "metadata": {"order_no": "DJP1"}, "payment_intent": "pi_1"}}})
        .to_string()
    }

    /// PAY-25: completed + unpaid stays pending; completed + paid and async success succeed.
    #[tokio::test]
    async fn pay_25_payment_status_decides() {
        let gw = StripeGateway::new(env_with(MockTransport::fixed(500, "")));
        let now = Utc.timestamp_opt(NOW_UNIX, 0).single().unwrap_or_default();
        let run = |body: String| {
            let gw = gw.clone();
            async move {
                gw.parse_webhook(&cfg(), &signed(&body, NOW_UNIX), body.as_bytes(), now)
                    .await
            }
        };
        let unpaid = run(session_event("checkout.session.completed", "unpaid"))
            .await
            .unwrap_or_default();
        assert_eq!(unpaid.status, Some(PaymentStatus::Pending));
        let paid = run(session_event("checkout.session.completed", "paid"))
            .await
            .unwrap_or_default();
        assert_eq!(paid.status, Some(PaymentStatus::Success));
        assert_eq!(paid.amount, Amount::from_cents(1050));
        assert_eq!(paid.currency, "USD");
        assert_eq!(paid.order_no, "DJP1");
        assert_eq!(paid.provider_ref, "cs_1");
        let async_ok = run(session_event(
            "checkout.session.async_payment_succeeded",
            "unpaid",
        ))
        .await
        .unwrap_or_default();
        assert_eq!(async_ok.status, Some(PaymentStatus::Success));
        let body = session_event("checkout.session.completed", "paid");
        let stale = gw
            .parse_webhook(&cfg(), &signed(&body, NOW_UNIX - 301), body.as_bytes(), now)
            .await;
        assert!(matches!(stale, Err(GatewayError::SignatureInvalid(_))));
        let bad = Headers::from([("Stripe-Signature", format!("t={NOW_UNIX},v1=00").as_str())]);
        assert!(
            gw.parse_webhook(&cfg(), &bad, body.as_bytes(), now)
                .await
                .is_err()
        );
    }

    /// PAY-08: capture/query works for channels saved with non-redirect modes (no mode check).
    #[tokio::test]
    async fn pay_08_query_without_mode_check() {
        let mock = MockTransport::fixed(
            200,
            r#"{"id":"cs_1","payment_status":"paid","status":"complete","currency":"usd","amount_total":1050,"created":1782680000}"#,
        );
        let gw = StripeGateway::new(env_with(mock.clone()));
        let r = gw.query_payment(&cfg(), "cs_1").await.unwrap_or_default();
        assert_eq!(r.status, Some(PaymentStatus::Success));
        assert_eq!(r.amount, Amount::from_cents(1050));
        assert_eq!(
            mock.requests()[0].url,
            "https://stripe.example.com/v1/checkout/sessions/cs_1?expand[]=payment_intent"
        );
        assert!(gw.validate_config(&cfg(), "qr").is_err());
        assert!(gw.validate_config(&cfg(), "").is_ok());
    }
}
