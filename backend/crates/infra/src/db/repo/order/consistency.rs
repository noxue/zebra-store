//! Shared order-family lock for procurement, refunds and delivery. Never hold it
//! across a network request: persist the pending operation before releasing it.
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use zs_domain::integration::procurement::{ProcurementStatus, maybe_executed};
use zs_domain::order::model::{Order, OrderStatus, fulfillment_type, keys};
use zs_domain::payment::refund::status as refund_status;
use zs_domain::{Error, Id, Result};

use super::map;
use crate::db::entity::{
    gateway_refund_attempts, orders, procurement_deliveries, procurement_orders,
};
use crate::db::repo::support::DbResultExt;

pub(crate) const PROCESSING: &str = "error.order_procurement_processing";

/// Parent first, then all direct children by id. All competing writers use this
/// order, including child-only operations, so parent refunds cannot race delivery.
pub(crate) async fn lock_family<C: ConnectionTrait>(conn: &C, id: Id) -> Result<Order> {
    let missing = || Error::not_found(keys::ORDER_NOT_FOUND);
    let head = orders::Entity::find_by_id(id)
        .one(conn)
        .await
        .dom()?
        .ok_or_else(missing)?;
    let root = head.parent_id.filter(|id| *id > 0).unwrap_or(id);
    orders::Entity::find_by_id(root)
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(missing)?;
    orders::Entity::find()
        .filter(orders::Column::ParentId.eq(root))
        .order_by_asc(orders::Column::Id)
        .lock_exclusive()
        .all(conn)
        .await
        .dom()?;
    map::load(conn, id).await?.ok_or_else(missing)
}

/// Called while holding the family lock, before starting/restarting procurement
/// or publishing any delivery. Partial refunds block only the refunded child.
pub(crate) async fn ensure_order_open<C: ConnectionTrait>(conn: &C, order: &Order) -> Result<()> {
    if !matches!(
        order.status,
        OrderStatus::Paid
            | OrderStatus::Fulfilling
            | OrderStatus::Delivered
            | OrderStatus::Completed
    ) || order.refunded_amount.is_positive()
    {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    let root = order.parent_id.filter(|id| *id > 0).unwrap_or(order.id);
    if root != order.id {
        let parent = orders::Entity::find_by_id(root)
            .one(conn)
            .await
            .dom()?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        if matches!(parent.status.as_str(), "refunded" | "canceled") {
            return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
        }
    }
    if gateway_refund_attempts::Entity::find()
        .filter(gateway_refund_attempts::Column::OrderId.is_in([root, order.id]))
        .filter(gateway_refund_attempts::Column::Status.is_in([
            refund_status::PREPARING,
            refund_status::PENDING,
            refund_status::MANUAL_PREPARING,
            refund_status::MANUAL_LOCKING,
            refund_status::MANUAL_AWAITING,
            refund_status::MANUAL_CANCELING,
        ]))
        .one(conn)
        .await
        .dom()?
        .is_some()
    {
        return Err(Error::bad_request("error.gateway_refund_pending"));
    }
    Ok(())
}

/// A new purchase/retry must also be undelivered; completed orders are only
/// allowed through the idempotent delivery path, never through procurement.
pub(crate) async fn ensure_procurement_open<C: ConnectionTrait>(
    conn: &C,
    order: &Order,
) -> Result<()> {
    ensure_order_open(conn, order).await?;
    if !matches!(order.status, OrderStatus::Paid | OrderStatus::Fulfilling)
        || order
            .fulfillment
            .as_ref()
            .is_some_and(|f| f.status == "delivered")
    {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    Ok(())
}

/// A refund must not release funds while the supplier can still charge/deliver.
/// The caller holds the family lock for monetary writes; read-only availability
/// calls are advisory and the transaction repeats this check.
pub(crate) async fn ensure_refundable<C: ConnectionTrait>(conn: &C, order: &Order) -> Result<()> {
    let targets: Vec<&Order> = if order.children.is_empty() {
        vec![order]
    } else {
        order.children.iter().collect()
    };
    for target in targets {
        let rows = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::LocalOrderId.eq(target.id))
            .all(conn)
            .await
            .dom()?;
        let delivered = target
            .fulfillment
            .as_ref()
            .is_some_and(|f| f.status == "delivered");
        if rows.is_empty()
            && !delivered
            && target
                .items
                .iter()
                .any(|i| i.fulfillment_type == fulfillment_type::UPSTREAM)
        {
            return Err(Error::bad_request(PROCESSING));
        }
        for row in rows {
            let held = procurement_deliveries::Entity::find()
                .filter(procurement_deliveries::Column::ProcurementOrderId.eq(row.id))
                .one(conn)
                .await
                .dom()?
                .is_some();
            let safe = match ProcurementStatus::parse(&row.status) {
                Some(ProcurementStatus::Fulfilled | ProcurementStatus::Completed) => delivered,
                Some(ProcurementStatus::Refunded) => true,
                Some(ProcurementStatus::Rejected) => {
                    row.upstream_order_id == 0
                        && row.upstream_order_no.is_empty()
                        && !held
                        && row.next_retry_at.is_none()
                        && !maybe_executed(&row.error_message)
                }
                Some(ProcurementStatus::Canceled) => {
                    !held
                        && row.next_retry_at.is_none()
                        && !maybe_executed(&row.error_message)
                        && (row.error_message == "supplier cancellation confirmed"
                            || (row.upstream_order_id == 0
                                && row.upstream_order_no.is_empty()
                                && row.error_message == "manually canceled before submission"))
                }
                _ => false,
            };
            if !safe {
                return Err(Error::bad_request(PROCESSING));
            }
        }
    }
    Ok(())
}
