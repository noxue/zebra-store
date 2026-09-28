//! Third-party logins (Telegram, Google): linked identities, error keys, the
//! gateway ports and the JWT claim rules shared by both providers
//! (original `externalidentity`, `telegramauth`, `googleauth`).

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::user::{NewUser, User};
use crate::{Error, Id, Result};

/// `user_oauth_identities.provider` of Telegram identities.
pub const PROVIDER_TELEGRAM: &str = "telegram";
/// `user_oauth_identities.provider` of Google identities.
pub const PROVIDER_GOOGLE: &str = "google";

/// Login-log source of Telegram logins (original `LoginLogSourceTelegram`).
pub const SOURCE_TELEGRAM: &str = "telegram";
/// Login-log source of Google logins (original `LoginLogSourceGoogle`).
pub const SOURCE_GOOGLE: &str = "google";

/// Login-log failure reasons of third-party logins (original `LoginLogFailReason*`).
pub mod reasons {
    pub const TELEGRAM_INVALID: &str = "telegram_invalid";
    pub const TELEGRAM_EXPIRED: &str = "telegram_expired";
    pub const TELEGRAM_REPLAYED: &str = "telegram_replayed";
    pub const TELEGRAM_CONFIG: &str = "telegram_config_invalid";
    pub const GOOGLE_INVALID: &str = "google_invalid";
    pub const GOOGLE_CONFIG: &str = "google_config_invalid";
}

/// Message keys (identical to the original handlers).
pub mod keys {
    pub const TELEGRAM_DISABLED: &str = "error.telegram_auth_disabled";
    pub const TELEGRAM_CONFIG_INVALID: &str = "error.telegram_auth_config_invalid";
    pub const TELEGRAM_PAYLOAD_INVALID: &str = "error.telegram_auth_payload_invalid";
    pub const TELEGRAM_SIGNATURE_INVALID: &str = "error.telegram_auth_signature_invalid";
    pub const TELEGRAM_EXPIRED: &str = "error.telegram_auth_expired";
    pub const TELEGRAM_REPLAYED: &str = "error.telegram_auth_replayed";
    pub const TELEGRAM_OIDC_STATE_INVALID: &str = "error.telegram_oidc_state_invalid";
    pub const TELEGRAM_OIDC_EXCHANGE_FAILED: &str = "error.telegram_oidc_token_exchange_failed";
    pub const TELEGRAM_OIDC_ID_TOKEN_INVALID: &str = "error.telegram_oidc_id_token_invalid";
    pub const TELEGRAM_BIND_CONFLICT: &str = "error.telegram_bind_conflict";
    pub const TELEGRAM_ALREADY_BOUND: &str = "error.telegram_already_bound";
    pub const TELEGRAM_NOT_BOUND: &str = "error.telegram_not_bound";
    pub const TELEGRAM_UNBIND_REQUIRES_EMAIL: &str = "error.telegram_unbind_requires_email";

    pub const GOOGLE_DISABLED: &str = "error.google_auth_disabled";
    pub const GOOGLE_CONFIG_INVALID: &str = "error.google_auth_config_invalid";
    pub const GOOGLE_CREDENTIAL_INVALID: &str = "error.google_credential_invalid";
    pub const GOOGLE_CREDENTIAL_EXPIRED: &str = "error.google_credential_expired";
    pub const GOOGLE_EMAIL_UNVERIFIED: &str = "error.google_email_unverified";
    pub const GOOGLE_UNAVAILABLE: &str = "error.google_service_unavailable";
    pub const GOOGLE_AUTO_LINK_FORBIDDEN: &str = "error.google_auto_link_forbidden";
    pub const GOOGLE_BIND_CONFLICT: &str = "error.google_bind_conflict";
    pub const GOOGLE_ALREADY_BOUND: &str = "error.google_already_bound";
    pub const GOOGLE_NOT_BOUND: &str = "error.google_not_bound";
    pub const GOOGLE_UNBIND_LOCKED: &str = "error.google_unbind_locked";
    pub const GOOGLE_REDIRECT_SESSION_EXPIRED: &str = "error.google_redirect_session_expired";
    pub const GOOGLE_REDIRECT_CONTEXT_MISMATCH: &str = "error.google_redirect_context_mismatch";

    pub const USER_NOT_FOUND: &str = "error.user_not_found";
    pub const USER_DISABLED: &str = "error.user_disabled";
    pub const USER_FETCH_FAILED: &str = "error.user_fetch_failed";
    pub const USER_UPDATE_FAILED: &str = "error.user_update_failed";
    pub const LOGIN_FAILED: &str = "error.login_failed";
    pub const REQUEST_TOO_LARGE: &str = "error.request_too_large";
}

/// `error.telegram_auth_config_invalid` (HTTP envelope 500 like the original).
pub fn telegram_config_invalid(detail: &str) -> Error {
    Error::internal_msg(format!("telegram auth config invalid: {detail}"))
        .or_internal(keys::TELEGRAM_CONFIG_INVALID)
}

/// `error.google_auth_config_invalid` (HTTP envelope 500 like the original).
pub fn google_config_invalid(detail: &str) -> Error {
    Error::internal_msg(format!("google auth config invalid: {detail}"))
        .or_internal(keys::GOOGLE_CONFIG_INVALID)
}

/// `error.google_service_unavailable` (JWKS or state store failures, envelope 500).
pub fn google_unavailable(detail: &str) -> Error {
    Error::internal_msg(format!("google service unavailable: {detail}"))
        .or_internal(keys::GOOGLE_UNAVAILABLE)
}

/// A `user_oauth_identities` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalIdentity {
    pub id: Id,
    pub user_id: Id,
    pub provider: String,
    pub provider_user_id: String,
    pub username: String,
    pub avatar_url: String,
    pub auth_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Fields of a new identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewExternalIdentity {
    pub user_id: Id,
    pub provider: String,
    pub provider_user_id: String,
    pub username: String,
    pub avatar_url: String,
    pub auth_at: Option<DateTime<Utc>>,
}

/// Persistence port of linked identities (`(provider, provider_user_id)` is unique).
#[async_trait]
pub trait ExternalIdentityRepo: Send + Sync {
    async fn by_provider_user(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<ExternalIdentity>>;
    async fn by_user_provider(
        &self,
        user_id: Id,
        provider: &str,
    ) -> Result<Option<ExternalIdentity>>;
    /// Inserts; fails on the unique key.
    async fn create(&self, identity: &NewExternalIdentity) -> Result<ExternalIdentity>;
    /// Persists provider user id, username, avatar and `auth_at`.
    async fn update(&self, identity: &ExternalIdentity) -> Result<()>;
    /// Creates the user and its identity in one transaction (`identity.user_id`
    /// is ignored); nothing is written when either insert fails.
    async fn create_user_with_identity(
        &self,
        user: &NewUser,
        identity: &NewExternalIdentity,
    ) -> Result<(User, ExternalIdentity)>;
}

/// Upstream endpoints (overridable so tests can point them at a mock server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthEndpoints {
    pub telegram_auth: String,
    pub telegram_token: String,
    pub telegram_jwks: String,
    pub google_jwks: String,
}

impl Default for OAuthEndpoints {
    fn default() -> Self {
        Self {
            telegram_auth: "https://oauth.telegram.org/auth".into(),
            telegram_token: "https://oauth.telegram.org/token".into(),
            telegram_jwks: "https://oauth.telegram.org/.well-known/jwks.json".into(),
            google_jwks: "https://www.googleapis.com/oauth2/v3/certs".into(),
        }
    }
}

/// Key-selection and caching rules of a JWKS endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JwksProfile {
    /// `kid` required; cache per `Cache-Control: max-age` (default 1 h); an unknown
    /// kid forces at most one refresh per 5 s.
    Google,
    /// Missing `kid` means `_default`; cache 10 min; a single-key set matches any kid.
    Telegram,
}

/// Why an RS256 token could not be verified.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JwtFailure {
    #[error("malformed token: {0}")]
    Malformed(String),
    /// The JWKS could not be fetched or contained no usable key.
    #[error("jwks unavailable: {0}")]
    KeyUnavailable(String),
    #[error("signing key not found")]
    UnknownKey,
    #[error("signature invalid")]
    BadSignature,
}

/// Verifies RS256 JWT signatures against a JWKS URL and returns the claims.
/// Claim validation is done by the caller ([`check_times`] and friends).
#[async_trait]
pub trait JwtVerifier: Send + Sync {
    async fn verify_rs256(
        &self,
        token: &str,
        jwks_url: &str,
        profile: JwksProfile,
    ) -> std::result::Result<Value, JwtFailure>;
}

/// OAuth 2.0 authorization-code exchange (client secret via HTTP Basic).
#[async_trait]
pub trait OidcTokenClient: Send + Sync {
    /// Posts `form` to `token_url` and returns the `id_token` of a 2xx response;
    /// any other outcome is `error.telegram_oidc_token_exchange_failed`.
    async fn exchange(
        &self,
        token_url: &str,
        form: &[(String, String)],
        client_id: &str,
        client_secret: &str,
    ) -> Result<String>;
}

/// Failure of the registered time claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeFailure {
    Expired,
    Invalid,
}

/// Registered time-claim rules (golang-jwt v5 semantics).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRules {
    pub leeway_seconds: i64,
    pub require_exp: bool,
    pub check_iat: bool,
}

/// A NumericDate claim in milliseconds (fractions truncated to seconds like jwt v5).
fn numeric_date_ms(claims: &Value, name: &str) -> std::result::Result<Option<i64>, TimeFailure> {
    match claims.get(name) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::Number(n)) => {
            let secs = n.as_f64().ok_or(TimeFailure::Invalid)?.trunc();
            if !secs.is_finite() {
                return Err(TimeFailure::Invalid);
            }
            // Seconds of realistic dates fit in i64 milliseconds.
            #[expect(
                clippy::cast_possible_truncation,
                reason = "finite value truncated to whole seconds"
            )]
            let secs = secs as i64;
            Ok(Some(secs.saturating_mul(1000)))
        }
        Some(_) => Err(TimeFailure::Invalid),
    }
}

/// Checks `exp`, `nbf` and (optionally) `iat`. An expired token reports
/// [`TimeFailure::Expired`] even when other claims are also wrong (jwt v5 joins
/// the errors and the original tests `errors.Is(err, ErrTokenExpired)`).
pub fn check_times(
    claims: &Value,
    now: DateTime<Utc>,
    rules: TimeRules,
) -> std::result::Result<(), TimeFailure> {
    let now_ms = now.timestamp_millis();
    let leeway = rules.leeway_seconds.saturating_mul(1000);
    let exp = numeric_date_ms(claims, "exp")?;
    let iat = numeric_date_ms(claims, "iat")?;
    let nbf = numeric_date_ms(claims, "nbf")?;
    match exp {
        Some(exp) if now_ms - leeway >= exp => return Err(TimeFailure::Expired),
        None if rules.require_exp => return Err(TimeFailure::Invalid),
        _ => {}
    }
    if rules.check_iat && iat.is_some_and(|iat| now_ms + leeway < iat) {
        return Err(TimeFailure::Invalid);
    }
    if nbf.is_some_and(|nbf| now_ms + leeway < nbf) {
        return Err(TimeFailure::Invalid);
    }
    Ok(())
}

/// The `aud` claim as a list (a single string or an array of strings).
pub fn audiences(claims: &Value) -> Vec<String> {
    match claims.get("aud") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect(),
        _ => Vec::new(),
    }
}

/// A string claim (empty when absent or not a string).
pub fn str_claim<'a>(claims: &'a Value, name: &str) -> &'a str {
    claims.get(name).and_then(Value::as_str).unwrap_or("")
}

/// A NumericDate claim as a time.
pub fn time_claim(claims: &Value, name: &str) -> Option<DateTime<Utc>> {
    numeric_date_ms(claims, name)
        .ok()
        .flatten()
        .and_then(DateTime::from_timestamp_millis)
}

/// Longest accepted avatar/picture URL.
const MAX_PICTURE_URL_BYTES: usize = 2048;

/// Keeps an absolute http(s) URL with a host and no credentials; anything else
/// becomes empty (original `normalizeGooglePictureURL`).
pub fn normalize_picture_url(raw: &str) -> String {
    let url = raw.trim();
    if url.is_empty() || url.len() > MAX_PICTURE_URL_BYTES {
        return String::new();
    }
    let Some((scheme, rest)) = url.split_once("://") else {
        return String::new();
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return String::new();
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || authority.chars().any(char::is_whitespace)
    {
        return String::new();
    }
    url.to_owned()
}

/// Constant-time byte comparison.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Raw SHA-256 digest.
pub fn sha256(data: &[u8]) -> Vec<u8> {
    decode_hex(&zs_shared::crypto::sha256_hex(data)).unwrap_or_default()
}

/// Decodes lowercase/uppercase hex.
pub fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if !s.len().is_multiple_of(2) {
        return None;
    }
    let digit = |c: u8| (c as char).to_digit(16);
    s.as_bytes()
        .chunks(2)
        .map(|pair| {
            let hi = digit(pair[0])?;
            let lo = digit(pair[1])?;
            u8::try_from(hi * 16 + lo).ok()
        })
        .collect()
}

/// Go `url.ParseQuery` (also used for `application/x-www-form-urlencoded` bodies): `&`-separated pairs, `+` and `%XX` unescaped; any
/// malformed pair (bad escape, `;` separator) makes the whole query invalid.
pub fn parse_query(query: &str) -> Option<BTreeMap<Vec<u8>, Vec<Vec<u8>>>> {
    let mut out: BTreeMap<Vec<u8>, Vec<Vec<u8>>> = BTreeMap::new();
    for part in query.split('&') {
        if part.contains(';') {
            return None;
        }
        if part.is_empty() {
            continue;
        }
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        let key = unescape(k)?;
        let value = unescape(v)?;
        out.entry(key).or_default().push(value);
    }
    Some(out)
}

fn unescape(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let pair = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
                out.push(u8::from_str_radix(pair, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Some(out)
}

/// Storefront tenant bound into a Google redirect (original `GoogleRedirectTenant`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RedirectTenant {
    pub host: String,
    pub is_main: bool,
    pub reseller_id: Option<Id>,
}

impl RedirectTenant {
    /// Lower-cased host without a trailing dot; `None` for an inconsistent tenant
    /// (main with a reseller id, reseller without one, or no host).
    pub fn normalized(&self) -> Option<Self> {
        let host = self.host.trim().trim_end_matches('.').to_lowercase();
        if host.is_empty() {
            return None;
        }
        let consistent = if self.is_main {
            self.reseller_id.is_none()
        } else {
            self.reseller_id.is_some_and(|id| id > 0)
        };
        consistent.then_some(Self {
            host,
            is_main: self.is_main,
            reseller_id: self.reseller_id,
        })
    }

    /// Both tenants are valid and identical.
    pub fn same(&self, other: &Self) -> bool {
        matches!((self.normalized(), other.normalized()), (Some(a), Some(b)) if a == b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn at(secs: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(secs, 0).unwrap()
    }

    const GOOGLE: TimeRules = TimeRules {
        leeway_seconds: 60,
        require_exp: true,
        check_iat: true,
    };

    #[test]
    fn time_rules_follow_jwt_v5() {
        let c = json!({"exp": 1000, "iat": 900});
        assert_eq!(check_times(&c, at(999), GOOGLE), Ok(()));
        // Leeway of 60 s: still valid 59 s after exp, expired at 60 s.
        assert_eq!(check_times(&c, at(1059), GOOGLE), Ok(()));
        assert_eq!(check_times(&c, at(1060), GOOGLE), Err(TimeFailure::Expired));
        assert_eq!(
            check_times(&json!({"iat": 1}), at(10), GOOGLE),
            Err(TimeFailure::Invalid)
        );
        // iat in the future beyond the leeway.
        assert_eq!(
            check_times(&json!({"exp": 5000, "iat": 1100}), at(1000), GOOGLE),
            Err(TimeFailure::Invalid)
        );
        // Expired wins over another failing claim.
        assert_eq!(
            check_times(&json!({"exp": 10, "nbf": 9999}), at(500), GOOGLE),
            Err(TimeFailure::Expired)
        );
        assert_eq!(
            check_times(&json!({"exp": "soon"}), at(1), GOOGLE),
            Err(TimeFailure::Invalid)
        );
        let telegram = TimeRules {
            leeway_seconds: 0,
            require_exp: true,
            check_iat: false,
        };
        assert_eq!(
            check_times(&json!({"exp": 1000, "iat": 5000}), at(999), telegram),
            Ok(())
        );
        assert_eq!(
            check_times(&json!({"exp": 1000}), at(1000), telegram),
            Err(TimeFailure::Expired)
        );
    }

    #[test]
    fn audiences_accept_string_or_array() {
        assert_eq!(audiences(&json!({"aud": "a"})), vec!["a"]);
        assert_eq!(audiences(&json!({"aud": ["a", "b"]})), vec!["a", "b"]);
        assert!(audiences(&json!({})).is_empty());
    }

    #[test]
    fn picture_urls() {
        assert_eq!(
            normalize_picture_url(" https://lh3.googleusercontent.com/a/x "),
            "https://lh3.googleusercontent.com/a/x"
        );
        assert_eq!(normalize_picture_url("javascript:alert(1)"), "");
        assert_eq!(normalize_picture_url("https://user:pw@evil.example/x"), "");
        assert_eq!(normalize_picture_url("https:///x"), "");
        assert_eq!(
            normalize_picture_url(&format!("https://a/{}", "x".repeat(2100))),
            ""
        );
    }

    #[test]
    fn tenants() {
        let main = RedirectTenant {
            host: "Shop.Example.".into(),
            is_main: true,
            reseller_id: None,
        };
        assert_eq!(main.normalized().unwrap().host, "shop.example");
        assert!(main.same(&RedirectTenant {
            host: "shop.example".into(),
            ..main.clone()
        }));
        let reseller = RedirectTenant {
            host: "shop.example".into(),
            is_main: false,
            reseller_id: Some(3),
        };
        assert!(!main.same(&reseller));
        assert!(
            RedirectTenant {
                reseller_id: None,
                ..reseller
            }
            .normalized()
            .is_none()
        );
        assert!(
            RedirectTenant {
                reseller_id: Some(1),
                ..main
            }
            .normalized()
            .is_none()
        );
    }

    #[test]
    fn hex_and_sha() {
        assert_eq!(decode_hex("0aFF"), Some(vec![10, 255]));
        assert_eq!(decode_hex("abc"), None);
        assert_eq!(sha256(b"").len(), 32);
        assert!(constant_time_eq(b"ab", b"ab"));
        assert!(!constant_time_eq(b"ab", b"ac"));
    }
}
