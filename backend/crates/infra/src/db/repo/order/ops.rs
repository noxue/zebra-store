//! Order operations shared by the transactional store methods; every function takes the
//! caller's transaction handle (DB-01).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use zs_domain::catalog::ordering::{StockMove, StockTarget};
use zs_domain::order::model::{Order, OrderItem, OrderStatus, fulfillment_type};
use zs_domain::order::status::{calc_parent_status, should_mark_fulfilling};
use zs_domain::payment::types::PaymentStatus;
use zs_domain::wallet::model::txn_type;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use super::map;
use super::wallet;
use crate::db::entity::{orders, payments};
use crate::db::repo::catalog::ordering::{manual_stock_in, release_secrets_in};
use crate::db::repo::marketing::coupon::release_in as release_coupon_in;
use crate::db::repo::support::DbResultExt;

/// Writes a status (plus `updated_at`) on one order.
pub(crate) async fn write_status<C: ConnectionTrait>(
    conn: &C,
    id: Id,
    status: OrderStatus,
    now: DateTime<Utc>,
) -> Result<()> {
    orders::Entity::update_many()
        .col_expr(orders::Column::Status, Expr::value(status.as_str()))
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(id))
        .filter(orders::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?;
    Ok(())
}

/// Manual-stock quantities of the items: by SKU, and by product for legacy SKU-less items
/// (`summarizeManualStockItems`).
fn manual_targets(items: &[OrderItem]) -> BTreeMap<StockTargetKey, i32> {
    let mut out = BTreeMap::new();
    for item in items {
        if item.fulfillment_type.trim() != fulfillment_type::MANUAL
            || item.product_id <= 0
            || item.quantity <= 0
        {
            continue;
        }
        let key = if item.sku_id > 0 {
            StockTargetKey::Sku(item.sku_id)
        } else {
            StockTargetKey::Product(item.product_id)
        };
        *out.entry(key).or_insert(0) += item.quantity;
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum StockTargetKey {
    Product(Id),
    Sku(Id),
}

impl From<StockTargetKey> for StockTarget {
    fn from(k: StockTargetKey) -> Self {
        match k {
            StockTargetKey::Product(id) => Self::Product(id),
            StockTargetKey::Sku(id) => Self::Sku(id),
        }
    }
}

/// Returns reserved manual stock of the items (best effort, unlimited rows untouched).
pub(crate) async fn release_manual_stock<C: ConnectionTrait>(
    conn: &C,
    items: &[OrderItem],
) -> Result<()> {
    for (target, qty) in manual_targets(items) {
        manual_stock_in(conn, target.into(), StockMove::Release, qty).await?;
    }
    Ok(())
}

/// Consumes manual stock of paid items; a limited row that cannot be decremented fails the
/// whole transaction (ORD-03).
pub(crate) async fn consume_manual_stock<C: ConnectionTrait>(
    conn: &C,
    items: &[OrderItem],
) -> Result<()> {
    for (target, qty) in manual_targets(items) {
        manual_stock_in(conn, target.into(), StockMove::Consume, qty).await?;
    }
    Ok(())
}

/// Open payments of the orders become `expired` (ORD-04).
pub(crate) async fn expire_open_payments<C: ConnectionTrait>(
    conn: &C,
    order_ids: &[Id],
    at: DateTime<Utc>,
) -> Result<u64> {
    if order_ids.is_empty() {
        return Ok(0);
    }
    Ok(payments::Entity::update_many()
        .col_expr(
            payments::Column::Status,
            Expr::value(PaymentStatus::Expired.as_str()),
        )
        .col_expr(payments::Column::ExpiredAt, Expr::value(Some(at)))
        .col_expr(payments::Column::UpdatedAt, Expr::value(at))
        .filter(payments::Column::DeletedAt.is_null())
        .filter(payments::Column::OrderId.is_in(order_ids.to_vec()))
        .filter(payments::Column::Status.is_in([
            PaymentStatus::Initiated.as_str(),
            PaymentStatus::Pending.as_str(),
        ]))
        .exec(conn)
        .await
        .dom()?
        .rows_affected)
}

/// Marks a locked pending order paid (`markOrderPaid`): parent `paid` (or the aggregate of
/// its children), children `paid` / `fulfilling`, manual stock consumed. Updates `order`.
pub(crate) async fn mark_paid<C: ConnectionTrait>(
    conn: &C,
    order: &mut Order,
    now: DateTime<Utc>,
) -> Result<()> {
    if order.status != OrderStatus::PendingPayment {
        return Err(Error::bad_request(
            zs_domain::order::keys::ORDER_STATUS_INVALID,
        ));
    }
    let online = (order.total_amount - order.wallet_paid_amount).non_negative();
    let changed = orders::Entity::update_many()
        .col_expr(
            orders::Column::Status,
            Expr::value(OrderStatus::Paid.as_str()),
        )
        .col_expr(orders::Column::PaidAt, Expr::value(Some(now)))
        .col_expr(
            orders::Column::OnlinePaidAmount,
            Expr::value(online.decimal()),
        )
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order.id))
        .filter(orders::Column::Status.eq(OrderStatus::PendingPayment.as_str()))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if changed != 1 {
        return Err(Error::bad_request(
            zs_domain::order::keys::ORDER_STATUS_INVALID,
        ));
    }
    order.status = OrderStatus::Paid;
    order.paid_at = Some(now);
    order.online_paid_amount = online;
    order.updated_at = now;
    if order.children.is_empty() {
        return consume_manual_stock(conn, &order.items).await;
    }
    for child in &mut order.children {
        let status = if should_mark_fulfilling(&child.items) {
            OrderStatus::Fulfilling
        } else {
            OrderStatus::Paid
        };
        orders::Entity::update_many()
            .col_expr(orders::Column::Status, Expr::value(status.as_str()))
            .col_expr(orders::Column::PaidAt, Expr::value(Some(now)))
            .col_expr(orders::Column::UpdatedAt, Expr::value(now))
            .filter(orders::Column::Id.eq(child.id))
            .exec(conn)
            .await
            .dom()?;
        consume_manual_stock(conn, &child.items).await?;
        child.status = status;
        child.paid_at = Some(now);
        child.updated_at = now;
    }
    let statuses: Vec<OrderStatus> = order.children.iter().map(|c| c.status).collect();
    let parent = calc_parent_status(&statuses, OrderStatus::Paid);
    if parent != OrderStatus::Paid {
        write_status(conn, order.id, parent, now).await?;
        order.status = parent;
    }
    Ok(())
}

/// Cancels a locked order and its children (`cancelOrderWithChildren`, ORD-01/04):
/// the status is re-checked on the locked row, reservations, coupon usages and wallet money
/// are returned and open payments expire. `order` must be the freshly locked order.
pub(crate) async fn cancel_locked<C: ConnectionTrait>(
    conn: &C,
    order: &mut Order,
    now: DateTime<Utc>,
) -> Result<()> {
    if order.status != OrderStatus::PendingPayment {
        return Err(Error::bad_request(
            zs_domain::order::keys::ORDER_CANCEL_NOT_ALLOWED,
        ));
    }
    let family = order.family_ids();
    let changed = orders::Entity::update_many()
        .col_expr(
            orders::Column::Status,
            Expr::value(OrderStatus::Canceled.as_str()),
        )
        .col_expr(orders::Column::CanceledAt, Expr::value(Some(now)))
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order.id))
        .filter(orders::Column::Status.eq(OrderStatus::PendingPayment.as_str()))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if changed != 1 {
        return Err(Error::bad_request(
            zs_domain::order::keys::ORDER_CANCEL_NOT_ALLOWED,
        ));
    }
    if !order.children.is_empty() {
        orders::Entity::update_many()
            .col_expr(
                orders::Column::Status,
                Expr::value(OrderStatus::Canceled.as_str()),
            )
            .col_expr(orders::Column::CanceledAt, Expr::value(Some(now)))
            .col_expr(orders::Column::UpdatedAt, Expr::value(now))
            .filter(orders::Column::ParentId.eq(order.id))
            .filter(orders::Column::DeletedAt.is_null())
            .exec(conn)
            .await
            .dom()?;
    }
    if order.children.is_empty() {
        release_secrets_in(conn, order.id, now).await?;
        release_manual_stock(conn, &order.items).await?;
    } else {
        for child in &order.children {
            release_secrets_in(conn, child.id, now).await?;
            release_manual_stock(conn, &child.items).await?;
        }
    }
    release_coupon_in(conn, order.id, now).await?;
    wallet::release_order_balance(conn, order, txn_type::ORDER_REFUND, "订单取消退回余额", now)
        .await?;
    expire_open_payments(conn, &family, now).await?;
    order.status = OrderStatus::Canceled;
    order.canceled_at = Some(now);
    order.wallet_paid_amount = Amount::ZERO;
    order.updated_at = now;
    for child in &mut order.children {
        child.status = OrderStatus::Canceled;
        child.canceled_at = Some(now);
        child.updated_at = now;
    }
    Ok(())
}

/// Recomputes and stores a parent's status (`SyncParentStatus`); canceled parents stay.
pub(crate) async fn sync_parent<C: ConnectionTrait>(
    conn: &C,
    parent_id: Id,
    now: DateTime<Utc>,
) -> Result<Option<OrderStatus>> {
    let Some(parent) = map::load(conn, parent_id).await? else {
        return Ok(None);
    };
    if parent.parent_id.is_some() {
        return Ok(None);
    }
    if parent.status == OrderStatus::Canceled {
        return Ok(Some(parent.status));
    }
    let statuses: Vec<OrderStatus> = parent.children.iter().map(|c| c.status).collect();
    let next = calc_parent_status(&statuses, parent.status);
    if next != parent.status {
        write_status(conn, parent.id, next, now).await?;
    }
    Ok(Some(next))
}
