//! [`OrderViewRepo`]: parent orders carrying the reseller's snapshot
//! (`gormstore/order.go`). Scope: `orders.reseller_id = snapshot.reseller_id = ?`,
//! parent orders only (RSL-07 tenant isolation).

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::sea_query::Query;
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, QuerySelect,
};
use serde_json::Value;
use zs_domain::reseller::orders::{OrderHead, OrderItemView, ResellerOrderRow};
use zs_domain::reseller::ports::{OrderFilter, OrderStats, OrderViewRepo};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::{SeaResellerStore, ledger_model, load_users, snapshot_model};
use crate::db::entity::{
    order_items, orders, reseller_ledger_entries as ledger, reseller_order_snapshots as snapshots,
};
use crate::db::repo::support::DbResultExt;

fn scope(reseller_id: Id, f: &OrderFilter) -> Condition {
    let with_snapshot = Query::select()
        .column(snapshots::Column::OrderId)
        .from(snapshots::Entity)
        .and_where(snapshots::Column::ResellerId.eq(reseller_id))
        .and_where(snapshots::Column::DeletedAt.is_null())
        .to_owned();
    let mut cond = Condition::all()
        .add(orders::Column::ResellerId.eq(reseller_id))
        .add(orders::Column::ParentId.is_null())
        .add(orders::Column::DeletedAt.is_null())
        .add(orders::Column::Id.in_subquery(with_snapshot));
    if !f.status.trim().is_empty() {
        cond = cond.add(orders::Column::Status.eq(f.status.trim()));
    }
    if !f.order_no.trim().is_empty() {
        cond = cond.add(orders::Column::OrderNo.contains(f.order_no.trim()));
    }
    if let Some(v) = f.created_from {
        cond = cond.add(orders::Column::CreatedAt.gte(v));
    }
    if let Some(v) = f.created_to {
        cond = cond.add(orders::Column::CreatedAt.lte(v));
    }
    if let Some(v) = f.paid_from {
        cond = cond.add(orders::Column::PaidAt.gte(v));
    }
    if let Some(v) = f.paid_to {
        cond = cond.add(orders::Column::PaidAt.lte(v));
    }
    cond
}

fn item_view(m: order_items::Model) -> OrderItemView {
    OrderItemView {
        id: m.id,
        title: m.title_json.unwrap_or(Value::Null),
        sku_snapshot: m.sku_snapshot_json.unwrap_or(Value::Null),
        quantity: m.quantity,
        unit_price: Amount::new(m.unit_price),
        total_price: Amount::new(m.total_price),
    }
}

impl SeaResellerStore {
    /// Snapshot, items (parent's, else children's), ledger and buyer e-mail per order.
    async fn build_rows(
        &self,
        reseller_id: Id,
        heads: Vec<orders::Model>,
    ) -> Result<Vec<ResellerOrderRow>> {
        let ids: Vec<Id> = heads.iter().map(|o| o.id).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut snaps: HashMap<Id, _> = snapshots::Entity::find()
            .filter(snapshots::Column::OrderId.is_in(ids.clone()))
            .filter(snapshots::Column::ResellerId.eq(reseller_id))
            .filter(snapshots::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|s| (s.order_id, snapshot_model(s)))
            .collect();
        let children = orders::Entity::find()
            .filter(orders::Column::ParentId.is_in(ids.clone()))
            .filter(orders::Column::DeletedAt.is_null())
            .order_by_asc(orders::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        let child_parent: HashMap<Id, Id> = children
            .iter()
            .filter_map(|c| c.parent_id.map(|p| (c.id, p)))
            .collect();
        let mut item_ids = ids.clone();
        item_ids.extend(child_parent.keys());
        let mut own: HashMap<Id, Vec<OrderItemView>> = HashMap::new();
        let mut from_children: HashMap<Id, Vec<OrderItemView>> = HashMap::new();
        for item in order_items::Entity::find()
            .filter(order_items::Column::OrderId.is_in(item_ids))
            .filter(order_items::Column::DeletedAt.is_null())
            .order_by_asc(order_items::Column::Id)
            .all(&self.db)
            .await
            .dom()?
        {
            match child_parent.get(&item.order_id) {
                Some(parent) => from_children
                    .entry(*parent)
                    .or_default()
                    .push(item_view(item)),
                None => own.entry(item.order_id).or_default().push(item_view(item)),
            }
        }
        let mut entries: HashMap<Id, Vec<_>> = HashMap::new();
        for e in ledger::Entity::find()
            .filter(ledger::Column::ResellerId.eq(reseller_id))
            .filter(ledger::Column::OrderId.is_in(ids))
            .filter(ledger::Column::DeletedAt.is_null())
            .order_by_desc(ledger::Column::Id)
            .all(&self.db)
            .await
            .dom()?
        {
            if let Some(oid) = e.order_id {
                entries.entry(oid).or_default().push(ledger_model(e));
            }
        }
        let users = load_users(
            &self.db,
            &heads.iter().map(|o| o.user_id).collect::<Vec<_>>(),
        )
        .await?;
        let mut out = Vec::with_capacity(heads.len());
        for o in heads {
            let Some(snapshot) = snaps.remove(&o.id) else {
                continue;
            };
            let items = match own.remove(&o.id) {
                Some(items) if !items.is_empty() => items,
                _ => from_children.remove(&o.id).unwrap_or_default(),
            };
            out.push(ResellerOrderRow {
                snapshot,
                buyer_email: users
                    .get(&o.user_id)
                    .map(|u| u.email.clone())
                    .unwrap_or_default(),
                ledger: entries.remove(&o.id).unwrap_or_default(),
                items,
                order: OrderHead {
                    id: o.id,
                    order_no: o.order_no,
                    status: o.status,
                    user_id: o.user_id,
                    guest_email: o.guest_email,
                    total_amount: Amount::new(o.total_amount),
                    created_at: o.created_at,
                    paid_at: o.paid_at,
                },
            });
        }
        Ok(out)
    }
}

#[async_trait]
impl OrderViewRepo for SeaResellerStore {
    async fn list_orders(
        &self,
        reseller_id: Id,
        f: &OrderFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerOrderRow>> {
        let q = orders::Entity::find().filter(scope(reseller_id, f));
        let total = q.clone().count(&self.db).await.dom()?;
        let heads = q
            .order_by_desc(orders::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: self.build_rows(reseller_id, heads).await?,
            total,
        })
    }

    async fn order_stats(&self, reseller_id: Id, f: &OrderFilter) -> Result<OrderStats> {
        let rows: Vec<(Id, String)> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .column(orders::Column::Status)
            .filter(scope(reseller_id, f))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = rows.iter().map(|(id, _)| *id).collect();
        let currencies: HashMap<Id, String> = if ids.is_empty() {
            HashMap::new()
        } else {
            snapshots::Entity::find()
                .select_only()
                .column(snapshots::Column::OrderId)
                .column(snapshots::Column::Currency)
                .filter(snapshots::Column::OrderId.is_in(ids))
                .filter(snapshots::Column::ResellerId.eq(reseller_id))
                .filter(snapshots::Column::DeletedAt.is_null())
                .into_tuple::<(Id, String)>()
                .all(&self.db)
                .await
                .dom()?
                .into_iter()
                .collect()
        };
        let mut stats = OrderStats::default();
        for (id, status) in rows {
            stats.total += 1;
            *stats.by_status.entry(status).or_default() += 1;
            if let Some(c) = currencies.get(&id) {
                *stats.by_currency.entry(c.clone()).or_default() += 1;
            }
        }
        Ok(stats)
    }

    async fn order_by_no(
        &self,
        reseller_id: Id,
        order_no: &str,
    ) -> Result<Option<ResellerOrderRow>> {
        if order_no.is_empty() {
            return Ok(None);
        }
        let head = orders::Entity::find()
            .filter(scope(reseller_id, &OrderFilter::default()))
            .filter(orders::Column::OrderNo.eq(order_no))
            .one(&self.db)
            .await
            .dom()?;
        Ok(self
            .build_rows(reseller_id, head.into_iter().collect())
            .await?
            .pop())
    }
}
