//! [`NotificationLogRepo`] backed by `notification_logs`.

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use serde_json::Value;
use zs_domain::Result;
use zs_domain::notify::log::{LogFilter, NewNotificationLog, NotificationLog, NotificationLogRepo};
use zs_shared::page::Page;

use crate::db::entity::notification_logs;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`NotificationLogRepo`].
#[derive(Debug, Clone)]
pub struct SeaNotificationLogRepo {
    db: DatabaseConnection,
}

impl SeaNotificationLogRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: notification_logs::Model) -> NotificationLog {
    NotificationLog {
        id: m.id,
        event_type: m.event_type,
        biz_type: m.biz_type,
        biz_id: m.biz_id,
        channel: m.channel,
        recipient: m.recipient,
        locale: m.locale,
        title: m.title,
        body: m.body,
        status: m.status,
        error_message: m.error_message,
        is_test: m.is_test,
        variables: m
            .variables_json
            .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
        created_at: m.created_at,
    }
}

#[async_trait]
impl NotificationLogRepo for SeaNotificationLogRepo {
    async fn create(&self, log: &NewNotificationLog) -> Result<()> {
        notification_logs::ActiveModel {
            event_type: Set(log.event_type.clone()),
            biz_type: Set(log.biz_type.clone()),
            biz_id: Set(log.biz_id),
            channel: Set(log.channel.clone()),
            recipient: Set(log.recipient.clone()),
            locale: Set(log.locale.clone()),
            title: Set(log.title.clone()),
            body: Set(log.body.clone()),
            status: Set(log.status.clone()),
            error_message: Set(log.error_message.clone()),
            is_test: Set(log.is_test),
            variables_json: Set(Some(log.variables.clone())),
            created_at: Set(log.created_at),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn list(&self, f: &LogFilter) -> Result<Page<NotificationLog>> {
        use notification_logs::Column as C;
        let mut q = notification_logs::Entity::find();
        if !f.channel.is_empty() {
            q = q.filter(C::Channel.eq(f.channel.as_str()));
        }
        if !f.status.is_empty() {
            q = q.filter(C::Status.eq(f.status.as_str()));
        }
        if !f.event_type.is_empty() {
            q = q.filter(C::EventType.eq(f.event_type.as_str()));
        }
        if let Some(t) = f.is_test {
            q = q.filter(C::IsTest.eq(t));
        }
        if let Some(t) = f.created_from {
            q = q.filter(C::CreatedAt.gte(t));
        }
        if let Some(t) = f.created_to {
            q = q.filter(C::CreatedAt.lte(t));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
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
}
