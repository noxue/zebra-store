//! TOTP two-factor authentication shared by administrators and users.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Error, Id};

/// Number of digits of a TOTP code (original `totpDigits`).
pub const DIGITS: usize = 6;
/// TOTP period in seconds (original `totpPeriod`).
pub const PERIOD_SECONDS: u64 = 30;
/// Accepted clock drift in periods (original `totpSkew`).
pub const SKEW: u8 = 1;
/// Lifetime of a pending (not yet confirmed) secret (original `totpPendingTTL`).
pub const PENDING_TTL_MINUTES: i64 = 10;
/// Failed enable attempts before the pending secret is discarded (original `totpEnableMaxFailures`).
pub const ENABLE_MAX_FAILURES: u32 = 5;
/// Number of recovery codes generated on enable (original `RecoveryCodeCount`).
pub const RECOVERY_CODE_COUNT: usize = 10;
/// Lifetime of a login challenge token (original `challenge.TTL`).
pub const CHALLENGE_TTL_MINUTES: i64 = 5;
/// Failed verify attempts before a challenge is revoked (original `challenge.MaxFailures`).
pub const CHALLENGE_MAX_FAILURES: u32 = 5;
/// `purpose` / `typ` claim of challenge tokens (original `challenge.PurposeTwoFactor`).
pub const CHALLENGE_PURPOSE: &str = "2fa_challenge";

/// i18n keys used by the 2FA flows.
pub mod keys {
    pub const ALREADY_ENABLED: &str = "error.totp_already_enabled";
    pub const NOT_ENABLED: &str = "error.totp_not_enabled";
    pub const PENDING_EXPIRED: &str = "error.totp_pending_expired";
    pub const CODE_INVALID: &str = "error.totp_code_invalid";
    pub const CODE_REQUIRED: &str = "error.totp_code_required";
    pub const RECOVERY_INVALID: &str = "error.recovery_code_invalid";
    pub const TOO_MANY_ATTEMPTS: &str = "error.totp_too_many_attempts";
    pub const CHALLENGE_INVALID: &str = "error.totp_challenge_invalid";
    pub const CANNOT_RESET_SELF: &str = "error.totp_cannot_reset_self";
}

/// Typed 2FA failure, mapped to the right status code by each endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotpError {
    AlreadyEnabled,
    NotEnabled,
    PendingExpired,
    CodeInvalid,
    RecoveryInvalid,
    TooManyAttempts,
}

impl TotpError {
    pub fn key(self) -> &'static str {
        match self {
            Self::AlreadyEnabled => keys::ALREADY_ENABLED,
            Self::NotEnabled => keys::NOT_ENABLED,
            Self::PendingExpired => keys::PENDING_EXPIRED,
            Self::CodeInvalid => keys::CODE_INVALID,
            Self::RecoveryInvalid => keys::RECOVERY_INVALID,
            Self::TooManyAttempts => keys::TOO_MANY_ATTEMPTS,
        }
    }

    /// Returns the matching variant of a domain error, if any.
    pub fn of(error: &Error) -> Option<Self> {
        [
            Self::AlreadyEnabled,
            Self::NotEnabled,
            Self::PendingExpired,
            Self::CodeInvalid,
            Self::RecoveryInvalid,
            Self::TooManyAttempts,
        ]
        .into_iter()
        .find(|v| v.key() == error.key())
    }
}

impl From<TotpError> for Error {
    fn from(value: TotpError) -> Self {
        Error::bad_request(value.key())
    }
}

/// TOTP columns shared by `admins` and `users`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TotpState {
    /// Encrypted active secret.
    pub secret: String,
    pub enabled_at: Option<DateTime<Utc>>,
    /// Encrypted secret awaiting confirmation.
    pub pending_secret: String,
    pub pending_expires_at: Option<DateTime<Utc>>,
    /// JSON list of [`RecoveryCode`].
    pub recovery_codes: String,
}

impl TotpState {
    pub fn enabled(&self) -> bool {
        self.enabled_at.is_some()
    }
}

/// An account that can carry TOTP 2FA (administrator or user).
pub trait TotpAccount: Clone + Send + Sync + 'static {
    fn id(&self) -> Id;
    /// Account label embedded in the otpauth URL.
    fn totp_label(&self) -> String;
    fn totp_state(&self) -> TotpState;
    fn set_totp_state(&mut self, state: TotpState);
    /// Invalidates every issued access token.
    fn revoke_tokens(&mut self, now: DateTime<Utc>);
}

/// Persisted, hashed recovery code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryCode {
    pub hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_at: Option<DateTime<Utc>>,
}

/// Decodes the persisted recovery code list (empty string → empty list).
pub fn decode_recovery_codes(json: &str) -> Result<Vec<RecoveryCode>, serde_json::Error> {
    if json.is_empty() {
        return Ok(Vec::new());
    }
    serde_json::from_str(json)
}

/// `/2fa/status` response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TotpStatus {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled_at: Option<DateTime<Utc>>,
    pub recovery_codes_remaining: usize,
    pub recovery_codes_total: usize,
}

impl TotpStatus {
    pub fn of(state: &TotpState) -> Self {
        let mut status = Self {
            enabled: state.enabled(),
            enabled_at: state.enabled_at,
            recovery_codes_remaining: 0,
            recovery_codes_total: 0,
        };
        if let Ok(codes) = decode_recovery_codes(&state.recovery_codes) {
            status.recovery_codes_total = codes.len();
            status.recovery_codes_remaining = codes.iter().filter(|c| c.used_at.is_none()).count();
        }
        status
    }
}

/// Normalises a submitted recovery code (`xxxx-xxxx`, case-insensitive).
pub fn normalize_recovery_code(code: &str) -> String {
    code.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_counts_unused_codes() {
        let codes = vec![
            RecoveryCode {
                hash: "a".into(),
                used_at: None,
            },
            RecoveryCode {
                hash: "b".into(),
                used_at: Some(Utc::now()),
            },
        ];
        let state = TotpState {
            enabled_at: Some(Utc::now()),
            recovery_codes: serde_json::to_string(&codes).unwrap(),
            ..TotpState::default()
        };
        let s = TotpStatus::of(&state);
        assert!(s.enabled);
        assert_eq!((s.recovery_codes_remaining, s.recovery_codes_total), (1, 2));
        let off = TotpStatus::of(&TotpState::default());
        assert_eq!(
            serde_json::to_value(off).unwrap(),
            serde_json::json!({"enabled": false, "recovery_codes_remaining": 0, "recovery_codes_total": 0})
        );
    }

    #[test]
    fn maps_errors_back() {
        let e: Error = TotpError::CodeInvalid.into();
        assert_eq!(e.key(), keys::CODE_INVALID);
        assert_eq!(TotpError::of(&e), Some(TotpError::CodeInvalid));
        assert_eq!(TotpError::of(&Error::invalid()), None);
    }
}
