//! Email address normalisation and validation (ported from `normalizeEmail`,
//! `telegramidentity` and the registration domain allowlist).

use crate::{Error, Result};

/// Prefix of the placeholder emails generated for Telegram-only accounts.
const PLACEHOLDER_PREFIX: &str = "telegram_";
/// Domain of the placeholder emails generated for Telegram-only accounts.
const PLACEHOLDER_DOMAIN: &str = "@login.local";
/// Domain reserved for anonymous browser identities used by guest checkout.
const GUEST_BROWSER_DOMAIN: &str = "@uuid.com";

/// Error key for malformed email addresses.
pub const KEY_EMAIL_INVALID: &str = "error.email_invalid";

/// Returns true when `email` is a syntactically valid bare address (`local@domain`).
pub fn is_valid(email: &str) -> bool {
    let Some((local, domain)) = email.rsplit_once('@') else {
        return false;
    };
    const FORBIDDEN: &[char] = &['<', '>', '(', ')', ',', ';', ':', '"', '[', ']', '\\', '@'];
    if local.is_empty()
        || domain.is_empty()
        || local.len() > 64
        || email.len() > 254
        || email.chars().any(|c| c.is_whitespace() || c.is_control())
        || local.contains(FORBIDDEN)
        || local.starts_with('.')
        || local.ends_with('.')
        || local.contains("..")
    {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    })
}

/// Lower-cases and trims `email`, rejecting malformed input with `error.email_invalid`.
pub fn normalize(email: &str) -> Result<String> {
    let normalized = email.trim().to_lowercase();
    if normalized.is_empty() || !is_valid(&normalized) {
        return Err(Error::bad_request(KEY_EMAIL_INVALID));
    }
    Ok(normalized)
}

fn is_uuid_v4(value: &str) -> bool {
    if value.len() != 36 {
        return false;
    }
    value.bytes().enumerate().all(|(index, byte)| match index {
        8 | 13 | 18 | 23 => byte == b'-',
        14 => byte == b'4',
        19 => matches!(byte, b'8' | b'9' | b'a' | b'b'),
        _ => byte.is_ascii_hexdigit(),
    })
}

/// True for system-generated identities that must never receive e-mail.
pub fn is_placeholder(email: &str) -> bool {
    let normalized = email.trim().to_lowercase();
    (!normalized.is_empty()
        && normalized.starts_with(PLACEHOLDER_PREFIX)
        && normalized.ends_with(PLACEHOLDER_DOMAIN))
        || normalized
            .strip_suffix(GUEST_BROWSER_DOMAIN)
            .is_some_and(is_uuid_v4)
}

/// Normalises an address typed by a user (registration, email change): Telegram
/// placeholder addresses are reserved for the system and rejected.
pub fn normalize_user_supplied(email: &str) -> Result<String> {
    let normalized = normalize(email)?;
    if is_placeholder(&normalized) {
        return Err(Error::bad_request(KEY_EMAIL_INVALID));
    }
    Ok(normalized)
}

/// Default nickname: the local part of the address.
pub fn nickname_from(email: &str) -> String {
    match email.split_once('@') {
        Some((local, _)) if !local.trim().is_empty() => local.trim().to_owned(),
        _ => email.to_owned(),
    }
}

/// Domain part of a normalised address.
pub fn domain_of(email: &str) -> Option<&str> {
    email
        .rsplit_once('@')
        .map(|(_, d)| d)
        .filter(|d| !d.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_addresses() {
        assert!(is_valid("a@b"));
        assert!(is_valid("alice.smith+tag@example.co.uk"));
        assert!(!is_valid("alice"));
        assert!(!is_valid("@example.com"));
        assert!(!is_valid("alice@"));
        assert!(!is_valid("a b@example.com"));
        assert!(!is_valid("Alice <a@example.com>"));
        assert!(!is_valid("a@exa..mple.com"));
    }

    #[test]
    fn normalizes_and_rejects_placeholders() {
        assert_eq!(
            normalize(" Alice@Example.COM ").unwrap(),
            "alice@example.com"
        );
        assert_eq!(normalize("bad").unwrap_err().key(), KEY_EMAIL_INVALID);
        // Lesson 393079aa: Telegram placeholder addresses cannot be registered.
        assert!(is_placeholder("Telegram_123@login.local"));
        assert!(is_placeholder(
            "550e8400-e29b-41d4-a716-446655440000@uuid.com"
        ));
        assert!(!is_placeholder(
            "550e8400-e29b-11d4-a716-446655440000@uuid.com"
        ));
        assert_eq!(
            normalize_user_supplied("telegram_123@login.local")
                .unwrap_err()
                .key(),
            KEY_EMAIL_INVALID
        );
        assert_eq!(nickname_from("bob@x.io"), "bob");
        assert_eq!(domain_of("bob@x.io"), Some("x.io"));
    }
}
