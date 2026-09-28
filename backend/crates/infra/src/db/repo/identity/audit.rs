//! [`AuthzAuditRepo`] backed by `authz_audit_logs`.

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use zs_domain::Result;
use zs_domain::identity::audit::{
    AuthzAuditFilter, AuthzAuditLog, AuthzAuditRepo, NewAuthzAuditLog,
};
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::authz_audit_logs as logs;
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`AuthzAuditRepo`].
#[derive(Debug, Clone)]
pub struct SeaAuthzAuditRepo {
    db: DatabaseConnection,
}

impl SeaAuthzAuditRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: logs::Model) -> AuthzAuditLog {
    AuthzAuditLog {
        id: m.id,
        operator_admin_id: m.operator_admin_id,
        operator_username: m.operator_username,
        target_admin_id: m.target_admin_id,
        target_username: m.target_username,
        action: m.action,
        role: m.role,
        object: m.object,
        method: m.method,
        request_id: m.request_id,
        detail: m.detail_json.unwrap_or(serde_json::Value::Null),
        created_at: m.created_at,
    }
}

fn condition(f: &AuthzAuditFilter) -> Condition {
    let mut c = Condition::all();
    if let Some(id) = f.operator_admin_id {
        c = c.add(logs::Column::OperatorAdminId.eq(id));
    }
    if let Some(id) = f.target_admin_id {
        c = c.add(logs::Column::TargetAdminId.eq(id));
    }
    if !f.action.is_empty() {
        c = c.add(logs::Column::Action.eq(f.action.clone()));
    }
    if !f.role.is_empty() {
        c = c.add(logs::Column::Role.eq(f.role.clone()));
    }
    if !f.object.is_empty() {
        c = c.add(logs::Column::Object.eq(f.object.clone()));
    }
    if !f.method.is_empty() {
        c = c.add(logs::Column::Method.eq(f.method.to_uppercase()));
    }
    if let Some(from) = f.created_from {
        c = c.add(logs::Column::CreatedAt.gte(from));
    }
    if let Some(to) = f.created_to {
        c = c.add(logs::Column::CreatedAt.lte(to));
    }
    c
}

#[async_trait]
impl AuthzAuditRepo for SeaAuthzAuditRepo {
    async fn record(&self, l: &NewAuthzAuditLog) -> Result<()> {
        logs::ActiveModel {
            operator_admin_id: Set(l.operator_admin_id),
            operator_username: Set(l.operator_username.clone()),
            target_admin_id: Set(l.target_admin_id),
            target_username: Set(l.target_username.clone()),
            action: Set(l.action.clone()),
            role: Set(l.role.clone()),
            object: Set(l.object.clone()),
            method: Set(l.method.clone()),
            request_id: Set(l.request_id.clone()),
            detail_json: Set((!l.detail.is_null()).then(|| l.detail.clone())),
            created_at: Set(now()),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn list(
        &self,
        filter: &AuthzAuditFilter,
        page: PageRequest,
    ) -> Result<Page<AuthzAuditLog>> {
        let query = logs::Entity::find().filter(condition(filter));
        let total = query.clone().count(&self.db).await.dom()?;
        let rows = query
            .order_by_desc(logs::Column::CreatedAt)
            .order_by_desc(logs::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_domain).collect(),
            total,
        })
    }
}
