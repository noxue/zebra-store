//! Auto and manual delivery (`fulfillment/application/service.go`).

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use zs_domain::catalog::card_secret::SecretStatus;
use zs_domain::integration::hooks::UpstreamDelivery;
use zs_domain::order::model::{
    Fulfillment, JsonMap, OrderStatus, fulfillment_status, fulfillment_type, item_key, keys,
};
use zs_domain::order::ports::UpstreamDelivered;
use zs_domain::{Error, Id, Result};

use super::{map, ops};
use crate::db::entity::{card_secrets, fulfillments, orders};
use crate::db::repo::catalog::ordering::mark_secrets_used_in;
use crate::db::repo::support::{DbResultExt, to_json};

async fn ensure_no_fulfillment<C: ConnectionTrait>(conn: &C, order_id: Id) -> Result<()> {
    let existing = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.eq(order_id))
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?;
    if existing.is_some() {
        return Err(Error::bad_request(keys::FULFILLMENT_EXISTS));
    }
    Ok(())
}

async fn update_status_from<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    from: &[OrderStatus],
    to: OrderStatus,
    now: DateTime<Utc>,
) -> Result<()> {
    let changed = orders::Entity::update_many()
        .col_expr(orders::Column::Status, Expr::value(to.as_str()))
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order_id))
        .filter(orders::Column::Status.is_in(from.iter().map(|s| s.as_str())))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if changed != 1 {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    Ok(())
}

/// Auto delivery of a paid (child) order (`CreateAuto`, DLV-09).
pub(crate) async fn auto_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<Fulfillment> {
    let order = map::load_locked(conn, order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    if order.parent_id.is_none() && !order.children.is_empty() {
        return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
    }
    if order.status != OrderStatus::Paid {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    if order.items.is_empty()
        || order
            .items
            .iter()
            .any(|i| i.fulfillment_type.trim() != fulfillment_type::AUTO)
    {
        return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
    }
    ensure_no_fulfillment(conn, order_id).await?;

    let reserved = card_secrets::Entity::find()
        .filter(card_secrets::Column::OrderId.eq(order_id))
        .filter(card_secrets::Column::Status.eq(SecretStatus::Reserved.as_str()))
        .filter(card_secrets::Column::DeletedAt.is_null())
        .order_by_asc(card_secrets::Column::Id)
        .all(conn)
        .await
        .dom()?;
    let mut by_key: HashMap<String, Vec<card_secrets::Model>> = HashMap::new();
    for row in reserved {
        by_key
            .entry(item_key(row.product_id, row.sku_id))
            .or_default()
            .push(row);
    }
    let mut selected: Vec<card_secrets::Model> = Vec::new();
    for item in &order.items {
        if item.product_id <= 0 || item.quantity <= 0 {
            return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
        }
        let want = usize::try_from(item.quantity).unwrap_or(0);
        let pool = by_key
            .entry(item_key(item.product_id, item.sku_id))
            .or_default();
        let take = want.min(pool.len());
        let mut picked: Vec<card_secrets::Model> = pool.drain(..take).collect();
        if picked.len() < want {
            let mut q = card_secrets::Entity::find()
                .filter(card_secrets::Column::ProductId.eq(item.product_id))
                .filter(card_secrets::Column::Status.eq(SecretStatus::Available.as_str()))
                .filter(card_secrets::Column::DeletedAt.is_null());
            if item.sku_id > 0 {
                q = q.filter(card_secrets::Column::SkuId.eq(item.sku_id));
            }
            let taken: Vec<Id> = selected.iter().chain(picked.iter()).map(|s| s.id).collect();
            if !taken.is_empty() {
                q = q.filter(card_secrets::Column::Id.is_not_in(taken));
            }
            let need = u64::try_from(want - picked.len()).unwrap_or(0);
            picked.extend(
                q.order_by_asc(card_secrets::Column::Id)
                    .limit(need)
                    .all(conn)
                    .await
                    .dom()?,
            );
        }
        if picked.len() < want {
            return Err(Error::bad_request(keys::CARD_SECRET_INSUFFICIENT));
        }
        selected.extend(picked);
    }
    let ids: Vec<Id> = selected.iter().map(|s| s.id).collect();
    mark_secrets_used_in(conn, &ids, order_id, now).await?;
    let payload = selected
        .iter()
        .map(|s| s.secret.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let row = fulfillments::ActiveModel {
        order_id: Set(order_id),
        type_: Set(fulfillment_type::AUTO.to_owned()),
        status: Set(fulfillment_status::DELIVERED.to_owned()),
        payload: Set(payload),
        logistics_json: Set(None),
        delivered_by: Set(None),
        delivered_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .map_err(|e| Error::internal(e).or_internal(keys::FULFILLMENT_CREATE_FAILED))?;
    update_status_from(
        conn,
        order_id,
        &[OrderStatus::Paid],
        OrderStatus::Completed,
        now,
    )
    .await?;
    Ok(map::fulfillment_to_domain(row))
}

/// Starts an auto fulfillment that requires an external card conversion. Inventory
/// reservation is committed before the network call, while no buyer-visible payload is
/// written and the order remains in the processing state.
pub(crate) async fn prepare_auto_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<String> {
    let order = map::load_locked(conn, order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    if order.parent_id.is_none() && !order.children.is_empty() {
        return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
    }
    if order.items.is_empty()
        || order
            .items
            .iter()
            .any(|i| i.fulfillment_type.trim() != fulfillment_type::AUTO)
    {
        return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
    }

    if order.status == OrderStatus::Fulfilling {
        let pending = fulfillments::Entity::find()
            .filter(fulfillments::Column::OrderId.eq(order_id))
            .filter(fulfillments::Column::Status.eq(fulfillment_status::PENDING))
            .filter(fulfillments::Column::DeletedAt.is_null())
            .one(conn)
            .await
            .dom()?;
        if pending.is_none() {
            return Err(Error::bad_request(keys::FULFILLMENT_EXISTS));
        }
        let used = card_secrets::Entity::find()
            .filter(card_secrets::Column::OrderId.eq(order_id))
            .filter(card_secrets::Column::Status.eq(SecretStatus::Used.as_str()))
            .filter(card_secrets::Column::DeletedAt.is_null())
            .order_by_asc(card_secrets::Column::Id)
            .all(conn)
            .await
            .dom()?;
        let expected: usize = order
            .items
            .iter()
            .map(|item| usize::try_from(item.quantity).unwrap_or(0))
            .sum();
        if used.len() != expected || expected == 0 {
            return Err(Error::internal_msg(
                "pending card conversion inventory is incomplete",
            ));
        }
        return Ok(used
            .iter()
            .map(|row| row.secret.as_str())
            .collect::<Vec<_>>()
            .join("\n"));
    }
    if order.status != OrderStatus::Paid {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    ensure_no_fulfillment(conn, order_id).await?;

    let reserved = card_secrets::Entity::find()
        .filter(card_secrets::Column::OrderId.eq(order_id))
        .filter(card_secrets::Column::Status.eq(SecretStatus::Reserved.as_str()))
        .filter(card_secrets::Column::DeletedAt.is_null())
        .order_by_asc(card_secrets::Column::Id)
        .all(conn)
        .await
        .dom()?;
    let mut by_key: HashMap<String, Vec<card_secrets::Model>> = HashMap::new();
    for row in reserved {
        by_key
            .entry(item_key(row.product_id, row.sku_id))
            .or_default()
            .push(row);
    }
    let mut selected: Vec<card_secrets::Model> = Vec::new();
    for item in &order.items {
        if item.product_id <= 0 || item.quantity <= 0 {
            return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
        }
        let want = usize::try_from(item.quantity).unwrap_or(0);
        let pool = by_key
            .entry(item_key(item.product_id, item.sku_id))
            .or_default();
        let mut picked: Vec<card_secrets::Model> = pool.drain(..want.min(pool.len())).collect();
        if picked.len() < want {
            let mut query = card_secrets::Entity::find()
                .filter(card_secrets::Column::ProductId.eq(item.product_id))
                .filter(card_secrets::Column::Status.eq(SecretStatus::Available.as_str()))
                .filter(card_secrets::Column::DeletedAt.is_null());
            if item.sku_id > 0 {
                query = query.filter(card_secrets::Column::SkuId.eq(item.sku_id));
            }
            let taken: Vec<Id> = selected
                .iter()
                .chain(picked.iter())
                .map(|card| card.id)
                .collect();
            if !taken.is_empty() {
                query = query.filter(card_secrets::Column::Id.is_not_in(taken));
            }
            picked.extend(
                query
                    .order_by_asc(card_secrets::Column::Id)
                    .limit(u64::try_from(want - picked.len()).unwrap_or(0))
                    .all(conn)
                    .await
                    .dom()?,
            );
        }
        if picked.len() < want {
            return Err(Error::bad_request(keys::CARD_SECRET_INSUFFICIENT));
        }
        selected.extend(picked);
    }
    let ids: Vec<Id> = selected.iter().map(|card| card.id).collect();
    mark_secrets_used_in(conn, &ids, order_id, now).await?;
    let raw = selected
        .iter()
        .map(|card| card.secret.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    fulfillments::ActiveModel {
        order_id: Set(order_id),
        type_: Set(fulfillment_type::AUTO.to_owned()),
        status: Set(fulfillment_status::PENDING.to_owned()),
        payload: Set(String::new()),
        logistics_json: Set(None),
        delivered_by: Set(None),
        delivered_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .map_err(|e| Error::internal(e).or_internal(keys::FULFILLMENT_CREATE_FAILED))?;
    update_status_from(
        conn,
        order_id,
        &[OrderStatus::Paid],
        OrderStatus::Fulfilling,
        now,
    )
    .await?;
    if let Some(parent) = order.parent_id {
        ops::sync_parent(conn, parent, now).await?;
    }
    Ok(raw)
}

/// Makes only the final converted payload visible and completes the order.
pub(crate) async fn finalize_auto_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    payload: &str,
    now: DateTime<Utc>,
) -> Result<Fulfillment> {
    if payload.trim().is_empty() {
        return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
    }
    let order = map::load_locked(conn, order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    if order.status != OrderStatus::Fulfilling {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    let mut active: fulfillments::ActiveModel = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.eq(order_id))
        .filter(fulfillments::Column::Status.eq(fulfillment_status::PENDING))
        .filter(fulfillments::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?
        .ok_or_else(|| Error::bad_request(keys::FULFILLMENT_EXISTS))?
        .into();
    active.status = Set(fulfillment_status::DELIVERED.to_owned());
    active.payload = Set(payload.to_owned());
    active.delivered_at = Set(Some(now));
    active.updated_at = Set(now);
    let row = active.update(conn).await.dom()?;
    update_status_from(
        conn,
        order_id,
        &[OrderStatus::Fulfilling],
        OrderStatus::Completed,
        now,
    )
    .await?;
    if let Some(parent) = order.parent_id {
        ops::sync_parent(conn, parent, now).await?;
    }
    Ok(map::fulfillment_to_domain(row))
}

/// Manual delivery by an administrator (`CreateManual`).
pub(crate) async fn manual_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    admin_id: Id,
    payload: &str,
    delivery_data: &JsonMap,
    delivered_at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Fulfillment> {
    let order = map::load_locked(conn, order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    if order.parent_id.is_none() && !order.children.is_empty() {
        return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
    }
    if !matches!(order.status, OrderStatus::Paid | OrderStatus::Fulfilling) {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    ensure_no_fulfillment(conn, order_id).await?;
    let row = fulfillments::ActiveModel {
        order_id: Set(order_id),
        type_: Set(fulfillment_type::MANUAL.to_owned()),
        status: Set(fulfillment_status::DELIVERED.to_owned()),
        payload: Set(payload.to_owned()),
        logistics_json: Set(to_json(delivery_data)?),
        delivered_by: Set(Some(admin_id)),
        delivered_at: Set(Some(delivered_at)),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .map_err(|e| Error::internal(e).or_internal(keys::FULFILLMENT_CREATE_FAILED))?;
    update_status_from(
        conn,
        order_id,
        &[OrderStatus::Paid, OrderStatus::Fulfilling],
        OrderStatus::Delivered,
        now,
    )
    .await?;
    Ok(map::fulfillment_to_domain(row))
}

/// Conditional status move of an upstream order with the parent recomputed.
pub(crate) async fn upstream_move_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    from: OrderStatus,
    to: OrderStatus,
    now: DateTime<Utc>,
) -> Result<bool> {
    let Some(order) = map::load_locked(conn, order_id).await? else {
        return Err(Error::not_found(keys::ORDER_NOT_FOUND));
    };
    if order.status != from {
        return Ok(false);
    }
    update_status_from(conn, order_id, &[from], to, now).await?;
    if let Some(parent) = order.parent_id {
        ops::sync_parent(conn, parent, now).await?;
    }
    Ok(true)
}

/// Supplier delivery (`CreateUpstreamFulfillment` + status change): idempotent, one
/// `upstream` fulfillment per order.
pub(crate) async fn upstream_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    delivery: &UpstreamDelivery,
    now: DateTime<Utc>,
) -> Result<Option<UpstreamDelivered>> {
    let order = map::load_locked(conn, order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    let existing = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.eq(order_id))
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?;
    if existing.is_some() {
        return Ok(None);
    }
    let row = fulfillments::ActiveModel {
        order_id: Set(order_id),
        type_: Set(fulfillment_type::UPSTREAM.to_owned()),
        status: Set(fulfillment_status::DELIVERED.to_owned()),
        payload: Set(delivery.payload.clone()),
        logistics_json: Set(to_json(&delivery.delivery_data)?),
        delivered_by: Set(None),
        delivered_at: Set(Some(delivery.delivered_at.unwrap_or(now))),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .map_err(|e| Error::internal(e).or_internal(keys::FULFILLMENT_CREATE_FAILED))?;
    let mut status = order.status;
    if matches!(order.status, OrderStatus::Paid | OrderStatus::Fulfilling) {
        update_status_from(
            conn,
            order_id,
            &[OrderStatus::Paid, OrderStatus::Fulfilling],
            OrderStatus::Delivered,
            now,
        )
        .await?;
        status = OrderStatus::Delivered;
    }
    let notify = match order.parent_id {
        Some(parent) => ops::sync_parent(conn, parent, now)
            .await?
            .filter(|s| *s != OrderStatus::Canceled)
            .map(|s| (parent, s)),
        None => Some((order_id, status)),
    };
    Ok(Some(UpstreamDelivered {
        fulfillment: map::fulfillment_to_domain(row),
        notify,
    }))
}
