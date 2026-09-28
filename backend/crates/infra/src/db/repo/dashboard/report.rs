//! [`DashboardRepo`] over orders, payments, refunds, users, products, card
//! secrets, upstream mappings and wallets.
//!
//! Only light projections are loaded; aggregation happens in the domain so
//! results do not depend on the SQL dialect (DB-02, DB-05, DB-09). Group-by
//! results use explicit aliases (DB-07).

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde_json::Value;
use zs_domain::dashboard::inventory::{
    FULFILLMENT_AUTO, FULFILLMENT_UPSTREAM, InvProduct, InvSku, InventorySnapshot, UpstreamSku,
};
use zs_domain::dashboard::stats::{
    DashboardRepo, ItemFact, OrderFact, PROVIDER_WALLET, PaymentFact, RefundFact, is_profit_status,
};
use zs_domain::{Id, Result};

use crate::db::entity::{
    card_secrets, order_items, order_refund_records, orders, payment_channels, payments,
    product_mappings, product_skus, products, sku_mappings, users, wallet_accounts,
};
use crate::db::repo::support::DbResultExt;

/// Rows per `IN (…)` list.
const CHUNK: usize = 200;
/// Card secret status counted as stock.
const SECRET_AVAILABLE: &str = "available";
/// Locales tried, in order, for a localized title (original `SupportedLocales`).
const TITLE_LOCALES: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];

/// SeaORM implementation of [`DashboardRepo`].
#[derive(Debug, Clone)]
pub struct SeaDashboardRepo {
    db: DatabaseConnection,
}

impl SeaDashboardRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// First locale present in a localized JSON value (`COALESCE(json_extract…)`).
fn localized_title(v: Option<&Value>) -> String {
    let Some(obj) = v.and_then(Value::as_object) else {
        return String::new();
    };
    TITLE_LOCALES
        .iter()
        .find_map(|k| match obj.get(*k) {
            None | Some(Value::Null) => None,
            Some(Value::String(s)) => Some(s.clone()),
            Some(other) => Some(other.to_string()),
        })
        .unwrap_or_default()
}

fn unique(ids: impl IntoIterator<Item = Id>) -> Vec<Id> {
    let mut seen = HashSet::new();
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

impl SeaDashboardRepo {
    /// `(id, status, created_at)` of live orders with a profit status created in the window.
    async fn profit_orders(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<HashMap<Id, (String, DateTime<Utc>)>> {
        let rows: Vec<(Id, String, DateTime<Utc>)> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .column(orders::Column::Status)
            .column(orders::Column::CreatedAt)
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::CreatedAt.gte(start))
            .filter(orders::Column::CreatedAt.lt(end))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .filter(|(_, status, _)| is_profit_status(status))
            .map(|(id, status, at)| (id, (status, at)))
            .collect())
    }

    /// Live items of the given orders.
    async fn items_of(&self, order_ids: &[Id]) -> Result<Vec<order_items::Model>> {
        let mut out = Vec::new();
        for chunk in order_ids.chunks(CHUNK) {
            out.extend(
                order_items::Entity::find()
                    .filter(order_items::Column::DeletedAt.is_null())
                    .filter(order_items::Column::OrderId.is_in(chunk.to_vec()))
                    .order_by_asc(order_items::Column::Id)
                    .all(&self.db)
                    .await
                    .dom()?,
            );
        }
        Ok(out)
    }

    /// Item cost (`cost_price × quantity`) per order.
    async fn cost_by_order(&self, order_ids: &[Id]) -> Result<HashMap<Id, Decimal>> {
        let mut out: HashMap<Id, Decimal> = HashMap::new();
        for chunk in order_ids.chunks(CHUNK) {
            let rows: Vec<(Id, Decimal, i32)> = order_items::Entity::find()
                .select_only()
                .column(order_items::Column::OrderId)
                .column(order_items::Column::CostPrice)
                .column(order_items::Column::Quantity)
                .filter(order_items::Column::DeletedAt.is_null())
                .filter(order_items::Column::OrderId.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            for (order_id, cost, qty) in rows {
                *out.entry(order_id).or_default() += cost * Decimal::from(qty);
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl DashboardRepo for SeaDashboardRepo {
    async fn orders(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<OrderFact>> {
        let rows: Vec<(Id, String, String, Decimal, DateTime<Utc>)> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .column(orders::Column::Status)
            .column(orders::Column::Currency)
            .column(orders::Column::TotalAmount)
            .column(orders::Column::CreatedAt)
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::ParentId.is_null())
            .filter(orders::Column::CreatedAt.gte(start))
            .filter(orders::Column::CreatedAt.lt(end))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(
                |(id, status, currency, total_amount, created_at)| OrderFact {
                    id,
                    status,
                    currency,
                    total_amount,
                    created_at,
                },
            )
            .collect())
    }

    async fn latest_currency(&self) -> Result<String> {
        let row: Option<String> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Currency)
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::ParentId.is_null())
            .filter(orders::Column::Currency.ne(""))
            .order_by_desc(orders::Column::Id)
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.unwrap_or_default())
    }

    async fn payments(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<PaymentFact>> {
        let rows = payments::Entity::find()
            .filter(payments::Column::DeletedAt.is_null())
            .filter(payments::Column::ProviderType.ne(PROVIDER_WALLET))
            .filter(payments::Column::CreatedAt.gte(start))
            .filter(payments::Column::CreatedAt.lt(end))
            .all(&self.db)
            .await
            .dom()?;
        let channel_ids = unique(rows.iter().map(|p| p.channel_id).filter(|id| *id > 0));
        let mut names: HashMap<Id, String> = HashMap::new();
        for chunk in channel_ids.chunks(CHUNK) {
            let found: Vec<(Id, String)> = payment_channels::Entity::find()
                .select_only()
                .column(payment_channels::Column::Id)
                .column(payment_channels::Column::Name)
                .filter(payment_channels::Column::Id.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            names.extend(found);
        }
        Ok(rows
            .into_iter()
            .map(|p| PaymentFact {
                channel_name: names.get(&p.channel_id).cloned().unwrap_or_default(),
                status: p.status,
                amount: p.amount,
                fee_amount: p.fee_amount,
                fee_policy: p.fee_policy,
                channel_id: p.channel_id,
                provider_type: p.provider_type,
                channel_type: p.channel_type,
                created_at: p.created_at,
            })
            .collect())
    }

    async fn items(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<ItemFact>> {
        let orders = self.profit_orders(start, end).await?;
        let order_ids: Vec<Id> = orders.keys().copied().collect();
        let items = self.items_of(&order_ids).await?;
        let sku_ids = unique(items.iter().map(|i| i.sku_id).filter(|id| *id > 0));
        let mut skus: HashMap<Id, (String, Option<Value>)> = HashMap::new();
        for chunk in sku_ids.chunks(CHUNK) {
            let found: Vec<(Id, String, Option<Value>)> = product_skus::Entity::find()
                .select_only()
                .column(product_skus::Column::Id)
                .column(product_skus::Column::SkuCode)
                .column(product_skus::Column::SpecValuesJson)
                .filter(product_skus::Column::DeletedAt.is_null())
                .filter(product_skus::Column::Id.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            skus.extend(found.into_iter().map(|(id, code, spec)| (id, (code, spec))));
        }
        Ok(items
            .into_iter()
            .filter_map(|i| {
                let (status, created_at) = orders.get(&i.order_id)?.clone();
                let (sku_code, sku_spec_values) = skus.get(&i.sku_id).cloned().unwrap_or_default();
                Some(ItemFact {
                    order_id: i.order_id,
                    order_status: status,
                    order_created_at: created_at,
                    product_id: i.product_id,
                    sku_id: i.sku_id,
                    title: localized_title(i.title_json.as_ref()),
                    sku_code,
                    sku_spec_values,
                    quantity: i64::from(i.quantity),
                    total_price: i.total_price,
                    coupon_discount: i.coupon_discount,
                    cost_price: i.cost_price,
                })
            })
            .collect())
    }

    async fn refunds(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<RefundFact>> {
        let rows: Vec<(Id, Decimal, Decimal, DateTime<Utc>)> = order_refund_records::Entity::find()
            .select_only()
            .column(order_refund_records::Column::OrderId)
            .column(order_refund_records::Column::Amount)
            .column(order_refund_records::Column::PaymentFeeRefundedAmount)
            .column(order_refund_records::Column::CreatedAt)
            .filter(order_refund_records::Column::DeletedAt.is_null())
            .filter(order_refund_records::Column::CreatedAt.gte(start))
            .filter(order_refund_records::Column::CreatedAt.lt(end))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let refunded = unique(rows.iter().map(|r| r.0).filter(|id| *id > 0));
        // The refunded orders and their direct children (parent orders have no items).
        let mut totals: HashMap<Id, Decimal> = HashMap::new();
        let mut parent_of: HashMap<Id, Id> = HashMap::new();
        for chunk in refunded.chunks(CHUNK) {
            let found: Vec<(Id, Option<Id>, Decimal)> = orders::Entity::find()
                .select_only()
                .column(orders::Column::Id)
                .column(orders::Column::ParentId)
                .column(orders::Column::TotalAmount)
                .filter(orders::Column::DeletedAt.is_null())
                .filter(
                    sea_orm::Condition::any()
                        .add(orders::Column::Id.is_in(chunk.to_vec()))
                        .add(orders::Column::ParentId.is_in(chunk.to_vec())),
                )
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            for (id, parent, total) in found {
                if refunded.contains(&id) {
                    totals.insert(id, total);
                }
                if let Some(parent) = parent.filter(|p| refunded.contains(p)) {
                    parent_of.insert(id, parent);
                }
            }
        }
        let cost_orders = unique(totals.keys().chain(parent_of.keys()).copied());
        let direct = self.cost_by_order(&cost_orders).await?;
        let mut basis: HashMap<Id, Decimal> = HashMap::new();
        for (order, cost) in &direct {
            if totals.contains_key(order) {
                *basis.entry(*order).or_default() += *cost;
            }
            if let Some(parent) = parent_of.get(order) {
                *basis.entry(*parent).or_default() += *cost;
            }
        }
        Ok(rows
            .into_iter()
            .map(|(order_id, amount, fee, created_at)| RefundFact {
                order_id,
                amount,
                payment_fee_refunded: fee,
                order_total: totals.get(&order_id).copied().unwrap_or_default(),
                cost_basis: basis.get(&order_id).copied().unwrap_or_default(),
                created_at,
            })
            .collect())
    }

    async fn new_users(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<i64> {
        let n = users::Entity::find()
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::CreatedAt.gte(start))
            .filter(users::Column::CreatedAt.lt(end))
            .count(&self.db)
            .await
            .dom()?;
        Ok(i64::try_from(n).unwrap_or(i64::MAX))
    }

    async fn active_products(&self) -> Result<i64> {
        let n = products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::IsActive.eq(true))
            .count(&self.db)
            .await
            .dom()?;
        Ok(i64::try_from(n).unwrap_or(i64::MAX))
    }

    async fn total_user_balance(&self) -> Result<Decimal> {
        let balances: Vec<Decimal> = wallet_accounts::Entity::find()
            .select_only()
            .column(wallet_accounts::Column::Balance)
            .filter(wallet_accounts::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(balances.into_iter().sum())
    }

    async fn inventory(&self) -> Result<InventorySnapshot> {
        let rows = products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::IsActive.eq(true))
            .order_by_desc(products::Column::SortOrder)
            .order_by_desc(products::Column::CreatedAt)
            .order_by_desc(products::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = rows.iter().map(|p| p.id).collect();
        let mut skus: HashMap<Id, Vec<InvSku>> = HashMap::new();
        for chunk in ids.chunks(CHUNK) {
            let found = product_skus::Entity::find()
                .filter(product_skus::Column::DeletedAt.is_null())
                .filter(product_skus::Column::IsActive.eq(true))
                .filter(product_skus::Column::ProductId.is_in(chunk.to_vec()))
                .order_by_desc(product_skus::Column::SortOrder)
                .order_by_asc(product_skus::Column::CreatedAt)
                .order_by_asc(product_skus::Column::Id)
                .all(&self.db)
                .await
                .dom()?;
            for s in found {
                skus.entry(s.product_id).or_default().push(InvSku {
                    id: s.id,
                    sku_code: s.sku_code,
                    spec_values: s.spec_values_json,
                    manual_stock_total: i64::from(s.manual_stock_total),
                });
            }
        }
        let of_kind = |kind: &str| -> Vec<Id> {
            rows.iter()
                .filter(|p| p.fulfillment_type.trim() == kind)
                .map(|p| p.id)
                .collect()
        };
        let mut snap = InventorySnapshot::default();
        for chunk in of_kind(FULFILLMENT_AUTO).chunks(CHUNK) {
            let counts: Vec<(Id, Id, i64)> = card_secrets::Entity::find()
                .select_only()
                .column_as(card_secrets::Column::ProductId, "product_id")
                .column_as(card_secrets::Column::SkuId, "sku_id")
                .column_as(card_secrets::Column::Id.count(), "total")
                .filter(card_secrets::Column::DeletedAt.is_null())
                .filter(card_secrets::Column::Status.eq(SECRET_AVAILABLE))
                .filter(card_secrets::Column::ProductId.is_in(chunk.to_vec()))
                .group_by(card_secrets::Column::ProductId)
                .group_by(card_secrets::Column::SkuId)
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            for (product, sku, total) in counts {
                snap.secrets.entry(product).or_default().insert(sku, total);
            }
        }
        for chunk in of_kind(FULFILLMENT_UPSTREAM).chunks(CHUNK) {
            let mappings: Vec<(Id, Id)> = product_mappings::Entity::find()
                .select_only()
                .column(product_mappings::Column::Id)
                .column(product_mappings::Column::LocalProductId)
                .filter(product_mappings::Column::DeletedAt.is_null())
                .filter(product_mappings::Column::LocalProductId.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            let product_of: HashMap<Id, Id> = mappings.into_iter().collect();
            if product_of.is_empty() {
                continue;
            }
            let sku_rows: Vec<(Id, Id, i32, bool)> = sku_mappings::Entity::find()
                .select_only()
                .column(sku_mappings::Column::ProductMappingId)
                .column(sku_mappings::Column::LocalSkuId)
                .column(sku_mappings::Column::UpstreamStock)
                .column(sku_mappings::Column::UpstreamIsActive)
                .filter(sku_mappings::Column::DeletedAt.is_null())
                .filter(
                    sku_mappings::Column::ProductMappingId
                        .is_in(product_of.keys().copied().collect::<Vec<_>>()),
                )
                .order_by_asc(sku_mappings::Column::Id)
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            for (mapping, local_sku_id, stock, active) in sku_rows {
                if let Some(product) = product_of.get(&mapping) {
                    snap.upstream
                        .entry(*product)
                        .or_default()
                        .push(UpstreamSku {
                            local_sku_id,
                            upstream_stock: i64::from(stock),
                            upstream_is_active: active,
                        });
                }
            }
        }
        snap.products = rows
            .into_iter()
            .map(|p| InvProduct {
                skus: skus.remove(&p.id).unwrap_or_default(),
                id: p.id,
                title: p.title_json,
                fulfillment_type: p.fulfillment_type,
                manual_stock_total: i64::from(p.manual_stock_total),
            })
            .collect();
        Ok(snap)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn title_coalesces_locales() {
        assert_eq!(
            localized_title(Some(&json!({"en-US": "B", "zh-TW": "A"}))),
            "A"
        );
        assert_eq!(
            localized_title(Some(&json!({"zh-CN": "", "en-US": "B"}))),
            ""
        );
        assert_eq!(
            localized_title(Some(&json!({"zh-CN": null, "en-US": "B"}))),
            "B"
        );
        assert_eq!(localized_title(None), "");
    }
}
