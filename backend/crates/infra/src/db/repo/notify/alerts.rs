//! [`AlertSource`] over the catalog, card secret, mapping, order and payment
//! tables (consumer-side port of the original dashboard alert queries).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use serde_json::Value;
use zs_domain::notify::center::{InventoryAlertRow, PaymentOrderAlertCounts};
use zs_domain::notify::inventory::{
    StockProduct, StockSku, UpstreamSkuStock, collect_auto, collect_manual, collect_upstream,
};
use zs_domain::notify::ports::AlertSource;
use zs_domain::{Id, Result};

use crate::db::entity::{
    card_secrets, orders, payments, product_mappings, product_skus, products, sku_mappings,
};
use crate::db::repo::support::DbResultExt;

/// Card secret status counted as stock.
const SECRET_AVAILABLE: &str = "available";
/// Parent order status counted by the pending-payment alert.
const ORDER_PENDING_PAYMENT: &str = "pending_payment";
/// Payment status counted by the failed-payment alert.
const PAYMENT_FAILED: &str = "failed";
/// Wallet payments are not online payments.
const PROVIDER_WALLET: &str = "wallet";

/// SeaORM implementation of [`AlertSource`].
#[derive(Debug, Clone)]
pub struct SeaAlertSource {
    db: DatabaseConnection,
}

impl SeaAlertSource {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn active_products(&self) -> Result<Vec<StockProduct>> {
        let rows = products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::IsActive.eq(true))
            .order_by_desc(products::Column::SortOrder)
            .order_by_desc(products::Column::CreatedAt)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = rows.iter().map(|p| p.id).collect();
        let mut skus: HashMap<Id, Vec<StockSku>> = HashMap::new();
        if !ids.is_empty() {
            let sku_rows = product_skus::Entity::find()
                .filter(product_skus::Column::ProductId.is_in(ids))
                .filter(product_skus::Column::DeletedAt.is_null())
                .filter(product_skus::Column::IsActive.eq(true))
                .order_by_desc(product_skus::Column::SortOrder)
                .order_by_asc(product_skus::Column::CreatedAt)
                .all(&self.db)
                .await
                .dom()?;
            for s in sku_rows {
                skus.entry(s.product_id).or_default().push(StockSku {
                    id: s.id,
                    sku_code: s.sku_code,
                    spec_values: s.spec_values_json.unwrap_or(Value::Null),
                    is_active: s.is_active,
                    manual_stock_total: i64::from(s.manual_stock_total),
                });
            }
        }
        Ok(rows
            .into_iter()
            .map(|p| StockProduct {
                id: p.id,
                title: p.title_json.unwrap_or(Value::Null),
                fulfillment_type: p.fulfillment_type.trim().to_owned(),
                manual_stock_total: i64::from(p.manual_stock_total),
                skus: skus.remove(&p.id).unwrap_or_default(),
            })
            .collect())
    }

    async fn secret_counts(&self, product_ids: Vec<Id>) -> Result<HashMap<Id, HashMap<Id, i64>>> {
        let mut out: HashMap<Id, HashMap<Id, i64>> = HashMap::new();
        if product_ids.is_empty() {
            return Ok(out);
        }
        let rows: Vec<(Id, Id, i64)> = card_secrets::Entity::find()
            .select_only()
            .column(card_secrets::Column::ProductId)
            .column(card_secrets::Column::SkuId)
            .column_as(card_secrets::Column::Id.count(), "total")
            .filter(card_secrets::Column::ProductId.is_in(product_ids))
            .filter(card_secrets::Column::Status.eq(SECRET_AVAILABLE))
            .filter(card_secrets::Column::DeletedAt.is_null())
            .group_by(card_secrets::Column::ProductId)
            .group_by(card_secrets::Column::SkuId)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        for (product, sku, total) in rows {
            out.entry(product).or_default().insert(sku, total);
        }
        Ok(out)
    }

    async fn upstream_stock(
        &self,
        product_ids: Vec<Id>,
    ) -> Result<HashMap<Id, Vec<UpstreamSkuStock>>> {
        let mut out: HashMap<Id, Vec<UpstreamSkuStock>> = HashMap::new();
        if product_ids.is_empty() {
            return Ok(out);
        }
        let mappings = product_mappings::Entity::find()
            .filter(product_mappings::Column::LocalProductId.is_in(product_ids))
            .filter(product_mappings::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?;
        let by_mapping: HashMap<Id, Id> = mappings
            .iter()
            .map(|m| (m.id, m.local_product_id))
            .collect();
        if by_mapping.is_empty() {
            return Ok(out);
        }
        let rows = sku_mappings::Entity::find()
            .filter(
                sku_mappings::Column::ProductMappingId
                    .is_in(by_mapping.keys().copied().collect::<Vec<_>>()),
            )
            .filter(sku_mappings::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?;
        for r in rows {
            if let Some(product) = by_mapping.get(&r.product_mapping_id) {
                out.entry(*product).or_default().push(UpstreamSkuStock {
                    local_sku_id: r.local_sku_id,
                    upstream_stock: i64::from(r.upstream_stock),
                    upstream_is_active: r.upstream_is_active,
                });
            }
        }
        Ok(out)
    }
}

#[async_trait]
impl AlertSource for SeaAlertSource {
    async fn inventory_alerts(&self, low: i64) -> Result<Vec<InventoryAlertRow>> {
        let products = self.active_products().await?;
        let ids_of = |kind: &str| -> Vec<Id> {
            products
                .iter()
                .filter(|p| p.fulfillment_type == kind)
                .map(|p| p.id)
                .collect()
        };
        let secrets = self.secret_counts(ids_of("auto")).await?;
        let upstream = self.upstream_stock(ids_of("upstream")).await?;
        let empty_counts = HashMap::new();
        let mut rows = Vec::new();
        for p in &products {
            match p.fulfillment_type.as_str() {
                "auto" => rows.extend(collect_auto(
                    p,
                    secrets.get(&p.id).unwrap_or(&empty_counts),
                    low,
                )),
                "manual" => rows.extend(collect_manual(p, low)),
                "upstream" => rows.extend(collect_upstream(
                    p,
                    upstream.get(&p.id).map(Vec::as_slice).unwrap_or_default(),
                    low,
                )),
                _ => {}
            }
        }
        Ok(rows)
    }

    async fn payment_order_counts(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<PaymentOrderAlertCounts> {
        let pending = orders::Entity::find()
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::ParentId.is_null())
            .filter(orders::Column::Status.eq(ORDER_PENDING_PAYMENT))
            .filter(orders::Column::CreatedAt.gte(start))
            .filter(orders::Column::CreatedAt.lte(end))
            .count(&self.db)
            .await
            .dom()?;
        let failed = payments::Entity::find()
            .filter(payments::Column::DeletedAt.is_null())
            .filter(payments::Column::ProviderType.ne(PROVIDER_WALLET))
            .filter(payments::Column::Status.eq(PAYMENT_FAILED))
            .filter(payments::Column::CreatedAt.gte(start))
            .filter(payments::Column::CreatedAt.lte(end))
            .count(&self.db)
            .await
            .dom()?;
        Ok(PaymentOrderAlertCounts {
            pending_payment_orders: i64::try_from(pending).unwrap_or(i64::MAX),
            payments_failed: i64::try_from(failed).unwrap_or(i64::MAX),
        })
    }
}
