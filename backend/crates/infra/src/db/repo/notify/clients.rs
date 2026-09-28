//! [`ChannelClientRepo`] backed by `channel_clients`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use zs_domain::notify::channel::{
    ChannelClient, ChannelClientRepo, NewChannelClient, STATUS_ACTIVE,
};
use zs_domain::{Id, Result};

use crate::db::entity::channel_clients;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`ChannelClientRepo`].
#[derive(Debug, Clone)]
pub struct SeaChannelClientRepo {
    db: DatabaseConnection,
}

impl SeaChannelClientRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: channel_clients::Model) -> ChannelClient {
    ChannelClient {
        id: m.id,
        name: m.name,
        channel_type: m.channel_type,
        channel_key: m.channel_key,
        channel_secret: m.channel_secret,
        bot_token: m.bot_token,
        callback_url: m.callback_url,
        status: m.status,
        description: m.description,
        last_used_at: m.last_used_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn alive() -> sea_orm::Select<channel_clients::Entity> {
    channel_clients::Entity::find().filter(channel_clients::Column::DeletedAt.is_null())
}

#[async_trait]
impl ChannelClientRepo for SeaChannelClientRepo {
    async fn create(&self, c: &NewChannelClient, now: DateTime<Utc>) -> Result<ChannelClient> {
        let row = channel_clients::ActiveModel {
            name: Set(c.name.clone()),
            channel_type: Set(c.channel_type.clone()),
            channel_key: Set(c.channel_key.clone()),
            channel_secret: Set(c.channel_secret.clone()),
            bot_token: Set(c.bot_token.clone()),
            callback_url: Set(c.callback_url.clone()),
            status: Set(STATUS_ACTIVE),
            description: Set(c.description.clone()),
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

    async fn get(&self, id: Id) -> Result<Option<ChannelClient>> {
        let row = alive()
            .filter(channel_clients::Column::Id.eq(id))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn find_by_key(&self, key: &str) -> Result<Option<ChannelClient>> {
        let row = alive()
            .filter(channel_clients::Column::ChannelKey.eq(key))
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn find_active_by_type(&self, channel_type: &str) -> Result<Option<ChannelClient>> {
        let row = alive()
            .filter(channel_clients::Column::ChannelType.eq(channel_type))
            .filter(channel_clients::Column::Status.eq(STATUS_ACTIVE))
            .order_by_asc(channel_clients::Column::Id)
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn list(&self) -> Result<Vec<ChannelClient>> {
        let rows = alive()
            .order_by_desc(channel_clients::Column::CreatedAt)
            .order_by_desc(channel_clients::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn update(&self, c: &ChannelClient, now: DateTime<Utc>) -> Result<()> {
        use channel_clients::Column as C;
        channel_clients::Entity::update_many()
            .col_expr(C::Name, Expr::value(c.name.clone()))
            .col_expr(C::Description, Expr::value(c.description.clone()))
            .col_expr(C::CallbackUrl, Expr::value(c.callback_url.clone()))
            .col_expr(C::BotToken, Expr::value(c.bot_token.clone()))
            .col_expr(C::ChannelSecret, Expr::value(c.channel_secret.clone()))
            .col_expr(C::Status, Expr::value(c.status))
            .col_expr(C::UpdatedAt, Expr::value(now))
            .filter(C::Id.eq(c.id))
            .filter(C::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn touch(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
        use channel_clients::Column as C;
        channel_clients::Entity::update_many()
            .col_expr(C::LastUsedAt, Expr::value(Some(at)))
            .filter(C::Id.eq(id))
            .filter(C::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn soft_delete(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
        use channel_clients::Column as C;
        channel_clients::Entity::update_many()
            .col_expr(C::DeletedAt, Expr::value(Some(at)))
            .filter(C::Id.eq(id))
            .filter(C::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
