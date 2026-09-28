//! [`UserLoginLogRepo`] backed by `user_login_logs`.

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use zs_domain::Result;
use zs_domain::identity::login_log::{
    NewUserLoginLog, UserLoginFilter, UserLoginLog, UserLoginLogRepo,
};
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::user_login_logs as logs;
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`UserLoginLogRepo`].
#[derive(Debug, Clone)]
pub struct SeaUserLoginLogRepo {
    db: DatabaseConnection,
}

impl SeaUserLoginLogRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: logs::Model) -> UserLoginLog {
    UserLoginLog {
        id: m.id,
        user_id: m.user_id,
        email: m.email,
        status: m.status,
        fail_reason: m.fail_reason,
        client_ip: m.client_ip,
        user_agent: m.user_agent,
        login_source: m.login_source,
        request_id: m.request_id,
        created_at: m.created_at,
    }
}

fn condition(f: &UserLoginFilter) -> Condition {
    let mut c = Condition::all();
    if let Some(id) = f.user_id {
        c = c.add(logs::Column::UserId.eq(id));
    }
    if !f.email.is_empty() {
        c = c.add(logs::Column::Email.contains(f.email.to_lowercase()));
    }
    if !f.status.is_empty() {
        c = c.add(logs::Column::Status.eq(f.status.clone()));
    }
    if !f.fail_reason.is_empty() {
        c = c.add(logs::Column::FailReason.eq(f.fail_reason.clone()));
    }
    if !f.client_ip.is_empty() {
        c = c.add(logs::Column::ClientIp.eq(f.client_ip.clone()));
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
impl UserLoginLogRepo for SeaUserLoginLogRepo {
    async fn record(&self, l: &NewUserLoginLog) -> Result<()> {
        logs::ActiveModel {
            user_id: Set(l.user_id),
            email: Set(l.email.clone()),
            status: Set(l.status.clone()),
            fail_reason: Set(l.fail_reason.clone()),
            client_ip: Set(l.client_ip.clone()),
            user_agent: Set(l.user_agent.clone()),
            login_source: Set(l.login_source.clone()),
            request_id: Set(l.request_id.clone()),
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
        filter: &UserLoginFilter,
        page: PageRequest,
    ) -> Result<Page<UserLoginLog>> {
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
