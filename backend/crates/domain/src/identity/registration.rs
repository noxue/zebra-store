//! Registration switches (`registration_config`).

use serde_json::Value;

use super::email;
use crate::{Error, Result};

/// Registration is closed.
pub const KEY_REGISTRATION_DISABLED: &str = "error.registration_disabled";
/// Email domain not on the allowlist.
pub const KEY_DOMAIN_NOT_ALLOWED: &str = "error.email_domain_not_allowed";
/// Verification emails are turned off.
pub const KEY_VERIFICATION_DISABLED: &str = "error.email_verification_disabled";
/// Password reset requires email verification.
pub const KEY_PASSWORD_RESET_DISABLED: &str = "error.password_reset_disabled";

/// Longest accepted allowlist domain.
const MAX_DOMAIN_LEN: usize = 253;
/// Largest allowlist.
const MAX_DOMAINS: usize = 100;

/// The `registration_config` setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationConfig {
    pub registration_enabled: bool,
    pub email_verification_enabled: bool,
    pub email_domain_allowlist_enabled: bool,
    pub allowed_email_domains: Vec<String>,
}

impl Default for RegistrationConfig {
    fn default() -> Self {
        Self {
            registration_enabled: true,
            email_verification_enabled: true,
            email_domain_allowlist_enabled: false,
            allowed_email_domains: Vec::new(),
        }
    }
}

/// Lenient boolean parsing of stored settings (original `parseSettingBool`).
pub fn parse_bool(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => matches!(
            s.trim().to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        _ => false,
    }
}

impl RegistrationConfig {
    /// Reads a stored value; missing fields keep their defaults.
    pub fn from_value(value: Option<&Value>) -> Self {
        let mut cfg = Self::default();
        let Some(v) = value.filter(|v| v.is_object()) else {
            return cfg;
        };
        if let Some(b) = v.get("registration_enabled") {
            cfg.registration_enabled = parse_bool(b);
        }
        if let Some(b) = v.get("email_verification_enabled") {
            cfg.email_verification_enabled = parse_bool(b);
        }
        if let Some(b) = v.get("email_domain_allowlist_enabled") {
            cfg.email_domain_allowlist_enabled = parse_bool(b);
        }
        cfg.allowed_email_domains = normalize_domains(v.get("allowed_email_domains"));
        cfg
    }

    /// Checks `email` against the allowlist (exact domain match, subdomains excluded).
    pub fn check_email_domain(&self, email_addr: &str) -> Result<()> {
        let normalized = email::normalize(email_addr)?;
        if !self.email_domain_allowlist_enabled {
            return Ok(());
        }
        let domain = email::domain_of(&normalized)
            .ok_or_else(|| Error::bad_request(email::KEY_EMAIL_INVALID))?;
        if self.allowed_email_domains.iter().any(|d| d == domain) {
            Ok(())
        } else {
            Err(Error::bad_request(KEY_DOMAIN_NOT_ALLOWED))
        }
    }
}

fn valid_domain(domain: &str) -> bool {
    domain.contains('.')
        && !domain.contains("..")
        && domain.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && !label.starts_with('-')
                && !label.ends_with('-')
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        })
}

/// Normalises the allowlist: split on commas/whitespace, lower-case, strip `@`,
/// validate, de-duplicate, cap the count.
pub fn normalize_domains(raw: Option<&Value>) -> Vec<String> {
    let candidates: Vec<String> = match raw {
        Some(Value::String(s)) => s
            .split(|c: char| c == ',' || c.is_whitespace())
            .map(str::to_owned)
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .map(|i| match i {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            })
            .collect(),
        _ => Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    for c in candidates {
        let domain = c.trim().to_lowercase();
        let domain = domain.trim_start_matches('@').to_owned();
        if domain.is_empty() || domain.len() > MAX_DOMAIN_LEN || !valid_domain(&domain) {
            continue;
        }
        if out.contains(&domain) {
            continue;
        }
        out.push(domain);
        if out.len() >= MAX_DOMAINS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn defaults_when_missing() {
        let c = RegistrationConfig::from_value(None);
        assert!(c.registration_enabled && c.email_verification_enabled);
        let c = RegistrationConfig::from_value(Some(&json!({"registration_enabled": "false"})));
        assert!(!c.registration_enabled);
        assert!(c.email_verification_enabled);
    }

    #[test]
    fn allowlist_is_exact() {
        // AUTH-07
        let c = RegistrationConfig::from_value(Some(&json!({
            "email_domain_allowlist_enabled": true,
            "allowed_email_domains": "@Gmail.com, gmail.com bad..x"
        })));
        assert_eq!(c.allowed_email_domains, vec!["gmail.com"]);
        assert!(c.check_email_domain("a@gmail.com").is_ok());
        assert!(c.check_email_domain("a@GMAIL.COM").is_ok());
        assert_eq!(
            c.check_email_domain("a@mail.gmail.com").unwrap_err().key(),
            KEY_DOMAIN_NOT_ALLOWED
        );
        assert_eq!(
            c.check_email_domain("a@qq.com").unwrap_err().key(),
            KEY_DOMAIN_NOT_ALLOWED
        );
    }
}
