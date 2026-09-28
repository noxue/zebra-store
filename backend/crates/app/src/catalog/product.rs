//! Product use cases: admin queries and writes, batch operations, public catalog.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use zs_domain::catalog::card_secret::CardSecretRepo;
use zs_domain::catalog::category::CategoryRepo;
use zs_domain::catalog::manual_form;
use zs_domain::catalog::product::{
    CatalogLookup, FulfillmentType, Product, ProductFilter, ProductInput, ProductRepo,
    ProductSavePlan, PurchaseType, QuickUpdate, SkuPlan, SkuScope, StockFilter,
    apply_upstream_admin_display, encode_payment_channel_ids, keys, normalize_purchase_limit,
    normalize_sku_inputs, skus_losing_activation, unique_positive_ids,
};
use zs_domain::catalog::stock::{MANUAL_STOCK_UNLIMITED, StockDisplayMode};
use zs_domain::catalog::storefront::{
    Decoration, PublicProduct, apply_auto_stock_counts, apply_reseller_display,
    decorate_public_product, priced_product,
};
use zs_domain::catalog::wholesale::{WholesalePriceInput, normalize_wholesale_prices_for_skus};
use zs_domain::marketing::member_level::MemberLevelRepo;
use zs_domain::marketing::promotion::PromotionRepo;
use zs_domain::reseller::pricing::display_prices;
use zs_domain::reseller::tenant::ResellerTenant;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

use crate::reseller::PricingReads;

/// Related posts shown on a product page (original `publicRelatedPostsLimit`).
const RELATED_POSTS_LIMIT: u64 = 6;
/// Dashboard low-stock threshold default and accepted range (original `NormalizeDashboardSetting`).
const DEFAULT_LOW_STOCK_THRESHOLD: i64 = 5;
const MAX_LOW_STOCK_THRESHOLD: i64 = 500;

/// Admin list query.
#[derive(Debug, Clone, Default)]
pub struct AdminProductQuery {
    pub page: PageRequest,
    pub category_id: Option<Id>,
    pub search: String,
    pub fulfillment_type: String,
    pub stock_status: String,
    pub has_wholesale_prices: Option<bool>,
    pub is_active: Option<bool>,
}

/// Result of one item of a batch operation.
#[derive(Debug)]
pub struct BatchOutcome {
    pub total: usize,
    pub success_count: usize,
    pub failures: Vec<(Id, Error)>,
}

/// Dependencies of [`ProductService`].
#[derive(Clone)]
pub struct ProductDeps {
    pub products: Arc<dyn ProductRepo>,
    pub categories: Arc<dyn CategoryRepo>,
    pub secrets: Arc<dyn CardSecretRepo>,
    pub lookup: Arc<dyn CatalogLookup>,
    pub promotions: Arc<dyn PromotionRepo>,
    pub levels: Arc<dyn MemberLevelRepo>,
    pub settings: Arc<dyn SettingsStore>,
    pub clock: Arc<dyn Clock>,
    /// Reseller site prices and hidden products (LQA-R1).
    pub reseller: PricingReads,
}

impl std::fmt::Debug for ProductDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProductDeps")
    }
}

/// Product queries and admin commands.
#[derive(Clone)]
pub struct ProductService {
    d: ProductDeps,
}

impl std::fmt::Debug for ProductService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProductService")
    }
}

fn not_found() -> Error {
    Error::not_found(keys::NOT_FOUND)
}

impl ProductService {
    pub fn new(deps: ProductDeps) -> Self {
        Self { d: deps }
    }

    // ------------------------------------------------------------------ admin reads

    /// Dashboard low-stock threshold (`dashboard_config.alert.low_stock_threshold`, 1..=500, default 5).
    async fn low_stock_threshold(&self) -> i64 {
        let value = self
            .d
            .settings
            .get(setting_keys::DASHBOARD_CONFIG)
            .await
            .ok()
            .flatten();
        value
            .as_ref()
            .and_then(|v| v.get("alert"))
            .and_then(|a| a.get("low_stock_threshold"))
            .and_then(serde_json::Value::as_i64)
            .filter(|t| (1..=MAX_LOW_STOCK_THRESHOLD).contains(t))
            .unwrap_or(DEFAULT_LOW_STOCK_THRESHOLD)
    }

    pub async fn list_admin(&self, q: AdminProductQuery) -> Result<Page<Product>> {
        let filter = ProductFilter {
            page: q.page,
            category_id: q.category_id,
            category_ids: None,
            search: q.search.trim().to_owned(),
            fulfillment_type: q.fulfillment_type.trim().to_owned(),
            stock: StockFilter::parse(&q.stock_status),
            has_wholesale_prices: q.has_wholesale_prices,
            low_stock_threshold: self.low_stock_threshold().await,
            only_active: false,
            is_active: q.is_active,
            exclude_ids: Vec::new(),
        };
        let mut page = self
            .d
            .products
            .list(&filter)
            .await
            .map_err(|e| e.or_internal("error.product_fetch_failed"))?;
        self.decorate_admin(&mut page.items).await?;
        Ok(page)
    }

    pub async fn get_admin(&self, id: Id) -> Result<Product> {
        let product = self
            .d
            .products
            .get(id, SkuScope::All)
            .await
            .map_err(|e| e.or_internal("error.product_fetch_failed"))?
            .ok_or_else(not_found)?;
        let mut list = vec![product];
        self.decorate_admin(&mut list).await?;
        list.pop().ok_or_else(not_found)
    }

    async fn apply_stock(&self, products: &mut [Product]) -> Result<()> {
        let ids: Vec<Id> = products
            .iter()
            .filter(|p| p.is_auto())
            .map(|p| p.id)
            .collect();
        if ids.is_empty() {
            return Ok(());
        }
        let counts = self
            .d
            .secrets
            .stock_counts(&ids)
            .await
            .map_err(|e| e.or_internal("error.product_fetch_failed"))?;
        apply_auto_stock_counts(products, &counts);
        Ok(())
    }

    async fn decorate_admin(&self, products: &mut [Product]) -> Result<()> {
        self.apply_stock(products).await?;
        let upstream: Vec<Id> = products
            .iter()
            .filter(|p| p.fulfillment_type == FulfillmentType::Upstream)
            .map(|p| p.id)
            .collect();
        if upstream.is_empty() {
            return Ok(());
        }
        // Best effort, like the original: mapping lookup failures keep the stored values.
        if let Ok(mappings) = self.d.lookup.upstream_mappings(&upstream).await {
            for mapping in &mappings {
                if let Some(p) = products
                    .iter_mut()
                    .find(|p| p.id == mapping.local_product_id)
                {
                    apply_upstream_admin_display(p, mapping);
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ writes

    /// A product may only be assigned to an existing leaf category (keeping the current one is allowed).
    async fn validate_category_assignment(&self, category_id: Id, current: Id) -> Result<()> {
        if category_id == 0 {
            return Ok(());
        }
        let invalid = || Error::bad_request(keys::CATEGORY_INVALID);
        if self.d.categories.get(category_id).await?.is_none() {
            return Err(invalid());
        }
        if self.d.categories.count_children(category_id).await? > 0 && category_id != current {
            return Err(invalid());
        }
        Ok(())
    }

    /// Activation requires an existing, active leaf category (ORD-09).
    async fn validate_activation_category(&self, category_id: Id) -> Result<()> {
        let invalid = || Error::bad_request(keys::CATEGORY_INVALID);
        if category_id <= 0 {
            return Err(invalid());
        }
        match self.d.categories.get(category_id).await? {
            Some(c) if c.is_active => {}
            _ => return Err(invalid()),
        }
        if self.d.categories.count_children(category_id).await? > 0 {
            return Err(invalid());
        }
        Ok(())
    }

    /// Keeps only active payment channels (PAY-17); all invalid → unrestricted.
    async fn filter_payment_channels(&self, ids: &[Id]) -> Result<String> {
        let unique = unique_positive_ids(ids);
        if unique.is_empty() {
            return Ok(String::new());
        }
        let active: HashSet<Id> = self
            .d
            .lookup
            .active_payment_channel_ids(&unique)
            .await?
            .into_iter()
            .collect();
        let kept: Vec<Id> = unique
            .into_iter()
            .filter(|id| active.contains(id))
            .collect();
        Ok(encode_payment_channel_ids(&kept))
    }

    /// Rejects deactivating/removing auto SKUs that still hold unused card secrets (DLV-04).
    async fn guard_auto_sku_stock(&self, product_id: Id, lost: &[Id]) -> Result<()> {
        for sku_id in lost {
            let stats = self.d.secrets.stats(product_id, *sku_id).await?;
            if stats.available > 0 || stats.total - stats.used > 0 {
                return Err(Error::bad_request(keys::SKU_HAS_CARD_SECRET_STOCK));
            }
        }
        Ok(())
    }

    fn schema_for(
        fulfillment: FulfillmentType,
        input: &ProductInput,
    ) -> Result<zs_domain::catalog::product::JsonMap> {
        if fulfillment == FulfillmentType::Manual {
            manual_form::normalize_schema(&input.manual_form_schema.clone().unwrap_or_default())
        } else {
            Ok(Default::default())
        }
    }

    pub async fn create(&self, input: ProductInput) -> Result<Product> {
        self.validate_category_assignment(input.category_id, 0)
            .await?;
        if self.d.products.count_by_slug(&input.slug, None).await? > 0 {
            return Err(Error::bad_request(keys::SLUG_EXISTS));
        }
        let purchase_type = PurchaseType::parse_input(&input.purchase_type)
            .ok_or_else(|| Error::bad_request(keys::PURCHASE_INVALID))?;
        let fulfillment = FulfillmentType::parse_input(&input.fulfillment_type)
            .ok_or_else(|| Error::bad_request(keys::FULFILLMENT_INVALID))?;
        if input.skus.is_empty() && !input.price_amount.is_positive() {
            return Err(Error::bad_request(keys::PRICE_INVALID));
        }
        let mut manual_stock_total = input.manual_stock_total.unwrap_or(0);
        if manual_stock_total < MANUAL_STOCK_UNLIMITED {
            return Err(Error::bad_request(keys::MANUAL_STOCK_INVALID));
        }
        let max = input
            .max_purchase_quantity
            .map_or(0, normalize_purchase_limit);
        let min = input
            .min_purchase_quantity
            .map_or(0, normalize_purchase_limit);
        if min > 0 && max > 0 && min > max {
            return Err(Error::bad_request(keys::PURCHASE_LIMIT_INVALID));
        }
        let stock_display_mode =
            StockDisplayMode::parse_input(&input.stock_display_mode).ok_or_else(Error::invalid)?;

        let mut price = input.price_amount;
        let mut cost = input.cost_price_amount;
        let skus = if input.skus.is_empty() {
            None
        } else {
            let normalized = normalize_sku_inputs(&input.skus, fulfillment, None)?;
            price = normalized.min_price;
            cost = normalized.min_cost;
            manual_stock_total = normalized.manual_stock_total;
            Some(normalized.rows)
        };
        let payment_channel_ids = self
            .filter_payment_channels(&input.payment_channel_ids)
            .await?;
        let manual_form_schema = Self::schema_for(fulfillment, &input)?;

        let plan = ProductSavePlan {
            id: None,
            fields: zs_domain::catalog::product::ProductFields {
                category_id: input.category_id,
                slug: input.slug.clone(),
                seo_meta: input.seo_meta.clone().unwrap_or_default(),
                title: input.title.clone().unwrap_or_default(),
                description: input.description.clone().unwrap_or_default(),
                content: input.content.clone().unwrap_or_default(),
                instructions: input.instructions.clone().unwrap_or_default(),
                manual_form_schema,
                price_amount: price,
                cost_price_amount: cost,
                images: input.images.clone(),
                tags: input.tags.clone(),
                purchase_type,
                min_purchase_quantity: min,
                max_purchase_quantity: max,
                stock_display_mode,
                fulfillment_type: fulfillment,
                manual_stock_total,
                payment_channel_ids,
                is_affiliate_enabled: input.is_affiliate_enabled.unwrap_or(false),
                is_active: input.is_active.unwrap_or(true),
                sort_order: input.sort_order,
            },
            skus: match skus {
                Some(rows) => SkuPlan::Multi(rows),
                None => SkuPlan::Single {
                    price,
                    cost,
                    manual_stock_total,
                },
            },
            wholesale: input.wholesale_prices.clone(),
        };
        let id = self
            .d
            .products
            .save(&plan)
            .await
            .map_err(|e| e.or_internal("error.product_create_failed"))?;
        self.d
            .products
            .get(id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_create_failed"))?
            .ok_or_else(not_found)
    }

    pub async fn update(&self, id: Id, input: ProductInput) -> Result<Product> {
        if input.skus.is_empty() && !input.price_amount.is_positive() {
            return Err(Error::bad_request(keys::PRICE_INVALID));
        }
        let current = self
            .d
            .products
            .get(id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?
            .ok_or_else(not_found)?;
        self.validate_category_assignment(input.category_id, current.category_id)
            .await?;
        if self.d.products.count_by_slug(&input.slug, Some(id)).await? > 0 {
            return Err(Error::bad_request(keys::SLUG_USED));
        }
        let payment_channel_ids = self
            .filter_payment_channels(&input.payment_channel_ids)
            .await?;
        let purchase_type = if input.purchase_type.trim().is_empty() {
            current.purchase_type
        } else {
            PurchaseType::parse_input(&input.purchase_type)
                .ok_or_else(|| Error::bad_request(keys::PURCHASE_INVALID))?
        };
        let max = input
            .max_purchase_quantity
            .map_or(current.max_purchase_quantity, normalize_purchase_limit);
        let min = input
            .min_purchase_quantity
            .map_or(current.min_purchase_quantity, normalize_purchase_limit);
        if min > 0 && max > 0 && min > max {
            return Err(Error::bad_request(keys::PURCHASE_LIMIT_INVALID));
        }
        let stock_display_mode =
            StockDisplayMode::parse_input(&input.stock_display_mode).ok_or_else(Error::invalid)?;
        let mut fulfillment = if input.fulfillment_type.trim().is_empty() {
            current.fulfillment_type
        } else {
            FulfillmentType::parse_input(&input.fulfillment_type)
                .ok_or_else(|| Error::bad_request(keys::FULFILLMENT_INVALID))?
        };
        // Mapped products stay `upstream`; the admin list only displays the upstream type.
        if current.is_mapped {
            fulfillment = FulfillmentType::Upstream;
        }
        let manual_form_schema = Self::schema_for(fulfillment, &input)?;
        let mut manual_stock_total = input
            .manual_stock_total
            .unwrap_or(current.manual_stock_total);
        if manual_stock_total < MANUAL_STOCK_UNLIMITED {
            return Err(Error::bad_request(keys::MANUAL_STOCK_INVALID));
        }

        let mut price = input.price_amount;
        let mut cost = input.cost_price_amount;
        let sku_plan = if input.skus.is_empty() {
            SkuPlan::Single {
                price,
                cost,
                manual_stock_total,
            }
        } else {
            let existing = self.d.products.list_skus(id, false).await?;
            let ids: HashSet<Id> = existing.iter().map(|s| s.id).collect();
            let normalized = normalize_sku_inputs(&input.skus, fulfillment, Some(&ids))?;
            price = normalized.min_price;
            cost = normalized.min_cost;
            manual_stock_total = normalized.manual_stock_total;
            if fulfillment == FulfillmentType::Auto {
                let lost = skus_losing_activation(&existing, &normalized.rows);
                self.guard_auto_sku_stock(id, &lost).await?;
            }
            SkuPlan::Multi(normalized.rows)
        };

        let plan = ProductSavePlan {
            id: Some(id),
            fields: zs_domain::catalog::product::ProductFields {
                category_id: input.category_id,
                slug: input.slug.clone(),
                seo_meta: input.seo_meta.clone().unwrap_or_default(),
                title: input.title.clone().unwrap_or_default(),
                description: input.description.clone().unwrap_or_default(),
                content: input.content.clone().unwrap_or_default(),
                instructions: input.instructions.clone().unwrap_or_default(),
                manual_form_schema,
                price_amount: price,
                cost_price_amount: cost,
                images: input.images.clone(),
                tags: input.tags.clone(),
                purchase_type,
                min_purchase_quantity: min,
                max_purchase_quantity: max,
                stock_display_mode,
                fulfillment_type: fulfillment,
                manual_stock_total,
                payment_channel_ids,
                is_affiliate_enabled: input
                    .is_affiliate_enabled
                    .unwrap_or(current.is_affiliate_enabled),
                is_active: input.is_active.unwrap_or(current.is_active),
                sort_order: input.sort_order,
            },
            skus: sku_plan,
            wholesale: input.wholesale_prices.clone(),
        };
        self.d
            .products
            .save(&plan)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?;
        self.d
            .products
            .get(id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?
            .ok_or_else(not_found)
    }

    /// `PATCH /admin/products/:id`: status / sort / category.
    pub async fn quick_update(&self, id: Id, update: QuickUpdate) -> Result<Product> {
        let current = self
            .d
            .products
            .get(id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?
            .ok_or_else(not_found)?;
        if let Some(category_id) = update.category_id {
            // ORD-09 (2): the quick path must not bypass the leaf-category rule.
            if category_id <= 0 {
                if update.is_active == Some(true) {
                    return Err(Error::bad_request(keys::CATEGORY_INVALID));
                }
            } else {
                self.validate_category_assignment(category_id, current.category_id)
                    .await?;
            }
        }
        if update.is_active == Some(true) {
            let category_id = update.category_id.unwrap_or(current.category_id);
            self.validate_activation_category(category_id).await?;
        }
        self.d
            .products
            .quick_update(id, &update)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?;
        self.d
            .products
            .get(id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?
            .ok_or_else(not_found)
    }

    /// `PATCH /admin/products/:id/wholesale-prices`: replaces only the tiers (PRC-10).
    pub async fn update_wholesale_prices(
        &self,
        id: Id,
        inputs: &[WholesalePriceInput],
    ) -> Result<Product> {
        let product = self
            .d
            .products
            .get(id, SkuScope::All)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?
            .ok_or_else(not_found)?;
        let tiers = normalize_wholesale_prices_for_skus(inputs, &product.sku_refs())?;
        self.d
            .products
            .set_wholesale_prices(id, &tiers)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?;
        self.d
            .products
            .get(id, SkuScope::All)
            .await
            .map_err(|e| e.or_internal("error.product_update_failed"))?
            .ok_or_else(not_found)
    }

    /// Deletes a product without stock or order history (ORD-05). Checks run before the
    /// cascade transaction.
    pub async fn delete(&self, id: Id) -> Result<()> {
        let product = self
            .d
            .products
            .get(id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_delete_failed"))?
            .ok_or_else(not_found)?;
        let stats = self
            .d
            .secrets
            .stats(product.id, 0)
            .await
            .map_err(|e| e.or_internal("error.product_delete_failed"))?;
        if stats.available > 0 || stats.reserved > 0 {
            return Err(Error::bad_request(keys::HAS_STOCK));
        }
        if self
            .d
            .lookup
            .count_order_items(product.id)
            .await
            .map_err(|e| e.or_internal("error.product_delete_failed"))?
            > 0
        {
            return Err(Error::bad_request(keys::HAS_ORDER_RECORD));
        }
        self.d
            .products
            .delete_cascade(product.id)
            .await
            .map_err(|e| e.or_internal("error.product_delete_failed"))
    }

    async fn run_batch<F, Fut>(ids: &[Id], mut op: F) -> BatchOutcome
    where
        F: FnMut(Id) -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        let mut outcome = BatchOutcome {
            total: ids.len(),
            success_count: 0,
            failures: Vec::new(),
        };
        for id in ids {
            match op(*id).await {
                Ok(()) => outcome.success_count += 1,
                Err(e) => outcome.failures.push((*id, e)),
            }
        }
        outcome
    }

    pub async fn batch_status(&self, ids: &[Id], is_active: bool) -> BatchOutcome {
        Self::run_batch(ids, |id| async move {
            self.quick_update(
                id,
                QuickUpdate {
                    is_active: Some(is_active),
                    ..QuickUpdate::default()
                },
            )
            .await
            .map(|_| ())
        })
        .await
    }

    pub async fn batch_category(&self, ids: &[Id], category_id: Id) -> BatchOutcome {
        Self::run_batch(ids, |id| async move {
            self.quick_update(
                id,
                QuickUpdate {
                    category_id: Some(category_id),
                    ..QuickUpdate::default()
                },
            )
            .await
            .map(|_| ())
        })
        .await
    }

    pub async fn batch_delete(&self, ids: &[Id]) -> BatchOutcome {
        Self::run_batch(ids, |id| async move { self.delete(id).await }).await
    }

    // ------------------------------------------------------------------ public

    /// Expands a public category filter: parent → itself + active children; inactive → nothing.
    async fn expand_public_categories(&self, raw: &str) -> Result<Option<Vec<Id>>> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(None);
        }
        let Ok(id) = raw.parse::<Id>() else {
            return Ok(Some(Vec::new()));
        };
        if id <= 0 {
            return Ok(Some(vec![id]));
        }
        let Some(category) = self.d.categories.get(id).await? else {
            return Ok(Some(vec![id]));
        };
        if !category.is_active {
            return Ok(Some(Vec::new()));
        }
        if !category.is_root() {
            return Ok(Some(vec![id]));
        }
        let mut ids = vec![id];
        ids.extend(
            self.d
                .categories
                .list(false)
                .await?
                .into_iter()
                .filter(|c| c.parent_id == id && c.is_active)
                .map(|c| c.id),
        );
        Ok(Some(ids))
    }

    pub async fn list_public(
        &self,
        category_id: &str,
        search: &str,
        page: PageRequest,
    ) -> Result<Page<PublicProduct>> {
        let fetch = |e: Error| e.or_internal("error.product_fetch_failed");
        let filter = ProductFilter {
            page,
            category_ids: self
                .expand_public_categories(category_id)
                .await
                .map_err(fetch)?,
            search: search.trim().to_owned(),
            only_active: true,
            ..ProductFilter::default()
        };
        let mut result = self.d.products.list(&filter).await.map_err(fetch)?;
        self.apply_stock(&mut result.items).await?;
        let items = std::mem::take(&mut result.items);
        let decorated = self.decorate_public(items).await.map_err(fetch)?;
        Ok(Page {
            items: decorated,
            total: result.total,
        })
    }

    /// Public listing as seen on `tenant`: on a reseller site the products the reseller
    /// unlisted are excluded and every price is the reseller price the checkout charges
    /// (LQA-R1, original `ListPublicForTenant` + `decoratePublicProductForTenant`).
    pub async fn list_public_for(
        &self,
        tenant: &ResellerTenant,
        category_id: &str,
        search: &str,
        page: PageRequest,
    ) -> Result<Page<PublicProduct>> {
        let Some(reseller_id) = tenant.reseller_id.filter(|_| tenant.is_reseller()) else {
            return self.list_public(category_id, search, page).await;
        };
        let fetch = |e: Error| e.or_internal("error.product_fetch_failed");
        let exclude_ids = self
            .d
            .reseller
            .repo
            .hidden_product_ids(reseller_id)
            .await
            .map_err(fetch)?;
        let filter = ProductFilter {
            page,
            category_ids: self
                .expand_public_categories(category_id)
                .await
                .map_err(fetch)?,
            search: search.trim().to_owned(),
            only_active: true,
            exclude_ids,
            ..ProductFilter::default()
        };
        let mut result = self.d.products.list(&filter).await.map_err(fetch)?;
        self.apply_stock(&mut result.items).await?;
        let items = std::mem::take(&mut result.items);
        let count = items.len();
        let shown = self
            .reseller_views(reseller_id, items)
            .await
            .map_err(fetch)?;
        let dropped = u64::try_from(count - shown.len()).unwrap_or(0);
        Ok(Page {
            total: result.total.saturating_sub(dropped),
            items: shown,
        })
    }

    /// Product page as seen on `tenant` (LQA-R1): a product the reseller unlisted is
    /// `error.product_not_found`, exactly like an inactive one.
    pub async fn get_public_for(
        &self,
        tenant: &ResellerTenant,
        slug: &str,
    ) -> Result<PublicProduct> {
        let Some(reseller_id) = tenant.reseller_id.filter(|_| tenant.is_reseller()) else {
            return self.get_public(slug).await;
        };
        let fetch = |e: Error| e.or_internal("error.product_fetch_failed");
        let product = self
            .d
            .products
            .get_public_by_slug(slug)
            .await
            .map_err(fetch)?
            .ok_or_else(not_found)?;
        let id = product.id;
        let mut list = vec![product];
        self.apply_stock(&mut list).await?;
        let mut view = self
            .reseller_views(reseller_id, list)
            .await
            .map_err(fetch)?
            .pop()
            .ok_or_else(not_found)?;
        if let Ok(posts) = self.d.lookup.related_posts(id, RELATED_POSTS_LIMIT).await {
            view.related_posts = posts;
        }
        Ok(view)
    }

    /// Products unlisted on a reseller site (sitemap filter, LQA-R1).
    pub async fn hidden_on(&self, reseller_id: Id) -> Result<Vec<Id>> {
        self.d.reseller.repo.hidden_product_ids(reseller_id).await
    }

    /// Public views of `products` priced for a reseller site; unlisted products dropped.
    async fn reseller_views(
        &self,
        reseller_id: Id,
        products: Vec<Product>,
    ) -> Result<Vec<PublicProduct>> {
        if products.is_empty() {
            return Ok(Vec::new());
        }
        let pricing = &self.d.reseller;
        let Ok(profile) = pricing.active_profile(reseller_id).await else {
            // Inactive profile: nothing is sold on the site (the checkout refuses too).
            return Ok(Vec::new());
        };
        let priced: Vec<_> = products.iter().map(priced_product).collect();
        let product_ids: Vec<Id> = priced.iter().map(|p| p.id).collect();
        let sku_ids: Vec<Id> = priced
            .iter()
            .flat_map(|p| p.skus.iter().map(|s| s.id))
            .collect();
        let settings = pricing
            .repo
            .settings_for_pricing(reseller_id, &product_ids, &sku_ids)
            .await?;
        let views = self.decorate_public(products).await?;
        Ok(views
            .into_iter()
            .zip(&priced)
            .filter_map(|(view, p)| {
                let own: Vec<_> = settings
                    .iter()
                    .filter(|s| s.product_id == p.id)
                    .cloned()
                    .collect();
                apply_reseller_display(view, &display_prices(&profile, p, &own))
            })
            .collect())
    }

    pub async fn get_public(&self, slug: &str) -> Result<PublicProduct> {
        let product = self
            .d
            .products
            .get_public_by_slug(slug)
            .await
            .map_err(|e| e.or_internal("error.product_fetch_failed"))?
            .ok_or_else(not_found)?;
        let id = product.id;
        let mut list = vec![product];
        self.apply_stock(&mut list).await?;
        let mut view = self
            .decorate_public(list)
            .await
            .map_err(|e| e.or_internal("error.product_fetch_failed"))?
            .pop()
            .ok_or_else(not_found)?;
        // Best effort: a related-posts failure never breaks the product page.
        if let Ok(posts) = self.d.lookup.related_posts(id, RELATED_POSTS_LIMIT).await {
            view.related_posts = posts;
        }
        Ok(view)
    }

    async fn decorate_public(&self, products: Vec<Product>) -> Result<Vec<PublicProduct>> {
        if products.is_empty() {
            return Ok(Vec::new());
        }
        let now = self.d.clock.now();
        let active_levels = self.d.levels.list_active().await?;
        let upstream_ids: Vec<Id> = products
            .iter()
            .filter(|p| p.fulfillment_type == FulfillmentType::Upstream)
            .map(|p| p.id)
            .collect();
        let mut mappings = HashMap::new();
        if !upstream_ids.is_empty()
            && let Ok(list) = self.d.lookup.upstream_mappings(&upstream_ids).await
        {
            for m in list {
                mappings.insert(m.local_product_id, m);
            }
        }
        let mut out = Vec::with_capacity(products.len());
        for product in products {
            let promotions = self.d.promotions.list_effective(product.id, now).await?;
            let level_prices = self.d.levels.list_prices(product.id).await?;
            let deco = Decoration {
                promotions: &promotions,
                level_prices: &level_prices,
                active_levels: &active_levels,
                viewer_level: None,
                upstream: mappings.get(&product.id),
            };
            out.push(decorate_public_product(product, &deco));
        }
        Ok(out)
    }
}
