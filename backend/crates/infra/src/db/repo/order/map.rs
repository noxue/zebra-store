//! Entity ↔ domain mapping and loading of orders with their relations.

use std::collections::HashMap;

use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder};
use serde_json::Value;
use zs_domain::order::model::{Fulfillment, JsonMap, Order, OrderItem, OrderStatus, RefundRecord};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{fulfillments, order_items, order_refund_records, orders};
use crate::db::repo::support::{DbResultExt, from_json};

fn json_map(v: Option<Value>) -> JsonMap {
    from_json(v)
}

pub(crate) fn status_of(raw: &str, id: Id) -> OrderStatus {
    OrderStatus::parse(raw).unwrap_or_else(|| {
        tracing::warn!(order_id = id, status = %raw, "order_status_unknown");
        OrderStatus::PendingPayment
    })
}

/// Order row without relations.
pub(crate) fn order_to_domain(m: orders::Model) -> Order {
    Order {
        status: status_of(&m.status, m.id),
        id: m.id,
        order_no: m.order_no,
        parent_id: m.parent_id,
        user_id: m.user_id,
        guest_email: m.guest_email,
        guest_password: m.guest_password,
        guest_locale: m.guest_locale,
        currency: m.currency,
        original_amount: Amount::new(m.original_amount),
        discount_amount: Amount::new(m.discount_amount),
        member_discount_amount: Amount::new(m.member_discount_amount),
        promotion_discount_amount: Amount::new(m.promotion_discount_amount),
        wholesale_discount_amount: Amount::new(m.wholesale_discount_amount),
        total_amount: Amount::new(m.total_amount),
        wallet_paid_amount: Amount::new(m.wallet_paid_amount),
        online_paid_amount: Amount::new(m.online_paid_amount),
        refunded_amount: Amount::new(m.refunded_amount),
        member_level_id: m.member_level_id,
        coupon_id: m.coupon_id,
        promotion_id: m.promotion_id,
        affiliate_profile_id: m.affiliate_profile_id,
        affiliate_code: m.affiliate_code,
        reseller_id: m.reseller_id,
        reseller_domain: m.reseller_domain,
        reseller_profit_amount: Amount::new(m.reseller_profit_amount),
        client_ip: m.client_ip,
        risk_ip: m.risk_ip,
        expires_at: m.expires_at,
        paid_at: m.paid_at,
        canceled_at: m.canceled_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        items: Vec::new(),
        fulfillment: None,
        children: Vec::new(),
    }
}

pub(crate) fn item_to_domain(m: order_items::Model) -> OrderItem {
    OrderItem {
        id: m.id,
        order_id: m.order_id,
        product_id: m.product_id,
        sku_id: m.sku_id,
        title: json_map(m.title_json),
        sku_snapshot: json_map(m.sku_snapshot_json),
        tags: from_json(m.tags),
        original_unit_price: Amount::new(m.original_unit_price),
        unit_price: Amount::new(m.unit_price),
        cost_price: Amount::new(m.cost_price),
        quantity: m.quantity,
        original_total_price: Amount::new(m.original_total_price),
        total_price: Amount::new(m.total_price),
        coupon_discount: Amount::new(m.coupon_discount),
        member_discount: Amount::new(m.member_discount),
        promotion_discount: Amount::new(m.promotion_discount),
        wholesale_discount: Amount::new(m.wholesale_discount),
        promotion_id: m.promotion_id,
        promotion_name: String::new(),
        fulfillment_type: m.fulfillment_type,
        manual_form_schema: json_map(m.manual_form_schema_snapshot_json),
        manual_form_submission: json_map(m.manual_form_submission_json),
        instructions: json_map(m.instructions_json),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub(crate) fn fulfillment_to_domain(m: fulfillments::Model) -> Fulfillment {
    Fulfillment {
        id: m.id,
        order_id: m.order_id,
        kind: m.type_,
        status: m.status,
        payload: m.payload,
        payload_line_count: 0,
        logistics: json_map(m.logistics_json),
        delivered_by: m.delivered_by,
        delivered_at: m.delivered_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub(crate) fn refund_to_domain(m: order_refund_records::Model) -> RefundRecord {
    RefundRecord {
        id: m.id,
        user_id: m.user_id,
        guest_email: m.guest_email,
        order_id: m.order_id,
        kind: m.type_,
        amount: Amount::new(m.amount),
        payment_fee_refunded: m.payment_fee_refunded,
        payment_fee_refunded_amount: Amount::new(m.payment_fee_refunded_amount),
        currency: m.currency,
        remark: m.remark,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

/// Loads items and fulfillments of `orders` and attaches their live children
/// (`withChildren`): children ordered by id.
pub(crate) async fn attach<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<orders::Model>,
) -> Result<Vec<Order>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let parent_ids: Vec<Id> = rows.iter().map(|r| r.id).collect();
    let children_rows = orders::Entity::find()
        .filter(orders::Column::ParentId.is_in(parent_ids.clone()))
        .filter(orders::Column::DeletedAt.is_null())
        .order_by_asc(orders::Column::Id)
        .all(conn)
        .await
        .dom()?;
    let mut all_ids = parent_ids;
    all_ids.extend(children_rows.iter().map(|c| c.id));

    let mut items: HashMap<Id, Vec<OrderItem>> = HashMap::new();
    for row in order_items::Entity::find()
        .filter(order_items::Column::OrderId.is_in(all_ids.clone()))
        .filter(order_items::Column::DeletedAt.is_null())
        .order_by_asc(order_items::Column::Id)
        .all(conn)
        .await
        .dom()?
    {
        items
            .entry(row.order_id)
            .or_default()
            .push(item_to_domain(row));
    }
    let mut fulfills: HashMap<Id, Fulfillment> = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.is_in(all_ids))
        .filter(fulfillments::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|f| (f.order_id, fulfillment_to_domain(f)))
        .collect();

    let mut children: HashMap<Id, Vec<Order>> = HashMap::new();
    for row in children_rows {
        let mut child = order_to_domain(row);
        child.items = items.remove(&child.id).unwrap_or_default();
        child.fulfillment = fulfills.remove(&child.id);
        if let Some(parent) = child.parent_id {
            children.entry(parent).or_default().push(child);
        }
    }
    Ok(rows
        .into_iter()
        .map(|row| {
            let mut o = order_to_domain(row);
            o.items = items.remove(&o.id).unwrap_or_default();
            o.fulfillment = fulfills.remove(&o.id);
            o.children = children.remove(&o.id).unwrap_or_default();
            o
        })
        .collect())
}

/// One order with relations.
pub(crate) async fn load<C: ConnectionTrait>(conn: &C, id: Id) -> Result<Option<Order>> {
    let Some(row) = orders::Entity::find_by_id(id)
        .filter(orders::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?
    else {
        return Ok(None);
    };
    Ok(attach(conn, vec![row]).await?.pop())
}

/// One order with relations, the order row locked first (`GetByIDForUpdateWithChildren`).
pub(crate) async fn load_locked<C: ConnectionTrait>(conn: &C, id: Id) -> Result<Option<Order>> {
    use sea_orm::QuerySelect;
    let Some(row) = orders::Entity::find_by_id(id)
        .filter(orders::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
    else {
        return Ok(None);
    };
    Ok(attach(conn, vec![row]).await?.pop())
}
