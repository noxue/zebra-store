//! Helpers shared by gateway adapters: config parsing, Go-compatible number and JSON
//! formatting, RSA keys, constant-time comparison and the gateway environment.

use std::fmt::Write as _;
use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use rand::RngCore;
use rsa::pkcs1::{DecodeRsaPrivateKey, DecodeRsaPublicKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey};
use rsa::{Pkcs1v15Sign, RsaPrivateKey, RsaPublicKey};
use rust_decimal::Decimal;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::gateway::GatewayError;
use zs_shared::clock::{Clock, SystemClock};

use super::http::{HttpTransport, ReqwestTransport};

/// Randomness source (nonces); fixed in tests.
pub trait Entropy: Send + Sync + std::fmt::Debug {
    fn fill(&self, buf: &mut [u8]);
}

/// OS randomness.
#[derive(Debug, Clone, Copy, Default)]
pub struct OsEntropy;

impl Entropy for OsEntropy {
    fn fill(&self, buf: &mut [u8]) {
        rand::rng().fill_bytes(buf);
    }
}

/// Deterministic "randomness" for tests: repeats the given byte.
#[derive(Debug, Clone, Copy)]
pub struct FixedEntropy(pub u8);

impl Entropy for FixedEntropy {
    fn fill(&self, buf: &mut [u8]) {
        buf.fill(self.0);
    }
}

/// Dependencies injected into every gateway (HTTP, clock, randomness).
#[derive(Debug, Clone)]
pub struct GatewayEnv {
    pub http: Arc<dyn HttpTransport>,
    pub clock: Arc<dyn Clock>,
    pub entropy: Arc<dyn Entropy>,
}

impl GatewayEnv {
    pub fn new(
        http: Arc<dyn HttpTransport>,
        clock: Arc<dyn Clock>,
        entropy: Arc<dyn Entropy>,
    ) -> Self {
        Self {
            http,
            clock,
            entropy,
        }
    }

    /// Production environment.
    pub fn system() -> Self {
        Self::new(
            Arc::new(ReqwestTransport::new()),
            Arc::new(SystemClock),
            Arc::new(OsEntropy),
        )
    }

    /// `n` random bytes hex-encoded.
    pub fn random_hex(&self, n: usize) -> String {
        let mut buf = vec![0u8; n];
        self.entropy.fill(&mut buf);
        hex::encode(buf)
    }
}

/// `common.ParseConfig`: JSON round trip into a typed config. `null` values are ignored like
/// Go's decoder; a value of the wrong JSON type is a config error.
pub fn parse_config<T: DeserializeOwned>(
    config: &ChannelConfig,
    name: &str,
) -> Result<T, GatewayError> {
    let cleaned: Map<String, Value> = config
        .iter()
        .filter(|(_, v)| !v.is_null())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    serde_json::from_value(Value::Object(cleaned)).map_err(|e| {
        GatewayError::config(format!(
            "{name} config invalid: unmarshal config failed: {e}"
        ))
    })
}

/// `common.ExchangeRateConfig`: optional conversion into a target currency.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExchangeRate {
    pub target_currency: String,
    pub exchange_rate: String,
}

impl ExchangeRate {
    pub fn new(target_currency: &str, exchange_rate: &str) -> Self {
        Self {
            target_currency: target_currency.trim().to_ascii_uppercase(),
            exchange_rate: exchange_rate.trim().to_owned(),
        }
    }

    pub fn needs_conversion(&self) -> bool {
        !self.target_currency.is_empty() && !self.exchange_rate.is_empty()
    }

    /// `ConvertAmount`: amount × rate rounded to `precision` (half away from zero); rate must be > 0.
    pub fn convert(
        &self,
        amount: &str,
        currency: &str,
        precision: u32,
    ) -> Result<(String, String), GatewayError> {
        if !self.needs_conversion() {
            return Ok((amount.to_owned(), currency.to_owned()));
        }
        let amount_dec: Decimal = amount
            .trim()
            .parse()
            .map_err(|_| GatewayError::config(format!("invalid amount {amount:?}")))?;
        let rate: Decimal = self
            .exchange_rate
            .parse()
            .ok()
            .filter(|r: &Decimal| *r > Decimal::ZERO)
            .ok_or_else(|| {
                GatewayError::config(format!("invalid exchange_rate {:?}", self.exchange_rate))
            })?;
        let converted = (amount_dec * rate).round_dp_with_strategy(
            precision,
            rust_decimal::RoundingStrategy::MidpointAwayFromZero,
        );
        Ok((go_decimal_string(converted), self.target_currency.clone()))
    }

    /// Adds the audit fields written after a conversion.
    pub fn audit(
        &self,
        payload: &mut Map<String, Value>,
        original_amount: &str,
        original_currency: &str,
    ) {
        payload.insert(
            "exchange_rate".into(),
            Value::String(self.exchange_rate.clone()),
        );
        payload.insert(
            "original_amount".into(),
            Value::String(original_amount.to_owned()),
        );
        payload.insert(
            "original_currency".into(),
            Value::String(original_currency.to_owned()),
        );
    }
}

/// shopspring `decimal.String()`: no exponent, trailing zeros removed (`10.00` → `10`).
pub fn go_decimal_string(d: Decimal) -> String {
    let s = d.normalize().to_string();
    if s == "-0" { "0".to_owned() } else { s }
}

/// `strconv.FormatFloat(f, 'f', -1, 64)`: shortest round-trip digits, never an exponent.
pub fn go_float_f(f: f64) -> String {
    if f == 0.0 {
        return "0".to_owned();
    }
    format!("{f}")
}

/// `fmt.Sprintf("%v", f)` for float64: `%g` with shortest digits and exponent threshold 6
/// (`1e-05`, `1.234567e+06`), otherwise plain decimal.
pub fn go_float_v(f: f64) -> String {
    if f == 0.0 {
        return "0".to_owned();
    }
    if !f.is_finite() {
        return if f.is_nan() {
            "NaN".to_owned()
        } else if f > 0.0 {
            "+Inf".to_owned()
        } else {
            "-Inf".to_owned()
        };
    }
    let sci = format!("{f:e}");
    let (mantissa, exp) = sci.split_once('e').unwrap_or((sci.as_str(), "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    if !(-4..6).contains(&exp) {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{mantissa}e{sign}{:02}", exp.abs())
    } else {
        format!("{f}")
    }
}

/// Go `encoding/json` float encoding (`12.5`, `100`, `1e-7`).
fn go_json_float(f: f64) -> String {
    let abs = f.abs();
    if abs != 0.0 && !(1e-6..1e21).contains(&abs) {
        let s = format!("{f:e}");
        return match s.split_once('e') {
            Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
            _ => s,
        };
    }
    go_float_f(f)
}

/// Writes a JSON string like Go's `encoding/json` (HTML-safe: `<`, `>`, `&` escaped).
pub fn go_json_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '<' | '>' | '&' | '\u{2028}' | '\u{2029}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c if u32::from(c) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn go_json_write(out: &mut String, value: &Value, sort_keys: bool) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                let _ = write!(out, "{i}");
            } else if let Some(u) = n.as_u64() {
                let _ = write!(out, "{u}");
            } else {
                out.push_str(&go_json_float(n.as_f64().unwrap_or_default()));
            }
        }
        Value::String(s) => go_json_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                go_json_write(out, item, sort_keys);
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            if sort_keys {
                entries.sort_by(|a, b| a.0.cmp(b.0));
            }
            for (i, (k, v)) in entries.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                go_json_string(out, k);
                out.push(':');
                go_json_write(out, v, sort_keys);
            }
            out.push('}');
        }
    }
}

/// `json.Marshal` of a Go map (keys sorted, HTML escaped, integral floats without `.0`).
pub fn go_json_map(value: &Value) -> String {
    let mut out = String::new();
    go_json_write(&mut out, value, true);
    out
}

/// `json.Marshal` of a Go struct: field order as inserted.
pub fn go_json_ordered(value: &Value) -> String {
    let mut out = String::new();
    go_json_write(&mut out, value, false);
    out
}

/// JSON number for a float that Go would encode without a fraction when integral.
pub fn json_float(f: f64) -> Value {
    if f.fract() == 0.0 && f.abs() < 9.0e15 {
        // Exact: integral and far below 2^53.
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked integral and in range"
        )]
        let i = f as i64;
        Value::from(i)
    } else {
        serde_json::Number::from_f64(f).map_or(Value::Null, Value::Number)
    }
}

/// Constant-time, case-insensitive comparison of two signatures (PAY-01).
pub fn signature_eq(expected: &str, got: &str) -> bool {
    let a = expected.to_ascii_lowercase();
    let b = got.to_ascii_lowercase();
    a.len() == b.len() && bool::from(a.as_bytes().ct_eq(b.as_bytes()))
}

/// Constant-time comparison of raw bytes.
pub fn bytes_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && bool::from(a.ct_eq(b))
}

/// Lower-case hex MD5.
pub fn md5_hex(data: &[u8]) -> String {
    use md5::Md5;
    hex::encode(Md5::digest(data))
}

/// Raw HMAC-SHA256.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    zs_shared::sign::hmac_sha256(key, data)
}

/// Approximation of Go `url.ParseRequestURI`: an absolute path or `scheme:` URI without
/// whitespace/control characters.
pub fn is_request_uri(raw: &str) -> bool {
    if raw.is_empty() || raw.chars().any(|c| c.is_control() || c == ' ') {
        return false;
    }
    if raw.starts_with('/') {
        return true;
    }
    let Some((scheme, rest)) = raw.split_once(':') else {
        return false;
    };
    let valid_scheme = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid_scheme {
        return false;
    }
    match rest.strip_prefix("//") {
        Some(authority) => {
            let host = authority.split(['/', '?', '#']).next().unwrap_or_default();
            !host.contains(['<', '>', '"', '{', '}', '|', '\\', '^', '`'])
        }
        None => true,
    }
}

/// Decodes the first PEM block (`pem.Decode`): returns `(type, der)`.
pub fn pem_decode(text: &str) -> Option<(String, Vec<u8>)> {
    let start = text.find("-----BEGIN ")?;
    let after = &text[start + "-----BEGIN ".len()..];
    let type_end = after.find("-----")?;
    let kind = after[..type_end].to_owned();
    let body_start = after[type_end + 5..].trim_start_matches(['\r', '\n']);
    let end_marker = format!("-----END {kind}-----");
    let body_end = body_start.find(&end_marker)?;
    let body: String = body_start[..body_end]
        .lines()
        .filter(|l| !l.contains(':'))
        .flat_map(|l| l.chars().filter(|c| !c.is_whitespace()))
        .collect();
    let der = B64.decode(body).ok()?;
    Some((kind, der))
}

fn key_body(raw: &str) -> Option<Vec<u8>> {
    let body: String = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with("-----BEGIN ") && !l.starts_with("-----END "))
        .collect();
    if body.is_empty() {
        return None;
    }
    B64.decode(body).ok()
}

fn normalize_key_text(raw: &str) -> String {
    raw.trim().replace("\\n", "\n").replace("\r\n", "\n")
}

fn private_from_der(der: &[u8]) -> Option<RsaPrivateKey> {
    RsaPrivateKey::from_pkcs8_der(der)
        .ok()
        .or_else(|| RsaPrivateKey::from_pkcs1_der(der).ok())
}

fn public_from_der(der: &[u8]) -> Option<RsaPublicKey> {
    RsaPublicKey::from_public_key_der(der)
        .ok()
        .or_else(|| RsaPublicKey::from_pkcs1_der(der).ok())
}

/// Lenient RSA private key parsing (PEM PKCS#8/PKCS#1, or a bare base64 body; literal `\n` allowed).
pub fn parse_rsa_private_key(raw: &str) -> Option<RsaPrivateKey> {
    let normalized = normalize_key_text(raw);
    if let Some((_, der)) = pem_decode(&normalized)
        && let Some(key) = private_from_der(&der)
    {
        return Some(key);
    }
    key_body(&normalized).and_then(|der| private_from_der(&der))
}

/// Lenient RSA public key parsing (PEM PKIX/PKCS#1 or bare base64 body).
pub fn parse_rsa_public_key(raw: &str) -> Option<RsaPublicKey> {
    let normalized = normalize_key_text(raw);
    if let Some((_, der)) = pem_decode(&normalized)
        && let Some(key) = public_from_der(&der)
    {
        return Some(key);
    }
    key_body(&normalized).and_then(|der| public_from_der(&der))
}

/// Hash used by an RSA signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RsaHash {
    Sha1,
    Sha256,
}

/// PKCS#1 v1.5 signature, base64 (deterministic, identical to Go `rsa.SignPKCS1v15`).
pub fn rsa_sign(key: &RsaPrivateKey, hash: RsaHash, content: &[u8]) -> Option<String> {
    let sig = match hash {
        RsaHash::Sha256 => key.sign(Pkcs1v15Sign::new::<Sha256>(), &Sha256::digest(content)),
        RsaHash::Sha1 => key.sign(
            Pkcs1v15Sign::new::<sha1::Sha1>(),
            &sha1::Sha1::digest(content),
        ),
    };
    sig.ok().map(|s| B64.encode(s))
}

/// Verifies a base64 PKCS#1 v1.5 signature.
pub fn rsa_verify(key: &RsaPublicKey, hash: RsaHash, content: &[u8], signature_b64: &str) -> bool {
    let Ok(sig) = B64.decode(signature_b64.trim()) else {
        return false;
    };
    match hash {
        RsaHash::Sha256 => key
            .verify(
                Pkcs1v15Sign::new::<Sha256>(),
                &Sha256::digest(content),
                &sig,
            )
            .is_ok(),
        RsaHash::Sha1 => key
            .verify(
                Pkcs1v15Sign::new::<sha1::Sha1>(),
                &sha1::Sha1::digest(content),
                &sig,
            )
            .is_ok(),
    }
}

/// Parses a decimal amount leniently (`decimal.NewFromString`), `None` when invalid.
pub fn parse_decimal(raw: &str) -> Option<Decimal> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    raw.parse::<Decimal>()
        .ok()
        .or_else(|| Decimal::from_scientific(raw).ok())
}

/// Callback amount: parsed value rounded to two decimals, zero when missing/invalid.
pub fn callback_amount(raw: &str) -> zs_shared::money::Amount {
    parse_decimal(raw)
        .map(zs_shared::money::Amount::new)
        .unwrap_or_default()
}

/// Reads a string-ish value of a JSON map (`common.ReadString`).
pub fn read_string(map: &Map<String, Value>, key: &str) -> String {
    match map.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n
            .as_i64()
            .map(|i| i.to_string())
            .or_else(|| {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "Go converts float64 with int64(v)"
                )]
                n.as_f64().map(|f| (f as i64).to_string())
            })
            .unwrap_or_default(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

/// Follows a path of object keys / array indices and returns a string (`fmt.Sprint` style).
pub fn read_path(value: &Value, path: &[&str]) -> String {
    let mut current = value;
    for seg in path {
        let next = match (seg.parse::<usize>(), current) {
            (Ok(i), Value::Array(items)) => items.get(i),
            (_, Value::Object(map)) => map.get(*seg),
            _ => None,
        };
        match next {
            Some(v) => current = v,
            None => return String::new(),
        }
    }
    match current {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n
            .as_i64()
            .map(|i| i.to_string())
            .unwrap_or_else(|| go_float_v(n.as_f64().unwrap_or_default())),
        other => other.to_string(),
    }
}

/// Decodes a JSON object body.
pub fn decode_object(body: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice::<Value>(body).ok()? {
        Value::Object(map) => Some(map),
        _ => None,
    }
}

/// Picks the first non-blank value (`common.PickFirstNonEmpty`).
pub fn first_non_empty(values: &[&str]) -> String {
    values
        .iter()
        .map(|v| v.trim())
        .find(|v| !v.is_empty())
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Vectors from Go `%v` / `strconv.FormatFloat('f', -1)` / `decimal.String()`.
    #[test]
    fn go_number_formatting() {
        assert_eq!(go_float_v(0.00001), "1e-05");
        assert_eq!(go_float_v(1e21), "1e+21");
        assert_eq!(go_float_v(123_456_789.25), "1.2345678925e+08");
        assert_eq!(go_float_v(0.0001), "0.0001");
        assert_eq!(go_float_v(1e20), "1e+20");
        assert_eq!(go_float_v(1e-7), "1e-07");
        assert_eq!(go_float_v(100.0), "100");
        assert_eq!(go_float_v(12.5), "12.5");
        assert_eq!(go_float_v(0.1 + 0.2), "0.30000000000000004");
        assert_eq!(go_float_v(1_234_567.0), "1.234567e+06");
        assert_eq!(go_float_f(100.0), "100");
        assert_eq!(go_float_f(12.50), "12.5");
        assert_eq!(go_decimal_string("10.00".parse().unwrap_or_default()), "10");
        assert_eq!(go_decimal_string("9.90".parse().unwrap_or_default()), "9.9");
    }

    /// Vectors from Go `common.ExchangeRateConfig.ConvertAmount` (PAY-10).
    #[test]
    fn pay_10_exchange_conversion_matches_go() {
        let r = ExchangeRate::new("cny", " 7.2 ");
        assert_eq!(
            r.convert("10", "USD", 2).ok(),
            Some(("72".into(), "CNY".into()))
        );
        let r = ExchangeRate::new("CNY", "7.25");
        assert_eq!(
            r.convert("9.99", "USD", 2).ok().map(|x| x.0),
            Some("72.43".into())
        );
        let r = ExchangeRate::new("CNY", "0.5");
        assert_eq!(
            r.convert("0.01", "USD", 2).ok().map(|x| x.0),
            Some("0.01".into())
        );
        assert_eq!(
            r.convert("0.03", "USD", 2).ok().map(|x| x.0),
            Some("0.02".into())
        );
        let r = ExchangeRate::new("USDT", "0.13888");
        assert_eq!(
            r.convert("100", "CNY", 8).ok().map(|x| x.0),
            Some("13.888".into())
        );
        for bad in ["0", "abc", "-1.5"] {
            assert!(matches!(
                ExchangeRate::new("CNY", bad).convert("10", "USD", 2),
                Err(GatewayError::ConfigInvalid(_))
            ));
        }
        assert_eq!(
            ExchangeRate::new("", "7").convert("10", "USD", 2).ok(),
            Some(("10".into(), "USD".into()))
        );
    }

    /// Go `json.Marshal` of a map (vector from the DujiaoPay create body).
    #[test]
    fn go_json_map_matches_go() {
        let v = json!({"fiat_currency": "CNY", "fiat_amount": "9.9", "merchant_order_id": "DJP1", "chain": "tron",
            "token_id": "tron-usdt", "success_url": "https://r/pay?a=1&b=<2>",
            "metadata": {"payment_id": 5, "order_id": 3, "subject": "DJ1 订单"}});
        assert_eq!(
            go_json_map(&v),
            r#"{"chain":"tron","fiat_amount":"9.9","fiat_currency":"CNY","merchant_order_id":"DJP1","metadata":{"order_id":3,"payment_id":5,"subject":"DJ1 订单"},"success_url":"https://r/pay?a=1\u0026b=\u003c2\u003e","token_id":"tron-usdt"}"#
        );
        assert_eq!(
            go_json_map(&json!({"a": json_float(12.5), "b": json_float(100.0)})),
            r#"{"a":12.5,"b":100}"#
        );
    }

    #[test]
    fn request_uri_validation() {
        assert!(is_request_uri("https://example.com/cb"));
        assert!(is_request_uri("/relative"));
        assert!(!is_request_uri("example.com"));
        assert!(!is_request_uri("https://exa mple.com"));
        assert!(!is_request_uri(""));
    }

    /// Keys are accepted as PKCS#8 or PKCS#1 PEM, with literal `\n`, or as a bare body.
    #[test]
    fn rsa_key_formats() {
        use crate::payment::test_support::{PRIV_PEM, PRIV_PKCS1_PEM, PUB_PEM};
        let pkcs8 = parse_rsa_private_key(PRIV_PEM);
        let pkcs1 = parse_rsa_private_key(PRIV_PKCS1_PEM);
        assert!(pkcs8.is_some() && pkcs8 == pkcs1);
        let escaped = PRIV_PEM.replace('\n', "\\n");
        assert!(parse_rsa_private_key(&escaped).is_some());
        let body: String = PUB_PEM
            .lines()
            .filter(|l| !l.starts_with("-----"))
            .collect();
        assert!(parse_rsa_public_key(&body).is_some());
        assert!(parse_rsa_public_key("garbage").is_none());
    }

    #[test]
    fn constant_time_signature_compare() {
        assert!(signature_eq("ABCdef", "abcDEF"));
        assert!(!signature_eq("abc", "abcd"));
    }
}
