//! [`ProcurementRepo`] on `procurement_orders` and [`MappingLookup`] on the mapping tables.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::integration::procurement::{
    MappingLookup, NewProcurement, ProcurementChange, ProcurementFilter, ProcurementOrder,
    ProcurementRepo, ProcurementStatus,
};
use zs_domain::integration::protocol::RemoteFulfillment;
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

use super::connection::many_in as connections_in;
use super::orders::SeaIntegrationOrders;
use crate::db::entity::{
    procurement_deliveries, procurement_orders, product_mappings, sku_mappings,
};
use crate::db::repo::support::{DbResultExt, from_json, to_json};

/// SeaORM implementation of [`ProcurementRepo`].
#[derive(Debug, Clone)]
pub struct SeaProcurementRepo {
    db: DatabaseConnection,
    orders: SeaIntegrationOrders,
}

impl SeaProcurementRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            orders: SeaIntegrationOrders::new(db.clone()),
            db,
        }
    }

    /// Attaches `connection` and `local_order` (items only, like the original).
    async fn attach(&self, rows: Vec<procurement_orders::Model>) -> Result<Vec<ProcurementOrder>> {
        let conn_ids: Vec<Id> = rows.iter().map(|r| r.connection_id).collect();
        let conns: HashMap<Id, _> = connections_in(&self.db, &conn_ids)
            .await?
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        let order_ids: Vec<Id> = rows.iter().map(|r| r.local_order_id).collect();
        let locals: HashMap<Id, _> = self
            .orders
            .flat(&order_ids)
            .await?
            .into_iter()
            .map(|o| (o.id, o))
            .collect();
        Ok(rows
            .into_iter()
            .map(|r| {
                let mut p = to_domain(r);
                p.connection = conns.get(&p.connection_id).cloned();
                p.local_order = locals.get(&p.local_order_id).cloned();
                p
            })
            .collect())
    }

    /// Fills `held_delivery` of the given purchase orders.
    async fn with_deliveries(
        &self,
        mut orders: Vec<ProcurementOrder>,
    ) -> Result<Vec<ProcurementOrder>> {
        let ids: Vec<Id> = orders.iter().map(|o| o.id).collect();
        if ids.is_empty() {
            return Ok(orders);
        }
        let mut held: HashMap<Id, RemoteFulfillment> = procurement_deliveries::Entity::find()
            .filter(procurement_deliveries::Column::ProcurementOrderId.is_in(ids))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|d| {
                (
                    d.procurement_order_id,
                    RemoteFulfillment {
                        kind: d.kind,
                        status: d.status,
                        payload: d.payload,
                        delivery_data: from_json(d.delivery_data),
                        delivered_at: d.delivered_at,
                    },
                )
            })
            .collect();
        for o in &mut orders {
            o.held_delivery = held.remove(&o.id);
            o.has_held_delivery = o.held_delivery.is_some();
        }
        Ok(orders)
    }

    async fn first(&self, cond: Condition) -> Result<Option<ProcurementOrder>> {
        let Some(row) = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::DeletedAt.is_null())
            .filter(cond)
            .order_by_desc(procurement_orders::Column::Id)
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let attached = self.attach(vec![row]).await?;
        Ok(self.with_deliveries(attached).await?.pop())
    }
}

pub(crate) fn to_domain(m: procurement_orders::Model) -> ProcurementOrder {
    ProcurementOrder {
        id: m.id,
        connection_id: m.connection_id,
        local_order_id: m.local_order_id,
        local_order_no: m.local_order_no,
        upstream_order_id: m.upstream_order_id,
        upstream_order_no: m.upstream_order_no,
        status: ProcurementStatus::from_stored(&m.status),
        upstream_amount: Amount::new(m.upstream_amount),
        upstream_currency: m.upstream_currency,
        local_sell_amount: Amount::new(m.local_sell_amount),
        currency: m.currency,
        error_message: m.error_message,
        retry_count: m.retry_count,
        next_retry_at: m.next_retry_at,
        upstream_payload_line_count: 0,
        upstream_payload: m.upstream_payload,
        trace_id: m.trace_id,
        created_at: m.created_at,
        updated_at: m.updated_at,
        connection: None,
        local_order: None,
        parent_order_no: String::new(),
        upstream_refund_records: Vec::new(),
        upstream_refunded_amount: String::new(),
        has_held_delivery: false,
        held_delivery: None,
    }
}

/// Stores (or replaces) the delivery held for a purchase order.
async fn hold_delivery<C: ConnectionTrait>(
    db: &C,
    procurement_order_id: Id,
    f: &RemoteFulfillment,
    now: DateTime<Utc>,
) -> Result<()> {
    use procurement_deliveries::Column as D;
    let data = match &f.delivery_data {
        Some(m) => to_json(m)?,
        None => None,
    };
    let existing = procurement_deliveries::Entity::find()
        .filter(D::ProcurementOrderId.eq(procurement_order_id))
        .one(db)
        .await
        .dom()?;
    let mut row = procurement_deliveries::ActiveModel {
        procurement_order_id: Set(procurement_order_id),
        kind: Set(f.kind.clone()),
        status: Set(f.status.clone()),
        payload: Set(f.payload.clone()),
        delivery_data: Set(data),
        delivered_at: Set(f.delivered_at),
        updated_at: Set(now),
        ..Default::default()
    };
    match existing {
        Some(e) => {
            row.id = Set(e.id);
            row.update(db).await.dom()?;
        }
        None => {
            row.created_at = Set(now);
            row.insert(db).await.dom()?;
        }
    }
    Ok(())
}

fn filter_condition(f: &ProcurementFilter, with_status: bool) -> Condition {
    let mut c = Condition::all().add(procurement_orders::Column::DeletedAt.is_null());
    if f.connection_id > 0 {
        c = c.add(procurement_orders::Column::ConnectionId.eq(f.connection_id));
    }
    if with_status && let Some(s) = f.status {
        c = c.add(procurement_orders::Column::Status.eq(s.as_str()));
    }
    if !f.local_order_no.is_empty() {
        c = c.add(procurement_orders::Column::LocalOrderNo.eq(f.local_order_no.clone()));
    }
    if !f.upstream_order_no.is_empty() {
        c = c.add(procurement_orders::Column::UpstreamOrderNo.eq(f.upstream_order_no.clone()));
    }
    if let Some(from) = f.created_from {
        c = c.add(procurement_orders::Column::CreatedAt.gte(from));
    }
    if let Some(to) = f.created_to {
        c = c.add(procurement_orders::Column::CreatedAt.lte(to));
    }
    c
}

#[async_trait]
impl ProcurementRepo for SeaProcurementRepo {
    async fn get(&self, id: Id) -> Result<Option<ProcurementOrder>> {
        self.first(Condition::all().add(procurement_orders::Column::Id.eq(id)))
            .await
    }

    async fn get_by_local_order_id(&self, local_order_id: Id) -> Result<Option<ProcurementOrder>> {
        self.first(
            Condition::all().add(procurement_orders::Column::LocalOrderId.eq(local_order_id)),
        )
        .await
    }

    async fn get_by_local_order_no(
        &self,
        local_order_no: &str,
    ) -> Result<Option<ProcurementOrder>> {
        if local_order_no.is_empty() {
            return Ok(None);
        }
        self.first(
            Condition::all().add(procurement_orders::Column::LocalOrderNo.eq(local_order_no)),
        )
        .await
    }

    async fn create_once(
        &self,
        new: &NewProcurement,
        now: DateTime<Utc>,
    ) -> Result<Option<ProcurementOrder>> {
        // Existence check and insert share one transaction (SQLite serializes writers;
        // other backends rely on the order group calling this once per payment).
        let txn = self.db.begin().await.dom()?;
        let exists = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::LocalOrderId.eq(new.local_order_id))
            .filter(procurement_orders::Column::DeletedAt.is_null())
            .count(&txn)
            .await
            .dom()?;
        if exists > 0 {
            return Ok(None);
        }
        let row = procurement_orders::ActiveModel {
            connection_id: Set(new.connection_id),
            local_order_id: Set(new.local_order_id),
            local_order_no: Set(new.local_order_no.clone()),
            upstream_order_id: Set(0),
            upstream_order_no: Set(String::new()),
            status: Set(ProcurementStatus::Pending.as_str().to_owned()),
            upstream_amount: Set(rust_decimal::Decimal::ZERO),
            upstream_currency: Set(String::new()),
            local_sell_amount: Set(new.local_sell_amount.decimal()),
            currency: Set(new.currency.clone()),
            error_message: Set(String::new()),
            retry_count: Set(0),
            next_retry_at: Set(None),
            upstream_payload: Set(String::new()),
            trace_id: Set(new.trace_id.clone()),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        txn.commit().await.dom()?;
        Ok(Some(to_domain(row)))
    }

    async fn update(
        &self,
        id: Id,
        from: &[ProcurementStatus],
        change: &ProcurementChange,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        use procurement_orders::Column as C;
        let mut upd =
            procurement_orders::Entity::update_many().col_expr(C::UpdatedAt, Expr::value(now));
        if let Some(s) = change.status {
            upd = upd.col_expr(C::Status, Expr::value(s.as_str()));
        }
        if let Some(v) = change.upstream_order_id {
            upd = upd.col_expr(C::UpstreamOrderId, Expr::value(v));
        }
        if let Some(v) = &change.upstream_order_no {
            upd = upd.col_expr(C::UpstreamOrderNo, Expr::value(v.clone()));
        }
        if let Some(v) = change.upstream_amount {
            upd = upd.col_expr(C::UpstreamAmount, Expr::value(v.decimal()));
        }
        if let Some(v) = &change.upstream_currency {
            upd = upd.col_expr(C::UpstreamCurrency, Expr::value(v.clone()));
        }
        if let Some(v) = &change.error_message {
            upd = upd.col_expr(C::ErrorMessage, Expr::value(v.clone()));
        }
        if let Some(v) = change.retry_count {
            upd = upd.col_expr(C::RetryCount, Expr::value(v));
        }
        if let Some(v) = change.next_retry_at {
            upd = upd.col_expr(C::NextRetryAt, Expr::value(v));
        }
        if let Some(v) = &change.upstream_payload {
            upd = upd.col_expr(C::UpstreamPayload, Expr::value(v.clone()));
        }
        let mut q = upd.filter(C::Id.eq(id)).filter(C::DeletedAt.is_null());
        if !from.is_empty() {
            q = q.filter(C::Status.is_in(from.iter().map(|s| s.as_str())));
        }
        let Some(delivery) = &change.held_delivery else {
            return Ok(q.exec(&self.db).await.dom()?.rows_affected > 0);
        };
        // The guarded status change and the held delivery commit together.
        let txn = self.db.begin().await.dom()?;
        let changed = q.exec(&txn).await.dom()?.rows_affected > 0;
        if changed {
            hold_delivery(&txn, id, delivery, now).await?;
        }
        txn.commit().await.dom()?;
        Ok(changed)
    }

    async fn list(&self, filter: &ProcurementFilter) -> Result<Page<ProcurementOrder>> {
        let q = procurement_orders::Entity::find().filter(filter_condition(filter, true));
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(procurement_orders::Column::CreatedAt)
            .order_by_desc(procurement_orders::Column::Id)
            .offset(filter.page.offset())
            .limit(filter.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: self.attach(rows).await?,
            total,
        })
    }

    async fn stats(&self, filter: &ProcurementFilter) -> Result<Vec<(String, u64)>> {
        let rows: Vec<(String, i64)> = procurement_orders::Entity::find()
            .select_only()
            .column(procurement_orders::Column::Status)
            .column_as(procurement_orders::Column::Id.count(), "count")
            .filter(filter_condition(filter, false))
            .group_by(procurement_orders::Column::Status)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|(s, n)| (s, u64::try_from(n).unwrap_or(0)))
            .collect())
    }

    async fn list_accepted(&self, limit: u64) -> Result<Vec<ProcurementOrder>> {
        let rows = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::DeletedAt.is_null())
            .filter(procurement_orders::Column::Status.eq(ProcurementStatus::Accepted.as_str()))
            .order_by_asc(procurement_orders::Column::UpdatedAt)
            .limit(limit)
            .all(&self.db)
            .await
            .dom()?;
        self.with_deliveries(rows.into_iter().map(to_domain).collect())
            .await
    }

    async fn list_accepted_after(&self, after_id: Id, limit: u64) -> Result<Vec<ProcurementOrder>> {
        let rows = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::DeletedAt.is_null())
            .filter(procurement_orders::Column::Status.eq(ProcurementStatus::Accepted.as_str()))
            .filter(procurement_orders::Column::Id.gt(after_id))
            .order_by_asc(procurement_orders::Column::Id)
            .limit(limit)
            .all(&self.db)
            .await
            .dom()?;
        self.with_deliveries(rows.into_iter().map(to_domain).collect())
            .await
    }

    async fn list_by_connection_between(
        &self,
        connection_id: Id,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ProcurementOrder>> {
        let rows = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::DeletedAt.is_null())
            .filter(procurement_orders::Column::ConnectionId.eq(connection_id))
            .filter(procurement_orders::Column::CreatedAt.gte(start))
            .filter(procurement_orders::Column::CreatedAt.lte(end))
            .order_by_asc(procurement_orders::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_domain).collect())
    }
}

/// [`MappingLookup`] on the mapping tables.
#[derive(Debug, Clone)]
pub struct SeaMappingLookup {
    db: DatabaseConnection,
}

impl SeaMappingLookup {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl MappingLookup for SeaMappingLookup {
    async fn connection_of_product(&self, product_id: Id) -> Result<Option<Id>> {
        Ok(product_mappings::Entity::find()
            .filter(product_mappings::Column::LocalProductId.eq(product_id))
            .filter(product_mappings::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(|m| m.connection_id))
    }

    async fn upstream_sku_of(&self, sku_id: Id) -> Result<Option<Id>> {
        Ok(sku_mappings::Entity::find()
            .filter(sku_mappings::Column::LocalSkuId.eq(sku_id))
            .filter(sku_mappings::Column::DeletedAt.is_null())
            .order_by_desc(sku_mappings::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(|m| m.upstream_sku_id))
    }
}
