//! Telegram login verification rules (port of `telegramauth/application`):
//! Login Widget and Mini App HMAC checks, auth-date freshness and the OIDC
//! `id_token` claims.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use serde_json::Value;
use zs_shared::sign::hmac_sha256;

use super::oauth::{self, TimeRules, check_times, keys};
use crate::{Error, Result};

/// Issuer of Telegram OIDC `id_token`s.
pub const OIDC_ISSUER: &str = "https://oauth.telegram.org";
/// Clock skew tolerated for a future `auth_date` (original: one minute).
const FUTURE_SKEW_SECONDS: i64 = 60;

/// A verified Telegram identity (original `IdentityVerified`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramIdentity {
    /// Numeric Telegram user id (canonical `provider_user_id`).
    pub provider_user_id: String,
    /// Historic ids of the same account (the OIDC `sub` when it differs, AUTH-02).
    pub aliases: Vec<String>,
    pub username: String,
    pub avatar_url: String,
    pub first_name: String,
    pub last_name: String,
    pub auth_at: DateTime<Utc>,
}

impl TelegramIdentity {
    /// True when `provider_user_id` is this identity's id or one of its aliases.
    pub fn matches(&self, provider_user_id: &str) -> bool {
        let id = provider_user_id.trim();
        !id.is_empty()
            && (id == self.provider_user_id || self.aliases.iter().any(|a| a.trim() == id))
    }
}

fn payload_invalid() -> Error {
    Error::bad_request(keys::TELEGRAM_PAYLOAD_INVALID)
}

/// Login Widget payload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WidgetPayload {
    pub id: i64,
    pub first_name: String,
    pub last_name: String,
    pub username: String,
    pub photo_url: String,
    pub auth_date: i64,
    pub hash: String,
}

impl WidgetPayload {
    /// Trims the fields and lower-cases the hash; id, auth_date and hash are required.
    pub fn normalized(&self) -> Result<Self> {
        let n = Self {
            id: self.id,
            first_name: self.first_name.trim().to_owned(),
            last_name: self.last_name.trim().to_owned(),
            username: self.username.trim().to_owned(),
            photo_url: self.photo_url.trim().to_owned(),
            auth_date: self.auth_date,
            hash: self.hash.trim().to_lowercase(),
        };
        if n.id <= 0 || n.auth_date <= 0 || n.hash.is_empty() {
            return Err(payload_invalid());
        }
        Ok(n)
    }

    /// `key=value` lines sorted by key; empty optional fields are left out.
    pub fn data_check_string(&self) -> String {
        let mut fields = BTreeMap::new();
        fields.insert("auth_date", self.auth_date.to_string());
        fields.insert("id", self.id.to_string());
        for (k, v) in [
            ("first_name", &self.first_name),
            ("last_name", &self.last_name),
            ("username", &self.username),
            ("photo_url", &self.photo_url),
        ] {
            if !v.is_empty() {
                fields.insert(k, v.clone());
            }
        }
        fields
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Widget hash: `hex(HMAC_SHA256(key = SHA256(bot_token), data_check_string))`.
pub fn widget_hash(bot_token: &str, data_check_string: &str) -> String {
    let secret = oauth::sha256(bot_token.trim().as_bytes());
    hex(&hmac_sha256(&secret, data_check_string.as_bytes()))
}

/// Mini App hash: secret `HMAC_SHA256(key = "WebAppData", bot_token)`, then
/// `hex(HMAC_SHA256(secret, data_check_string))`.
pub fn miniapp_hash(bot_token: &str, data_check_string: &str) -> String {
    let secret = hmac_sha256(b"WebAppData", bot_token.trim().as_bytes());
    hex(&hmac_sha256(&secret, data_check_string.as_bytes()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Rejects an `auth_date` more than a minute ahead (payload invalid) or older
/// than `expire_seconds` (expired).
pub fn check_auth_time(
    now: DateTime<Utc>,
    auth_at: DateTime<Utc>,
    expire_seconds: i64,
) -> Result<()> {
    if auth_at > now + Duration::seconds(FUTURE_SKEW_SECONDS) {
        return Err(payload_invalid());
    }
    if now - auth_at > Duration::seconds(expire_seconds) {
        return Err(Error::bad_request(keys::TELEGRAM_EXPIRED));
    }
    Ok(())
}

/// `auth_date` as a time (`payload_invalid` when out of range).
pub fn auth_time(auth_date: i64) -> Result<DateTime<Utc>> {
    DateTime::from_timestamp(auth_date, 0).ok_or_else(payload_invalid)
}

/// Compares the expected hash with the submitted one in constant time.
pub fn check_hash(expected: &str, submitted: &str) -> Result<()> {
    if oauth::constant_time_eq(expected.as_bytes(), submitted.as_bytes()) {
        Ok(())
    } else {
        Err(Error::bad_request(keys::TELEGRAM_SIGNATURE_INVALID))
    }
}

/// Mini App `user` object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct MiniAppUser {
    pub id: i64,
    #[serde(default, deserialize_with = "null_string")]
    pub first_name: String,
    #[serde(default, deserialize_with = "null_string")]
    pub last_name: String,
    #[serde(default, deserialize_with = "null_string")]
    pub username: String,
    #[serde(default, deserialize_with = "null_string")]
    pub photo_url: String,
}

fn null_string<'de, D: serde::Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    Ok(Option::<String>::deserialize(d)?.unwrap_or_default())
}

/// Parsed Mini App `initData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MiniAppInitData {
    pub auth_date: i64,
    pub hash: String,
    pub user: MiniAppUser,
    /// Every field except `hash` (the newer `signature` included, AUTH-04).
    pub data_check_string: Vec<u8>,
}

/// Parses `initData` like the original (`url.ParseQuery`, then `hash`,
/// `auth_date` and `user` are required and `user.id` must be positive).
pub fn parse_miniapp_init_data(raw: &str) -> Result<MiniAppInitData> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(payload_invalid());
    }
    let values = oauth::parse_query(trimmed).ok_or_else(payload_invalid)?;
    let first = |k: &str| {
        values
            .get(k.as_bytes())
            .and_then(|v| v.first())
            .map(|v| String::from_utf8_lossy(v).trim().to_owned())
            .unwrap_or_default()
    };
    let hash = first("hash").to_lowercase();
    let auth_date_text = first("auth_date");
    let user_raw = first("user");
    if hash.is_empty() || auth_date_text.is_empty() || user_raw.is_empty() {
        return Err(payload_invalid());
    }
    let auth_date: i64 = auth_date_text.parse().map_err(|_| payload_invalid())?;
    if auth_date <= 0 {
        return Err(payload_invalid());
    }
    let mut user: MiniAppUser = serde_json::from_str(&user_raw).map_err(|_| payload_invalid())?;
    for field in [
        &mut user.first_name,
        &mut user.last_name,
        &mut user.username,
        &mut user.photo_url,
    ] {
        *field = field.trim().to_owned();
    }
    if user.id <= 0 {
        return Err(payload_invalid());
    }
    let mut dcs = Vec::new();
    for (key, vals) in &values {
        if key.as_slice() == b"hash" {
            continue;
        }
        if !dcs.is_empty() {
            dcs.push(b'\n');
        }
        dcs.extend_from_slice(key);
        dcs.push(b'=');
        if let Some(v) = vals.first() {
            dcs.extend_from_slice(v);
        }
    }
    Ok(MiniAppInitData {
        auth_date,
        hash,
        user,
        data_check_string: dcs,
    })
}

/// Mini App hash over raw data-check bytes.
pub fn miniapp_hash_bytes(bot_token: &str, data_check_string: &[u8]) -> String {
    let secret = hmac_sha256(b"WebAppData", bot_token.trim().as_bytes());
    hex(&hmac_sha256(&secret, data_check_string))
}

/// Splits an OIDC `name` at the first space into first/last name.
pub fn split_name(name: &str) -> (String, String) {
    let name = name.trim();
    match name.split_once(' ') {
        Some((first, last)) if !first.is_empty() => {
            (first.trim().to_owned(), last.trim().to_owned())
        }
        _ => (name.to_owned(), String::new()),
    }
}

/// Claims of a verified Telegram OIDC `id_token`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcClaims {
    /// The numeric Telegram id (`id` claim).
    pub id: i64,
    pub identity: TelegramIdentity,
}

fn id_token_invalid() -> Error {
    Error::bad_request(keys::TELEGRAM_OIDC_ID_TOKEN_INVALID)
}

/// Validates the claims of a signature-checked `id_token`: `iss`, `aud`
/// (contains `client_id`), required unexpired `exp`, and a positive numeric
/// `id` which becomes the provider user id; a different `sub` is kept as an
/// alias (AUTH-02).
pub fn validate_oidc_claims(
    claims: &Value,
    client_id: &str,
    now: DateTime<Utc>,
) -> Result<OidcClaims> {
    let rules = TimeRules {
        leeway_seconds: 0,
        require_exp: true,
        check_iat: false,
    };
    check_times(claims, now, rules).map_err(|_| id_token_invalid())?;
    if oauth::str_claim(claims, "iss") != OIDC_ISSUER {
        return Err(id_token_invalid());
    }
    if !oauth::audiences(claims).iter().any(|a| a == client_id) {
        return Err(id_token_invalid());
    }
    let id = match claims.get("id") {
        Some(Value::Number(n)) => n.as_i64(),
        Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
    .filter(|id| *id > 0)
    .ok_or_else(id_token_invalid)?;
    let provider_user_id = id.to_string();
    let sub = oauth::str_claim(claims, "sub").trim().to_owned();
    let aliases = if sub.is_empty() || sub == provider_user_id {
        Vec::new()
    } else {
        vec![sub]
    };
    let (first_name, last_name) = split_name(oauth::str_claim(claims, "name"));
    Ok(OidcClaims {
        id,
        identity: TelegramIdentity {
            provider_user_id,
            aliases,
            username: oauth::str_claim(claims, "preferred_username")
                .trim()
                .to_owned(),
            avatar_url: oauth::str_claim(claims, "picture").trim().to_owned(),
            first_name,
            last_name,
            auth_at: oauth::time_claim(claims, "iat").unwrap_or(now),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Reference values produced by the original Go implementation
    // (`buildTelegramHash`, `buildTelegramMiniAppHash`, `s256Challenge`).
    const WIDGET_TOKEN: &str = "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11";
    const MINI_TOKEN: &str = "5768337691:AAH5YkoiEuPk8-FZa32hStHTqXiLPtAEhx8";

    fn widget() -> WidgetPayload {
        WidgetPayload {
            id: 123_456_789,
            first_name: "Zebra".into(),
            last_name: "Store".into(),
            username: "zebra_shop".into(),
            photo_url: "https://t.me/i/userpic/320/a.jpg".into(),
            auth_date: 1_780_000_000,
            hash: "x".into(),
        }
    }

    #[test]
    fn widget_hash_matches_go_vector() {
        let dcs = widget().data_check_string();
        assert_eq!(
            dcs,
            "auth_date=1780000000\nfirst_name=Zebra\nid=123456789\nlast_name=Store\nphoto_url=https://t.me/i/userpic/320/a.jpg\nusername=zebra_shop"
        );
        assert_eq!(
            widget_hash(WIDGET_TOKEN, &dcs),
            "4771a90ae42cd784e7d755544583a1902863577d9d96d999f85fd34fe56ca5b5"
        );
        let minimal = WidgetPayload {
            id: 42,
            auth_date: 1_780_000_000,
            hash: "x".into(),
            ..WidgetPayload::default()
        };
        assert_eq!(
            widget_hash(WIDGET_TOKEN, &minimal.data_check_string()),
            "8d4a04dd66c0efe4547f9de7fbca6fc4468a71cb4c300a09a11882143e5c1a97"
        );
    }

    #[test]
    fn widget_payload_requires_id_date_hash() {
        assert!(widget().normalized().is_ok());
        for bad in [
            WidgetPayload { id: 0, ..widget() },
            WidgetPayload {
                auth_date: 0,
                ..widget()
            },
            WidgetPayload {
                hash: "  ".into(),
                ..widget()
            },
        ] {
            assert_eq!(
                bad.normalized().unwrap_err().key(),
                keys::TELEGRAM_PAYLOAD_INVALID
            );
        }
        let n = WidgetPayload {
            hash: " ABCD ".into(),
            username: " u ".into(),
            ..widget()
        }
        .normalized()
        .unwrap();
        assert_eq!((n.hash.as_str(), n.username.as_str()), ("abcd", "u"));
    }

    const MINI_RAW: &str = "query_id=AAHdF6IQAAAAAN0XohDhrOrc&user=%7B%22id%22%3A279058397%2C%22first_name%22%3A%22Vladislav%22%2C%22last_name%22%3A%22Kibenko%22%2C%22username%22%3A%22vdkfrost%22%2C%22language_code%22%3A%22ru%22%2C%22is_premium%22%3Atrue%7D&auth_date=1662771648&signature=abc+def&hash=ignored";

    // AUTH-04: the `signature` field stays in the data-check string.
    #[test]
    fn miniapp_hash_matches_go_vector_including_signature() {
        let parsed = parse_miniapp_init_data(MINI_RAW).unwrap();
        assert_eq!(
            String::from_utf8(parsed.data_check_string.clone()).unwrap(),
            "auth_date=1662771648\nquery_id=AAHdF6IQAAAAAN0XohDhrOrc\nsignature=abc def\nuser={\"id\":279058397,\"first_name\":\"Vladislav\",\"last_name\":\"Kibenko\",\"username\":\"vdkfrost\",\"language_code\":\"ru\",\"is_premium\":true}"
        );
        assert_eq!(
            miniapp_hash_bytes(MINI_TOKEN, &parsed.data_check_string),
            "fd8c5257000cb354f82e0bfa9a23dfe7790dece7810936d22a03a41326e7d6b2"
        );
        assert_eq!(
            miniapp_hash(
                MINI_TOKEN,
                std::str::from_utf8(&parsed.data_check_string).unwrap()
            ),
            "fd8c5257000cb354f82e0bfa9a23dfe7790dece7810936d22a03a41326e7d6b2"
        );
        assert_eq!(parsed.user.id, 279_058_397);
        assert_eq!(parsed.user.username, "vdkfrost");
        assert_eq!(parsed.auth_date, 1_662_771_648);
        assert_eq!(parsed.hash, "ignored");
    }

    #[test]
    fn miniapp_rejects_malformed_input() {
        for raw in [
            "",
            "auth_date=1&hash=a",
            "user=%7B%22id%22%3A1%7D&hash=a",
            "user=%7B%22id%22%3A0%7D&auth_date=1&hash=a",
            "user=%7B%22id%22%3A1%7D&auth_date=x&hash=a",
            "user=%7B%22id%22%3A1%7D&auth_date=1&hash=a&bad=%zz",
            "user=%7B%22id%22%3A1%7D&auth_date=1;hash=a",
        ] {
            assert_eq!(
                parse_miniapp_init_data(raw).unwrap_err().key(),
                keys::TELEGRAM_PAYLOAD_INVALID,
                "{raw}"
            );
        }
    }

    // AUTH-04: freshness window and future skew.
    #[test]
    fn auth_time_window() {
        let now = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let ok = |secs| check_auth_time(now, now - Duration::seconds(secs), 3600);
        assert!(ok(3600).is_ok());
        assert_eq!(ok(7200).unwrap_err().key(), keys::TELEGRAM_EXPIRED);
        assert_eq!(
            check_auth_time(now, now + Duration::seconds(120), 3600)
                .unwrap_err()
                .key(),
            keys::TELEGRAM_PAYLOAD_INVALID
        );
        assert!(check_auth_time(now, now + Duration::seconds(30), 3600).is_ok());
    }

    #[test]
    fn hash_compare() {
        assert!(check_hash("ab", "ab").is_ok());
        assert_eq!(
            check_hash("ab", "ac").unwrap_err().key(),
            keys::TELEGRAM_SIGNATURE_INVALID
        );
    }

    // AUTH-02: the numeric `id` is canonical and `sub` becomes an alias.
    #[test]
    fn oidc_claims() {
        let now = DateTime::from_timestamp(1_000, 0).unwrap();
        let claims = json!({
            "iss": OIDC_ISSUER, "aud": "123456", "exp": 2000, "iat": 900,
            "sub": "opaque-sub", "id": 777, "name": "Zebra Store Fan",
            "preferred_username": "zfan", "picture": "https://t.me/p.jpg"
        });
        let c = validate_oidc_claims(&claims, "123456", now).unwrap();
        assert_eq!(c.id, 777);
        assert_eq!(c.identity.provider_user_id, "777");
        assert_eq!(c.identity.aliases, vec!["opaque-sub"]);
        assert_eq!(c.identity.first_name, "Zebra");
        assert_eq!(c.identity.last_name, "Store Fan");
        assert_eq!(c.identity.auth_at.timestamp(), 900);
        assert!(c.identity.matches("opaque-sub"));
        assert!(c.identity.matches("777"));
        assert!(!c.identity.matches("778"));

        let bad = |patch: Value| {
            let mut c = claims.clone();
            for (k, v) in patch.as_object().unwrap() {
                c[k] = v.clone();
            }
            validate_oidc_claims(&c, "123456", now)
                .unwrap_err()
                .key()
                .to_owned()
        };
        for patch in [
            json!({"aud": "999"}),
            json!({"exp": 1000}),
            json!({"exp": null}),
            json!({"iss": "https://evil"}),
            json!({"id": 0}),
            json!({"id": "abc"}),
        ] {
            assert_eq!(bad(patch), keys::TELEGRAM_OIDC_ID_TOKEN_INVALID);
        }
        let same_sub = json!({
            "iss": OIDC_ISSUER, "aud": ["x", "123456"], "exp": 2000, "sub": "777", "id": "777"
        });
        let c = validate_oidc_claims(&same_sub, "123456", now).unwrap();
        assert!(c.identity.aliases.is_empty());
        assert_eq!(c.identity.auth_at, now);
    }

    #[test]
    fn names() {
        assert_eq!(split_name(" A B C "), ("A".into(), "B C".into()));
        assert_eq!(split_name("Solo"), ("Solo".into(), String::new()));
        assert_eq!(split_name(""), (String::new(), String::new()));
    }
}
