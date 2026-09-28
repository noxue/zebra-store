//! Guest order credentials (ORD-01, ORD-02, ORD-07).
//!
//! The order password is stored as `hmac-sha256:<hex>` over
//! `lower(trim(email)) || 0x00 || password` with the application secret, and only ever
//! received through `Authorization: Guest <base64url(email "\n" password)>`.

use super::model::keys;
use crate::settings::schema::value::is_email_address;
use crate::{Error, Result};

/// Minimum password length in characters (`guestPasswordMinLength`).
pub const GUEST_PASSWORD_MIN_CHARS: usize = 6;
/// Prefix of stored credential digests.
pub const CREDENTIAL_HASH_PREFIX: &str = "hmac-sha256:";

/// Trimmed, lower-cased email (ORD-07): the single normalization used everywhere.
pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// Normalizes and validates a guest email for a new order (`normalizeGuestEmail`).
pub fn validate_guest_email(raw: &str) -> Result<String> {
    let email = normalize_email(raw);
    if email.is_empty() {
        return Err(Error::bad_request(keys::GUEST_EMAIL_REQUIRED));
    }
    if !is_email_address(&email) {
        return Err(Error::bad_request(keys::EMAIL_INVALID));
    }
    Ok(email)
}

/// Trimmed password of at least six characters (ORD-01, counted in Unicode scalars).
pub fn validate_guest_password(raw: &str) -> Result<String> {
    let password = raw.trim();
    if password.is_empty() {
        return Err(Error::bad_request(keys::GUEST_PASSWORD_REQUIRED));
    }
    if password.chars().count() < GUEST_PASSWORD_MIN_CHARS {
        return Err(Error::bad_request(keys::GUEST_PASSWORD_TOO_SHORT));
    }
    Ok(password.to_owned())
}

fn to_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

/// The stored credential digest (`hashGuestCredential`).
pub fn hash_credential(secret: &str, email: &str, password: &str) -> String {
    let mut data = normalize_email(email).into_bytes();
    data.push(0);
    data.extend_from_slice(password.as_bytes());
    let mac = zs_shared::sign::hmac_sha256(secret.trim().as_bytes(), &data);
    format!("{CREDENTIAL_HASH_PREFIX}{}", to_hex(&mac))
}

/// Largest accepted `Authorization` header (`maxGuestAuthorizationSize`).
pub const MAX_AUTHORIZATION_BYTES: usize = 4096;
/// Largest accepted email / password in the header.
pub const MAX_EMAIL_BYTES: usize = 320;
pub const MAX_PASSWORD_BYTES: usize = 256;

/// Splits the decoded `Guest` credential payload (`email "\n" password`); both parts are
/// trimmed and bounded, the email normalized (`GetGuestCredentials`).
pub fn split_credentials(decoded: &str) -> Option<(String, String)> {
    let (email, password) = decoded.split_once('\n')?;
    let email = email.trim();
    let password = password.trim();
    if email.is_empty()
        || password.is_empty()
        || email.len() > MAX_EMAIL_BYTES
        || password.len() > MAX_PASSWORD_BYTES
    {
        return None;
    }
    Some((normalize_email(email), password.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ORD-01: password strength counts characters.
    #[test]
    fn ord_01_password_length() {
        assert_eq!(
            validate_guest_password("12345")
                .err()
                .map(|e| e.key().to_owned()),
            Some("error.guest_password_too_short".into())
        );
        assert!(validate_guest_password("一二三四五六").is_ok());
        assert_eq!(
            validate_guest_password("   ")
                .err()
                .map(|e| e.key().to_owned()),
            Some("error.guest_password_required".into())
        );
        assert_eq!(
            validate_guest_password(" abcdef ").ok().as_deref(),
            Some("abcdef")
        );
    }

    /// ORD-02 / ORD-07: keyed digest independent of email case.
    #[test]
    fn ord_02_credential_digest() {
        let h = hash_credential("secret", " A@B.com ", "abcdef");
        assert!(h.starts_with("hmac-sha256:"));
        assert_eq!(h.len(), CREDENTIAL_HASH_PREFIX.len() + 64);
        assert_eq!(h, hash_credential("secret", "a@b.com", "abcdef"));
        assert_ne!(h, hash_credential("other", "a@b.com", "abcdef"));
        assert_ne!(h, hash_credential("secret", "a@b.com", "abcdeg"));
    }

    #[test]
    fn ord_07_email_normalization() {
        assert_eq!(
            validate_guest_email(" A@B.COM ").ok().as_deref(),
            Some("a@b.com")
        );
        assert_eq!(
            validate_guest_email("").err().map(|e| e.key().to_owned()),
            Some("error.guest_email_required".into())
        );
        assert_eq!(
            validate_guest_email("nope")
                .err()
                .map(|e| e.key().to_owned()),
            Some("error.email_invalid".into())
        );
        assert_eq!(
            split_credentials(" A@B.COM \n abcdef "),
            Some(("a@b.com".into(), "abcdef".into()))
        );
        assert_eq!(split_credentials("a@b.com"), None);
        assert_eq!(split_credentials("a@b.com\n "), None);
    }
}
