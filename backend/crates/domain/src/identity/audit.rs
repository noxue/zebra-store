//! Permission change audit trail (`authz_audit_logs`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

/// `action` values written by the permission endpoints.
pub mod actions {
    pub const ROLE_CREATE: &str = "role_create";
    pub const ROLE_DELETE: &str = "role_delete";
    pub const POLICY_GRANT: &str = "policy_grant";
    pub const POLICY_REVOKE: &str = "policy_revoke";
    pub const ADMIN_ROLES_UPDATE: &str = "admin_roles_update";
    pub const ADMIN_CREATE: &str = "admin_create";
    pub const ADMIN_UPDATE: &str = "admin_update";
    pub const ADMIN_DELETE: &str = "admin_delete";
}

/// One audit row (response shape of `/admin/authz/audit-logs`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthzAuditLog {
    pub id: Id,
    pub operator_admin_id: Id,
    pub operator_username: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_admin_id: Option<Id>,
    pub target_username: String,
    pub action: String,
    pub role: String,
    pub object: String,
    pub method: String,
    pub request_id: String,
    pub detail: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

/// A change to record.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NewAuthzAuditLog {
    pub operator_admin_id: Id,
    pub operator_username: String,
    pub target_admin_id: Option<Id>,
    pub target_username: String,
    pub action: String,
    pub role: String,
    pub object: String,
    pub method: String,
    pub request_id: String,
    pub detail: serde_json::Value,
}

impl NewAuthzAuditLog {
    /// Trims text fields and upper-cases the method; returns `None` for records
    /// the original ignores (no operator or no action).
    pub fn normalized(mut self) -> Option<Self> {
        if self.operator_admin_id <= 0 || self.action.trim().is_empty() {
            return None;
        }
        self.operator_username = self.operator_username.trim().to_owned();
        self.target_username = self.target_username.trim().to_owned();
        self.action = self.action.trim().to_owned();
        self.role = self.role.trim().to_owned();
        self.object = self.object.trim().to_owned();
        self.method = self.method.trim().to_uppercase();
        self.request_id = self.request_id.trim().to_owned();
        Some(self)
    }
}

/// Filter of `/admin/authz/audit-logs`.
#[derive(Debug, Clone, Default)]
pub struct AuthzAuditFilter {
    pub operator_admin_id: Option<Id>,
    pub target_admin_id: Option<Id>,
    pub action: String,
    pub role: String,
    pub object: String,
    pub method: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Persistence port of the audit trail.
#[async_trait]
pub trait AuthzAuditRepo: Send + Sync {
    async fn record(&self, log: &NewAuthzAuditLog) -> Result<()>;
    /// Newest first.
    async fn list(
        &self,
        filter: &AuthzAuditFilter,
        page: PageRequest,
    ) -> Result<Page<AuthzAuditLog>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_incomplete_and_normalizes() {
        assert!(
            NewAuthzAuditLog {
                action: "grant".into(),
                ..Default::default()
            }
            .normalized()
            .is_none()
        );
        let log = NewAuthzAuditLog {
            operator_admin_id: 7,
            operator_username: " root ".into(),
            action: " grant ".into(),
            object: " /admin/users ".into(),
            method: " post ".into(),
            request_id: " r2 ".into(),
            ..Default::default()
        }
        .normalized()
        .unwrap();
        assert_eq!(log.operator_username, "root");
        assert_eq!(log.action, "grant");
        assert_eq!(log.object, "/admin/users");
        assert_eq!(log.method, "POST");
        assert_eq!(log.request_id, "r2");
    }
}
