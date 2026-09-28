//! [`CredentialRepo`] on `api_credentials` (owner info from `users`).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, Func, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use zs_domain::integration::credential::{
    ApiCredential, AuthCredential, CredentialFilter, CredentialRepo, CredentialStatus,
    CredentialUser,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

use crate::db::entity::{api_credentials, users};
use crate::db::repo::catalog::sql::contains_pattern;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`CredentialRepo`].
#[derive(Debug, Clone)]
pub struct SeaCredentialRepo {
    db: DatabaseConnection,
}

impl SeaCredentialRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn users(&self, ids: &[Id]) -> Result<HashMap<Id, users::Model>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(users::Entity::find()
            .filter(users::Column::Id.is_in(ids.to_vec()))
            .filter(users::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|u| (u.id, u))
            .collect())
    }
}

fn to_domain(m: api_credentials::Model) -> ApiCredential {
    ApiCredential {
        id: m.id,
        user_id: m.user_id,
        api_key: m.api_key,
        api_secret: m.api_secret,
        status: CredentialStatus::parse(&m.status).unwrap_or(CredentialStatus::PendingReview),
        reject_reason: m.reject_reason,
        approved_at: m.approved_at,
        is_active: m.is_active,
        last_used_at: m.last_used_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        user: None,
        deleted: m.deleted_at.is_some(),
    }
}

fn user_of(u: &users::Model) -> CredentialUser {
    CredentialUser {
        id: u.id,
        email: u.email.clone(),
        display_name: u.display_name.clone(),
        locale: u.locale.clone(),
        status: u.status.clone(),
        member_level_id: u.member_level_id,
        total_recharged: Amount::new(u.total_recharged),
        total_spent: Amount::new(u.total_spent),
        admin_note: u.admin_note.clone(),
        totp_enabled_at: u.totp_enabled_at,
        email_verified_at: u.email_verified_at,
        last_login_at: u.last_login_at,
        created_at: u.created_at,
        updated_at: u.updated_at,
    }
}

#[async_trait]
impl CredentialRepo for SeaCredentialRepo {
    async fn get(&self, id: Id) -> Result<Option<ApiCredential>> {
        Ok(api_credentials::Entity::find_by_id(id)
            .filter(api_credentials::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_by_user(&self, user_id: Id) -> Result<Option<ApiCredential>> {
        Ok(api_credentials::Entity::find()
            .filter(api_credentials::Column::UserId.eq(user_id))
            .filter(api_credentials::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_any_by_user(&self, user_id: Id) -> Result<Option<ApiCredential>> {
        Ok(api_credentials::Entity::find()
            .filter(api_credentials::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn find_for_auth(&self, api_key: &str) -> Result<Option<AuthCredential>> {
        if api_key.is_empty() {
            return Ok(None);
        }
        let Some(row) = api_credentials::Entity::find()
            .filter(api_credentials::Column::ApiKey.eq(api_key))
            .filter(api_credentials::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let user_status = self
            .users(&[row.user_id])
            .await?
            .remove(&row.user_id)
            .map(|u| u.status);
        Ok(Some(AuthCredential {
            credential: to_domain(row),
            user_status,
        }))
    }

    async fn create(
        &self,
        user_id: Id,
        api_key: &str,
        now: DateTime<Utc>,
    ) -> Result<ApiCredential> {
        let row = api_credentials::ActiveModel {
            user_id: Set(user_id),
            api_key: Set(api_key.to_owned()),
            api_secret: Set(String::new()),
            status: Set(CredentialStatus::PendingReview.as_str().to_owned()),
            reject_reason: Set(String::new()),
            approved_at: Set(None),
            is_active: Set(false),
            last_used_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_domain(row))
    }

    async fn save(&self, c: &ApiCredential, now: DateTime<Utc>) -> Result<()> {
        api_credentials::ActiveModel {
            id: Set(c.id),
            api_key: Set(c.api_key.clone()),
            api_secret: Set(c.api_secret.clone()),
            status: Set(c.status.as_str().to_owned()),
            reject_reason: Set(c.reject_reason.clone()),
            approved_at: Set(c.approved_at),
            is_active: Set(c.is_active),
            last_used_at: Set(c.last_used_at),
            updated_at: Set(now),
            deleted_at: Set(if c.deleted { Some(now) } else { None }),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn touch(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
        api_credentials::Entity::update_many()
            .col_expr(api_credentials::Column::LastUsedAt, Expr::value(at))
            .filter(api_credentials::Column::Id.eq(id))
            .filter(api_credentials::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
        api_credentials::Entity::update_many()
            .col_expr(api_credentials::Column::DeletedAt, Expr::value(at))
            .col_expr(api_credentials::Column::IsActive, Expr::value(false))
            .filter(api_credentials::Column::Id.eq(id))
            .filter(api_credentials::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn list(&self, filter: &CredentialFilter) -> Result<Page<ApiCredential>> {
        let mut q =
            api_credentials::Entity::find().filter(api_credentials::Column::DeletedAt.is_null());
        if !filter.status.trim().is_empty() {
            q = q.filter(api_credentials::Column::Status.eq(filter.status.trim()));
        }
        if filter.user_id > 0 {
            q = q.filter(api_credentials::Column::UserId.eq(filter.user_id));
        }
        let search = filter.search.trim();
        if !search.is_empty() {
            let pattern = contains_pattern(search);
            let sub = Query::select()
                .column(users::Column::Id)
                .from(users::Entity)
                .cond_where(
                    Condition::any()
                        .add(
                            Expr::expr(Func::lower(Expr::col(users::Column::Email)))
                                .like(pattern.clone()),
                        )
                        .add(
                            Expr::expr(Func::lower(Expr::col(users::Column::DisplayName)))
                                .like(pattern),
                        ),
                )
                .to_owned();
            q = q.filter(api_credentials::Column::UserId.in_subquery(sub));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(api_credentials::Column::CreatedAt)
            .order_by_desc(api_credentials::Column::Id)
            .offset(filter.page.offset())
            .limit(filter.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = rows.iter().map(|r| r.user_id).collect();
        let owners = self.users(&ids).await?;
        let items = rows
            .into_iter()
            .map(|r| {
                let owner = owners.get(&r.user_id).map(user_of);
                let mut c = to_domain(r);
                c.user = owner;
                c
            })
            .collect();
        Ok(Page { items, total })
    }
}
