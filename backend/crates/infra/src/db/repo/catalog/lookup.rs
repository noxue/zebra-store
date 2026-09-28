//! [`CatalogLookup`]: catalog reads of other groups' tables (consumer-owned port).

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use zs_domain::catalog::product::{
    CatalogLookup, RelatedPost, UpstreamMapping, UpstreamSkuMapping,
};
use zs_domain::{Id, Result};

use crate::db::entity::{
    order_items, payment_channels, post_products, posts, product_mappings, sku_mappings,
};
use crate::db::repo::support::{DbResultExt, from_json};

/// Post type of blog articles (`constants.PostTypeBlog`).
const POST_TYPE_BLOG: &str = "blog";

/// SeaORM implementation of [`CatalogLookup`].
#[derive(Debug, Clone)]
pub struct SeaCatalogLookup {
    db: DatabaseConnection,
}

impl SeaCatalogLookup {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl CatalogLookup for SeaCatalogLookup {
    async fn active_payment_channel_ids(&self, ids: &[Id]) -> Result<Vec<Id>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        payment_channels::Entity::find()
            .select_only()
            .column(payment_channels::Column::Id)
            .filter(payment_channels::Column::Id.is_in(ids.to_vec()))
            .filter(payment_channels::Column::IsActive.eq(true))
            .filter(payment_channels::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()
    }

    async fn count_order_items(&self, product_id: Id) -> Result<u64> {
        order_items::Entity::find()
            .filter(order_items::Column::ProductId.eq(product_id))
            .filter(order_items::Column::DeletedAt.is_null())
            .count(&self.db)
            .await
            .dom()
    }

    async fn upstream_mappings(&self, product_ids: &[Id]) -> Result<Vec<UpstreamMapping>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mappings = product_mappings::Entity::find()
            .filter(product_mappings::Column::LocalProductId.is_in(product_ids.to_vec()))
            .filter(product_mappings::Column::DeletedAt.is_null())
            .order_by_asc(product_mappings::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        if mappings.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Id> = mappings.iter().map(|m| m.id).collect();
        let mut skus: HashMap<Id, Vec<UpstreamSkuMapping>> = HashMap::new();
        for s in sku_mappings::Entity::find()
            .filter(sku_mappings::Column::ProductMappingId.is_in(ids))
            .filter(sku_mappings::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
        {
            skus.entry(s.product_mapping_id)
                .or_default()
                .push(UpstreamSkuMapping {
                    local_sku_id: s.local_sku_id,
                    upstream_stock: s.upstream_stock,
                    upstream_is_active: s.upstream_is_active,
                });
        }
        let mut seen = std::collections::HashSet::new();
        Ok(mappings
            .into_iter()
            .filter(|m| seen.insert(m.local_product_id))
            .map(|m| UpstreamMapping {
                local_product_id: m.local_product_id,
                upstream_fulfillment_type: m.upstream_fulfillment_type,
                skus: skus.remove(&m.id).unwrap_or_default(),
            })
            .collect())
    }

    async fn related_posts(&self, product_id: Id, limit: u64) -> Result<Vec<RelatedPost>> {
        let links = post_products::Entity::find()
            .filter(post_products::Column::ProductId.eq(product_id))
            .order_by_asc(post_products::Column::Sort)
            .order_by_asc(post_products::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        if links.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<Id> = links.iter().map(|l| l.post_id).collect();
        let mut by_id: HashMap<Id, posts::Model> = posts::Entity::find()
            .filter(posts::Column::Id.is_in(ids))
            .filter(posts::Column::DeletedAt.is_null())
            .filter(posts::Column::Type.eq(POST_TYPE_BLOG))
            .filter(posts::Column::IsPublished.eq(true))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        Ok(links
            .iter()
            .filter_map(|l| by_id.remove(&l.post_id))
            .take(usize::try_from(limit).unwrap_or(usize::MAX))
            .map(|p| RelatedPost {
                id: p.id,
                slug: p.slug,
                kind: p.type_,
                title: from_json(p.title_json),
                summary: from_json(p.summary_json),
                thumbnail: p.thumbnail,
                published_at: p.published_at,
            })
            .collect())
    }
}
