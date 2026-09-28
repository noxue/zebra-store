//! Administrator accounts and role management with the permission audit trail
//! (original `adminauthorization` handlers).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::json;
use zs_domain::authz::{self, Policy};
use zs_domain::identity::admin::{self, Admin, AdminRepo, NewAdmin};
use zs_domain::identity::audit::{NewAuthzAuditLog, actions};
use zs_domain::identity::password::PasswordPolicy;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;

use super::admin_auth::{AdminPrincipal, ClientInfo};
use super::audit::AuditService;
use super::authz::AuthzService;
use super::password;

/// Row of `GET /admin/authz/admins`.
#[derive(Debug, Clone, Serialize)]
pub struct AdminListItem {
    pub id: Id,
    pub username: String,
    pub is_super: bool,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub roles: Vec<String>,
    pub totp_enabled: bool,
    pub totp_enabled_at: Option<DateTime<Utc>>,
}

/// Row of `GET /admin/authz/roles?include_metadata=true`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RoleItem {
    pub role: String,
    pub immutable: bool,
}

/// Partial update of an administrator.
#[derive(Debug, Clone, Default)]
pub struct AdminPatch {
    pub username: Option<String>,
    pub password: Option<String>,
    pub is_super: Option<bool>,
}

/// Who performs a change (for the audit trail).
#[derive(Debug, Clone, Copy)]
pub struct Operator<'a> {
    pub admin: &'a AdminPrincipal,
    pub client: &'a ClientInfo,
}

/// Admin account and RBAC management service.
#[derive(Clone)]
pub struct AdminAccountService {
    repo: Arc<dyn AdminRepo>,
    authz: AuthzService,
    audit: AuditService,
    clock: Arc<dyn Clock>,
    policy: PasswordPolicy,
}

impl std::fmt::Debug for AdminAccountService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminAccountService")
    }
}

fn bad_request<E>(_: E) -> Error {
    Error::bad_request("error.bad_request")
}

/// Role errors: immutable built-in roles keep their key, everything else is `error.bad_request`.
fn role_error(e: Error) -> Error {
    if e.key() == "error.authz_builtin_role_immutable" {
        e
    } else {
        bad_request(e)
    }
}

impl AdminAccountService {
    pub fn new(
        repo: Arc<dyn AdminRepo>,
        authz: AuthzService,
        audit: AuditService,
        clock: Arc<dyn Clock>,
        policy: PasswordPolicy,
    ) -> Self {
        Self {
            repo,
            authz,
            audit,
            clock,
            policy,
        }
    }

    async fn audit(&self, op: Operator<'_>, target: Option<&Admin>, log: NewAuthzAuditLog) {
        self.audit
            .record_authz(NewAuthzAuditLog {
                operator_admin_id: op.admin.id,
                operator_username: op.admin.username.clone(),
                target_admin_id: target.map(|t| t.id),
                target_username: target.map(|t| t.username.clone()).unwrap_or_default(),
                request_id: op.client.request_id.clone(),
                ..log
            })
            .await;
    }

    /// Roles, optionally with the `immutable` flag.
    pub fn roles(&self) -> Result<Vec<RoleItem>> {
        Ok(self
            .authz
            .roles()
            .map_err(|e| e.or_internal("error.config_fetch_failed"))?
            .into_iter()
            .map(|role| RoleItem {
                immutable: authz::is_immutable_builtin(&role),
                role,
            })
            .collect())
    }

    pub async fn create_role(&self, op: Operator<'_>, role: &str) -> Result<String> {
        let role = self.authz.ensure_role(role).await.map_err(bad_request)?;
        self.audit(
            op,
            None,
            NewAuthzAuditLog {
                action: actions::ROLE_CREATE.into(),
                role: role.clone(),
                detail: json!({ "role": role }),
                ..Default::default()
            },
        )
        .await;
        tracing::info!(operator_admin_id = op.admin.id, %role, "admin_authz_role_created");
        Ok(role)
    }

    pub async fn delete_role(&self, op: Operator<'_>, role: &str) -> Result<()> {
        if role.trim().is_empty() {
            return Err(Error::invalid());
        }
        self.authz.delete_role(role).await.map_err(role_error)?;
        self.audit(
            op,
            None,
            NewAuthzAuditLog {
                action: actions::ROLE_DELETE.into(),
                role: role.to_owned(),
                detail: json!({ "role": role }),
                ..Default::default()
            },
        )
        .await;
        tracing::info!(operator_admin_id = op.admin.id, %role, "admin_authz_role_deleted");
        Ok(())
    }

    pub fn role_policies(&self, role: &str) -> Result<Vec<Policy>> {
        if role.trim().is_empty() {
            return Err(Error::invalid());
        }
        self.authz.role_policies(role).map_err(bad_request)
    }

    /// Grants (`grant = true`) or revokes a policy on a custom role.
    pub async fn change_policy(
        &self,
        op: Operator<'_>,
        grant: bool,
        role: &str,
        object: &str,
        method: &str,
    ) -> Result<()> {
        if role.trim().is_empty() || object.trim().is_empty() || method.trim().is_empty() {
            return Err(Error::invalid());
        }
        if grant {
            self.authz.grant(role, object, method).await
        } else {
            self.authz.revoke(role, object, method).await
        }
        .map_err(role_error)?;
        self.audit(
            op,
            None,
            NewAuthzAuditLog {
                action: if grant {
                    actions::POLICY_GRANT
                } else {
                    actions::POLICY_REVOKE
                }
                .into(),
                role: role.to_owned(),
                object: object.to_owned(),
                method: method.to_owned(),
                detail: json!({
                    "role": role,
                    "object": object,
                    "method": method.trim().to_uppercase(),
                }),
                ..Default::default()
            },
        )
        .await;
        Ok(())
    }

    pub async fn list(&self) -> Result<Vec<AdminListItem>> {
        let admins = self
            .repo
            .list()
            .await
            .map_err(|e| e.or_internal("error.config_fetch_failed"))?;
        admins
            .into_iter()
            .map(|a| {
                Ok(AdminListItem {
                    roles: self
                        .authz
                        .admin_roles(a.id)
                        .map_err(|e| e.or_internal("error.config_fetch_failed"))?,
                    id: a.id,
                    username: a.username,
                    is_super: a.is_super,
                    last_login_at: a.last_login_at,
                    created_at: a.created_at,
                    totp_enabled: a.totp_enabled_at.is_some(),
                    totp_enabled_at: a.totp_enabled_at,
                })
            })
            .collect()
    }

    async fn find(&self, id: Id, fail_key: &'static str) -> Result<Admin> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(fail_key))?
            .ok_or_else(|| Error::bad_request("error.admin_id_invalid"))
    }

    fn check_password(&self, password: &str) -> Result<String> {
        let password = password.trim();
        if password.is_empty() {
            return Err(Error::bad_request("error.password_weak"));
        }
        self.policy.validate(password)?;
        Ok(password.to_owned())
    }

    pub async fn create(
        &self,
        op: Operator<'_>,
        username: &str,
        password_raw: &str,
        is_super: Option<bool>,
    ) -> Result<Admin> {
        let username = admin::normalize_username(username)
            .ok_or_else(|| Error::bad_request("error.admin_username_invalid"))?;
        if password_raw.trim().is_empty() {
            return Err(Error::bad_request("error.password_weak"));
        }
        if self
            .repo
            .get_by_username(&username)
            .await
            .map_err(|e| e.or_internal("error.admin_create_failed"))?
            .is_some()
        {
            return Err(Error::bad_request("error.admin_username_exists"));
        }
        let password = self.check_password(password_raw)?;
        let hash = password::hash(&password)
            .await
            .map_err(|e| e.or_internal("error.admin_create_failed"))?;
        let is_super = is_super.unwrap_or(false) || admin::is_protected_username(&username);
        let created = self
            .repo
            .create(&NewAdmin {
                username,
                password_hash: hash,
                is_super,
            })
            .await
            .map_err(|e| e.or_internal("error.admin_create_failed"))?;
        self.audit(
            op,
            Some(&created),
            NewAuthzAuditLog {
                action: actions::ADMIN_CREATE.into(),
                detail: json!({
                    "target_admin_id": created.id,
                    "target_username": created.username,
                    "is_super": created.is_super,
                }),
                ..Default::default()
            },
        )
        .await;
        tracing::info!(
            operator_admin_id = op.admin.id,
            target_admin_id = created.id,
            "admin_authz_admin_created"
        );
        Ok(created)
    }

    pub async fn update(&self, op: Operator<'_>, id: Id, patch: AdminPatch) -> Result<Admin> {
        let mut target = self.find(id, "error.admin_update_failed").await?;
        let mut updated: Vec<&str> = Vec::new();
        if let Some(name) = &patch.username {
            let name = admin::normalize_username(name)
                .ok_or_else(|| Error::bad_request("error.admin_username_invalid"))?;
            if name != target.username {
                let existing = self
                    .repo
                    .get_by_username(&name)
                    .await
                    .map_err(|e| e.or_internal("error.admin_update_failed"))?;
                if existing.is_some_and(|e| e.id != target.id) {
                    return Err(Error::bad_request("error.admin_username_exists"));
                }
                target.username = name;
                updated.push("username");
            }
        }
        if let Some(flag) = patch.is_super {
            let next = flag || admin::is_protected_username(&target.username);
            if target.is_super != next {
                target.is_super = next;
                updated.push("is_super");
            }
        }
        if let Some(pw) = &patch.password {
            let pw = self.check_password(pw)?;
            target.password_hash = password::hash(&pw)
                .await
                .map_err(|e| e.or_internal("error.admin_update_failed"))?;
            target.token_version += 1;
            target.token_invalid_before = Some(self.clock.now());
            updated.push("password");
        }
        if updated.is_empty() {
            return Err(Error::invalid());
        }
        self.repo
            .save(&target)
            .await
            .map_err(|e| e.or_internal("error.admin_update_failed"))?;
        updated.sort_unstable();
        self.audit(
            op,
            Some(&target),
            NewAuthzAuditLog {
                action: actions::ADMIN_UPDATE.into(),
                detail: json!({
                    "target_admin_id": target.id,
                    "target_username": target.username,
                    "updated_fields": updated,
                    "is_super": target.is_super,
                }),
                ..Default::default()
            },
        )
        .await;
        Ok(target)
    }

    pub async fn delete(&self, op: Operator<'_>, id: Id) -> Result<()> {
        let target = self.find(id, "error.admin_delete_failed").await?;
        if op.admin.id == id {
            return Err(Error::bad_request("error.admin_delete_self_forbidden"));
        }
        if admin::is_protected_username(&target.username) {
            return Err(Error::bad_request("error.admin_delete_protected"));
        }
        let count = self
            .repo
            .count()
            .await
            .map_err(|e| e.or_internal("error.admin_delete_failed"))?;
        if count <= 1 {
            return Err(Error::bad_request("error.admin_delete_last_forbidden"));
        }
        self.authz
            .set_admin_roles(id, &[])
            .await
            .map_err(|e| Error::internal(e).or_internal("error.admin_delete_failed"))?;
        self.repo
            .delete(id)
            .await
            .map_err(|e| e.or_internal("error.admin_delete_failed"))?;
        self.audit(
            op,
            Some(&target),
            NewAuthzAuditLog {
                action: actions::ADMIN_DELETE.into(),
                detail: json!({
                    "target_admin_id": target.id,
                    "target_username": target.username,
                }),
                ..Default::default()
            },
        )
        .await;
        Ok(())
    }

    pub async fn admin_roles(&self, id: Id) -> Result<Vec<String>> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal("error.config_fetch_failed"))?;
        self.authz
            .admin_roles(id)
            .map_err(|e| e.or_internal("error.config_fetch_failed"))
    }

    pub async fn set_admin_roles(&self, op: Operator<'_>, id: Id, roles: &[String]) -> Result<()> {
        let target = self.find(id, "error.save_failed").await?;
        self.authz
            .set_admin_roles(id, roles)
            .await
            .map_err(bad_request)?;
        self.audit(
            op,
            Some(&target),
            NewAuthzAuditLog {
                action: actions::ADMIN_ROLES_UPDATE.into(),
                detail: json!({
                    "target_admin_id": target.id,
                    "target_username": target.username,
                    "roles": roles,
                }),
                ..Default::default()
            },
        )
        .await;
        Ok(())
    }
}
