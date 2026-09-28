//! [`CartRepo`] backed by `cart_items` (rows are hard-deleted so the unique key
//! `(user_id, product_id, sku_id)` can always be reused, ORD-11).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};
use zs_domain::order::ports::{CartRepo, CartRow};
use zs_domain::{Id, Result};

use crate::db::entity::cart_items;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`CartRepo`].
#[derive(Debug, Clone)]
pub struct SeaCartRepo {
    db: DatabaseConnection,
}

impl SeaCartRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CartRepo for SeaCartRepo {
    async fn list(&self, user_id: Id) -> Result<Vec<CartRow>> {
        Ok(cart_items::Entity::find()
            .filter(cart_items::Column::UserId.eq(user_id))
            .filter(cart_items::Column::DeletedAt.is_null())
            .order_by_asc(cart_items::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|r| CartRow {
                product_id: r.product_id,
                sku_id: r.sku_id,
                quantity: r.quantity,
                fulfillment_type: r.fulfillment_type,
            })
            .collect())
    }

    async fn upsert(&self, user_id: Id, row: &CartRow, now: DateTime<Utc>) -> Result<()> {
        let existing = cart_items::Entity::find()
            .filter(cart_items::Column::UserId.eq(user_id))
            .filter(cart_items::Column::ProductId.eq(row.product_id))
            .filter(cart_items::Column::SkuId.eq(row.sku_id))
            .one(&self.db)
            .await
            .dom()?;
        match existing {
            Some(e) => {
                cart_items::Entity::update_many()
                    .col_expr(cart_items::Column::Quantity, Expr::value(row.quantity))
                    .col_expr(
                        cart_items::Column::FulfillmentType,
                        Expr::value(row.fulfillment_type.clone()),
                    )
                    .col_expr(
                        cart_items::Column::DeletedAt,
                        Expr::value(Option::<DateTime<Utc>>::None),
                    )
                    .col_expr(cart_items::Column::UpdatedAt, Expr::value(now))
                    .filter(cart_items::Column::Id.eq(e.id))
                    .exec(&self.db)
                    .await
                    .dom()?;
            }
            None => {
                cart_items::ActiveModel {
                    user_id: Set(user_id),
                    product_id: Set(row.product_id),
                    sku_id: Set(row.sku_id),
                    quantity: Set(row.quantity),
                    fulfillment_type: Set(row.fulfillment_type.clone()),
                    created_at: Set(now),
                    updated_at: Set(now),
                    deleted_at: Set(None),
                    ..Default::default()
                }
                .insert(&self.db)
                .await
                .dom()?;
            }
        }
        Ok(())
    }

    async fn delete(&self, user_id: Id, product_id: Id, sku_id: Id) -> Result<()> {
        cart_items::Entity::delete_many()
            .filter(cart_items::Column::UserId.eq(user_id))
            .filter(cart_items::Column::ProductId.eq(product_id))
            .filter(cart_items::Column::SkuId.eq(sku_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
