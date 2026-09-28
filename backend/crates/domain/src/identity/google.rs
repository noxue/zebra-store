//! Google Identity Services ID-token rules (port of `googleauth/application`,
//! AUTH-01).

use chrono::{DateTime, Utc};
use serde_json::Value;

use super::email;
use super::oauth::{self, TimeFailure, TimeRules, keys};
use crate::{Error, Result};

/// Longest accepted credential (original `maxGoogleCredentialBytes`, 64 KiB).
pub const MAX_CREDENTIAL_BYTES: usize = 64 << 10;
/// Longest accepted `sub` (original `maxGoogleSubjectBytes`).
const MAX_SUBJECT_BYTES: usize = 255;
/// Longest email stored on an identity (original `maxGoogleIdentityEmailBytes`).
const MAX_IDENTITY_EMAIL_BYTES: usize = 128;
/// Clock skew tolerated on `exp` / `iat` (original `googleTokenClockSkew`).
pub const CLOCK_SKEW_SECONDS: i64 = 60;
/// Longest display name of an account created from Google (runes).
pub const MAX_DISPLAY_NAME_CHARS: usize = 128;
/// Accepted issuers.
const ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];

/// A verified Google identity (original `VerifiedIdentity`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoogleIdentity {
    pub sub: String,
    pub email: String,
    pub name: String,
    pub picture: String,
    pub hd: String,
    /// Audience the credential was verified for (a changed client id
    /// invalidates redirect handoffs).
    pub client_id: String,
    /// `@gmail.com` or a Workspace (`hd`) account: Google is authoritative for
    /// the address, so it may be linked to / create a local account.
    pub email_authoritative: bool,
    pub auth_at: DateTime<Utc>,
}

fn invalid() -> Error {
    Error::bad_request(keys::GOOGLE_CREDENTIAL_INVALID)
}

/// Validates the claims of a signature-checked credential for `client_id`.
pub fn validate_claims(
    claims: &Value,
    client_id: &str,
    now: DateTime<Utc>,
) -> Result<GoogleIdentity> {
    let rules = TimeRules {
        leeway_seconds: CLOCK_SKEW_SECONDS,
        require_exp: true,
        check_iat: true,
    };
    match oauth::check_times(claims, now, rules) {
        Err(TimeFailure::Expired) => {
            return Err(Error::bad_request(keys::GOOGLE_CREDENTIAL_EXPIRED));
        }
        Err(TimeFailure::Invalid) => return Err(invalid()),
        Ok(()) => {}
    }
    let audiences = oauth::audiences(claims);
    // Exactly one audience equal to our client id (no confused-deputy tokens).
    if audiences.len() != 1 || audiences[0] != client_id {
        return Err(invalid());
    }
    if !ISSUERS.contains(&oauth::str_claim(claims, "iss")) {
        return Err(invalid());
    }
    let azp = oauth::str_claim(claims, "azp");
    if !azp.is_empty() && azp != client_id {
        return Err(invalid());
    }
    let sub = oauth::str_claim(claims, "sub").trim();
    if sub.is_empty() || sub.len() > MAX_SUBJECT_BYTES {
        return Err(invalid());
    }
    let issued_at = oauth::time_claim(claims, "iat").ok_or_else(invalid)?;
    if issued_at > now + chrono::Duration::seconds(CLOCK_SKEW_SECONDS) {
        return Err(invalid());
    }
    let raw_email = oauth::str_claim(claims, "email");
    let email_addr = email::normalize(raw_email).map_err(|_| invalid())?;
    if email_addr != raw_email.trim().to_lowercase() {
        return Err(invalid());
    }
    if claims.get("email_verified").and_then(Value::as_bool) != Some(true) {
        return Err(Error::bad_request(keys::GOOGLE_EMAIL_UNVERIFIED));
    }
    let hd = oauth::str_claim(claims, "hd").trim().to_lowercase();
    Ok(GoogleIdentity {
        sub: sub.to_owned(),
        email_authoritative: email_addr.ends_with("@gmail.com") || !hd.is_empty(),
        email: email_addr,
        name: oauth::str_claim(claims, "name").trim().to_owned(),
        picture: oauth::normalize_picture_url(oauth::str_claim(claims, "picture")),
        hd,
        client_id: client_id.to_owned(),
        auth_at: issued_at,
    })
}

/// Re-normalises an identity before it is stored (original
/// `normalizeVerifiedGoogleIdentity`).
pub fn normalize_identity(identity: &GoogleIdentity) -> Result<GoogleIdentity> {
    let sub = identity.sub.trim();
    if sub.is_empty() || sub.len() > MAX_SUBJECT_BYTES {
        return Err(invalid());
    }
    let email_addr = email::normalize(&identity.email).map_err(|_| invalid())?;
    if email_addr.len() > MAX_IDENTITY_EMAIL_BYTES {
        return Err(invalid());
    }
    Ok(GoogleIdentity {
        sub: sub.to_owned(),
        email: email_addr,
        name: identity.name.trim().to_owned(),
        picture: oauth::normalize_picture_url(&identity.picture),
        hd: identity.hd.trim().to_lowercase(),
        ..identity.clone()
    })
}

/// Display name of an account created from Google: the name (at most 128
/// characters) or the local part of the email.
pub fn display_name(identity: &GoogleIdentity) -> String {
    let name: String = identity
        .name
        .trim()
        .chars()
        .take(MAX_DISPLAY_NAME_CHARS)
        .collect();
    if name.is_empty() {
        email::nickname_from(&identity.email)
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(10_000, 0).unwrap()
    }

    fn claims() -> Value {
        json!({
            "iss": "https://accounts.google.com", "aud": "cid", "azp": "cid",
            "sub": "1234567890", "email": "Alice@Gmail.com", "email_verified": true,
            "name": " Alice ", "picture": "https://lh3.example/p", "iat": 9_990, "exp": 13_000
        })
    }

    fn with(patch: Value) -> Value {
        let mut c = claims();
        for (k, v) in patch.as_object().unwrap() {
            if v.is_null() {
                c.as_object_mut().unwrap().remove(k);
            } else {
                c[k] = v.clone();
            }
        }
        c
    }

    fn key(c: &Value) -> String {
        validate_claims(c, "cid", now())
            .unwrap_err()
            .key()
            .to_owned()
    }

    #[test]
    fn accepts_a_gmail_credential() {
        let id = validate_claims(&claims(), "cid", now()).unwrap();
        assert_eq!(id.sub, "1234567890");
        assert_eq!(id.email, "alice@gmail.com");
        assert_eq!(id.name, "Alice");
        assert!(id.email_authoritative);
        assert_eq!(id.client_id, "cid");
        assert_eq!(id.auth_at.timestamp(), 9_990);
    }

    // AUTH-01: strict audience/issuer/email rules.
    #[test]
    fn rejects_bad_claims() {
        assert_eq!(
            key(&with(json!({"aud": "other"}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"aud": ["cid", "other"]}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"azp": "other"}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"iss": "evil"}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"sub": " "}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"iat": null}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"exp": null}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"iat": 10_100}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"email": "not-an-email"}))),
            keys::GOOGLE_CREDENTIAL_INVALID
        );
        assert_eq!(
            key(&with(json!({"email_verified": false}))),
            keys::GOOGLE_EMAIL_UNVERIFIED
        );
        assert_eq!(
            key(&with(json!({"email_verified": "true"}))),
            keys::GOOGLE_EMAIL_UNVERIFIED
        );
        assert_eq!(
            key(&with(json!({"exp": 9_000}))),
            keys::GOOGLE_CREDENTIAL_EXPIRED
        );
    }

    #[test]
    fn authority_of_the_email() {
        let custom =
            validate_claims(&with(json!({"email": "a@corp.example"})), "cid", now()).unwrap();
        assert!(!custom.email_authoritative);
        let workspace = validate_claims(
            &with(json!({"email": "a@corp.example", "hd": "Corp.Example"})),
            "cid",
            now(),
        )
        .unwrap();
        assert!(workspace.email_authoritative);
        assert_eq!(workspace.hd, "corp.example");
    }

    #[test]
    fn display_names() {
        let mut id = validate_claims(&claims(), "cid", now()).unwrap();
        assert_eq!(display_name(&id), "Alice");
        id.name = "x".repeat(200);
        assert_eq!(display_name(&id).chars().count(), MAX_DISPLAY_NAME_CHARS);
        id.name.clear();
        assert_eq!(display_name(&id), "alice");
        id.email = format!("{}@gmail.com", "a".repeat(130));
        assert!(normalize_identity(&id).is_err());
    }
}
