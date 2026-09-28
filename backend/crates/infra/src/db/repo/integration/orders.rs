//! Read-only projections of the order tables used by procurement and downstream
//! callbacks (consumer-defined ports; the order group owns the writes).

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use zs_domain::integration::downstream::{CallbackOrders, OrderSnapshot};
use zs_domain::integration::procurement::{LocalOrder, LocalOrderItem, LocalOrders};
use zs_domain::integration::protocol::RemoteFulfillment;
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{fulfillments, order_items, orders};
use crate::db::repo::support::{DbResultExt, from_json};

/// Order projections on `orders`, `order_items` and `fulfillments`.
#[derive(Debug, Clone)]
pub struct SeaIntegrationOrders {
    db: DatabaseConnection,
}

fn item_of(m: order_items::Model) -> LocalOrderItem {
    LocalOrderItem {
        product_id: m.product_id,
        sku_id: m.sku_id,
        title: from_json(m.title_json),
        sku_snapshot: from_json(m.sku_snapshot_json),
        cost_price: Amount::new(m.cost_price),
        quantity: m.quantity,
        total_price: Amount::new(m.total_price),
        fulfillment_type: m.fulfillment_type,
        manual_form_submission: from_json(m.manual_form_submission_json),
    }
}

fn order_of(m: orders::Model, items: Vec<LocalOrderItem>) -> LocalOrder {
    LocalOrder {
        id: m.id,
        order_no: m.order_no,
        parent_id: m.parent_id.filter(|p| *p > 0),
        user_id: m.user_id,
        guest_email: m.guest_email,
        status: m.status,
        currency: m.currency,
        total_amount: Amount::new(m.total_amount),
        refunded_amount: Amount::new(m.refunded_amount),
        items,
        children: Vec::new(),
    }
}

impl SeaIntegrationOrders {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn rows(&self, ids: &[Id]) -> Result<Vec<orders::Model>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        orders::Entity::find()
            .filter(orders::Column::Id.is_in(ids.to_vec()))
            .filter(orders::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()
    }

    async fn children_rows(&self, parent_id: Id) -> Result<Vec<orders::Model>> {
        orders::Entity::find()
            .filter(orders::Column::ParentId.eq(parent_id))
            .filter(orders::Column::DeletedAt.is_null())
            .order_by_asc(orders::Column::Id)
            .all(&self.db)
            .await
            .dom()
    }

    async fn items(&self, order_ids: &[Id]) -> Result<HashMap<Id, Vec<LocalOrderItem>>> {
        let mut out: HashMap<Id, Vec<LocalOrderItem>> = HashMap::new();
        if order_ids.is_empty() {
            return Ok(out);
        }
        for m in order_items::Entity::find()
            .filter(order_items::Column::OrderId.is_in(order_ids.to_vec()))
            .filter(order_items::Column::DeletedAt.is_null())
            .order_by_asc(order_items::Column::Id)
            .all(&self.db)
            .await
            .dom()?
        {
            out.entry(m.order_id).or_default().push(item_of(m));
        }
        Ok(out)
    }

    /// Orders with their items, without children.
    pub async fn flat(&self, ids: &[Id]) -> Result<Vec<LocalOrder>> {
        let rows = self.rows(ids).await?;
        let ids: Vec<Id> = rows.iter().map(|r| r.id).collect();
        let mut items = self.items(&ids).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let its = items.remove(&r.id).unwrap_or_default();
                order_of(r, its)
            })
            .collect())
    }

    async fn fulfillments(&self, order_ids: &[Id]) -> Result<HashMap<Id, RemoteFulfillment>> {
        if order_ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(fulfillments::Entity::find()
            .filter(fulfillments::Column::OrderId.is_in(order_ids.to_vec()))
            .filter(fulfillments::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|f| {
                (
                    f.order_id,
                    RemoteFulfillment {
                        kind: f.type_,
                        status: f.status,
                        payload: f.payload,
                        delivery_data: f.logistics_json.and_then(|v| v.as_object().cloned()),
                        delivered_at: f.delivered_at,
                    },
                )
            })
            .collect())
    }
}

#[async_trait]
impl LocalOrders for SeaIntegrationOrders {
    async fn get(&self, id: Id) -> Result<Option<LocalOrder>> {
        let Some(mut order) = self.flat(&[id]).await?.pop() else {
            return Ok(None);
        };
        let child_ids: Vec<Id> = self
            .children_rows(id)
            .await?
            .into_iter()
            .map(|c| c.id)
            .collect();
        let mut children = self.flat(&child_ids).await?;
        children.sort_by_key(|c| c.id);
        order.children = children;
        Ok(Some(order))
    }

    async fn get_many(&self, ids: &[Id]) -> Result<Vec<LocalOrder>> {
        self.flat(ids).await
    }
}

#[async_trait]
impl CallbackOrders for SeaIntegrationOrders {
    async fn snapshot(&self, order_id: Id) -> Result<Option<OrderSnapshot>> {
        let Some(row) = self.rows(&[order_id]).await?.pop() else {
            return Ok(None);
        };
        let children = self.children_rows(order_id).await?;
        let mut ids: Vec<Id> = children.iter().map(|c| c.id).collect();
        ids.push(order_id);
        let mut f = self.fulfillments(&ids).await?;
        Ok(Some(OrderSnapshot {
            id: row.id,
            order_no: row.order_no,
            parent_id: row.parent_id.filter(|p| *p > 0),
            status: row.status,
            fulfillment: f.remove(&order_id),
            children: children
                .into_iter()
                .map(|c| OrderSnapshot {
                    id: c.id,
                    order_no: c.order_no,
                    parent_id: c.parent_id,
                    status: c.status,
                    fulfillment: f.remove(&c.id),
                    children: Vec::new(),
                })
                .collect(),
        }))
    }
}
