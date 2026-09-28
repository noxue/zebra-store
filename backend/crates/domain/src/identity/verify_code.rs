//! Email verification codes (`email_verify_codes`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{Error, Id, Result};

/// i18n keys of the verification code flows.
pub mod keys {
    pub const INVALID: &str = "error.verify_code_invalid";
    pub const EXPIRED: &str = "error.verify_code_expired";
    pub const ATTEMPTS_EXCEEDED: &str = "error.verify_code_attempts_exceeded";
    pub const TOO_FREQUENT: &str = "error.verify_code_too_frequent";
    pub const PURPOSE_INVALID: &str = "error.verify_purpose_invalid";
}

/// What a code is for (original `VerifyPurpose*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    Register,
    Reset,
    TelegramBind,
    ChangeEmailOld,
    ChangeEmailNew,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Register => "register",
            Self::Reset => "reset",
            Self::TelegramBind => "telegram_bind",
            Self::ChangeEmailOld => "change_email_old",
            Self::ChangeEmailNew => "change_email_new",
        }
    }

    /// Parses a purpose case-insensitively.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_lowercase().as_str() {
            "register" => Some(Self::Register),
            "reset" => Some(Self::Reset),
            "telegram_bind" => Some(Self::TelegramBind),
            "change_email_old" => Some(Self::ChangeEmailOld),
            "change_email_new" => Some(Self::ChangeEmailNew),
            _ => None,
        }
    }
}

/// Default code lifetime in minutes (original `resolveExpireMinutes`).
const DEFAULT_EXPIRE_MINUTES: i64 = 10;
/// Default resend interval in seconds.
const DEFAULT_SEND_INTERVAL_SECONDS: i64 = 60;
/// Default attempt limit.
const DEFAULT_MAX_ATTEMPTS: i32 = 5;
/// Default code length; lengths outside 4..=10 fall back to it.
const DEFAULT_LENGTH: usize = 6;

/// Verification code policy (config `email.verify_code`, overridable by `smtp_config.verify_code`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub expire_minutes: i64,
    pub send_interval_seconds: i64,
    pub max_attempts: i32,
    pub length: usize,
}

impl Policy {
    /// Replaces out-of-range values with the original defaults.
    pub fn resolved(self) -> Self {
        Self {
            expire_minutes: if self.expire_minutes <= 0 {
                DEFAULT_EXPIRE_MINUTES
            } else {
                self.expire_minutes
            },
            send_interval_seconds: if self.send_interval_seconds <= 0 {
                DEFAULT_SEND_INTERVAL_SECONDS
            } else {
                self.send_interval_seconds
            },
            max_attempts: if self.max_attempts <= 0 {
                DEFAULT_MAX_ATTEMPTS
            } else {
                self.max_attempts
            },
            length: if (4..=10).contains(&self.length) {
                self.length
            } else {
                DEFAULT_LENGTH
            },
        }
    }
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            expire_minutes: DEFAULT_EXPIRE_MINUTES,
            send_interval_seconds: DEFAULT_SEND_INTERVAL_SECONDS,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            length: DEFAULT_LENGTH,
        }
    }
}

/// A stored code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyCode {
    pub id: Id,
    pub email: String,
    pub user_id: Option<Id>,
    pub purpose: String,
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub attempt_count: i32,
    pub sent_at: DateTime<Utc>,
}

/// Fields of a new code.
#[derive(Debug, Clone)]
pub struct NewVerifyCode {
    pub email: String,
    pub user_id: Option<Id>,
    pub purpose: Purpose,
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub sent_at: DateTime<Utc>,
}

/// Outcome of checking a submitted code against the latest record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The code matches; the record must be marked verified.
    Valid,
    /// The code does not match; the attempt counter must be bumped.
    Mismatch,
}

/// Validates `submitted` against `record` (original `verifyCode`): missing or
/// already-verified records are invalid, expiry is checked before the attempt
/// limit, and only a mismatch counts as an attempt.
pub fn check(
    record: Option<&VerifyCode>,
    submitted: &str,
    now: DateTime<Utc>,
    max_attempts: i32,
) -> Result<Check> {
    let record = record.ok_or_else(|| Error::bad_request(keys::INVALID))?;
    if record.verified_at.is_some() {
        return Err(Error::bad_request(keys::INVALID));
    }
    if record.expires_at < now {
        return Err(Error::bad_request(keys::EXPIRED));
    }
    if max_attempts > 0 && record.attempt_count >= max_attempts {
        return Err(Error::bad_request(keys::ATTEMPTS_EXCEEDED));
    }
    if record.code.trim() != submitted.trim() {
        return Ok(Check::Mismatch);
    }
    Ok(Check::Valid)
}

/// Persistence port for verification codes (soft-deleted rows excluded).
#[async_trait]
pub trait VerifyCodeRepo: Send + Sync {
    /// Latest code for `email` + `purpose` (by `sent_at`, then id).
    async fn latest(&self, email: &str, purpose: Purpose) -> Result<Option<VerifyCode>>;
    async fn create(&self, code: &NewVerifyCode) -> Result<()>;
    /// Marks the code verified if it is not yet; returns whether this call did it.
    async fn mark_verified(&self, id: Id, at: DateTime<Utc>) -> Result<bool>;
    async fn increment_attempt(&self, id: Id) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use chrono::Duration;

    use super::*;

    fn record(now: DateTime<Utc>) -> VerifyCode {
        VerifyCode {
            id: 1,
            email: "a@b.c".into(),
            user_id: None,
            purpose: "register".into(),
            code: "123456".into(),
            expires_at: now + Duration::minutes(10),
            verified_at: None,
            attempt_count: 0,
            sent_at: now,
        }
    }

    #[test]
    fn checks_codes() {
        let now = Utc::now();
        let r = record(now);
        assert_eq!(check(Some(&r), " 123456 ", now, 5).unwrap(), Check::Valid);
        assert_eq!(check(Some(&r), "000000", now, 5).unwrap(), Check::Mismatch);
        assert_eq!(check(None, "1", now, 5).unwrap_err().key(), keys::INVALID);
        let mut used = r.clone();
        used.verified_at = Some(now);
        assert_eq!(
            check(Some(&used), "123456", now, 5).unwrap_err().key(),
            keys::INVALID
        );
        let mut exhausted = r.clone();
        exhausted.attempt_count = 5;
        assert_eq!(
            check(Some(&exhausted), "123456", now, 5).unwrap_err().key(),
            keys::ATTEMPTS_EXCEEDED
        );
        assert_eq!(
            check(Some(&r), "123456", now + Duration::minutes(11), 5)
                .unwrap_err()
                .key(),
            keys::EXPIRED
        );
    }

    #[test]
    fn policy_defaults_and_purposes() {
        let p = Policy {
            expire_minutes: 0,
            send_interval_seconds: -1,
            max_attempts: 0,
            length: 2,
        }
        .resolved();
        assert_eq!(p, Policy::default());
        assert_eq!(Purpose::parse(" RESET "), Some(Purpose::Reset));
        assert_eq!(Purpose::parse("other"), None);
        assert_eq!(Purpose::ChangeEmailNew.as_str(), "change_email_new");
    }
}
