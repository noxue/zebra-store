//! API credentials of downstream shops (users applying for supplier API access).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

/// Credential review / lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    PendingReview,
    Approved,
    Rejected,
    Disabled,
}

impl CredentialStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "pending_review" => Some(Self::PendingReview),
            "approved" => Some(Self::Approved),
            "rejected" => Some(Self::Rejected),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingReview => "pending_review",
            Self::Approved => "approved",
            Self::Rejected => "rejected",
            Self::Disabled => "disabled",
        }
    }
}

/// Owner shown with a credential (admin list): the original preloads the whole `User`
/// model, so this carries every JSON field of `users` (schema.json; `admin_note` and
/// `totp_enabled_at` are `omitempty`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CredentialUser {
    pub id: Id,
    pub email: String,
    pub display_name: String,
    pub locale: String,
    pub status: String,
    pub member_level_id: Id,
    pub total_recharged: Amount,
    pub total_spent: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub admin_note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub totp_enabled_at: Option<DateTime<Utc>>,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// An API credential (admin JSON shape; the secret is never serialized).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApiCredential {
    pub id: Id,
    pub user_id: Id,
    pub api_key: String,
    /// Encrypted secret (empty until approved).
    #[serde(skip)]
    pub api_secret: String,
    pub status: CredentialStatus,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reject_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approved_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<CredentialUser>,
    /// Soft-delete marker (repository bookkeeping).
    #[serde(skip)]
    pub deleted: bool,
}

impl ApiCredential {
    /// Usable for API calls: approved and enabled (UPS-18: a key alone is not enough).
    pub fn usable(&self) -> bool {
        self.status == CredentialStatus::Approved && self.is_active && !self.deleted
    }

    /// Resets a rejected or deleted credential for a new application (UPS-18): a new
    /// key, no secret, pending review, every approval trace cleared, restored.
    pub fn reset_for_reapply(&mut self, new_key: String) {
        self.api_key = new_key;
        self.api_secret.clear();
        self.status = CredentialStatus::PendingReview;
        self.reject_reason.clear();
        self.approved_at = None;
        self.last_used_at = None;
        self.is_active = false;
        self.deleted = false;
    }
}

/// Outcome of evaluating an application against the existing row (original `Apply`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDecision {
    /// No row: create a new pending credential.
    Create,
    /// Deleted or rejected row: reset it in place (UPS-18).
    Reset,
    /// A pending application exists.
    Pending,
    /// Approved or disabled: already has a credential.
    Exists,
}

pub fn apply_decision(existing: Option<&ApiCredential>) -> ApplyDecision {
    let Some(c) = existing else {
        return ApplyDecision::Create;
    };
    if c.deleted {
        return ApplyDecision::Reset;
    }
    match c.status {
        CredentialStatus::PendingReview => ApplyDecision::Pending,
        CredentialStatus::Rejected => ApplyDecision::Reset,
        CredentialStatus::Approved | CredentialStatus::Disabled => ApplyDecision::Exists,
    }
}

/// Admin list filter.
#[derive(Debug, Clone, Default)]
pub struct CredentialFilter {
    pub page: PageRequest,
    pub status: String,
    pub user_id: Id,
    pub search: String,
}

/// A credential with its owner status, for request authentication.
#[derive(Debug, Clone)]
pub struct AuthCredential {
    pub credential: ApiCredential,
    /// `None` when the owner no longer exists.
    pub user_status: Option<String>,
}

/// Persistence of `api_credentials`.
#[async_trait]
pub trait CredentialRepo: Send + Sync {
    /// Live credential by id.
    async fn get(&self, id: Id) -> Result<Option<ApiCredential>>;
    /// Live credential of a user.
    async fn get_by_user(&self, user_id: Id) -> Result<Option<ApiCredential>>;
    /// Credential of a user including a soft-deleted row (`deleted` set).
    async fn get_any_by_user(&self, user_id: Id) -> Result<Option<ApiCredential>>;
    /// Live credential by key with its owner status.
    async fn find_for_auth(&self, api_key: &str) -> Result<Option<AuthCredential>>;
    async fn create(&self, user_id: Id, api_key: &str, now: DateTime<Utc>)
    -> Result<ApiCredential>;
    /// Saves every mutable column, restoring a soft-deleted row when `deleted` is false.
    async fn save(&self, credential: &ApiCredential, now: DateTime<Utc>) -> Result<()>;
    /// Writes only `last_used_at` (original `TouchLastUsedAt`).
    async fn touch(&self, id: Id, at: DateTime<Utc>) -> Result<()>;
    async fn delete(&self, id: Id, at: DateTime<Utc>) -> Result<()>;
    async fn list(&self, filter: &CredentialFilter) -> Result<Page<ApiCredential>>;
}

/// Lifetime of a pending secret rotation: both secrets verify until then, afterwards
/// the new one is promoted automatically (zebra-store spec §2).
pub const ROTATION_DAYS: i64 = 7;

/// A pending secret rotation (`secret_next` encrypted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rotation {
    pub credential_id: Id,
    pub secret_next: String,
    pub expires_at: DateTime<Utc>,
}

/// Secret rotation and request-nonce storage of API credentials.
#[async_trait]
pub trait CredentialSecurityRepo: Send + Sync {
    async fn rotation(&self, credential_id: Id) -> Result<Option<Rotation>>;
    /// Creates or replaces the pending rotation.
    async fn put_rotation(&self, rotation: &Rotation, now: DateTime<Utc>) -> Result<()>;
    /// Atomically makes the pending secret the current one and drops the rotation;
    /// `false` when no rotation was pending (a concurrent promotion won).
    async fn promote_rotation(&self, credential_id: Id, now: DateTime<Utc>) -> Result<bool>;
    async fn clear_rotation(&self, credential_id: Id) -> Result<()>;
    /// Records a request nonce; `false` when the credential already used it (replay).
    async fn claim_nonce(&self, credential_id: Id, nonce: &str, now: DateTime<Utc>)
    -> Result<bool>;
    /// Drops nonces recorded before `before`.
    async fn purge_nonces(&self, before: DateTime<Utc>) -> Result<u64>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cred(status: CredentialStatus, deleted: bool) -> ApiCredential {
        ApiCredential {
            id: 1,
            user_id: 2,
            api_key: "old".into(),
            api_secret: "enc".into(),
            status,
            reject_reason: "no".into(),
            approved_at: Some(DateTime::<Utc>::MIN_UTC),
            is_active: true,
            last_used_at: Some(DateTime::<Utc>::MIN_UTC),
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
            user: None,
            deleted,
        }
    }

    // UPS-18 (2): reapply decisions.
    #[test]
    fn ups18_apply_decisions() {
        assert_eq!(apply_decision(None), ApplyDecision::Create);
        assert_eq!(
            apply_decision(Some(&cred(CredentialStatus::PendingReview, false))),
            ApplyDecision::Pending
        );
        assert_eq!(
            apply_decision(Some(&cred(CredentialStatus::Approved, false))),
            ApplyDecision::Exists
        );
        assert_eq!(
            apply_decision(Some(&cred(CredentialStatus::Disabled, false))),
            ApplyDecision::Exists
        );
        assert_eq!(
            apply_decision(Some(&cred(CredentialStatus::Rejected, false))),
            ApplyDecision::Reset
        );
        assert_eq!(
            apply_decision(Some(&cred(CredentialStatus::Approved, true))),
            ApplyDecision::Reset
        );
    }

    // UPS-18: a reset clears every trace of the previous approval.
    #[test]
    fn ups18_reset_clears_old_secret() {
        let mut c = cred(CredentialStatus::Approved, true);
        c.reset_for_reapply("new".into());
        assert_eq!(c.api_key, "new");
        assert!(c.api_secret.is_empty());
        assert_eq!(c.status, CredentialStatus::PendingReview);
        assert!(c.reject_reason.is_empty() && c.approved_at.is_none() && c.last_used_at.is_none());
        assert!(!c.is_active && !c.deleted);
        assert!(!c.usable());
    }
}
