//! Administrator accounts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::totp::{TotpAccount, TotpState};
use crate::{Id, Result};

/// Username that is always a super administrator and cannot be deleted
/// (original `protectedSuperAdminUsername`).
pub const PROTECTED_SUPER_USERNAME: &str = "admin";

/// An administrator. Secrets are never serialized.
#[derive(Debug, Clone, Serialize)]
pub struct Admin {
    pub id: Id,
    pub username: String,
    #[serde(skip)]
    pub password_hash: String,
    #[serde(skip)]
    pub token_version: i64,
    #[serde(skip)]
    pub token_invalid_before: Option<DateTime<Utc>>,
    pub is_super: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    /// Encrypted TOTP secret (empty when 2FA is off).
    #[serde(skip)]
    pub totp_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_enabled_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub totp_pending_secret: String,
    #[serde(skip)]
    pub totp_pending_expires_at: Option<DateTime<Utc>>,
    /// JSON list of `{hash, used_at}` recovery codes.
    #[serde(skip)]
    pub recovery_codes: String,
    pub created_at: DateTime<Utc>,
}

impl Admin {
    pub fn totp_enabled(&self) -> bool {
        self.totp_enabled_at.is_some()
    }

    /// True when a token issued at `iat` with `version` is still valid for this account.
    pub fn accepts_token(&self, version: i64, iat: i64) -> bool {
        version == self.token_version
            && self
                .token_invalid_before
                .is_none_or(|before| iat >= before.timestamp())
    }
}

impl TotpAccount for Admin {
    fn id(&self) -> Id {
        self.id
    }

    fn totp_label(&self) -> String {
        self.username.clone()
    }

    fn totp_state(&self) -> TotpState {
        TotpState {
            secret: self.totp_secret.clone(),
            enabled_at: self.totp_enabled_at,
            pending_secret: self.totp_pending_secret.clone(),
            pending_expires_at: self.totp_pending_expires_at,
            recovery_codes: self.recovery_codes.clone(),
        }
    }

    fn set_totp_state(&mut self, state: TotpState) {
        self.totp_secret = state.secret;
        self.totp_enabled_at = state.enabled_at;
        self.totp_pending_secret = state.pending_secret;
        self.totp_pending_expires_at = state.pending_expires_at;
        self.recovery_codes = state.recovery_codes;
    }

    fn revoke_tokens(&mut self, now: DateTime<Utc>) {
        self.token_version += 1;
        self.token_invalid_before = Some(now);
    }
}

/// Validates and trims an administrator username (3–64 chars, no whitespace).
pub fn normalize_username(username: &str) -> Option<String> {
    let trimmed = username.trim();
    let len = trimmed.chars().count();
    (!trimmed.is_empty() && !trimmed.contains(char::is_whitespace) && (3..=64).contains(&len))
        .then(|| trimmed.to_owned())
}

/// True for the protected default super administrator.
pub fn is_protected_username(username: &str) -> bool {
    username
        .trim()
        .eq_ignore_ascii_case(PROTECTED_SUPER_USERNAME)
}

/// Fields for creating an administrator.
#[derive(Debug, Clone)]
pub struct NewAdmin {
    pub username: String,
    pub password_hash: String,
    pub is_super: bool,
}

/// Persistence port for administrators (soft-deleted rows are excluded).
#[async_trait]
pub trait AdminRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<Admin>>;
    async fn get_by_username(&self, username: &str) -> Result<Option<Admin>>;
    async fn count(&self) -> Result<u64>;
    async fn list(&self) -> Result<Vec<Admin>>;
    async fn create(&self, admin: &NewAdmin) -> Result<Admin>;
    /// Persists every mutable column of `admin`.
    async fn save(&self, admin: &Admin) -> Result<()>;
    async fn delete(&self, id: Id) -> Result<()>;
    /// Replaces `recovery_codes` only when it still equals `expected`
    /// (compare-and-swap so a recovery code cannot be consumed twice).
    async fn replace_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool>;
}

/// `admin_login_logs.event_type` values (original `constants/admin_login_event.go`).
pub mod events {
    pub const LOGIN_PASSWORD: &str = "login_password";
    pub const LOGIN_2FA_VERIFY: &str = "login_2fa_verify";
    pub const LOGIN_RECOVERY_CODE: &str = "login_recovery_code";
    pub const TOTP_SETUP: &str = "2fa_setup";
    pub const TOTP_ENABLED: &str = "2fa_enabled";
    pub const TOTP_DISABLED: &str = "2fa_disabled";
    pub const RECOVERY_REGENERATED: &str = "recovery_regenerated";
    pub const TOTP_RESET_BY_ADMIN: &str = "2fa_reset_by_admin";
    pub const PASSWORD_RESET_BY_CLI: &str = "password_reset_by_cli";

    pub const STATUS_SUCCESS: &str = "success";
    pub const STATUS_FAILED: &str = "failed";

    pub const FAIL_INVALID_CREDENTIALS: &str = "invalid_credentials";
    pub const FAIL_INVALID_TOTP_CODE: &str = "invalid_totp_code";
    pub const FAIL_INVALID_RECOVERY_CODE: &str = "invalid_recovery_code";
    pub const FAIL_TOO_MANY_ATTEMPTS: &str = "too_many_attempts";
    pub const FAIL_PENDING_EXPIRED: &str = "pending_expired";
    pub const FAIL_INTERNAL: &str = "internal_error";
    pub const FAIL_ALREADY_ENABLED: &str = "already_enabled";
}

/// One row of `admin_login_logs`.
#[derive(Debug, Clone, Serialize)]
pub struct AdminLoginLog {
    pub admin_id: Id,
    pub username: String,
    /// e.g. `login_password`, `login_2fa`.
    pub event_type: String,
    /// `success` or `failed`.
    pub status: String,
    pub fail_reason: String,
    pub client_ip: String,
    pub user_agent: String,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_id: Option<Id>,
}

/// Append-only admin login audit port.
#[async_trait]
pub trait AdminLoginLogRepo: Send + Sync {
    async fn record(&self, log: AdminLoginLog) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn token_validity_respects_version_and_invalid_before() {
        let mut admin = Admin {
            id: 1,
            username: "a".into(),
            password_hash: String::new(),
            token_version: 2,
            token_invalid_before: Some(Utc.timestamp_opt(1000, 0).unwrap()),
            is_super: true,
            last_login_at: None,
            totp_secret: String::new(),
            totp_enabled_at: None,
            totp_pending_secret: String::new(),
            totp_pending_expires_at: None,
            recovery_codes: String::new(),
            created_at: Utc::now(),
        };
        assert!(admin.accepts_token(2, 1000));
        assert!(!admin.accepts_token(2, 999));
        assert!(!admin.accepts_token(1, 2000));
        admin.token_invalid_before = None;
        assert!(admin.accepts_token(2, 0));
    }
}
