//! Official WeChat Pay API v3: request signing, response/notification verification
//! (platform certificates, WeChat Pay public key, or combined), AES-256-GCM resource
//! decryption and the public-key security test (PAY-32, PAY-33, PAY-07).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use chrono::{DateTime, Utc};
use rsa::pkcs1::DecodeRsaPublicKey;
use rsa::pkcs8::DecodePublicKey;
use rsa::{RsaPrivateKey, RsaPublicKey};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Map, Value};
use x509_cert::Certificate;
use x509_cert::der::{DecodePem, Encode};
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{path_escape, query_escape};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, GatewayQueryResult, GatewaySecurityTestResult, Headers, PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{InteractionMode, PaymentStatus, SITE_CURRENCY_DEFAULT};
use zs_shared::money::Amount;

use super::common::{
    ExchangeRate, GatewayEnv, RsaHash, decode_object, go_decimal_string, go_json_map,
    is_request_uri, parse_config, pem_decode, rsa_sign, rsa_verify,
};
use super::http::{HttpRequest, HttpResponse};

const DEFAULT_BASE_URL: &str = "https://api.mch.weixin.qq.com";
/// The SDK always downloads platform certificates from the official host.
const CERTIFICATES_URL: &str = "https://api.mch.weixin.qq.com/v3/certificates";
const SECURITY_ECHO_PATH: &str = "/v3/security/echo";
const MODE_PLATFORM: &str = "platform_certificate";
const MODE_PUBLIC_KEY: &str = "wechatpay_public_key";
const MODE_COMBINED: &str = "combined";
const SIGNATURE_TYPE: &str = "WECHATPAY2-SHA256-RSA2048";
const AEAD_ALGORITHM: &str = "AEAD_AES_256_GCM";
/// Allowed clock skew of signed responses/notifications (`consts.FiveMinute`).
const MAX_SKEW_SECS: i64 = 300;
/// Platform certificates are refreshed daily (`DefaultDownloadInterval`).
const CERT_REFRESH_SECS: i64 = 24 * 3600;
const NONCE_SYMBOLS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const NONCE_LEN: usize = 32;
/// API v3 keys are 32-byte AES-256 keys.
const API_V3_KEY_LEN: usize = 32;
const GCM_NONCE_LEN: usize = 12;

/// WeChat Pay channel config.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub appid: String,
    pub mchid: String,
    pub merchant_serial_no: String,
    pub merchant_private_key: String,
    pub api_v3_key: String,
    pub verification_mode: String,
    pub wechatpay_public_key_id: String,
    pub wechatpay_public_key: String,
    pub notify_url: String,
    pub h5_redirect_url: String,
    pub h5_type: String,
    pub h5_wap_url: String,
    pub h5_wap_name: String,
    pub base_url: String,
    pub target_currency: String,
    pub exchange_rate: String,
}

fn config_err(msg: &str) -> GatewayError {
    GatewayError::config(format!("wechatpay config invalid: {msg}"))
}

impl Config {
    pub fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "wechatpay")?;
        for f in [
            &mut c.appid,
            &mut c.mchid,
            &mut c.merchant_serial_no,
            &mut c.merchant_private_key,
            &mut c.api_v3_key,
            &mut c.wechatpay_public_key_id,
            &mut c.wechatpay_public_key,
            &mut c.notify_url,
            &mut c.h5_redirect_url,
            &mut c.h5_wap_url,
            &mut c.h5_wap_name,
        ] {
            *f = f.trim().to_owned();
        }
        c.verification_mode = c.verification_mode.trim().to_ascii_lowercase();
        if c.verification_mode.is_empty() {
            c.verification_mode = MODE_PLATFORM.to_owned();
        }
        c.h5_type = c.h5_type.trim().to_ascii_uppercase();
        if c.h5_type.is_empty() {
            c.h5_type = "WAP".to_owned();
        }
        c.base_url = c.base_url.trim().trim_end_matches('/').to_owned();
        if c.base_url.is_empty() {
            c.base_url = DEFAULT_BASE_URL.to_owned();
        }
        Ok(c)
    }

    /// `validateBaseConfig`.
    pub fn validate_base(&self) -> Result<(), GatewayError> {
        for (name, value) in [
            ("appid", &self.appid),
            ("mchid", &self.mchid),
            ("merchant_serial_no", &self.merchant_serial_no),
            ("merchant_private_key", &self.merchant_private_key),
            ("api_v3_key", &self.api_v3_key),
        ] {
            if value.is_empty() {
                return Err(config_err(&format!("{name} is required")));
            }
        }
        if self.api_v3_key.len() != API_V3_KEY_LEN {
            return Err(config_err("api_v3_key must be 32 chars"));
        }
        if self.notify_url.is_empty() {
            return Err(config_err("notify_url is required"));
        }
        if !is_request_uri(&self.notify_url) {
            return Err(config_err("notify_url is invalid"));
        }
        if !is_request_uri(&self.base_url) {
            return Err(config_err("base_url is invalid"));
        }
        parse_private_key(&self.merchant_private_key)?;
        match self.verification_mode.as_str() {
            MODE_PLATFORM => Ok(()),
            MODE_PUBLIC_KEY | MODE_COMBINED => {
                if self.wechatpay_public_key_id.is_empty() {
                    return Err(config_err("wechatpay_public_key_id is required"));
                }
                if self.wechatpay_public_key.is_empty() {
                    return Err(config_err("wechatpay_public_key is required"));
                }
                parse_public_key(&self.wechatpay_public_key).map(|_| ())
            }
            _ => Err(config_err("verification_mode is invalid")),
        }
    }

    /// `ValidateConfig(cfg, interactionMode)`.
    pub fn validate(&self, mode: &str) -> Result<(), GatewayError> {
        self.validate_base()?;
        if !self.h5_redirect_url.is_empty() && !is_request_uri(&self.h5_redirect_url) {
            return Err(config_err("h5_redirect_url is invalid"));
        }
        if !self.h5_wap_url.is_empty() && !is_request_uri(&self.h5_wap_url) {
            return Err(config_err("h5_wap_url is invalid"));
        }
        let mode = InteractionMode::parse(mode);
        if !matches!(mode, Some(InteractionMode::Qr | InteractionMode::Redirect)) {
            return Err(config_err("interaction_mode is not supported"));
        }
        if mode == Some(InteractionMode::Redirect) && self.h5_redirect_url.is_empty() {
            return Err(config_err("h5_redirect_url is required for mode redirect"));
        }
        if !matches!(self.h5_type.as_str(), "WAP" | "IOS" | "ANDROID") {
            return Err(config_err("h5_type is invalid"));
        }
        Ok(())
    }
}

fn wrap_pem(raw: &str, label: &str) -> String {
    let normalized = raw.replace("\\n", "\n").trim().to_owned();
    if normalized.contains("BEGIN") {
        normalized
    } else {
        format!("-----BEGIN {label}-----\n{normalized}\n-----END {label}-----")
    }
}

/// Merchant private key (PEM PKCS#8 or PKCS#1, or a bare body wrapped as PKCS#8).
pub fn parse_private_key(raw: &str) -> Result<RsaPrivateKey, GatewayError> {
    if raw.trim().is_empty() {
        return Err(config_err("merchant_private_key is empty"));
    }
    let pem = wrap_pem(raw, "PRIVATE KEY");
    let (_, der) =
        pem_decode(&pem).ok_or_else(|| config_err("merchant_private_key pem decode failed"))?;
    use rsa::pkcs1::DecodeRsaPrivateKey;
    use rsa::pkcs8::DecodePrivateKey;
    RsaPrivateKey::from_pkcs8_der(&der)
        .or_else(|_| RsaPrivateKey::from_pkcs1_der(&der))
        .map_err(|_| config_err("parse merchant_private_key failed"))
}

/// WeChat Pay public key: PEM `PUBLIC KEY` or `RSA PUBLIC KEY` only.
pub fn parse_public_key(raw: &str) -> Result<RsaPublicKey, GatewayError> {
    if raw.trim().is_empty() {
        return Err(config_err("wechatpay_public_key is empty"));
    }
    let pem = wrap_pem(raw, "PUBLIC KEY");
    let (kind, der) =
        pem_decode(&pem).ok_or_else(|| config_err("wechatpay_public_key pem decode failed"))?;
    match kind.as_str() {
        "PUBLIC KEY" => RsaPublicKey::from_public_key_der(&der)
            .map_err(|_| config_err("parse wechatpay_public_key failed")),
        "RSA PUBLIC KEY" => RsaPublicKey::from_pkcs1_der(&der)
            .map_err(|_| config_err("parse wechatpay_public_key failed")),
        _ => Err(config_err("wechatpay_public_key pem type is invalid")),
    }
}

/// AES-256-GCM decryption of a notification resource or downloaded certificate.
pub fn decrypt_aes_gcm(
    api_v3_key: &str,
    associated_data: &str,
    nonce: &str,
    ciphertext_b64: &str,
) -> Result<Vec<u8>, String> {
    let data = B64
        .decode(ciphertext_b64.trim())
        .map_err(|e| e.to_string())?;
    if nonce.len() != GCM_NONCE_LEN {
        return Err("invalid nonce length".into());
    }
    let cipher = Aes256Gcm::new_from_slice(api_v3_key.as_bytes()).map_err(|e| e.to_string())?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce.as_bytes()),
            Payload {
                msg: &data,
                aad: associated_data.as_bytes(),
            },
        )
        .map_err(|_| "decrypt failed".to_owned())
}

/// A downloaded platform certificate.
#[derive(Debug, Clone)]
struct PlatformCert {
    key: RsaPublicKey,
    not_before: u64,
}

/// Response/notification signature verifier (`auth.Verifier`).
#[derive(Debug, Clone)]
pub enum Verifier {
    PublicKey {
        id: String,
        key: RsaPublicKey,
    },
    Certificates {
        certs: HashMap<String, RsaPublicKey>,
        newest: String,
    },
    Combined {
        id: String,
        key: RsaPublicKey,
        certs: HashMap<String, RsaPublicKey>,
    },
}

impl Verifier {
    /// `Wechatpay-Serial` asked for in requests.
    pub fn accept_serial(&self) -> String {
        match self {
            Self::PublicKey { id, .. } | Self::Combined { id, .. } => id.clone(),
            Self::Certificates { newest, .. } => newest.clone(),
        }
    }

    pub fn verify(&self, serial: &str, message: &str, signature: &str) -> bool {
        if serial.trim().is_empty() || message.trim().is_empty() || signature.trim().is_empty() {
            return false;
        }
        let key = match self {
            Self::PublicKey { id, key } => (id == serial).then_some(key),
            Self::Certificates { certs, .. } => certs.get(serial),
            Self::Combined { id, key, certs } => {
                if id == serial {
                    Some(key)
                } else {
                    certs.get(serial)
                }
            }
        };
        key.is_some_and(|k| rsa_verify(k, RsaHash::Sha256, message.as_bytes(), signature))
    }
}

/// Validates signed response/notification headers against the body (`wechatPayValidator`).
pub fn validate_signed(
    verifier: &Verifier,
    headers: &Headers,
    body: &[u8],
    now: i64,
) -> Result<(), String> {
    let serial = headers.get("Wechatpay-Serial");
    let signature = headers.get("Wechatpay-Signature");
    let timestamp = headers.get("Wechatpay-Timestamp");
    let nonce = headers.get("Wechatpay-Nonce");
    if serial.is_empty() || signature.is_empty() || nonce.is_empty() {
        return Err("missing wechatpay signature headers".into());
    }
    let ts: i64 = if timestamp.is_empty() {
        0
    } else {
        timestamp
            .parse()
            .map_err(|_| "invalid Wechatpay-Timestamp".to_owned())?
    };
    if (now - ts).abs() >= MAX_SKEW_SECS {
        return Err(format!("timestamp=[{ts}] expires"));
    }
    let message = format!("{ts}\n{nonce}\n{}\n", String::from_utf8_lossy(body));
    if !verifier.verify(&serial, &message, &signature) {
        return Err(format!("verify fail serial=[{serial}]"));
    }
    Ok(())
}

fn response_headers(resp: &HttpResponse) -> Headers {
    let mut h = Headers::new();
    for (k, v) in &resp.headers {
        h.insert(k, v);
    }
    h
}

/// `trade_state` → payment status; unknown states are errors.
pub fn to_payment_status(trade_state: &str) -> Option<PaymentStatus> {
    match trade_state.trim().to_ascii_uppercase().as_str() {
        "SUCCESS" | "REFUND" => Some(PaymentStatus::Success),
        "NOTPAY" | "USERPAYING" => Some(PaymentStatus::Pending),
        "CLOSED" | "REVOKED" | "PAYERROR" => Some(PaymentStatus::Failed),
        _ => None,
    }
}

/// `convertAmountToFen`: positive and at most two decimals.
pub fn amount_to_fen(amount: &str) -> Result<i64, GatewayError> {
    let d: Decimal = amount
        .trim()
        .parse()
        .map_err(|_| config_err("amount is invalid"))?;
    if d <= Decimal::ZERO {
        return Err(config_err("amount must be greater than zero"));
    }
    let fen = d * Decimal::ONE_HUNDRED;
    if fen != fen.trunc() {
        return Err(config_err("amount precision exceeds fen"));
    }
    i64::try_from(fen).map_err(|_| config_err("amount is invalid"))
}

fn fen_to_amount(fen: i64) -> String {
    Amount::from_cents(fen).to_string()
}

/// `normalizeClientIP`: canonical IP (also from `host:port`), else `127.0.0.1`.
pub fn normalize_client_ip(raw: &str) -> String {
    let canonical = |ip: std::net::IpAddr| match ip {
        std::net::IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or_else(|| v6.to_string(), |v4| v4.to_string()),
        std::net::IpAddr::V4(v4) => v4.to_string(),
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return "127.0.0.1".into();
    }
    if let Ok(ip) = raw.parse::<std::net::IpAddr>() {
        return canonical(ip);
    }
    if let Ok(sock) = raw.parse::<std::net::SocketAddr>() {
        return canonical(sock.ip());
    }
    "127.0.0.1".into()
}

fn read_str(map: &Map<String, Value>, path: &[&str]) -> String {
    let mut cur: Option<&Value> = None;
    for (i, key) in path.iter().enumerate() {
        cur = if i == 0 {
            map.get(*key)
        } else {
            cur.and_then(Value::as_object).and_then(|m| m.get(*key))
        };
    }
    cur.and_then(Value::as_str)
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

fn read_i64(map: &Map<String, Value>, path: &[&str]) -> Option<i64> {
    let mut cur: Option<&Value> = None;
    for (i, key) in path.iter().enumerate() {
        cur = if i == 0 {
            map.get(*key)
        } else {
            cur.and_then(Value::as_object).and_then(|m| m.get(*key))
        };
    }
    match cur? {
        Value::Number(n) => n.as_i64().or_else(|| {
            #[expect(
                clippy::cast_possible_truncation,
                reason = "Go converts float64 with int64(v)"
            )]
            n.as_f64().map(|f| f as i64)
        }),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn parse_time(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw.trim())
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

type CertCache = HashMap<String, (i64, Vec<(String, PlatformCert)>)>;

/// The official WeChat Pay adapter (platform certificates cached per merchant).
#[derive(Debug, Clone)]
pub struct WechatpayGateway {
    env: GatewayEnv,
    certs: Arc<Mutex<CertCache>>,
}

impl WechatpayGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self {
            env,
            certs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn nonce(&self) -> String {
        let mut buf = [0u8; NONCE_LEN];
        self.env.entropy.fill(&mut buf);
        buf.iter()
            .map(|b| char::from(NONCE_SYMBOLS[usize::from(*b) % NONCE_SYMBOLS.len()]))
            .collect()
    }

    /// `Authorization` header of an API v3 request.
    pub fn authorization(
        &self,
        cfg: &Config,
        key: &RsaPrivateKey,
        method: &str,
        uri: &str,
        body: &str,
    ) -> Result<String, GatewayError> {
        let nonce = self.nonce();
        let ts = self.env.clock.now().timestamp();
        let message = format!("{method}\n{uri}\n{ts}\n{nonce}\n{body}\n");
        let signature = rsa_sign(key, RsaHash::Sha256, message.as_bytes())
            .ok_or_else(|| GatewayError::config("wechatpay config invalid: sign request failed"))?;
        Ok(format!(
            "{SIGNATURE_TYPE} mchid=\"{}\",nonce_str=\"{nonce}\",timestamp=\"{ts}\",serial_no=\"{}\",signature=\"{signature}\"",
            cfg.mchid, cfg.merchant_serial_no
        ))
    }

    async fn send_signed(
        &self,
        cfg: &Config,
        key: &RsaPrivateKey,
        method: &str,
        url: &str,
        body: &str,
        verifier: Option<&Verifier>,
    ) -> Result<HttpResponse, GatewayError> {
        let uri = url
            .split_once("://")
            .and_then(|(_, rest)| rest.find('/').map(|i| &rest[i..]))
            .unwrap_or("/");
        let mut req = HttpRequest::new(method, url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header(
                "Authorization",
                self.authorization(cfg, key, method, uri, body)?,
            );
        if let Some(v) = verifier {
            let serial = v.accept_serial();
            if !serial.is_empty() {
                req = req.header("Wechatpay-Serial", serial);
            }
        }
        let resp = self
            .env
            .http
            .send(req.body(body.as_bytes().to_vec()))
            .await
            .map_err(|e| GatewayError::request(format!("wechatpay request failed: {e}")))?;
        if !resp.is_success() {
            let message = decode_object(&resp.body)
                .and_then(|m| m.get("message").and_then(Value::as_str).map(str::to_owned))
                .unwrap_or_default();
            return Err(GatewayError::response(format!(
                "wechatpay response invalid: {}",
                message.trim()
            )));
        }
        if let Some(v) = verifier {
            validate_signed(
                v,
                &response_headers(&resp),
                &resp.body,
                self.env.clock.now().timestamp(),
            )
            .map_err(|e| GatewayError::signature(format!("verify API response failed: {e}")))?;
        }
        Ok(resp)
    }

    /// Downloads (and caches for a day) the platform certificates of a merchant.
    async fn platform_certs(
        &self,
        cfg: &Config,
        key: &RsaPrivateKey,
    ) -> Result<Vec<(String, PlatformCert)>, GatewayError> {
        let now = self.env.clock.now().timestamp();
        let cached = self
            .certs
            .lock()
            .ok()
            .and_then(|m| m.get(&cfg.mchid).cloned());
        if let Some((fetched, certs)) = &cached
            && now - fetched < CERT_REFRESH_SECS
        {
            return Ok(certs.clone());
        }
        // The first download is unauthenticated (like the SDK); refreshes are verified with the
        // previously trusted certificates.
        let previous = cached.map(|(_, c)| certificates_verifier(&c));
        let resp = self
            .send_signed(cfg, key, "GET", CERTIFICATES_URL, "", previous.as_ref())
            .await
            .map_err(|_| GatewayError::request("register certificate downloader failed"))?;
        let body = decode_object(&resp.body)
            .ok_or_else(|| GatewayError::request("register certificate downloader failed"))?;
        let mut certs = Vec::new();
        for item in body
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let Some(item) = item.as_object() else {
                continue;
            };
            let serial = item
                .get("serial_no")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let enc = item
                .get("encrypt_certificate")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default();
            let field = |k: &str| {
                enc.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned()
            };
            let pem = decrypt_aes_gcm(
                &cfg.api_v3_key,
                &field("associated_data"),
                &field("nonce"),
                &field("ciphertext"),
            )
            .map_err(|_| GatewayError::request("register certificate downloader failed"))?;
            let cert = parse_certificate(&pem)
                .ok_or_else(|| GatewayError::request("register certificate downloader failed"))?;
            certs.push((serial, cert));
        }
        if certs.is_empty() {
            return Err(GatewayError::request(
                "register certificate downloader failed: no certificate downloaded",
            ));
        }
        if let Ok(mut m) = self.certs.lock() {
            m.insert(cfg.mchid.clone(), (now, certs.clone()));
        }
        Ok(certs)
    }

    /// `createWechatPayVerifier`: mode-dependent verifier shared by API responses and notifications.
    async fn verifier(&self, cfg: &Config, key: &RsaPrivateKey) -> Result<Verifier, GatewayError> {
        match cfg.verification_mode.as_str() {
            MODE_PUBLIC_KEY => Ok(Verifier::PublicKey {
                id: cfg.wechatpay_public_key_id.clone(),
                key: parse_public_key(&cfg.wechatpay_public_key)?,
            }),
            MODE_PLATFORM => Ok(certificates_verifier(&self.platform_certs(cfg, key).await?)),
            MODE_COMBINED => {
                let certs = self.platform_certs(cfg, key).await?;
                Ok(Verifier::Combined {
                    id: cfg.wechatpay_public_key_id.clone(),
                    key: parse_public_key(&cfg.wechatpay_public_key)?,
                    certs: certs.into_iter().map(|(s, c)| (s, c.key)).collect(),
                })
            }
            _ => Err(config_err("verification_mode is invalid")),
        }
    }

    /// Pre-seeds platform certificates (tests and warm starts).
    pub fn seed_certificates(&self, mchid: &str, certs: &[(&str, &str)]) -> bool {
        let parsed: Option<Vec<(String, PlatformCert)>> = certs
            .iter()
            .map(|(serial, pem)| {
                parse_certificate(pem.as_bytes()).map(|c| ((*serial).to_owned(), c))
            })
            .collect();
        match (parsed, self.certs.lock()) {
            (Some(parsed), Ok(mut m)) => {
                m.insert(mchid.to_owned(), (self.env.clock.now().timestamp(), parsed));
                true
            }
            _ => false,
        }
    }

    /// `VerifyAndDecodeWebhook`.
    async fn decode_notification(
        &self,
        cfg: &Config,
        headers: &Headers,
        body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        cfg.validate_base()?;
        if body.is_empty() {
            return Err(GatewayError::response(
                "wechatpay response invalid: empty webhook body",
            ));
        }
        let key = parse_private_key(&cfg.merchant_private_key)?;
        let verifier = self.verifier(cfg, &key).await?;
        let sig_type = match headers.get("Wechatpay-Signature-Type") {
            t if t.is_empty() => SIGNATURE_TYPE.to_owned(),
            t => t,
        };
        let sig_err =
            |m: String| GatewayError::signature(format!("wechatpay signature invalid: {m}"));
        if sig_type != SIGNATURE_TYPE {
            return Err(sig_err(format!(
                "unsupported Wechatpay-Signature-Type: {sig_type}"
            )));
        }
        validate_signed(&verifier, headers, body, self.env.clock.now().timestamp())
            .map_err(sig_err)?;
        let raw = decode_object(body).ok_or_else(|| sig_err("parse request body error".into()))?;
        let resource = raw
            .get("resource")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let field = |k: &str| {
            resource
                .get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        if field("algorithm") != AEAD_ALGORITHM {
            return Err(sig_err("possible invalid notification".into()));
        }
        let plaintext = decrypt_aes_gcm(
            &cfg.api_v3_key,
            &field("associated_data"),
            &field("nonce"),
            &field("ciphertext"),
        )
        .map_err(|e| sig_err(format!("{AEAD_ALGORITHM} decrypt error: {e}")))?;
        let tx = decode_object(&plaintext)
            .ok_or_else(|| sig_err("unmarshal plaintext to content failed".into()))?;
        let status = to_payment_status(&read_str(&tx, &["trade_state"])).ok_or_else(|| {
            GatewayError::response("wechatpay response invalid: unsupported trade_state")
        })?;
        let amount = read_i64(&tx, &["amount", "total"])
            .map(fen_to_amount)
            .unwrap_or_default();
        let mut payload = raw;
        payload.insert("resource_plaintext".into(), Value::Object(tx.clone()));
        Ok(GatewayCallbackResult {
            order_no: read_str(&tx, &["out_trade_no"]),
            provider_ref: read_str(&tx, &["transaction_id"]),
            status: Some(status),
            amount: super::common::callback_amount(&amount),
            currency: read_str(&tx, &["amount", "currency"]).to_ascii_uppercase(),
            paid_at: parse_time(&read_str(&tx, &["success_time"])),
            payload,
        })
    }
}

fn certificates_verifier(certs: &[(String, PlatformCert)]) -> Verifier {
    let newest = certs
        .iter()
        .max_by_key(|(_, c)| c.not_before)
        .map(|(s, _)| s.clone())
        .unwrap_or_default();
    Verifier::Certificates {
        certs: certs
            .iter()
            .map(|(s, c)| (s.clone(), c.key.clone()))
            .collect(),
        newest,
    }
}

fn parse_certificate(pem: &[u8]) -> Option<PlatformCert> {
    let cert = Certificate::from_pem(pem).ok()?;
    let spki = cert.tbs_certificate.subject_public_key_info.to_der().ok()?;
    let key = RsaPublicKey::from_public_key_der(&spki).ok()?;
    let not_before = cert
        .tbs_certificate
        .validity
        .not_before
        .to_unix_duration()
        .as_secs();
    Some(PlatformCert { key, not_before })
}

fn description(subject: &str, order_no: &str) -> String {
    let subject = subject.trim();
    if !subject.is_empty() {
        return subject.to_owned();
    }
    let order_no = order_no.trim();
    if order_no.is_empty() {
        "微信支付订单".to_owned()
    } else {
        format!("订单 {order_no}")
    }
}

/// JSON body as produced by `json.NewEncoder(...).Encode` (sorted map, trailing newline).
fn encoder_body(value: &Value) -> String {
    format!("{}\n", go_json_map(value))
}

#[async_trait]
impl PaymentGateway for WechatpayGateway {
    fn key(&self) -> &'static str {
        "official:wechat"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            query: true,
            webhook: true,
            callback: false,
            security_test: true,
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
        let mode = input
            .interaction_mode
            .map(InteractionMode::as_str)
            .unwrap_or_default();
        let cfg = Config::parse(config)?;
        cfg.validate(mode)?;
        let exchange = ExchangeRate::new(&cfg.target_currency, &cfg.exchange_rate);
        let original_amount = go_decimal_string(input.amount.decimal());
        let (pay_amount, pay_currency) = exchange.convert(&original_amount, &input.currency, 2)?;
        if input.order_no.trim().is_empty() || pay_amount.trim().is_empty() {
            return Err(config_err("order input is invalid"));
        }
        let fen = amount_to_fen(&pay_amount)?;
        let key = parse_private_key(&cfg.merchant_private_key)?;
        let verifier = self.verifier(&cfg, &key).await?;
        let notify_url = match input.notify_url.trim() {
            "" => cfg.notify_url.clone(),
            v => v.to_owned(),
        };
        let client_ip = normalize_client_ip(&input.client_ip);
        let mut payload = serde_json::json!({
            "appid": cfg.appid,
            "mchid": cfg.mchid,
            "description": description(&input.subject, &input.order_no),
            "out_trade_no": input.order_no,
            "notify_url": notify_url,
            "amount": {"total": fen, "currency": SITE_CURRENCY_DEFAULT},
        });
        let endpoint = match input.interaction_mode {
            Some(InteractionMode::Redirect) => {
                let mut h5 = serde_json::json!({"type": cfg.h5_type});
                if !cfg.h5_wap_name.is_empty() {
                    h5["app_name"] = Value::String(cfg.h5_wap_name.clone());
                }
                if !cfg.h5_wap_url.is_empty() {
                    h5["app_url"] = Value::String(cfg.h5_wap_url.clone());
                }
                payload["scene_info"] =
                    serde_json::json!({"payer_client_ip": client_ip, "h5_info": h5});
                "/v3/pay/transactions/h5"
            }
            Some(InteractionMode::Qr) => {
                payload["scene_info"] = serde_json::json!({"payer_client_ip": client_ip});
                "/v3/pay/transactions/native"
            }
            _ => return Err(config_err("interaction_mode is not supported")),
        };
        let url = format!("{}{endpoint}", cfg.base_url);
        let resp = self
            .send_signed(
                &cfg,
                &key,
                "POST",
                &url,
                &encoder_body(&payload),
                Some(&verifier),
            )
            .await?;
        let raw = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("wechatpay response invalid: decode response failed")
        })?;
        let prepay_id = read_str(&raw, &["prepay_id"]);
        let (redirect_url, qr_code_url) =
            if input.interaction_mode == Some(InteractionMode::Redirect) {
                let h5_url = read_str(&raw, &["h5_url"]);
                if h5_url.is_empty() {
                    return Err(GatewayError::response(
                        "wechatpay response invalid: missing h5_url",
                    ));
                }
                let pay_url = if cfg.h5_redirect_url.is_empty() {
                    h5_url
                } else {
                    let mut params = BTreeMap::new();
                    params.insert("redirect_url".to_owned(), cfg.h5_redirect_url.clone());
                    append_query_params(&h5_url, &params)
                };
                (pay_url, String::new())
            } else {
                let code_url = read_str(&raw, &["code_url"]);
                if code_url.is_empty() {
                    return Err(GatewayError::response(
                        "wechatpay response invalid: missing code_url",
                    ));
                }
                (String::new(), code_url)
            };
        let mut result_payload = Map::new();
        result_payload.insert("prepay_id".into(), Value::String(prepay_id));
        result_payload.insert("raw".into(), Value::Object(raw));
        if exchange.needs_conversion() {
            exchange.audit(&mut result_payload, &original_amount, &input.currency);
        }
        Ok(GatewayCreateResult {
            provider_ref: String::new(),
            redirect_url,
            qr_code_url,
            payload: result_payload,
            display_channel_type: String::new(),
            amount_sent: pay_amount,
            currency_sent: pay_currency,
        })
    }

    /// Queries by the merchant order number actually sent (the gateway order number, PAY-33);
    /// no interaction-mode validation here (PAY-08).
    async fn query_payment(
        &self,
        config: &ChannelConfig,
        provider_ref: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        let cfg = Config::parse(config)?;
        cfg.validate_base()?;
        let order_no = provider_ref.trim();
        if order_no.is_empty() {
            return Err(config_err("order no is required"));
        }
        let key = parse_private_key(&cfg.merchant_private_key)?;
        let verifier = self.verifier(&cfg, &key).await?;
        let url = format!(
            "{}/v3/pay/transactions/out-trade-no/{}?mchid={}",
            cfg.base_url,
            path_escape(order_no),
            query_escape(&cfg.mchid)
        );
        let resp = self
            .send_signed(&cfg, &key, "GET", &url, "", Some(&verifier))
            .await?;
        let raw = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("wechatpay response invalid: decode response failed")
        })?;
        let status = to_payment_status(&read_str(&raw, &["trade_state"])).ok_or_else(|| {
            GatewayError::response("wechatpay response invalid: unsupported trade_state")
        })?;
        let amount = read_i64(&raw, &["amount", "total"])
            .map(fen_to_amount)
            .unwrap_or_default();
        Ok(GatewayQueryResult {
            provider_ref: read_str(&raw, &["transaction_id"]),
            status: Some(status),
            amount: super::common::callback_amount(&amount),
            currency: read_str(&raw, &["amount", "currency"]).to_ascii_uppercase(),
            paid_at: parse_time(&read_str(&raw, &["success_time"])),
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
        let cfg = Config::parse(config)?;
        self.decode_notification(&cfg, headers, body).await
    }

    /// `TestWechatPayPublicKey`: POST `/v3/security/echo`, verify with the configured public key only.
    async fn test_security(
        &self,
        config: &ChannelConfig,
    ) -> Result<GatewaySecurityTestResult, GatewayError> {
        let cfg = Config::parse(config)?;
        cfg.validate_base()?;
        if cfg.verification_mode != MODE_PUBLIC_KEY && cfg.verification_mode != MODE_COMBINED {
            return Err(config_err(
                "public key or combined verification mode is required",
            ));
        }
        if !cfg.wechatpay_public_key_id.starts_with("PUB_KEY_ID_") {
            return Err(config_err(
                "wechatpay_public_key_id must start with PUB_KEY_ID_",
            ));
        }
        let key = parse_private_key(&cfg.merchant_private_key)?;
        let verifier = Verifier::PublicKey {
            id: cfg.wechatpay_public_key_id.clone(),
            key: parse_public_key(&cfg.wechatpay_public_key)?,
        };
        let echo = format!("dujiao-next-{}", self.env.random_hex(16));
        let body = encoder_body(&serde_json::json!({"echo_message": echo}));
        let url = format!("{}{SECURITY_ECHO_PATH}", cfg.base_url);
        let resp = self
            .send_signed(&cfg, &key, "POST", &url, &body, Some(&verifier))
            .await?;
        let serial = resp.header_value("Wechatpay-Serial");
        let raw = decode_object(&resp.body).ok_or_else(|| {
            GatewayError::response("wechatpay response invalid: decode response failed")
        })?;
        if serial != cfg.wechatpay_public_key_id {
            return Err(GatewayError::signature(
                "wechatpay signature invalid: unexpected response serial",
            ));
        }
        if read_str(&raw, &["echo_message"]) != echo {
            return Err(GatewayError::response(
                "wechatpay response invalid: echo_message mismatch",
            ));
        }
        Ok(GatewaySecurityTestResult {
            verification_mode: MODE_PUBLIC_KEY.to_owned(),
            response_serial: serial,
            request_signature_accepted: true,
            response_signature_valid: true,
            echo_message_matched: true,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::payment::http::MockTransport;
    use crate::payment::test_support::{CERT_PEM, NOW_UNIX, PRIV_PEM, PUB_PEM, env_with, obj};
    use serde_json::json;

    pub(crate) const API_V3_KEY: &str = "0123456789abcdef0123456789abcdef";
    pub(crate) const PUB_KEY_ID: &str = "PUB_KEY_ID_0114";
    pub(crate) const CERT_SERIAL: &str = "5157F09EFDC096DE15EBE81A47057A7232F1B8E1";

    /// Go vector: `utils.SignSHA256WithRSA` of a request message.
    #[test]
    fn request_signature_matches_go() {
        let key = parse_private_key(PRIV_PEM).ok();
        let msg = "POST\n/v3/pay/transactions/native\n1782680000\nNONCE123\n{\"a\":1}\n";
        let sig = key
            .and_then(|k| rsa_sign(&k, RsaHash::Sha256, msg.as_bytes()))
            .unwrap_or_default();
        assert_eq!(
            sig,
            "M37KHwaQV+RsZsg1QrfW6kfJi6e/JFWz7GRG6tdAoL1I1EnJFUvf5cwMPSawdd0t/ps93iueeMIqEpF1Dwi/W6NQgyt0+cOFJh5JzrChQ0NtMjwmQIivdX21R9rWsV7k+6T2qMnbihNRqxPt2wwgjDRiNsvYJ6tHM+0M5rgIlO+7w85XNZbrfX5i49NeLvNCi5e1bzY2eFSR30lMX5qVg7Z+0lDf96dXd0T6+OuS+Rc9H9yFBpXTJpRjcSe+4Xs+f2z+TNTviNidnEsCYnhMw8PW5VMEb1Nw737v34DX8gRTkQiz9OvNsYAWT7w5NGHkJI4t0r8elPnV7jsOjBmdjg=="
        );
    }

    pub(crate) const GO_CIPHERTEXT: &str = "Eu6myr+6GaECeJdOZPZ6BX518U+IokN5asxf8qE9rZ4Fp86jRzDQmWhOopA+o7fsvHmewMk/Ep6BjDss2Miqf+6g3I65tTUeJ3UhG1wLh1iJhrQAwfJCK+tzQuITiVcFiKLBjq2ZcjSQ/lKvSJtQlzGKzHwHcBAXkFH5J5p2Niild0JsZd1Ug/xWtCbcZq2a5pFJn7PiZzSHuYHz18s4G4yLXOVundf1yDZ7rHi3G6emLZ4X+oMBBfpr";

    /// Go vector: AES-256-GCM resource decryption.
    #[test]
    fn decrypts_go_ciphertext() {
        let pt = decrypt_aes_gcm(API_V3_KEY, "transaction", "abcdefghijkl", GO_CIPHERTEXT)
            .unwrap_or_default();
        assert_eq!(
            String::from_utf8_lossy(&pt),
            r#"{"out_trade_no":"DJP1","transaction_id":"4200001","trade_state":"SUCCESS","success_time":"2026-09-24T18:01:02+08:00","amount":{"total":7243,"currency":"CNY"},"attach":""}"#
        );
        assert!(decrypt_aes_gcm(API_V3_KEY, "other-aad", "abcdefghijkl", GO_CIPHERTEXT).is_err());
        assert!(decrypt_aes_gcm(API_V3_KEY, "transaction", "short", GO_CIPHERTEXT).is_err());
    }

    pub(crate) fn cfg(mode: &str) -> ChannelConfig {
        obj(json!({
            "appid": "wx1", "mchid": "m1", "merchant_serial_no": "MSN1", "merchant_private_key": PRIV_PEM,
            "api_v3_key": API_V3_KEY, "verification_mode": mode, "wechatpay_public_key_id": PUB_KEY_ID,
            "wechatpay_public_key": PUB_PEM, "notify_url": "https://shop/api/v1/payments/callback",
            "h5_redirect_url": "https://shop/pay"
        }))
    }

    /// Builds a notification signed with the shared test key.
    pub(crate) fn notification(serial: &str, ts: i64) -> (Headers, String) {
        let body = json!({"id": "EV-1", "create_time": "2026-09-24T18:01:03+08:00", "event_type": "TRANSACTION.SUCCESS",
            "resource_type": "encrypt-resource", "resource": {"algorithm": "AEAD_AES_256_GCM", "ciphertext": GO_CIPHERTEXT,
            "associated_data": "transaction", "original_type": "transaction", "nonce": "abcdefghijkl"}, "summary": "ok"})
        .to_string();
        let msg = format!("{ts}\nNONCE456\n{body}\n");
        let key = parse_private_key(PRIV_PEM).ok();
        let sig = key
            .and_then(|k| rsa_sign(&k, RsaHash::Sha256, msg.as_bytes()))
            .unwrap_or_default();
        let headers = Headers::from([
            ("Wechatpay-Serial", serial),
            ("Wechatpay-Signature", sig.as_str()),
            ("Wechatpay-Timestamp", ts.to_string().as_str()),
            ("Wechatpay-Nonce", "NONCE456"),
        ]);
        (headers, body)
    }

    /// PAY-32: public-key mode notification verification, serial mismatch and stale timestamps.
    #[tokio::test]
    async fn pay_32_public_key_notifications() {
        let gw = WechatpayGateway::new(env_with(MockTransport::fixed(500, "")));
        let (h, body) = notification(PUB_KEY_ID, NOW_UNIX);
        let res = gw
            .parse_webhook(
                &cfg("wechatpay_public_key"),
                &h,
                body.as_bytes(),
                Utc::now(),
            )
            .await
            .unwrap_or_default();
        assert_eq!(res.status, Some(PaymentStatus::Success));
        assert_eq!(res.amount, Amount::from_cents(7243));
        assert_eq!(res.currency, "CNY");
        assert_eq!(res.order_no, "DJP1");
        assert_eq!(res.provider_ref, "4200001");
        assert_eq!(res.payload["resource_plaintext"]["trade_state"], "SUCCESS");
        let (wrong_serial, body) = notification("PUB_KEY_ID_OTHER", NOW_UNIX);
        let r = gw
            .parse_webhook(
                &cfg("wechatpay_public_key"),
                &wrong_serial,
                body.as_bytes(),
                Utc::now(),
            )
            .await;
        assert!(matches!(r, Err(GatewayError::SignatureInvalid(_))));
        let (stale, body) = notification(PUB_KEY_ID, NOW_UNIX - 300);
        let r = gw
            .parse_webhook(
                &cfg("wechatpay_public_key"),
                &stale,
                body.as_bytes(),
                Utc::now(),
            )
            .await;
        assert!(matches!(r, Err(GatewayError::SignatureInvalid(_))));
        let mut other_key = cfg("wechatpay_public_key");
        other_key.insert(
            "api_v3_key".into(),
            json!("ffffffffffffffffffffffffffffffff"),
        );
        let (h, body) = notification(PUB_KEY_ID, NOW_UNIX);
        let r = gw
            .parse_webhook(&other_key, &h, body.as_bytes(), Utc::now())
            .await;
        assert!(matches!(r, Err(GatewayError::SignatureInvalid(_))));
    }

    /// PAY-32: platform-certificate and combined modes use (seeded) platform certificates.
    #[tokio::test]
    async fn pay_32_platform_and_combined_modes() {
        let gw = WechatpayGateway::new(env_with(MockTransport::fixed(500, "")));
        assert!(gw.seed_certificates("m1", &[(CERT_SERIAL, CERT_PEM)]));
        let (h, body) = notification(CERT_SERIAL, NOW_UNIX);
        assert!(
            gw.parse_webhook(
                &cfg("platform_certificate"),
                &h,
                body.as_bytes(),
                Utc::now()
            )
            .await
            .is_ok()
        );
        assert!(
            gw.parse_webhook(&cfg("combined"), &h, body.as_bytes(), Utc::now())
                .await
                .is_ok()
        );
        let (h, body) = notification(PUB_KEY_ID, NOW_UNIX);
        assert!(
            gw.parse_webhook(&cfg("combined"), &h, body.as_bytes(), Utc::now())
                .await
                .is_ok()
        );
        assert!(
            gw.parse_webhook(
                &cfg("platform_certificate"),
                &h,
                body.as_bytes(),
                Utc::now()
            )
            .await
            .is_err()
        );
    }

    fn signed_response(body: &str, serial: &str) -> HttpResponse {
        let msg = format!("{NOW_UNIX}\nNONCE456\n{body}\n");
        let sig = parse_private_key(PRIV_PEM)
            .ok()
            .and_then(|k| rsa_sign(&k, RsaHash::Sha256, msg.as_bytes()))
            .unwrap_or_default();
        HttpResponse::new(200, body)
            .header("Wechatpay-Serial", serial)
            .header("Wechatpay-Signature", sig)
            .header("Wechatpay-Timestamp", NOW_UNIX.to_string())
            .header("Wechatpay-Nonce", "NONCE456")
    }

    /// PAY-32 / Go vector: native order request body, auth header; forged responses rejected.
    #[tokio::test]
    async fn pay_32_create_verifies_response() {
        let good = MockTransport::new(|_| {
            Ok(signed_response(
                r#"{"code_url":"weixin://wxpay/bizpayurl?pr=abc"}"#,
                PUB_KEY_ID,
            ))
        });
        let gw = WechatpayGateway::new(env_with(good.clone()));
        let input = GatewayCreateInput {
            order_no: "DJP1".into(),
            subject: String::new(),
            amount: Amount::from_cents(7243),
            currency: "CNY".into(),
            client_ip: "1.2.3.4:5678".into(),
            interaction_mode: Some(InteractionMode::Qr),
            ..GatewayCreateInput::default()
        };
        let res = gw
            .create_payment(&cfg("wechatpay_public_key"), &input)
            .await
            .unwrap_or_default();
        assert_eq!(res.qr_code_url, "weixin://wxpay/bizpayurl?pr=abc");
        let req = &good.requests()[0];
        assert_eq!(
            req.url,
            "https://api.mch.weixin.qq.com/v3/pay/transactions/native"
        );
        assert_eq!(
            String::from_utf8_lossy(&req.body),
            "{\"amount\":{\"currency\":\"CNY\",\"total\":7243},\"appid\":\"wx1\",\"description\":\"订单 DJP1\",\"mchid\":\"m1\",\"notify_url\":\"https://shop/api/v1/payments/callback\",\"out_trade_no\":\"DJP1\",\"scene_info\":{\"payer_client_ip\":\"1.2.3.4\"}}\n"
        );
        let auth = req
            .header_value("Authorization")
            .unwrap_or_default()
            .to_owned();
        assert!(
            auth.starts_with("WECHATPAY2-SHA256-RSA2048 mchid=\"m1\",nonce_str=\"")
                && auth.contains("timestamp=\"1782680000\",serial_no=\"MSN1\"")
        );
        assert_eq!(req.header_value("Wechatpay-Serial"), Some(PUB_KEY_ID));
        let forged = MockTransport::new(|_| {
            let mut r = signed_response(r#"{"code_url":"weixin://x"}"#, PUB_KEY_ID);
            r.body = br#"{"code_url":"weixin://evil"}"#.to_vec();
            Ok(r)
        });
        let gw = WechatpayGateway::new(env_with(forged));
        assert!(matches!(
            gw.create_payment(&cfg("wechatpay_public_key"), &input)
                .await,
            Err(GatewayError::SignatureInvalid(_))
        ));
    }

    /// PAY-33 / PAY-08: query by gateway order number; empty interaction mode is fine for queries.
    #[tokio::test]
    async fn pay_33_query_uses_gateway_order_no() {
        let mock = MockTransport::new(|_| {
            Ok(signed_response(
                r#"{"out_trade_no":"DJP20260924","transaction_id":"4200","trade_state":"SUCCESS","amount":{"total":100,"currency":"CNY"},"success_time":"2026-09-24T18:01:02+08:00"}"#,
                PUB_KEY_ID,
            ))
        });
        let gw = WechatpayGateway::new(env_with(mock.clone()));
        let mut c = cfg("wechatpay_public_key");
        c.remove("h5_redirect_url");
        let r = gw
            .query_payment(&c, "DJP20260924")
            .await
            .unwrap_or_default();
        assert_eq!(r.status, Some(PaymentStatus::Success));
        assert_eq!(r.amount, Amount::from(1));
        assert_eq!(
            mock.requests()[0].url,
            "https://api.mch.weixin.qq.com/v3/pay/transactions/out-trade-no/DJP20260924?mchid=m1"
        );
    }

    #[test]
    fn helpers_match_go() {
        assert_eq!(normalize_client_ip("1.2.3.4:5678"), "1.2.3.4");
        assert_eq!(normalize_client_ip("::1"), "::1");
        assert_eq!(normalize_client_ip("bad"), "127.0.0.1");
        assert_eq!(amount_to_fen("72.43").ok(), Some(7243));
        assert!(amount_to_fen("0.001").is_err());
        let mut p = BTreeMap::new();
        p.insert(
            "redirect_url".to_owned(),
            "https://shop/pay?order_no=DJ1&wechat_return=1".to_owned(),
        );
        assert_eq!(
            append_query_params(
                "https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?prepay_id=wx1&package=123",
                &p
            ),
            "https://wx.tenpay.com/cgi-bin/mmpayweb-bin/checkmweb?package=123&prepay_id=wx1&redirect_url=https%3A%2F%2Fshop%2Fpay%3Forder_no%3DDJ1%26wechat_return%3D1"
        );
    }
}
