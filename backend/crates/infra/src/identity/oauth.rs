//! Telegram/Google HTTP adapters: JWKS-backed RS256 verification (with the
//! caching rules of the original `googleauth` / `telegramauth` services) and
//! the OIDC authorization-code exchange.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration, Utc};
use rsa::pkcs1v15::{Signature, VerifyingKey};
use rsa::signature::Verifier as _;
use rsa::{BigUint, RsaPublicKey};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use tokio::sync::Mutex;
use zs_domain::identity::oauth::{JwksProfile, JwtFailure, JwtVerifier, OidcTokenClient, keys};
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;

/// Largest JWKS / token response read (original `maxJWKSResponseBytes`, 1 MiB).
const MAX_BODY_BYTES: usize = 1 << 20;
/// Google JWKS request timeout (original `defaultHTTPTimeout`).
const GOOGLE_TIMEOUT: StdDuration = StdDuration::from_secs(5);
/// Telegram request timeout (original HTTP client timeout).
const TELEGRAM_TIMEOUT: StdDuration = StdDuration::from_secs(10);
/// Google keys cached without `Cache-Control: max-age` (original 1 h).
const GOOGLE_DEFAULT_TTL_SECONDS: i64 = 3600;
/// Minimum spacing of forced refreshes for unknown Google kids (original 5 s).
const GOOGLE_UNKNOWN_KID_COOLDOWN_SECONDS: i64 = 5;
/// Telegram keys cache lifetime (original 10 min).
const TELEGRAM_TTL_SECONDS: i64 = 600;
/// Kid used for Telegram keys without one.
const TELEGRAM_DEFAULT_KID: &str = "_default";

#[derive(Debug, Default)]
struct KeySet {
    keys: HashMap<String, RsaPublicKey>,
    until: Option<DateTime<Utc>>,
    last_forced: Option<DateTime<Utc>>,
}

impl KeySet {
    fn fresh(&self, now: DateTime<Utc>) -> bool {
        self.until.is_some_and(|u| now < u)
    }
}

#[derive(Debug, Deserialize)]
struct Jwk {
    #[serde(default)]
    kty: String,
    #[serde(default)]
    kid: String,
    #[serde(default, rename = "use")]
    usage: String,
    #[serde(default)]
    alg: String,
    #[serde(default)]
    n: String,
    #[serde(default)]
    e: String,
}

#[derive(Debug, Deserialize)]
struct JwkSet {
    #[serde(default)]
    keys: Vec<Jwk>,
}

/// Google key (`parseGoogleRSAJWK`): RSA, kid, `use` sig, `alg` RS256, e ≥ 3.
fn google_key(k: &Jwk) -> Option<(String, RsaPublicKey)> {
    let kid = k.kid.trim();
    if k.kty != "RSA" || kid.is_empty() {
        return None;
    }
    if !(k.usage.is_empty() || k.usage == "sig") || !(k.alg.is_empty() || k.alg == "RS256") {
        return None;
    }
    let n = URL_SAFE_NO_PAD
        .decode(&k.n)
        .ok()
        .filter(|n| !n.is_empty())?;
    let e = URL_SAFE_NO_PAD
        .decode(&k.e)
        .ok()
        .filter(|e| !e.is_empty() && e.len() <= 4)?;
    let exponent = e.iter().fold(0u64, |acc, b| (acc << 8) | u64::from(*b));
    if exponent < 3 {
        return None;
    }
    let key = RsaPublicKey::new(BigUint::from_bytes_be(&n), BigUint::from(exponent)).ok()?;
    Some((kid.to_owned(), key))
}

/// Telegram key (`parseTelegramJWKS`): RSA (any case), padding tolerated.
fn telegram_key(k: &Jwk) -> Option<(String, RsaPublicKey)> {
    if !k.kty.eq_ignore_ascii_case("RSA") {
        return None;
    }
    let n = URL_SAFE_NO_PAD.decode(k.n.trim_end_matches('=')).ok()?;
    let e = URL_SAFE_NO_PAD.decode(k.e.trim_end_matches('=')).ok()?;
    let e = BigUint::from_bytes_be(&e);
    if e == BigUint::from(0u8) {
        return None;
    }
    let key = RsaPublicKey::new(BigUint::from_bytes_be(&n), e).ok()?;
    let kid = if k.kid.is_empty() {
        TELEGRAM_DEFAULT_KID.to_owned()
    } else {
        k.kid.clone()
    };
    Some((kid, key))
}

/// `max-age` of a `Cache-Control` header.
fn max_age(cache_control: &str) -> Option<i64> {
    cache_control.split(',').find_map(|d| {
        let (name, value) = d.trim().split_once('=')?;
        if !name.trim().eq_ignore_ascii_case("max-age") {
            return None;
        }
        value
            .trim()
            .trim_matches('"')
            .parse::<i64>()
            .ok()
            .filter(|s| *s > 0)
    })
}

/// Reads at most [`MAX_BODY_BYTES`] of a response body.
async fn read_limited(mut res: reqwest::Response) -> std::result::Result<Vec<u8>, String> {
    let mut body = Vec::new();
    while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
        let room = MAX_BODY_BYTES.saturating_sub(body.len());
        body.extend_from_slice(&chunk[..chunk.len().min(room)]);
        if body.len() >= MAX_BODY_BYTES {
            break;
        }
    }
    Ok(body)
}

/// JWKS-backed RS256 verifier.
#[derive(Clone)]
pub struct HttpJwtVerifier {
    http: reqwest::Client,
    clock: Arc<dyn Clock>,
    /// Key sets per JWKS URL; the lock also serialises refreshes.
    sets: Arc<Mutex<HashMap<String, KeySet>>>,
}

impl std::fmt::Debug for HttpJwtVerifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("HttpJwtVerifier")
    }
}

impl HttpJwtVerifier {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            http: reqwest::Client::new(),
            clock,
            sets: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Downloads and parses a JWKS; `Err` when unreachable or without usable keys.
    async fn fetch(
        &self,
        url: &str,
        profile: JwksProfile,
    ) -> std::result::Result<(HashMap<String, RsaPublicKey>, Option<i64>), String> {
        let timeout = match profile {
            JwksProfile::Google => GOOGLE_TIMEOUT,
            JwksProfile::Telegram => TELEGRAM_TIMEOUT,
        };
        let res = self
            .http
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = res.status();
        let ok = match profile {
            JwksProfile::Google => status == reqwest::StatusCode::OK,
            JwksProfile::Telegram => status.is_success(),
        };
        if !ok {
            return Err(format!("jwks http {status}"));
        }
        let cache_control = res
            .headers()
            .get(reqwest::header::CACHE_CONTROL)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_owned();
        let body = read_limited(res).await?;
        let set: JwkSet = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        let parse = match profile {
            JwksProfile::Google => google_key,
            JwksProfile::Telegram => telegram_key,
        };
        let keys: HashMap<_, _> = set.keys.iter().filter_map(parse).collect();
        if keys.is_empty() {
            return Err("no usable RSA signing keys".into());
        }
        Ok((keys, max_age(&cache_control)))
    }

    async fn refresh(
        &self,
        set: &mut KeySet,
        url: &str,
        profile: JwksProfile,
    ) -> std::result::Result<(), JwtFailure> {
        let (keys, max_age) = self
            .fetch(url, profile)
            .await
            .map_err(JwtFailure::KeyUnavailable)?;
        let ttl = match profile {
            JwksProfile::Google => max_age.unwrap_or(GOOGLE_DEFAULT_TTL_SECONDS),
            JwksProfile::Telegram => TELEGRAM_TTL_SECONDS,
        };
        set.keys = keys;
        set.until = Some(self.clock.now() + Duration::seconds(ttl));
        Ok(())
    }

    async fn google_key_for(
        &self,
        url: &str,
        kid: &str,
    ) -> std::result::Result<RsaPublicKey, JwtFailure> {
        let mut sets = self.sets.lock().await;
        let set = sets.entry(url.to_owned()).or_default();
        let now = self.clock.now();
        if set.fresh(now) {
            if let Some(key) = set.keys.get(kid) {
                return Ok(key.clone());
            }
            // Fresh cache without this kid: at most one forced refresh per
            // cooldown window so arbitrary kids cannot amplify JWKS traffic.
            let cooling = set
                .last_forced
                .is_some_and(|t| now - t < Duration::seconds(GOOGLE_UNKNOWN_KID_COOLDOWN_SECONDS));
            if !cooling {
                let refreshed = self.refresh(set, url, JwksProfile::Google).await;
                set.last_forced = Some(self.clock.now());
                refreshed?;
            }
        } else {
            self.refresh(set, url, JwksProfile::Google).await?;
        }
        set.keys.get(kid).cloned().ok_or(JwtFailure::UnknownKey)
    }

    async fn telegram_key_for(
        &self,
        url: &str,
        kid: &str,
    ) -> std::result::Result<RsaPublicKey, JwtFailure> {
        let kid = if kid.is_empty() {
            TELEGRAM_DEFAULT_KID
        } else {
            kid
        };
        let mut sets = self.sets.lock().await;
        let set = sets.entry(url.to_owned()).or_default();
        if set.fresh(self.clock.now())
            && let Some(key) = set.keys.get(kid)
        {
            return Ok(key.clone());
        }
        self.refresh(set, url, JwksProfile::Telegram).await?;
        if let Some(key) = set.keys.get(kid) {
            return Ok(key.clone());
        }
        match set.keys.values().next() {
            Some(only) if set.keys.len() == 1 => Ok(only.clone()),
            _ => Err(JwtFailure::UnknownKey),
        }
    }
}

fn segment(raw: &str) -> std::result::Result<Vec<u8>, JwtFailure> {
    URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|e| JwtFailure::Malformed(e.to_string()))
}

#[async_trait]
impl JwtVerifier for HttpJwtVerifier {
    async fn verify_rs256(
        &self,
        token: &str,
        jwks_url: &str,
        profile: JwksProfile,
    ) -> std::result::Result<Value, JwtFailure> {
        let parts: Vec<&str> = token.split('.').collect();
        let [header_raw, claims_raw, signature_raw] = parts.as_slice() else {
            return Err(JwtFailure::Malformed(
                "token must have three segments".into(),
            ));
        };
        let header: Value = serde_json::from_slice(&segment(header_raw)?)
            .map_err(|e| JwtFailure::Malformed(e.to_string()))?;
        let claims: Value = serde_json::from_slice(&segment(claims_raw)?)
            .map_err(|e| JwtFailure::Malformed(e.to_string()))?;
        if !claims.is_object() {
            return Err(JwtFailure::Malformed("claims must be an object".into()));
        }
        if header.get("alg").and_then(Value::as_str) != Some("RS256") {
            return Err(JwtFailure::BadSignature);
        }
        let signature = segment(signature_raw)?;
        let kid = header.get("kid").and_then(Value::as_str).unwrap_or("");
        let key = match profile {
            JwksProfile::Google => {
                let kid = kid.trim();
                if kid.is_empty() {
                    return Err(JwtFailure::UnknownKey);
                }
                self.google_key_for(jwks_url, kid).await?
            }
            JwksProfile::Telegram => self.telegram_key_for(jwks_url, kid).await?,
        };
        let signature =
            Signature::try_from(signature.as_slice()).map_err(|_| JwtFailure::BadSignature)?;
        let signing_input = format!("{header_raw}.{claims_raw}");
        VerifyingKey::<Sha256>::new(key)
            .verify(signing_input.as_bytes(), &signature)
            .map_err(|_| JwtFailure::BadSignature)?;
        Ok(claims)
    }
}

/// Authorization-code exchange over HTTP.
#[derive(Debug, Clone, Default)]
pub struct HttpOidcTokenClient {
    http: reqwest::Client,
}

impl HttpOidcTokenClient {
    pub fn new() -> Self {
        Self::default()
    }
}

fn exchange_failed(detail: impl std::fmt::Display) -> Error {
    tracing::warn!(error = %detail, "telegram oidc token exchange failed");
    Error::bad_request(keys::TELEGRAM_OIDC_EXCHANGE_FAILED)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    id_token: String,
}

#[async_trait]
impl OidcTokenClient for HttpOidcTokenClient {
    async fn exchange(
        &self,
        token_url: &str,
        form: &[(String, String)],
        client_id: &str,
        client_secret: &str,
    ) -> Result<String> {
        let basic = STANDARD.encode(format!("{client_id}:{client_secret}"));
        let res = self
            .http
            .post(token_url)
            .timeout(TELEGRAM_TIMEOUT)
            .header(reqwest::header::AUTHORIZATION, format!("Basic {basic}"))
            .form(form)
            .send()
            .await
            .map_err(exchange_failed)?;
        let status = res.status();
        let body = read_limited(res).await.map_err(exchange_failed)?;
        if !status.is_success() {
            return Err(exchange_failed(format!("http {status}")));
        }
        let parsed: TokenResponse = serde_json::from_slice(&body).map_err(exchange_failed)?;
        let id_token = parsed.id_token.trim();
        if id_token.is_empty() {
            return Err(exchange_failed("id_token missing"));
        }
        Ok(id_token.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_control_max_age() {
        assert_eq!(
            max_age("public, max-age=19204, must-revalidate"),
            Some(19204)
        );
        assert_eq!(max_age("max-age=\"60\""), Some(60));
        assert_eq!(max_age("no-cache"), None);
        assert_eq!(max_age("max-age=0"), None);
    }

    fn jwk(kty: &str, kid: &str, usage: &str, alg: &str, e: &str) -> Jwk {
        Jwk {
            kty: kty.into(),
            kid: kid.into(),
            usage: usage.into(),
            alg: alg.into(),
            // 512-bit odd modulus is enough for parsing rules.
            n: URL_SAFE_NO_PAD.encode([0xC5u8; 64]),
            e: e.into(),
        }
    }

    #[test]
    fn google_jwk_rules() {
        assert!(google_key(&jwk("RSA", "k1", "sig", "RS256", "AQAB")).is_some());
        assert!(google_key(&jwk("RSA", "k1", "", "", "AQAB")).is_some());
        assert!(google_key(&jwk("rsa", "k1", "sig", "RS256", "AQAB")).is_none());
        assert!(google_key(&jwk("RSA", " ", "sig", "RS256", "AQAB")).is_none());
        assert!(google_key(&jwk("RSA", "k1", "enc", "RS256", "AQAB")).is_none());
        assert!(google_key(&jwk("RSA", "k1", "sig", "RS512", "AQAB")).is_none());
        // e = 1 is rejected; padding is not accepted.
        assert!(google_key(&jwk("RSA", "k1", "sig", "RS256", "AQ")).is_none());
        assert!(google_key(&jwk("RSA", "k1", "sig", "RS256", "AQAB=")).is_none());
    }

    #[test]
    fn telegram_jwk_rules() {
        let (kid, _) = telegram_key(&jwk("rsa", "", "", "", "AQAB=")).unwrap();
        assert_eq!(kid, TELEGRAM_DEFAULT_KID);
        assert!(telegram_key(&jwk("EC", "k", "", "", "AQAB")).is_none());
        assert!(telegram_key(&jwk("RSA", "k", "", "", "AA")).is_none());
    }
}
