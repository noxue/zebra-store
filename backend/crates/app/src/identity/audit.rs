//! Audit trails: permission changes, user logins and admin login events.

use std::sync::Arc;

use zs_domain::identity::admin::{AdminLoginLog, AdminLoginLogRepo};
use zs_domain::identity::audit::{
    AuthzAuditFilter, AuthzAuditLog, AuthzAuditRepo, NewAuthzAuditLog,
};
use zs_domain::identity::login_log::{
    NewUserLoginLog, UserLoginFilter, UserLoginLog, UserLoginLogRepo,
};
use zs_domain::{Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::admin_auth::ClientInfo;

/// Largest page of a user's own login history (original `ListByUser`).
const MAX_OWN_PAGE_SIZE: u64 = 100;

/// Audit service (cheap to clone). Recording never fails the calling request.
#[derive(Clone)]
pub struct AuditService {
    authz: Arc<dyn AuthzAuditRepo>,
    user_logins: Arc<dyn UserLoginLogRepo>,
    admin_logins: Arc<dyn AdminLoginLogRepo>,
}

impl std::fmt::Debug for AuditService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AuditService")
    }
}

/// Parameters of one admin login event.
#[derive(Debug, Clone, Copy)]
pub struct AdminEvent<'a> {
    pub admin_id: Id,
    pub username: &'a str,
    pub event_type: &'a str,
    pub status: &'a str,
    pub fail_reason: &'a str,
    pub operator_id: Option<Id>,
}

impl AuditService {
    pub fn new(
        authz: Arc<dyn AuthzAuditRepo>,
        user_logins: Arc<dyn UserLoginLogRepo>,
        admin_logins: Arc<dyn AdminLoginLogRepo>,
    ) -> Self {
        Self {
            authz,
            user_logins,
            admin_logins,
        }
    }

    /// Records a permission change (ignored without operator/action).
    pub async fn record_authz(&self, log: NewAuthzAuditLog) {
        let Some(log) = log.normalized() else {
            return;
        };
        if let Err(error) = self.authz.record(&log).await {
            tracing::warn!(%error, action = %log.action, "failed to record authz audit log");
        }
    }

    pub async fn list_authz(
        &self,
        filter: &AuthzAuditFilter,
        page: PageRequest,
    ) -> Result<Page<AuthzAuditLog>> {
        self.authz.list(filter, page).await
    }

    /// Records a user login attempt (source `web`).
    pub async fn record_user_login(
        &self,
        email: &str,
        user_id: Id,
        status: &str,
        fail_reason: &str,
        client: &ClientInfo,
    ) {
        self.record_user_login_from(email, user_id, status, fail_reason, "", client)
            .await;
    }

    /// Records a user login attempt from `source` (`web`, `telegram`, `google`;
    /// empty means `web`).
    pub async fn record_user_login_from(
        &self,
        email: &str,
        user_id: Id,
        status: &str,
        fail_reason: &str,
        source: &str,
        client: &ClientInfo,
    ) {
        let log = NewUserLoginLog {
            user_id,
            email: email.to_owned(),
            status: status.to_owned(),
            fail_reason: fail_reason.to_owned(),
            client_ip: client.ip.clone(),
            user_agent: client.user_agent.clone(),
            login_source: source.to_owned(),
            request_id: client.request_id.clone(),
        }
        .normalized();
        if let Err(error) = self.user_logins.record(&log).await {
            tracing::warn!(%error, "failed to record user login log");
        }
    }

    pub async fn list_user_logins(
        &self,
        filter: &UserLoginFilter,
        page: PageRequest,
    ) -> Result<Page<UserLoginLog>> {
        self.user_logins.list(filter, page).await
    }

    /// A user's own history (page size capped at 100).
    pub async fn list_own_logins(
        &self,
        user_id: Id,
        page: PageRequest,
    ) -> Result<(Page<UserLoginLog>, PageRequest)> {
        let page = PageRequest {
            page: page.page,
            page_size: page.page_size.min(MAX_OWN_PAGE_SIZE),
        };
        let filter = UserLoginFilter {
            user_id: Some(user_id),
            ..UserLoginFilter::default()
        };
        Ok((self.user_logins.list(&filter, page).await?, page))
    }

    /// Records an admin login / 2FA event.
    pub async fn record_admin(&self, event: AdminEvent<'_>, client: &ClientInfo) {
        let entry = AdminLoginLog {
            admin_id: event.admin_id,
            username: event.username.trim().to_owned(),
            event_type: event.event_type.to_owned(),
            status: event.status.to_owned(),
            fail_reason: event.fail_reason.to_owned(),
            client_ip: client.ip.trim().to_owned(),
            user_agent: client.user_agent.trim().to_owned(),
            request_id: client.request_id.trim().to_owned(),
            operator_id: event.operator_id,
        };
        if let Err(error) = self.admin_logins.record(entry).await {
            tracing::warn!(%error, "failed to record admin login log");
        }
    }
}
