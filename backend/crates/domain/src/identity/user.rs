//! Storefront user accounts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::money::Amount;

use super::totp::{TotpAccount, TotpState};
use crate::{Id, Result};

/// `users.status` of an enabled account.
pub const STATUS_ACTIVE: &str = "active";
/// `users.status` of a disabled account.
pub const STATUS_DISABLED: &str = "disabled";

/// A user (admin-facing JSON shape; secrets never serialized).
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: Id,
    pub email: String,
    #[serde(skip)]
    pub password_hash: String,
    #[serde(skip)]
    pub password_setup_required: bool,
    pub display_name: String,
    pub locale: String,
    pub status: String,
    pub member_level_id: Id,
    pub total_recharged: Amount,
    pub total_spent: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub admin_note: String,
    #[serde(skip)]
    pub token_version: i64,
    #[serde(skip)]
    pub token_invalid_before: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub totp_secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_enabled_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub totp_pending_secret: String,
    #[serde(skip)]
    pub totp_pending_expires_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    pub recovery_codes: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn is_active(&self) -> bool {
        self.status.trim().eq_ignore_ascii_case(STATUS_ACTIVE)
    }

    /// True when a token issued at `iat` with `version` is still valid.
    pub fn accepts_token(&self, version: i64, iat: i64) -> bool {
        version == self.token_version
            && self
                .token_invalid_before
                .is_none_or(|before| iat >= before.timestamp())
    }
}

impl TotpAccount for User {
    fn id(&self) -> Id {
        self.id
    }

    fn totp_label(&self) -> String {
        if self.email.trim().is_empty() {
            format!("user-{}", self.id)
        } else {
            self.email.clone()
        }
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

/// Fields for creating a user.
#[derive(Debug, Clone)]
pub struct NewUser {
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub locale: String,
    pub member_level_id: Id,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub password_setup_required: bool,
}

/// Persistence port for users (soft-deleted rows excluded).
#[async_trait]
pub trait UserRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<User>>;
    /// Lookup by email (emails are stored lower-cased).
    async fn get_by_email(&self, email: &str) -> Result<Option<User>>;
    async fn create(&self, user: &NewUser) -> Result<User>;
    /// Persists the account columns of `user` (and bumps `updated_at`).
    /// `member_level_id`, `total_recharged` and `total_spent` are owned by the
    /// member-level/wallet/order flows and are never overwritten here, so a stale
    /// copy cannot roll back concurrent updates.
    async fn save(&self, user: &User) -> Result<()>;
    /// Replaces `recovery_codes` only when it still equals `expected`.
    async fn replace_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool>;
}

/// Looks up the default member level assigned to new users (consumer-side port
/// over the member-level table).
#[async_trait]
pub trait DefaultMemberLevel: Send + Sync {
    /// Id of the active default level, `None` when there is none.
    async fn default_level_id(&self) -> Result<Option<Id>>;
}
