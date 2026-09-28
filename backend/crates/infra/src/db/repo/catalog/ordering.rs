//! [`CatalogOrdering`]: stock movements and card-secret reservation for the order group.
//!
//! The `*_in` functions take any connection so the order group can run them inside its
//! own transaction (DB-01); the trait methods wrap each call in a transaction of its own.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, LockBehavior, LockType};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use zs_domain::catalog::card_secret::{CardSecret, SecretStatus, keys as secret_keys};
use zs_domain::catalog::ordering::{CatalogOrdering, StockMove, StockTarget};
use zs_domain::catalog::product::{Product, SkuScope, keys};
use zs_domain::catalog::stock::MANUAL_STOCK_UNLIMITED;
use zs_domain::{Error, Id, Result};

use super::card_secret::secret_to_domain;
use super::product::load_product;
use crate::db::entity::{card_secrets, product_skus, products};
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`CatalogOrdering`].
#[derive(Debug, Clone)]
pub struct SeaCatalogOrdering {
    db: DatabaseConnection,
}

impl SeaCatalogOrdering {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Builds the conditional manual-stock UPDATE for one table.
macro_rules! manual_stock_update {
    ($conn:expr, $entity:ident, $id:expr, $movement:expr, $q:expr) => {{
        use $entity as t;
        let q = $q;
        let base = t::Entity::update_many()
            .filter(t::Column::Id.eq($id))
            .filter(t::Column::DeletedAt.is_null())
            .filter(t::Column::ManualStockTotal.gte(0));
        let stmt = match $movement {
            StockMove::Reserve => base
                .filter(t::Column::ManualStockTotal.gte(q))
                .col_expr(t::Column::ManualStockTotal, Expr::col(t::Column::ManualStockTotal).sub(q))
                .col_expr(t::Column::ManualStockLocked, Expr::col(t::Column::ManualStockLocked).add(q)),
            StockMove::Release => base
                .filter(t::Column::ManualStockLocked.gte(q))
                .col_expr(t::Column::ManualStockTotal, Expr::col(t::Column::ManualStockTotal).add(q))
                .col_expr(t::Column::ManualStockLocked, Expr::col(t::Column::ManualStockLocked).sub(q)),
            StockMove::Consume => base
                .filter(
                    Condition::any()
                        .add(t::Column::ManualStockLocked.gte(q))
                        .add(Expr::cust(format!("manual_stock_total >= ({q} - manual_stock_locked)"))),
                )
                .col_expr(
                    t::Column::ManualStockTotal,
                    Expr::cust(format!(
                        "manual_stock_total - CASE WHEN manual_stock_locked >= {q} THEN 0 ELSE {q} - manual_stock_locked END"
                    )),
                )
                .col_expr(
                    t::Column::ManualStockLocked,
                    Expr::cust(format!(
                        "CASE WHEN manual_stock_locked >= {q} THEN manual_stock_locked - {q} ELSE 0 END"
                    )),
                )
                .col_expr(t::Column::ManualStockSold, Expr::col(t::Column::ManualStockSold).add(q)),
        };
        let affected = stmt.exec($conn).await.dom()?.rows_affected;
        let unlimited = if affected == 0 {
            t::Entity::find_by_id($id)
                .filter(t::Column::DeletedAt.is_null())
                .one($conn)
                .await
                .dom()?
                .is_some_and(|m| m.manual_stock_total == MANUAL_STOCK_UNLIMITED)
        } else {
            false
        };
        (affected, unlimited)
    }};
}

/// Applies a manual stock movement with a conditional UPDATE (ORD-03, ORD-06).
///
/// Unlimited rows are never modified and succeed with 0 affected rows; otherwise
/// Reserve/Consume matching no row fail with `error.manual_stock_insufficient`.
pub async fn manual_stock_in<C: ConnectionTrait>(
    conn: &C,
    target: StockTarget,
    movement: StockMove,
    quantity: i32,
) -> Result<u64> {
    if quantity <= 0 {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
    }
    let (affected, unlimited) = match target {
        StockTarget::Product(id) => manual_stock_update!(conn, products, id, movement, quantity),
        StockTarget::Sku(id) => manual_stock_update!(conn, product_skus, id, movement, quantity),
    };
    if affected == 0 && !unlimited && movement != StockMove::Release {
        return Err(Error::bad_request(keys::MANUAL_STOCK_INSUFFICIENT));
    }
    Ok(affected)
}

/// Reserves `quantity` available secrets (oldest first) for an order (DLV-03).
pub async fn reserve_secrets_in<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    sku_id: Id,
    quantity: u64,
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<Vec<Id>> {
    let insufficient = || Error::bad_request(secret_keys::INSUFFICIENT);
    if product_id <= 0 || quantity == 0 || order_id <= 0 {
        return Err(insufficient());
    }
    let mut q = card_secrets::Entity::find()
        .select_only()
        .column(card_secrets::Column::Id)
        .filter(card_secrets::Column::ProductId.eq(product_id))
        .filter(card_secrets::Column::Status.eq(SecretStatus::Available.as_str()))
        .filter(card_secrets::Column::DeletedAt.is_null());
    if sku_id > 0 {
        q = q.filter(card_secrets::Column::SkuId.eq(sku_id));
    }
    let ids: Vec<Id> = q
        .order_by_asc(card_secrets::Column::Id)
        .limit(quantity)
        .lock_with_behavior(LockType::Update, LockBehavior::SkipLocked)
        .into_tuple()
        .all(conn)
        .await
        .dom()?;
    if (ids.len() as u64) < quantity {
        return Err(insufficient());
    }
    let affected = card_secrets::Entity::update_many()
        .col_expr(
            card_secrets::Column::Status,
            Expr::value(SecretStatus::Reserved.as_str()),
        )
        .col_expr(card_secrets::Column::OrderId, Expr::value(order_id))
        .col_expr(card_secrets::Column::ReservedAt, Expr::value(now))
        .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
        .filter(card_secrets::Column::Id.is_in(ids.clone()))
        .filter(card_secrets::Column::Status.eq(SecretStatus::Available.as_str()))
        .filter(card_secrets::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if affected != ids.len() as u64 {
        return Err(insufficient());
    }
    Ok(ids)
}

/// Returns an order's reserved secrets to stock.
pub async fn release_secrets_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<u64> {
    if order_id <= 0 {
        return Ok(0);
    }
    Ok(card_secrets::Entity::update_many()
        .col_expr(
            card_secrets::Column::Status,
            Expr::value(SecretStatus::Available.as_str()),
        )
        .col_expr(
            card_secrets::Column::OrderId,
            Expr::value(Option::<Id>::None),
        )
        .col_expr(
            card_secrets::Column::ReservedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
        .filter(card_secrets::Column::OrderId.eq(order_id))
        .filter(card_secrets::Column::Status.eq(SecretStatus::Reserved.as_str()))
        .filter(card_secrets::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?
        .rows_affected)
}

/// Marks secrets used by an order; every id must flip.
pub async fn mark_secrets_used_in<C: ConnectionTrait>(
    conn: &C,
    ids: &[Id],
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<u64> {
    if ids.is_empty() || order_id <= 0 {
        return Ok(0);
    }
    let affected = card_secrets::Entity::update_many()
        .col_expr(
            card_secrets::Column::Status,
            Expr::value(SecretStatus::Used.as_str()),
        )
        .col_expr(card_secrets::Column::OrderId, Expr::value(order_id))
        .col_expr(card_secrets::Column::UsedAt, Expr::value(now))
        .col_expr(
            card_secrets::Column::ReservedAt,
            Expr::value(Option::<DateTime<Utc>>::None),
        )
        .col_expr(card_secrets::Column::UpdatedAt, Expr::value(now))
        .filter(card_secrets::Column::Id.is_in(ids.to_vec()))
        .filter(card_secrets::Column::Status.is_in([
            SecretStatus::Available.as_str(),
            SecretStatus::Reserved.as_str(),
        ]))
        .filter(
            Condition::any()
                .add(card_secrets::Column::OrderId.is_null())
                .add(card_secrets::Column::OrderId.eq(order_id)),
        )
        .filter(card_secrets::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if affected != ids.len() as u64 {
        return Err(Error::bad_request(secret_keys::INSUFFICIENT));
    }
    Ok(affected)
}

#[async_trait]
impl CatalogOrdering for SeaCatalogOrdering {
    async fn orderable_product(&self, product_id: Id) -> Result<Option<Product>> {
        load_product(&self.db, product_id, SkuScope::Active, true).await
    }

    async fn move_manual_stock(
        &self,
        target: StockTarget,
        movement: StockMove,
        quantity: i32,
    ) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let affected = manual_stock_in(&txn, target, movement, quantity).await?;
        txn.commit().await.dom()?;
        Ok(affected)
    }

    async fn reserve_secrets(
        &self,
        product_id: Id,
        sku_id: Id,
        quantity: u64,
        order_id: Id,
        now: DateTime<Utc>,
    ) -> Result<Vec<Id>> {
        let txn = self.db.begin().await.dom()?;
        let ids = reserve_secrets_in(&txn, product_id, sku_id, quantity, order_id, now).await?;
        txn.commit().await.dom()?;
        Ok(ids)
    }

    async fn release_secrets(&self, order_id: Id, now: DateTime<Utc>) -> Result<u64> {
        release_secrets_in(&self.db, order_id, now).await
    }

    async fn mark_secrets_used(&self, ids: &[Id], order_id: Id, now: DateTime<Utc>) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let affected = mark_secrets_used_in(&txn, ids, order_id, now).await?;
        txn.commit().await.dom()?;
        Ok(affected)
    }

    async fn order_secrets(&self, order_id: Id, status: Option<&str>) -> Result<Vec<CardSecret>> {
        let mut q = card_secrets::Entity::find()
            .filter(card_secrets::Column::OrderId.eq(order_id))
            .filter(card_secrets::Column::DeletedAt.is_null());
        if let Some(status) = status.filter(|s| !s.is_empty()) {
            q = q.filter(card_secrets::Column::Status.eq(status));
        }
        Ok(q.order_by_asc(card_secrets::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(secret_to_domain)
            .collect())
    }
}
