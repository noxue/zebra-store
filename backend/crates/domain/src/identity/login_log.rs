//! User login history (`user_login_logs`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

/// `status` of a successful login.
pub const STATUS_SUCCESS: &str = "success";
/// `status` of a failed login.
pub const STATUS_FAILED: &str = "failed";
/// Login source of email/password logins.
pub const SOURCE_WEB: &str = "web";

/// `fail_reason` values (original `LoginLogFailReason*`).
pub mod reasons {
    pub const BAD_REQUEST: &str = "bad_request";
    pub const CAPTCHA_REQUIRED: &str = "captcha_required";
    pub const CAPTCHA_INVALID: &str = "captcha_invalid";
    pub const CAPTCHA_CONFIG_INVALID: &str = "captcha_config_invalid";
    pub const CAPTCHA_VERIFY_FAILED: &str = "captcha_verify_failed";
    pub const INVALID_EMAIL: &str = "invalid_email";
    pub const INVALID_CREDENTIALS: &str = "invalid_credentials";
    pub const EMAIL_NOT_VERIFIED: &str = "email_not_verified";
    pub const USER_DISABLED: &str = "user_disabled";
    pub const INTERNAL_ERROR: &str = "internal_error";
    pub const INVALID_TOTP_CODE: &str = "invalid_totp_code";
    pub const INVALID_RECOVERY_CODE: &str = "invalid_recovery_code";
    pub const CHALLENGE_INVALID: &str = "challenge_invalid";
}

/// One login attempt (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserLoginLog {
    pub id: Id,
    pub user_id: Id,
    pub email: String,
    pub status: String,
    pub fail_reason: String,
    pub client_ip: String,
    pub user_agent: String,
    pub login_source: String,
    pub request_id: String,
    pub created_at: DateTime<Utc>,
}

/// A login attempt to record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewUserLoginLog {
    pub user_id: Id,
    pub email: String,
    pub status: String,
    pub fail_reason: String,
    pub client_ip: String,
    pub user_agent: String,
    pub login_source: String,
    pub request_id: String,
}

impl NewUserLoginLog {
    /// Applies the original normalisation: email lower-cased when valid, unknown
    /// statuses become `failed`, successes carry no reason, failures default to
    /// `internal_error`, the source defaults to `web`.
    pub fn normalized(mut self) -> Self {
        let email = self.email.trim().to_owned();
        self.email = super::email::normalize(&email).unwrap_or(email);
        let status = self.status.trim().to_lowercase();
        self.status = if status == STATUS_SUCCESS {
            STATUS_SUCCESS.into()
        } else {
            STATUS_FAILED.into()
        };
        let reason = self.fail_reason.trim().to_lowercase();
        self.fail_reason = if self.status == STATUS_SUCCESS {
            String::new()
        } else if reason.is_empty() {
            reasons::INTERNAL_ERROR.into()
        } else {
            reason
        };
        let source = self.login_source.trim().to_lowercase();
        self.login_source = if source.is_empty() {
            SOURCE_WEB.into()
        } else {
            source
        };
        self.client_ip = self.client_ip.trim().to_owned();
        self.user_agent = self.user_agent.trim().to_owned();
        self.request_id = self.request_id.trim().to_owned();
        self
    }
}

/// Admin-side filter of `/admin/user-login-logs`.
#[derive(Debug, Clone, Default)]
pub struct UserLoginFilter {
    pub user_id: Option<Id>,
    pub email: String,
    pub status: String,
    pub fail_reason: String,
    pub client_ip: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Persistence port of user login logs.
#[async_trait]
pub trait UserLoginLogRepo: Send + Sync {
    async fn record(&self, log: &NewUserLoginLog) -> Result<()>;
    /// Newest first.
    async fn list(&self, filter: &UserLoginFilter, page: PageRequest)
    -> Result<Page<UserLoginLog>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_like_the_original() {
        let log = NewUserLoginLog {
            user_id: 42,
            email: " Alice@Example.COM ".into(),
            status: "unexpected".into(),
            client_ip: " 127.0.0.1 ".into(),
            user_agent: " ua ".into(),
            request_id: " r1 ".into(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(log.email, "alice@example.com");
        assert_eq!(log.status, STATUS_FAILED);
        assert_eq!(log.fail_reason, reasons::INTERNAL_ERROR);
        assert_eq!(log.login_source, SOURCE_WEB);
        assert_eq!(
            (
                log.client_ip.as_str(),
                log.user_agent.as_str(),
                log.request_id.as_str()
            ),
            ("127.0.0.1", "ua", "r1")
        );
        let ok = NewUserLoginLog {
            status: "success".into(),
            fail_reason: "whatever".into(),
            ..Default::default()
        }
        .normalized();
        assert_eq!(ok.fail_reason, "");
    }
}
