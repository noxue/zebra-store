//! [`BroadcastRepo`] backed by `telegram_broadcasts`, plus the Telegram
//! recipient directory (`user_oauth_identities` ⋈ `users`).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use serde_json::Value;
use zs_domain::notify::broadcast::{
    Broadcast, BroadcastFilter, BroadcastRepo, TelegramUser, TelegramUserQuery,
};
use zs_domain::notify::channel::PROVIDER_TELEGRAM;
use zs_domain::{Id, Result};
use zs_shared::page::Page;

use crate::db::entity::{telegram_broadcasts, user_oauth_identities, users};
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`BroadcastRepo`].
#[derive(Debug, Clone)]
pub struct SeaBroadcastRepo {
    db: DatabaseConnection,
}

impl SeaBroadcastRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: telegram_broadcasts::Model) -> Broadcast {
    let chat_ids = m
        .recipient_chat_ids
        .and_then(|v| serde_json::from_value::<Vec<Value>>(v).ok())
        .unwrap_or_default()
        .into_iter()
        .map(|v| match v {
            Value::String(s) => s,
            other => other.to_string(),
        })
        .collect();
    Broadcast {
        id: m.id,
        title: m.title,
        recipient_type: m.recipient_type,
        filters: m
            .filters_json
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        recipient_chat_ids: chat_ids,
        recipient_count: m.recipient_count,
        success_count: m.success_count,
        failed_count: m.failed_count,
        status: m.status,
        message_html: m.message_html,
        attachment_url: m.attachment_url,
        attachment_name: m.attachment_name,
        started_at: m.started_at,
        completed_at: m.completed_at,
        last_error: m.last_error,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(&needle.to_lowercase())
}

#[async_trait]
impl BroadcastRepo for SeaBroadcastRepo {
    async fn create(&self, b: &Broadcast) -> Result<Broadcast> {
        let row = telegram_broadcasts::ActiveModel {
            title: Set(b.title.clone()),
            recipient_type: Set(b.recipient_type.clone()),
            filters_json: Set(Some(b.filters.clone())),
            recipient_chat_ids: Set(Some(serde_json::to_value(&b.recipient_chat_ids)?)),
            recipient_count: Set(b.recipient_count),
            success_count: Set(b.success_count),
            failed_count: Set(b.failed_count),
            status: Set(b.status.clone()),
            message_html: Set(b.message_html.clone()),
            attachment_url: Set(b.attachment_url.clone()),
            attachment_name: Set(b.attachment_name.clone()),
            started_at: Set(b.started_at),
            completed_at: Set(b.completed_at),
            last_error: Set(b.last_error.clone()),
            created_at: Set(b.created_at),
            updated_at: Set(b.updated_at),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_domain(row))
    }

    async fn get(&self, id: Id) -> Result<Option<Broadcast>> {
        let row = telegram_broadcasts::Entity::find_by_id(id)
            .filter(telegram_broadcasts::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn list(&self, f: &BroadcastFilter) -> Result<Page<Broadcast>> {
        use telegram_broadcasts::Column as C;
        let mut q = telegram_broadcasts::Entity::find().filter(C::DeletedAt.is_null());
        if !f.keyword.trim().is_empty() {
            let like = format!("%{}%", f.keyword.trim());
            q = q.filter(
                Condition::any()
                    .add(C::Title.like(like.clone()))
                    .add(C::MessageHtml.like(like)),
            );
        }
        if !f.recipient_type.trim().is_empty() {
            q = q.filter(C::RecipientType.eq(f.recipient_type.trim()));
        }
        if !f.status.trim().is_empty() {
            q = q.filter(C::Status.eq(f.status.trim()));
        }
        if let Some(t) = f.created_from {
            q = q.filter(C::CreatedAt.gte(t));
        }
        if let Some(t) = f.created_to {
            q = q.filter(C::CreatedAt.lte(t));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(C::CreatedAt)
            .order_by_desc(C::Id)
            .offset(f.page.offset())
            .limit(f.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_domain).collect(),
            total,
        })
    }

    async fn update(&self, b: &Broadcast) -> Result<()> {
        use telegram_broadcasts::Column as C;
        telegram_broadcasts::Entity::update_many()
            .col_expr(C::Status, Expr::value(b.status.clone()))
            .col_expr(C::SuccessCount, Expr::value(b.success_count))
            .col_expr(C::FailedCount, Expr::value(b.failed_count))
            .col_expr(C::StartedAt, Expr::value(b.started_at))
            .col_expr(C::CompletedAt, Expr::value(b.completed_at))
            .col_expr(C::LastError, Expr::value(b.last_error.clone()))
            .col_expr(C::UpdatedAt, Expr::value(b.updated_at))
            .filter(C::Id.eq(b.id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn soft_delete(&self, id: Id, at: DateTime<Utc>) -> Result<bool> {
        use telegram_broadcasts::Column as C;
        let res = telegram_broadcasts::Entity::update_many()
            .col_expr(C::DeletedAt, Expr::value(Some(at)))
            .filter(C::Id.eq(id))
            .filter(C::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }

    async fn telegram_users(&self, q: &TelegramUserQuery) -> Result<Page<TelegramUser>> {
        use user_oauth_identities::Column as I;
        let mut iq =
            user_oauth_identities::Entity::find().filter(I::Provider.eq(PROVIDER_TELEGRAM));
        if !q.user_ids.is_empty() {
            iq = iq.filter(I::UserId.is_in(q.user_ids.clone()));
        }
        if let Some(t) = q.created_from {
            iq = iq.filter(I::CreatedAt.gte(t));
        }
        if let Some(t) = q.created_to {
            iq = iq.filter(I::CreatedAt.lte(t));
        }
        let identities = iq
            .order_by_desc(I::CreatedAt)
            .order_by_desc(I::Id)
            .all(&self.db)
            .await
            .dom()?;
        let user_ids: Vec<Id> = identities.iter().map(|i| i.user_id).collect();
        let people: HashMap<Id, users::Model> = if user_ids.is_empty() {
            HashMap::new()
        } else {
            users::Entity::find()
                .filter(users::Column::Id.is_in(user_ids))
                .filter(users::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?
                .into_iter()
                .map(|u| (u.id, u))
                .collect()
        };
        let keyword = q.keyword.trim();
        let matched: Vec<TelegramUser> = identities
            .into_iter()
            .filter_map(|i| {
                let u = people.get(&i.user_id)?;
                let keep = (keyword.is_empty()
                    || contains_ci(&u.display_name, keyword)
                    || contains_ci(&i.username, keyword)
                    || contains_ci(&i.provider_user_id, keyword))
                    && (q.display_name.trim().is_empty()
                        || contains_ci(&u.display_name, q.display_name.trim()))
                    && (q.telegram_username.trim().is_empty()
                        || contains_ci(&i.username, q.telegram_username.trim()))
                    && (q.telegram_user_id.trim().is_empty()
                        || contains_ci(&i.provider_user_id, q.telegram_user_id.trim()));
                keep.then(|| TelegramUser {
                    user_id: u.id,
                    display_name: u.display_name.clone(),
                    user_email: u.email.clone(),
                    telegram_username: i.username.clone(),
                    telegram_user_id: i.provider_user_id.clone(),
                    bound_at: i.created_at,
                    user_created_at: u.created_at,
                })
            })
            .collect();
        let total = matched.len() as u64;
        let items = match q.page {
            Some(p) => matched
                .into_iter()
                .skip(usize::try_from(p.offset()).unwrap_or(usize::MAX))
                .take(usize::try_from(p.page_size).unwrap_or(usize::MAX))
                .collect(),
            None => matched,
        };
        Ok(Page { items, total })
    }
}
