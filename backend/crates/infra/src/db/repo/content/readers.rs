//! Read-only adapters over other modules' tables used by the content group:
//! public payment channels and sitemap catalogue entries.

use async_trait::async_trait;
use sea_orm::sea_query::Query;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use zs_domain::Result;
use zs_domain::content::public::{
    PaymentChannelReader, PaymentChannelView, SitemapCatalogReader, SitemapEntry,
};
use zs_shared::money::Amount;

use crate::db::entity::{categories, payment_channels, products};
use crate::db::repo::support::{DbResultExt, from_json};

/// Page size of the original `GetAvailableChannels` listing.
const CHANNEL_LIMIT: u64 = 200;

/// Reads `payment_channels` for `/public/config`.
#[derive(Debug, Clone)]
pub struct SeaPaymentChannelReader {
    db: DatabaseConnection,
}

impl SeaPaymentChannelReader {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl PaymentChannelReader for SeaPaymentChannelReader {
    async fn active_channels(&self) -> Result<Vec<PaymentChannelView>> {
        let rows = payment_channels::Entity::find()
            .filter(payment_channels::Column::DeletedAt.is_null())
            .filter(payment_channels::Column::IsActive.eq(true))
            .order_by_desc(payment_channels::Column::SortOrder)
            .order_by_asc(payment_channels::Column::Id)
            .limit(CHANNEL_LIMIT)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|m| {
                let channel_type = m.channel_type;
                let provider_type = m.provider_type;
                let config: serde_json::Value = from_json(m.config_json);
                let configured_types = if provider_type == "huifu" {
                    config
                        .get("supported_channel_types")
                        .and_then(serde_json::Value::as_array)
                        .map(|values| {
                            values
                                .iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(str::to_owned)
                                .filter(|value| matches!(value.as_str(), "wechat" | "alipay"))
                                .collect::<Vec<_>>()
                        })
                        .filter(|values| !values.is_empty())
                        .unwrap_or_else(|| vec![channel_type.clone()])
                } else if provider_type == "epusdt"
                    && config
                        .get("order_mode")
                        .and_then(serde_json::Value::as_str)
                        .is_none_or(|mode| mode != "cashier")
                {
                    let token = config
                        .get("token")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("usdt")
                        .trim()
                        .to_ascii_lowercase();
                    let network = config
                        .get("network")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("tron")
                        .trim()
                        .to_ascii_lowercase();
                    if token.is_empty() || network.is_empty() {
                        vec![channel_type.clone()]
                    } else {
                        vec![format!("{token}.{network}")]
                    }
                } else {
                    vec![channel_type.clone()]
                };
                let supported_channel_types = if configured_types.len() > 1
                    || configured_types
                        .first()
                        .is_some_and(|value| value != &channel_type)
                {
                    configured_types
                } else {
                    Vec::new()
                };
                PaymentChannelView {
                    id: m.id,
                    name: m.name,
                    icon: m.icon,
                    provider_type,
                    channel_type,
                    supported_channel_types,
                    interaction_mode: m.interaction_mode,
                    fee_rate: Amount::new(m.fee_rate),
                    fixed_fee: Amount::new(m.fixed_fee),
                    min_amount: Amount::new(m.min_amount),
                    max_amount: Amount::new(m.max_amount),
                    hide_amount_out_range: m.hide_amount_out_range,
                    payment_roles: from_json(m.payment_roles),
                    member_levels: from_json(m.member_levels),
                    payment_types: from_json(m.payment_types),
                }
            })
            .collect())
    }
}

/// Reads active categories/products for the sitemap.
#[derive(Debug, Clone)]
pub struct SeaSitemapCatalogReader {
    db: DatabaseConnection,
}

impl SeaSitemapCatalogReader {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SitemapCatalogReader for SeaSitemapCatalogReader {
    async fn active_categories(&self) -> Result<Vec<SitemapEntry>> {
        let rows = categories::Entity::find()
            .filter(categories::Column::DeletedAt.is_null())
            .filter(categories::Column::IsActive.eq(true))
            .order_by_desc(categories::Column::SortOrder)
            .order_by_asc(categories::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|c| SitemapEntry {
                id: c.id,
                slug: c.slug,
                modified_at: c.created_at,
            })
            .collect())
    }

    async fn active_products(&self, limit: u64) -> Result<Vec<SitemapEntry>> {
        let active_categories = Query::select()
            .column(categories::Column::Id)
            .from(categories::Entity)
            .and_where(categories::Column::IsActive.eq(true))
            .and_where(categories::Column::DeletedAt.is_null())
            .to_owned();
        let rows = products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::IsActive.eq(true))
            .filter(products::Column::CategoryId.in_subquery(active_categories))
            .order_by_desc(products::Column::SortOrder)
            .order_by_desc(products::Column::CreatedAt)
            .limit(limit)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|p| SitemapEntry {
                id: p.id,
                slug: p.slug,
                modified_at: p.updated_at,
            })
            .collect())
    }
}
