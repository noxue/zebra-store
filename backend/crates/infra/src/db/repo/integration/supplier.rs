//! [`SupplierCatalog`]: catalog / buyer reads behind the upstream API we serve.

use std::sync::Arc;

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder,
    QuerySelect,
};
use zs_domain::catalog::card_secret::CardSecretRepo;
use zs_domain::catalog::category::{Category, CategoryRepo};
use zs_domain::catalog::product::{CatalogLookup, Product, SkuScope, UpstreamMapping};
use zs_domain::catalog::storefront::apply_auto_stock_counts;
use zs_domain::integration::protocol::ProductQuery;
use zs_domain::integration::supplier::{Buyer, SiteInfo, SupplierCatalog};
use zs_domain::marketing::member_level::{MemberLevel, MemberLevelPrice};
use zs_domain::settings::schema::site::{SiteBrand, normalize_currency};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

use crate::db::entity::{
    member_level_prices, member_levels, product_skus, products, users, wallet_accounts,
};
use crate::db::repo::catalog::card_secret::SeaCardSecretRepo;
use crate::db::repo::catalog::category::SeaCategoryRepo;
use crate::db::repo::catalog::lookup::SeaCatalogLookup;
use crate::db::repo::catalog::product::load_product;
use crate::db::repo::support::{DbResultExt, from_json};

/// `EXISTS` an active, live category of the product row.
const ACTIVE_CATEGORY_SQL: &str = "EXISTS (SELECT 1 FROM categories c WHERE c.id = products.category_id \
     AND c.is_active = true AND c.deleted_at IS NULL)";

/// SeaORM implementation of [`SupplierCatalog`].
#[derive(Clone)]
pub struct SeaSupplierCatalog {
    db: DatabaseConnection,
    settings: Arc<dyn SettingsStore>,
    categories: SeaCategoryRepo,
    secrets: SeaCardSecretRepo,
    lookup: SeaCatalogLookup,
}

impl std::fmt::Debug for SeaSupplierCatalog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeaSupplierCatalog")
    }
}

impl SeaSupplierCatalog {
    pub fn new(db: DatabaseConnection, settings: Arc<dyn SettingsStore>) -> Self {
        Self {
            categories: SeaCategoryRepo::new(db.clone()),
            secrets: SeaCardSecretRepo::new(db.clone()),
            lookup: SeaCatalogLookup::new(db.clone()),
            settings,
            db,
        }
    }

    async fn with_stock(&self, mut items: Vec<Product>) -> Result<Vec<Product>> {
        let ids: Vec<Id> = items.iter().map(|p| p.id).collect();
        match self.secrets.stock_counts(&ids).await {
            Ok(counts) => apply_auto_stock_counts(&mut items, &counts),
            Err(error) => tracing::warn!(%error, "upstream api stock counts failed"),
        }
        Ok(items)
    }
}

fn level_of(m: member_levels::Model) -> MemberLevel {
    MemberLevel {
        id: m.id,
        name: from_json(m.name_json),
        slug: m.slug,
        icon: m.icon,
        discount_rate: Amount::new(m.discount_rate),
        recharge_threshold: Amount::new(m.recharge_threshold),
        spend_threshold: Amount::new(m.spend_threshold),
        is_default: m.is_default,
        sort_order: m.sort_order,
        is_active: m.is_active,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

#[async_trait]
impl SupplierCatalog for SeaSupplierCatalog {
    async fn buyer(&self, user_id: Id) -> Result<Option<Buyer>> {
        let Some(user) = users::Entity::find_by_id(user_id)
            .filter(users::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let balance = wallet_accounts::Entity::find()
            .filter(wallet_accounts::Column::UserId.eq(user_id))
            .filter(wallet_accounts::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(|w| Amount::new(w.balance))
            .unwrap_or(Amount::ZERO);
        let member_level = if user.member_level_id > 0 {
            member_levels::Entity::find_by_id(user.member_level_id)
                .filter(member_levels::Column::DeletedAt.is_null())
                .one(&self.db)
                .await
                .dom()?
                .map(level_of)
        } else {
            None
        };
        Ok(Some(Buyer {
            user_id,
            balance,
            member_level,
        }))
    }

    async fn site_info(&self) -> Result<SiteInfo> {
        let raw = self.settings.get(setting_keys::SITE_CONFIG).await?;
        let mut site_name = raw
            .as_ref()
            .and_then(|v| v.get("site_name"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let brand = SiteBrand::decode(raw.as_ref());
        if site_name.trim().is_empty() {
            site_name = brand.site_name;
        }
        Ok(SiteInfo {
            site_name,
            currency: normalize_currency(raw.as_ref().and_then(|v| v.get("currency"))),
            site_url: brand.site_url,
        })
    }

    async fn categories(&self) -> Result<Vec<Category>> {
        self.categories.list(false).await
    }

    async fn products(&self, query: &ProductQuery) -> Result<Page<Product>> {
        let mut q = products::Entity::find().filter(products::Column::DeletedAt.is_null());
        if !query.include_inactive {
            q = q
                .filter(products::Column::IsActive.eq(true))
                .filter(Expr::cust(ACTIVE_CATEGORY_SQL));
        }
        if let Some(after) = query.updated_after {
            q = q.filter(products::Column::UpdatedAt.gt(after));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let page = u64::try_from(query.page.max(1)).unwrap_or(1);
        let size = u64::try_from(query.page_size.max(1)).unwrap_or(1);
        let ids: Vec<Id> = q
            .select_only()
            .column(products::Column::Id)
            .order_by_desc(products::Column::SortOrder)
            .order_by_desc(products::Column::CreatedAt)
            .order_by_desc(products::Column::Id)
            .offset((page - 1) * size)
            .limit(size)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(p) = load_product(&self.db, id, SkuScope::All, false).await? {
                items.push(p);
            }
        }
        Ok(Page {
            items: self.with_stock(items).await?,
            total,
        })
    }

    async fn products_after(&self, after_id: Id, limit: u64) -> Result<(Vec<Product>, u64)> {
        let live = products::Entity::find().filter(products::Column::DeletedAt.is_null());
        let total = live.clone().count(&self.db).await.dom()?;
        let ids: Vec<Id> = live
            .filter(products::Column::Id.gt(after_id))
            .select_only()
            .column(products::Column::Id)
            .order_by_asc(products::Column::Id)
            .limit(limit.max(1))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        let mut items = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(p) = load_product(&self.db, id, SkuScope::All, false).await? {
                items.push(p);
            }
        }
        Ok((self.with_stock(items).await?, total))
    }

    async fn product(&self, id: Id) -> Result<Option<Product>> {
        let Some(p) = load_product(&self.db, id, SkuScope::All, false).await? else {
            return Ok(None);
        };
        Ok(self.with_stock(vec![p]).await?.pop())
    }

    async fn product_of_sku(&self, sku_id: Id) -> Result<Option<(Product, bool)>> {
        let Some(sku) = product_skus::Entity::find_by_id(sku_id)
            .filter(product_skus::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let Some(p) = load_product(&self.db, sku.product_id, SkuScope::All, false).await? else {
            return Ok(None);
        };
        // Real auto stock (card secrets) like every other projection (UPS-15).
        Ok(self
            .with_stock(vec![p])
            .await?
            .pop()
            .map(|p| (p, sku.is_active)))
    }

    async fn upstream_mappings(&self, product_ids: &[Id]) -> Result<Vec<UpstreamMapping>> {
        self.lookup.upstream_mappings(product_ids).await
    }

    async fn member_prices(
        &self,
        level_id: Id,
        product_ids: &[Id],
    ) -> Result<Vec<MemberLevelPrice>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(member_level_prices::Entity::find()
            .filter(member_level_prices::Column::MemberLevelId.eq(level_id))
            .filter(member_level_prices::Column::ProductId.is_in(product_ids.to_vec()))
            .filter(member_level_prices::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|m| MemberLevelPrice {
                id: m.id,
                member_level_id: m.member_level_id,
                product_id: m.product_id,
                sku_id: m.sku_id,
                price_amount: Amount::new(m.price_amount),
                created_at: m.created_at,
                updated_at: m.updated_at,
            })
            .collect())
    }
}
