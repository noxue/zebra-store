//! Ports offered to the order group (ordering itself is not implemented here).
//!
//! Every decrement is a conditional UPDATE whose affected-row count is checked
//! (ORD-03, ORD-06, DLV-03); rows with `manual_stock_total = -1` never take part in
//! arithmetic. Order code that already runs its own transaction can call the same
//! statements through `zs_infra::db::repo::catalog::ordering::{manual_stock_in, reserve_secrets_in, …}`
//! with its transaction handle (DB-01).

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::card_secret::CardSecret;
use super::product::Product;
use crate::{Id, Result};

/// Which row carries the manual stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockTarget {
    Product(Id),
    Sku(Id),
}

/// Manual stock movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockMove {
    /// Order created: `total -= q, locked += q` where `total >= q` (total ≥ 0).
    Reserve,
    /// Order canceled/expired: `total += q, locked -= q` where `locked >= q` (total ≥ 0).
    Release,
    /// Order paid: consume the lock (or the missing part from `total` for legacy
    /// unreserved orders), `sold += q`.
    Consume,
}

/// Catalog operations needed to place, pay and deliver orders.
#[async_trait]
pub trait CatalogOrdering: Send + Sync {
    /// A sellable product: not deleted, active, in an active category; with active SKUs (ORD-09).
    async fn orderable_product(&self, product_id: Id) -> Result<Option<Product>>;

    /// Applies a manual stock movement. Reserve/Consume fail with
    /// `error.manual_stock_insufficient` when the conditional update matches no row;
    /// Release is best effort (returns rows affected). Unlimited rows (`-1`) are never touched.
    async fn move_manual_stock(
        &self,
        target: StockTarget,
        movement: StockMove,
        quantity: i32,
    ) -> Result<u64>;

    /// Reserves `quantity` available secrets of a product/SKU (`sku_id` 0 = any) for an
    /// order, oldest first, with `UPDATE … WHERE status='available'`; fails with
    /// `error.card_secret_insufficient` (nothing reserved) when fewer rows were flipped.
    async fn reserve_secrets(
        &self,
        product_id: Id,
        sku_id: Id,
        quantity: u64,
        order_id: Id,
        now: DateTime<Utc>,
    ) -> Result<Vec<Id>>;

    /// Returns the order's reserved secrets to `available`.
    async fn release_secrets(&self, order_id: Id, now: DateTime<Utc>) -> Result<u64>;

    /// Marks secrets used for an order (`status IN (available, reserved)` and owned by the
    /// order or unowned); fails with `error.card_secret_insufficient` unless every id flipped.
    async fn mark_secrets_used(&self, ids: &[Id], order_id: Id, now: DateTime<Utc>) -> Result<u64>;

    /// Secrets linked to an order (optionally one status).
    async fn order_secrets(&self, order_id: Id, status: Option<&str>) -> Result<Vec<CardSecret>>;
}
