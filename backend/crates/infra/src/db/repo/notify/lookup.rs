//! [`ChannelLookup`] over `products`, `payment_channels`, `orders` and
//! `order_items` (consumer-side port; read only).

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use zs_domain::notify::channel::{ChannelLookup, PaymentMethod};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{order_items, orders, payment_channels, products};
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`ChannelLookup`].
#[derive(Debug, Clone)]
pub struct SeaChannelLookup {
    db: DatabaseConnection,
}

impl SeaChannelLookup {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl ChannelLookup for SeaChannelLookup {
    async fn product_slug(&self, id: Id) -> Result<Option<String>> {
        Ok(products::Entity::find_by_id(id)
            .filter(products::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(|p| p.slug))
    }

    async fn category_product_counts(&self) -> Result<HashMap<Id, i64>> {
        let rows: Vec<(Id, i64)> = products::Entity::find()
            .select_only()
            .column(products::Column::CategoryId)
            .column_as(products::Column::Id.count(), "total")
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::IsActive.eq(true))
            .group_by(products::Column::CategoryId)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().collect())
    }

    async fn active_payment_channels(&self, limit: u64) -> Result<Vec<PaymentMethod>> {
        let rows = payment_channels::Entity::find()
            .filter(payment_channels::Column::DeletedAt.is_null())
            .filter(payment_channels::Column::IsActive.eq(true))
            .order_by_desc(payment_channels::Column::SortOrder)
            .order_by_asc(payment_channels::Column::Id)
            .limit(limit)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|c| PaymentMethod {
                id: c.id,
                name: c.name,
                provider_type: c.provider_type,
                channel_type: c.channel_type,
                interaction_mode: c.interaction_mode,
                fee_rate: Amount::new(c.fee_rate),
                fixed_fee: Amount::new(c.fixed_fee),
            })
            .collect())
    }

    async fn order_product_ids(&self, user_id: Id, order_no: &str) -> Result<Option<Vec<Id>>> {
        let Some(order) = orders::Entity::find()
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::OrderNo.eq(order_no.trim()))
            .filter(orders::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let mut order_ids = vec![order.id];
        let children: Vec<Id> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::ParentId.eq(order.id))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        order_ids.extend(children);
        let mut ids: Vec<Id> = order_items::Entity::find()
            .select_only()
            .column(order_items::Column::ProductId)
            .filter(order_items::Column::DeletedAt.is_null())
            .filter(order_items::Column::OrderId.is_in(order_ids))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        ids.retain(|id| *id > 0);
        ids.sort_unstable();
        ids.dedup();
        Ok(Some(ids))
    }

    async fn product_payment_channel_ids(&self, product_ids: &[Id]) -> Result<Vec<String>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        products::Entity::find()
            .select_only()
            .column(products::Column::PaymentChannelIds)
            .filter(products::Column::Id.is_in(product_ids.to_vec()))
            .filter(products::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()
    }
}
