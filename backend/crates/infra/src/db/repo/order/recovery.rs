//! Recovery of a supplier delivery arriving after a completed buyer refund.
//! Fresh, never-published credentials enter local stock exactly once. Ambiguous
//! content stays in procurement holding for review; it is never given to the buyer.
use std::collections::HashSet;

use super::consistency;
use crate::db::entity::{
    card_secret_batches, card_secrets, fulfillments, orders, procurement_orders,
    procurement_stock_recoveries, product_skus,
};
use crate::db::repo::support::DbResultExt;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect, Set,
};
use zs_domain::integration::hooks::UpstreamDelivery;
use zs_domain::order::model::{OrderStatus, fulfillment_type};
use zs_domain::{Error, Id, Result};

pub(crate) const REVIEW: &str = "error.upstream_recovery_needs_review";

pub(crate) async fn recover_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    delivery: &UpstreamDelivery,
    now: DateTime<Utc>,
) -> Result<bool> {
    let order = consistency::lock_family(conn, order_id).await?;
    if order.status != OrderStatus::Refunded {
        return Ok(false);
    }
    // A full parent refund historically changes child status without copying its
    // refunded_amount. Verify the parent's monetary total rather than discarding
    // a real late card or treating a partial parent refund as full recovery.
    let mut fully_refunded = order.refunded_amount >= order.total_amount;
    if !fully_refunded && let Some(parent_id) = order.parent_id {
        fully_refunded = orders::Entity::find_by_id(parent_id)
            .one(conn)
            .await
            .dom()?
            .is_some_and(|parent| {
                parent.status == "refunded" && parent.refunded_amount >= parent.total_amount
            });
    }
    if !fully_refunded {
        return Err(Error::bad_request(REVIEW));
    }
    let review = || Error::bad_request(REVIEW);
    // Never recycle anything that was already visible through an order, including
    // a pending converter fulfillment: its disclosure cannot be disproved.
    if order.fulfillment.is_some()
        || order.items.len() != 1
        || delivery.kind != fulfillment_type::AUTO
        || delivery.status != "delivered"
    {
        return Err(review());
    }
    let item = &order.items[0];
    if item.fulfillment_type != fulfillment_type::UPSTREAM || item.sku_id <= 0 {
        return Err(review());
    }
    let purchase = procurement_orders::Entity::find()
        .filter(procurement_orders::Column::LocalOrderId.eq(order_id))
        .filter(procurement_orders::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?
        .ok_or_else(review)?;
    if purchase.upstream_order_id == 0 && purchase.upstream_order_no.is_empty() {
        return Err(review());
    }
    let cards: Vec<_> = delivery
        .payload
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let unique: HashSet<_> = cards.iter().collect();
    if cards.is_empty()
        || cards.len() != usize::try_from(item.quantity).unwrap_or_default()
        || unique.len() != cards.len()
    {
        return Err(review());
    }
    let sku = product_skus::Entity::find_by_id(item.sku_id)
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(review)?;
    if sku.product_id != item.product_id || sku.deleted_at.is_some() {
        return Err(review());
    }
    let batch_no = format!("UPSTREAM-RECOVERY-{}", purchase.id);
    if let Some(batch) = card_secret_batches::Entity::find()
        .filter(card_secret_batches::Column::BatchNo.eq(&batch_no))
        .one(conn)
        .await
        .dom()?
    {
        let prior = card_secrets::Entity::find()
            .filter(card_secrets::Column::BatchId.eq(batch.id))
            .all(conn)
            .await
            .dom()?;
        let prior: HashSet<_> = prior.iter().map(|c| &c.secret).collect();
        if prior != unique || batch.product_id != item.product_id || batch.sku_id != item.sku_id {
            return Err(review());
        }
        // Includes cards already resold/deleted: a duplicate callback never restocks them.
        return Ok(true);
    }
    for secret in &cards {
        if card_secrets::Entity::find()
            .filter(card_secrets::Column::Secret.eq(secret))
            .one(conn)
            .await
            .dom()?
            .is_some()
        {
            return Err(review());
        }
        // Direct upstream cards need not have a card_secrets row. Check disclosures
        // too; never log the search term or credential contents.
        let disclosed = fulfillments::Entity::find()
            .filter(fulfillments::Column::Payload.contains(secret))
            .all(conn)
            .await
            .dom()?;
        if disclosed
            .iter()
            .any(|f| f.payload.lines().any(|line| line.trim() == secret))
        {
            return Err(review());
        }
    }
    let count = i32::try_from(cards.len()).map_err(|_| review())?;
    let batch = card_secret_batches::ActiveModel {
        product_id: Set(item.product_id),
        sku_id: Set(item.sku_id),
        batch_no: Set(batch_no),
        // Raw supplier cards must still run any configured converter on their next sale.
        source: Set("manual".into()),
        total_count: Set(count),
        note: Set(format!(
            "refunded_upstream_recovery:{}:{}",
            order.order_no, purchase.id
        )),
        created_by: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    for secret in cards {
        let credential_hash = zs_shared::crypto::sha256_hex(secret.as_bytes());
        let card = card_secrets::ActiveModel {
            product_id: Set(item.product_id),
            sku_id: Set(item.sku_id),
            batch_id: Set(Some(batch.id)),
            secret: Set(secret),
            status: Set("available".into()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(conn)
        .await
        .dom()?;
        procurement_stock_recoveries::ActiveModel {
            procurement_order_id: Set(purchase.id),
            order_id: Set(order.id),
            batch_id: Set(batch.id),
            card_secret_id: Set(card.id),
            credential_hash: Set(credential_hash),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(conn)
        .await
        .map_err(|error| {
            if matches!(
                error.sql_err(),
                Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
            ) {
                review()
            } else {
                Error::internal(error)
            }
        })?;
    }
    tracing::info!(
        order_id,
        procurement_order_id = purchase.id,
        batch_id = batch.id,
        product_id = item.product_id,
        sku_id = item.sku_id,
        count,
        "refunded_upstream_delivery_recovered_to_stock"
    );
    Ok(true)
}
