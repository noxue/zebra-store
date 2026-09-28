//! Products and SKUs: models (admin JSON shape), write-side rules and persistence ports.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::category::Category;
use super::stock::{MANUAL_STOCK_UNLIMITED, StockDisplayMode};
use super::wholesale::{SkuRef, WholesalePriceInput, WholesaleTier};
use crate::{Error, Id, Result};

/// Free-form JSON object column (`*_json`).
pub type JsonMap = Map<String, Value>;

/// SKU code given to single-spec products (original `DefaultSKUCode`).
pub const DEFAULT_SKU_CODE: &str = "DEFAULT";

/// Error keys of the product module (identical to the original handler mapping).
pub mod keys {
    pub const NOT_FOUND: &str = "error.product_not_found";
    pub const SLUG_EXISTS: &str = "error.slug_exists";
    pub const SLUG_USED: &str = "error.slug_used";
    pub const PRICE_INVALID: &str = "error.product_price_invalid";
    pub const PURCHASE_INVALID: &str = "error.product_purchase_invalid";
    pub const CATEGORY_INVALID: &str = "error.product_category_invalid";
    pub const FULFILLMENT_INVALID: &str = "error.fulfillment_invalid";
    pub const MANUAL_STOCK_INVALID: &str = "error.manual_stock_invalid";
    pub const PURCHASE_LIMIT_INVALID: &str = "error.product_purchase_limit_invalid";
    pub const SKU_HAS_CARD_SECRET_STOCK: &str = "error.product_sku_has_card_secret_stock";
    pub const HAS_STOCK: &str = "error.product_has_stock";
    pub const HAS_ORDER_RECORD: &str = "error.product_has_order_record";
    pub const ORDER_ITEM_INVALID: &str = "error.order_item_invalid";
    pub const MIN_PURCHASE_NOT_MET: &str = "error.product_min_purchase_not_met";
    pub const MAX_PURCHASE_EXCEEDED: &str = "error.product_max_purchase_exceeded";
    pub const NOT_AVAILABLE: &str = "error.product_not_available";
    pub const MANUAL_STOCK_INSUFFICIENT: &str = "error.manual_stock_insufficient";
}

/// How an order for the product is delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FulfillmentType {
    Auto,
    #[default]
    Manual,
    Upstream,
}

impl FulfillmentType {
    /// Write-side parse: blank → manual, unknown → `None`.
    pub fn parse_input(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "manual" => Some(Self::Manual),
            "auto" => Some(Self::Auto),
            "upstream" => Some(Self::Upstream),
            _ => None,
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        Self::parse_input(raw).unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Manual => "manual",
            Self::Upstream => "upstream",
        }
    }
}

/// Who may buy the product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseType {
    Guest,
    #[default]
    Member,
}

impl PurchaseType {
    /// Write-side parse: blank → member, unknown → `None`.
    pub fn parse_input(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "member" => Some(Self::Member),
            "guest" => Some(Self::Guest),
            _ => None,
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        Self::parse_input(raw).unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Guest => "guest",
            Self::Member => "member",
        }
    }
}

/// A product SKU (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductSku {
    pub id: Id,
    pub product_id: Id,
    pub sku_code: String,
    pub spec_values: JsonMap,
    pub price_amount: Amount,
    pub cost_price_amount: Amount,
    pub manual_stock_total: i32,
    pub manual_stock_locked: i32,
    pub manual_stock_sold: i32,
    pub auto_stock_available: i64,
    pub auto_stock_total: i64,
    pub auto_stock_locked: i64,
    pub auto_stock_sold: i64,
    pub upstream_stock: i32,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ProductSku {
    pub fn is_default_code(&self) -> bool {
        self.sku_code.trim().eq_ignore_ascii_case(DEFAULT_SKU_CODE)
    }
}

/// A product (admin JSON shape, same field order as the original model).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Product {
    pub id: Id,
    pub category_id: Id,
    pub slug: String,
    pub seo_meta: JsonMap,
    pub title: JsonMap,
    pub description: JsonMap,
    pub content: JsonMap,
    pub instructions: JsonMap,
    pub price_amount: Amount,
    pub cost_price_amount: Amount,
    pub wholesale_prices: Vec<WholesaleTier>,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub purchase_type: PurchaseType,
    pub min_purchase_quantity: i32,
    pub max_purchase_quantity: i32,
    pub stock_display_mode: StockDisplayMode,
    pub fulfillment_type: FulfillmentType,
    pub manual_form_schema: JsonMap,
    pub manual_stock_total: i32,
    pub manual_stock_locked: i32,
    pub manual_stock_sold: i32,
    /// Raw JSON text (e.g. `"[3,7]"`, empty = any channel).
    pub payment_channel_ids: String,
    pub is_affiliate_enabled: bool,
    pub auto_stock_available: i64,
    pub auto_stock_total: i64,
    pub auto_stock_locked: i64,
    pub auto_stock_sold: i64,
    pub is_mapped: bool,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(serialize_with = "serialize_category")]
    pub category: Option<Category>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skus: Vec<ProductSku>,
}

/// The original always emits `category` (zero value when the relation is missing).
fn serialize_category<S: Serializer>(c: &Option<Category>, s: S) -> Result<S::Ok, S::Error> {
    match c {
        Some(c) => c.serialize(s),
        None => Category {
            id: 0,
            parent_id: 0,
            slug: String::new(),
            name: Default::default(),
            icon: String::new(),
            sort_order: 0,
            is_active: false,
            created_at: DateTime::<Utc>::MIN_UTC,
        }
        .serialize(s),
    }
}

impl Product {
    pub fn is_auto(&self) -> bool {
        self.fulfillment_type == FulfillmentType::Auto
    }

    pub fn sku_refs(&self) -> Vec<SkuRef> {
        self.skus
            .iter()
            .map(|s| SkuRef {
                id: s.id,
                code: s.sku_code.clone(),
            })
            .collect()
    }

    /// Allowed payment channels (`None` = unrestricted).
    pub fn allowed_payment_channels(&self) -> Option<Vec<Id>> {
        let ids = decode_payment_channel_ids(&self.payment_channel_ids);
        (!ids.is_empty()).then_some(ids)
    }
}

/// Decodes `payment_channel_ids`; invalid JSON or no positive id = unrestricted (empty).
pub fn decode_payment_channel_ids(raw: &str) -> Vec<Id> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Vec::new();
    }
    serde_json::from_str::<Vec<Id>>(trimmed)
        .map(|ids| ids.into_iter().filter(|id| *id > 0).collect())
        .unwrap_or_default()
}

/// Encodes channel ids as compact JSON (`""` when empty).
pub fn encode_payment_channel_ids(ids: &[Id]) -> String {
    if ids.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = ids.iter().map(ToString::to_string).collect();
    format!("[{}]", parts.join(","))
}

/// Deduplicates channel ids, dropping zero/negative values, keeping the first occurrence order.
pub fn unique_positive_ids(ids: &[Id]) -> Vec<Id> {
    let mut seen = HashSet::new();
    ids.iter()
        .copied()
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect()
}

/// Non-positive purchase limits mean "unlimited" (0).
pub fn normalize_purchase_limit(value: i32) -> i32 {
    value.max(0)
}

/// Validates a line quantity against the product's min/max purchase limits (ORD-10).
pub fn validate_purchase_quantity(min: i32, max: i32, quantity: i32) -> Result<()> {
    if quantity <= 0 {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
    }
    let min = normalize_purchase_limit(min);
    let max = normalize_purchase_limit(max);
    if min > 0 && quantity < min {
        return Err(Error::bad_request(keys::MIN_PURCHASE_NOT_MET));
    }
    if max > 0 && quantity > max {
        return Err(Error::bad_request(keys::MAX_PURCHASE_EXCEEDED));
    }
    Ok(())
}

/// Whether a SKU's manual stock must be enforced (legacy DEFAULT SKUs with negative
/// stock only when the product has several active SKUs).
pub fn should_enforce_manual_sku_stock(product: &Product, sku: &ProductSku) -> bool {
    if sku.manual_stock_total == MANUAL_STOCK_UNLIMITED {
        return false;
    }
    if sku.manual_stock_total >= 0 || !sku.is_default_code() {
        return true;
    }
    product.skus.iter().filter(|s| s.is_active).count() > 1
}

/// Available manual stock for rule checks; unlimited maps to `i32::MAX`.
pub fn manual_sku_available(sku: &ProductSku) -> i32 {
    match sku.manual_stock_total {
        MANUAL_STOCK_UNLIMITED => i32::MAX,
        v if v < 0 => 0,
        v => v,
    }
}

// ---------------------------------------------------------------------------
// Write-side inputs and SKU planning
// ---------------------------------------------------------------------------

/// A SKU row submitted by the admin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SkuInput {
    pub id: Id,
    pub sku_code: String,
    pub spec_values: Option<JsonMap>,
    pub price_amount: Amount,
    pub cost_price_amount: Amount,
    pub manual_stock_total: i32,
    pub is_active: Option<bool>,
    pub sort_order: i32,
}

/// A validated SKU row.
#[derive(Debug, Clone, PartialEq)]
pub struct SkuFields {
    pub sku_code: String,
    pub spec_values: JsonMap,
    pub price_amount: Amount,
    pub cost_price_amount: Amount,
    pub manual_stock_total: i32,
    pub is_active: bool,
    pub sort_order: i32,
}

/// A validated SKU row with its (optional) existing id.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedSku {
    pub id: Id,
    pub fields: SkuFields,
}

/// Aggregates derived from the SKU list.
#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedSkus {
    pub rows: Vec<NormalizedSku>,
    /// Lowest active SKU price (the product's price).
    pub min_price: Amount,
    /// Lowest active SKU cost price.
    pub min_cost: Amount,
    /// Sum of active manual stock (`-1` if any active SKU is unlimited, 0 for non-manual).
    pub manual_stock_total: i32,
}

/// Validates the SKU list of a multi-spec product.
///
/// `existing_ids` (update only) restricts `id > 0` rows to SKUs of the product.
pub fn normalize_sku_inputs(
    inputs: &[SkuInput],
    fulfillment: FulfillmentType,
    existing_ids: Option<&HashSet<Id>>,
) -> Result<NormalizedSkus> {
    let invalid_sku = || Error::bad_request(crate::error::keys::BAD_REQUEST);
    if inputs.is_empty() {
        return Err(invalid_sku());
    }
    let manual = fulfillment == FulfillmentType::Manual;
    let mut seen = HashSet::new();
    let mut rows = Vec::with_capacity(inputs.len());
    let mut min_price: Option<Amount> = None;
    let mut manual_total: i32 = 0;
    let mut unlimited = false;
    for input in inputs {
        let code = input.sku_code.trim();
        if code.is_empty() || !seen.insert(code.to_lowercase()) {
            return Err(invalid_sku());
        }
        if !input.price_amount.is_positive() || input.cost_price_amount.is_negative() {
            return Err(Error::bad_request(keys::PRICE_INVALID));
        }
        if input.manual_stock_total < MANUAL_STOCK_UNLIMITED {
            return Err(Error::bad_request(keys::MANUAL_STOCK_INVALID));
        }
        let stock = if manual { input.manual_stock_total } else { 0 };
        if let Some(ids) = existing_ids
            && input.id > 0
            && !ids.contains(&input.id)
        {
            return Err(invalid_sku());
        }
        let is_active = input.is_active.unwrap_or(true);
        if is_active {
            if min_price.is_none_or(|p| input.price_amount < p) {
                min_price = Some(input.price_amount);
            }
            if manual {
                if stock == MANUAL_STOCK_UNLIMITED {
                    unlimited = true;
                } else {
                    manual_total = manual_total.saturating_add(stock);
                }
            }
        }
        rows.push(NormalizedSku {
            id: input.id,
            fields: SkuFields {
                sku_code: code.to_owned(),
                spec_values: input.spec_values.clone().unwrap_or_default(),
                price_amount: input.price_amount,
                cost_price_amount: input.cost_price_amount,
                manual_stock_total: stock,
                is_active,
                sort_order: input.sort_order,
            },
        });
    }
    let Some(min_price) = min_price else {
        return Err(invalid_sku());
    };
    let manual_stock_total = if !manual {
        0
    } else if unlimited {
        MANUAL_STOCK_UNLIMITED
    } else {
        manual_total
    };
    let min_cost = rows
        .iter()
        .filter(|r| r.fields.is_active)
        .map(|r| r.fields.cost_price_amount)
        .min()
        .unwrap_or(Amount::ZERO);
    Ok(NormalizedSkus {
        rows,
        min_price,
        min_cost,
        manual_stock_total,
    })
}

/// One persistence step of a SKU sync, applied in order inside one transaction.
#[derive(Debug, Clone, PartialEq)]
pub enum SkuOp {
    /// Overwrite an existing SKU (sold/locked counters untouched).
    Update { id: Id, fields: SkuFields },
    /// Hard-delete soft-deleted rows with the same code, then insert (ORD-11).
    Create(SkuFields),
    /// Hard delete, so the code can be reused (ORD-11).
    Delete(Id),
}

/// Diffs the submitted SKU list against the stored SKUs: rows with `id` update that SKU,
/// rows matching an existing code update it, other rows are created; unmatched SKUs are deleted.
pub fn plan_multi_sku_sync(existing: &[ProductSku], rows: &[NormalizedSku]) -> Result<Vec<SkuOp>> {
    let by_id: HashMap<Id, &ProductSku> = existing.iter().map(|s| (s.id, s)).collect();
    let mut by_code: HashMap<String, Id> = existing
        .iter()
        .map(|s| (s.sku_code.trim().to_lowercase(), s.id))
        .collect();
    let mut kept = HashSet::new();
    let mut ops = Vec::with_capacity(rows.len() + existing.len());
    for row in rows {
        if row.id > 0 {
            let current = by_id
                .get(&row.id)
                .ok_or_else(|| Error::bad_request(crate::error::keys::BAD_REQUEST))?;
            let old_key = current.sku_code.trim().to_lowercase();
            if by_code.get(&old_key) == Some(&row.id) {
                by_code.remove(&old_key);
            }
            by_code.insert(row.fields.sku_code.trim().to_lowercase(), row.id);
            kept.insert(row.id);
            ops.push(SkuOp::Update {
                id: row.id,
                fields: row.fields.clone(),
            });
            continue;
        }
        let key = row.fields.sku_code.trim().to_lowercase();
        if let Some(id) = by_code.get(&key).copied() {
            let code = by_id
                .get(&id)
                .map_or_else(|| row.fields.sku_code.clone(), |s| s.sku_code.clone());
            kept.insert(id);
            ops.push(SkuOp::Update {
                id,
                fields: SkuFields {
                    sku_code: code,
                    ..row.fields.clone()
                },
            });
            continue;
        }
        ops.push(SkuOp::Create(row.fields.clone()));
    }
    for sku in existing {
        if !kept.contains(&sku.id) {
            ops.push(SkuOp::Delete(sku.id));
        }
    }
    Ok(ops)
}

/// Index of the SKU kept in single-spec mode: active DEFAULT, else first active, else DEFAULT, else first.
pub fn pick_single_mode_target(skus: &[ProductSku]) -> Option<usize> {
    if skus.is_empty() {
        return None;
    }
    skus.iter()
        .position(|s| s.is_active && s.is_default_code())
        .or_else(|| skus.iter().position(|s| s.is_active))
        .or_else(|| skus.iter().position(ProductSku::is_default_code))
        .or(Some(0))
}

/// Plans the single-spec sync: one active SKU carries the product price/stock, others are hard-deleted.
pub fn plan_single_sku_sync(
    existing: &[ProductSku],
    price: Amount,
    cost: Amount,
    manual_stock_total: i32,
) -> Vec<SkuOp> {
    let Some(target) = pick_single_mode_target(existing) else {
        return vec![SkuOp::Create(SkuFields {
            sku_code: DEFAULT_SKU_CODE.to_owned(),
            spec_values: JsonMap::new(),
            price_amount: price,
            cost_price_amount: cost,
            manual_stock_total,
            is_active: true,
            sort_order: 0,
        })];
    };
    let current = &existing[target];
    let code = if current.sku_code.trim().is_empty() {
        DEFAULT_SKU_CODE.to_owned()
    } else {
        current.sku_code.clone()
    };
    let mut ops = vec![SkuOp::Update {
        id: current.id,
        fields: SkuFields {
            sku_code: code,
            spec_values: current.spec_values.clone(),
            price_amount: price,
            cost_price_amount: cost,
            manual_stock_total,
            is_active: true,
            sort_order: current.sort_order,
        },
    }];
    ops.extend(
        existing
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != target)
            .map(|(_, s)| SkuOp::Delete(s.id)),
    );
    ops
}

/// Active SKUs that the submitted list deactivates or removes (card-secret stock guard, DLV-04).
pub fn skus_losing_activation(existing: &[ProductSku], rows: &[NormalizedSku]) -> Vec<Id> {
    let by_code: HashMap<String, Id> = existing
        .iter()
        .map(|s| (s.sku_code.trim().to_lowercase(), s.id))
        .collect();
    let mut next_active: HashMap<Id, bool> = HashMap::new();
    for row in rows {
        let id = if row.id > 0 {
            Some(row.id)
        } else {
            by_code
                .get(&row.fields.sku_code.trim().to_lowercase())
                .copied()
        };
        if let Some(id) = id {
            next_active.insert(id, row.fields.is_active);
        }
    }
    existing
        .iter()
        .filter(|s| s.is_active && !next_active.get(&s.id).copied().unwrap_or(false))
        .map(|s| s.id)
        .collect()
}

/// SKU plan of a product save.
#[derive(Debug, Clone, PartialEq)]
pub enum SkuPlan {
    /// Single-spec product: the DEFAULT SKU mirrors the product price/cost/stock.
    Single {
        price: Amount,
        cost: Amount,
        manual_stock_total: i32,
    },
    /// Multi-spec product.
    Multi(Vec<NormalizedSku>),
}

/// Product columns written by create / full update.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductFields {
    pub category_id: Id,
    pub slug: String,
    pub seo_meta: JsonMap,
    pub title: JsonMap,
    pub description: JsonMap,
    pub content: JsonMap,
    pub instructions: JsonMap,
    pub manual_form_schema: JsonMap,
    pub price_amount: Amount,
    pub cost_price_amount: Amount,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub purchase_type: PurchaseType,
    pub min_purchase_quantity: i32,
    pub max_purchase_quantity: i32,
    pub stock_display_mode: StockDisplayMode,
    pub fulfillment_type: FulfillmentType,
    pub manual_stock_total: i32,
    pub payment_channel_ids: String,
    pub is_affiliate_enabled: bool,
    pub is_active: bool,
    pub sort_order: i32,
}

/// Everything a create / update persists atomically: product row, SKU sync and
/// (when submitted) wholesale tiers validated against the SKUs after the sync (PRC-02, PRC-10).
#[derive(Debug, Clone, PartialEq)]
pub struct ProductSavePlan {
    /// `None` = create.
    pub id: Option<Id>,
    pub fields: ProductFields,
    pub skus: SkuPlan,
    /// `None` keeps the stored tiers; `Some(vec![])` clears them.
    pub wholesale: Option<Vec<WholesalePriceInput>>,
}

/// Admin create / full-update input.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProductInput {
    pub category_id: Id,
    pub slug: String,
    pub seo_meta: Option<JsonMap>,
    pub title: Option<JsonMap>,
    pub description: Option<JsonMap>,
    pub content: Option<JsonMap>,
    pub instructions: Option<JsonMap>,
    pub manual_form_schema: Option<JsonMap>,
    pub price_amount: Amount,
    pub cost_price_amount: Amount,
    pub wholesale_prices: Option<Vec<WholesalePriceInput>>,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub purchase_type: String,
    pub min_purchase_quantity: Option<i32>,
    pub max_purchase_quantity: Option<i32>,
    pub stock_display_mode: String,
    pub fulfillment_type: String,
    pub manual_stock_total: Option<i32>,
    pub skus: Vec<SkuInput>,
    pub payment_channel_ids: Vec<Id>,
    pub is_affiliate_enabled: Option<bool>,
    pub is_active: Option<bool>,
    pub sort_order: i32,
}

/// Fields of `PATCH /admin/products/:id`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct QuickUpdate {
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
    pub category_id: Option<Id>,
}

impl QuickUpdate {
    pub fn is_empty(&self) -> bool {
        self.is_active.is_none() && self.sort_order.is_none() && self.category_id.is_none()
    }
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// Admin stock-status filter (ORD-12).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StockFilter {
    Low,
    Normal,
    Unlimited,
}

impl StockFilter {
    /// `""`/`all`/unknown → no filter.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "low" => Some(Self::Low),
            "normal" => Some(Self::Normal),
            "unlimited" => Some(Self::Unlimited),
            _ => None,
        }
    }
}

/// Product list filter shared by public and admin queries.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProductFilter {
    pub page: PageRequest,
    pub category_id: Option<Id>,
    /// Expanded public category ids (takes precedence over `category_id`); `Some(empty)` matches nothing.
    pub category_ids: Option<Vec<Id>>,
    pub search: String,
    pub fulfillment_type: String,
    pub stock: Option<StockFilter>,
    pub has_wholesale_prices: Option<bool>,
    pub low_stock_threshold: i64,
    /// Public listing: product and category must be active; only active SKUs are loaded.
    pub only_active: bool,
    /// Admin tri-state `is_active` filter.
    pub is_active: Option<bool>,
    /// Products never returned (reseller site: products the reseller unlisted,
    /// original `ExcludeProductIDs`).
    pub exclude_ids: Vec<Id>,
}

/// Which SKUs to load with a product.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkuScope {
    Active,
    All,
}

/// Persistence port for products and SKUs. Soft-deleted rows are never returned.
#[async_trait]
pub trait ProductRepo: Send + Sync {
    /// Lists products (with category and SKUs, sorted `sort_order DESC, created_at DESC`).
    async fn list(&self, filter: &ProductFilter) -> Result<Page<Product>>;
    async fn get(&self, id: Id, skus: SkuScope) -> Result<Option<Product>>;
    /// Active product in an active category, with active SKUs.
    async fn get_public_by_slug(&self, slug: &str) -> Result<Option<Product>>;
    async fn count_by_slug(&self, slug: &str, exclude: Option<Id>) -> Result<u64>;
    async fn list_skus(&self, product_id: Id, active_only: bool) -> Result<Vec<ProductSku>>;
    async fn get_sku(&self, id: Id) -> Result<Option<ProductSku>>;
    async fn get_sku_by_code(&self, product_id: Id, code: &str) -> Result<Option<ProductSku>>;
    /// Persists a create / full update atomically; returns the product id.
    async fn save(&self, plan: &ProductSavePlan) -> Result<Id>;
    async fn quick_update(&self, id: Id, update: &QuickUpdate) -> Result<()>;
    /// Replaces only `wholesale_prices`.
    async fn set_wholesale_prices(&self, id: Id, tiers: &[WholesaleTier]) -> Result<()>;
    /// Cascading delete inside one transaction (ORD-05): card secrets, batches, SKUs,
    /// member-level prices, cart items, SKU/product mappings and the product.
    async fn delete_cascade(&self, id: Id) -> Result<()>;
}

/// Upstream mapping snapshot of a mapped product (read from the integration tables).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamMapping {
    pub local_product_id: Id,
    pub upstream_fulfillment_type: String,
    pub skus: Vec<UpstreamSkuMapping>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpstreamSkuMapping {
    pub local_sku_id: Id,
    /// `-1` unlimited, `0` sold out.
    pub upstream_stock: i32,
    pub upstream_is_active: bool,
}

/// Related blog post card shown on the product page.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelatedPost {
    pub id: Id,
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: JsonMap,
    #[serde(skip_serializing_if = "JsonMap::is_empty")]
    pub summary: JsonMap,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub thumbnail: String,
    pub published_at: Option<DateTime<Utc>>,
}

/// Reads of other groups' tables needed by the catalog (consumer-defined port).
#[async_trait]
pub trait CatalogLookup: Send + Sync {
    /// Keeps only ids of active, non-deleted payment channels (PAY-17).
    async fn active_payment_channel_ids(&self, ids: &[Id]) -> Result<Vec<Id>>;
    /// Non-deleted order items referencing the product (ORD-05).
    async fn count_order_items(&self, product_id: Id) -> Result<u64>;
    /// Upstream mappings (with SKU mappings) of the given local products.
    async fn upstream_mappings(&self, product_ids: &[Id]) -> Result<Vec<UpstreamMapping>>;
    /// Published blog posts linked to the product (`sort ASC, id ASC`).
    async fn related_posts(&self, product_id: Id, limit: u64) -> Result<Vec<RelatedPost>>;
}

// ---------------------------------------------------------------------------
// Upstream display (admin)
// ---------------------------------------------------------------------------

/// Display type of a mapped product: the upstream's `auto`, anything else `manual`.
pub fn upstream_display_type(mapping: &UpstreamMapping) -> FulfillmentType {
    if mapping.upstream_fulfillment_type.trim() == "auto" {
        FulfillmentType::Auto
    } else {
        FulfillmentType::Manual
    }
}

/// Shows an `upstream` product in the admin list with its real delivery type and upstream stock.
pub fn apply_upstream_admin_display(product: &mut Product, mapping: &UpstreamMapping) {
    let display = upstream_display_type(mapping);
    product.fulfillment_type = display;
    if mapping.skus.is_empty() {
        return;
    }
    let by_local: HashMap<Id, &UpstreamSkuMapping> =
        mapping.skus.iter().map(|m| (m.local_sku_id, m)).collect();
    let mut total: i64 = 0;
    let mut unlimited = false;
    for sku in &mut product.skus {
        let Some(m) = by_local.get(&sku.id).filter(|m| m.upstream_is_active) else {
            continue;
        };
        if m.upstream_stock == MANUAL_STOCK_UNLIMITED {
            unlimited = true;
        } else {
            total += i64::from(m.upstream_stock);
        }
        if display == FulfillmentType::Auto {
            sku.auto_stock_available = i64::from(m.upstream_stock);
            if m.upstream_stock > 0 {
                sku.auto_stock_total = i64::from(m.upstream_stock);
            }
        } else {
            sku.manual_stock_total = m.upstream_stock;
        }
    }
    if display == FulfillmentType::Auto {
        if unlimited {
            product.auto_stock_available = -1;
        } else {
            product.auto_stock_available = total;
            product.auto_stock_total = total;
        }
    } else if unlimited {
        product.manual_stock_total = MANUAL_STOCK_UNLIMITED;
    } else {
        product.manual_stock_total = i32::try_from(total).unwrap_or(i32::MAX);
    }
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn sku(id: Id, code: &str, active: bool) -> ProductSku {
        ProductSku {
            id,
            product_id: 1,
            sku_code: code.into(),
            spec_values: JsonMap::new(),
            price_amount: Amount::from(10),
            cost_price_amount: Amount::ZERO,
            manual_stock_total: 0,
            manual_stock_locked: 0,
            manual_stock_sold: 0,
            auto_stock_available: 0,
            auto_stock_total: 0,
            auto_stock_locked: 0,
            auto_stock_sold: 0,
            upstream_stock: 0,
            is_active: active,
            sort_order: 0,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        }
    }

    pub fn product(skus: Vec<ProductSku>) -> Product {
        Product {
            id: 1,
            category_id: 1,
            slug: "p".into(),
            seo_meta: JsonMap::new(),
            title: JsonMap::new(),
            description: JsonMap::new(),
            content: JsonMap::new(),
            instructions: JsonMap::new(),
            price_amount: Amount::from(10),
            cost_price_amount: Amount::ZERO,
            wholesale_prices: Vec::new(),
            images: Vec::new(),
            tags: Vec::new(),
            purchase_type: PurchaseType::Member,
            min_purchase_quantity: 0,
            max_purchase_quantity: 0,
            stock_display_mode: StockDisplayMode::Exact,
            fulfillment_type: FulfillmentType::Manual,
            manual_form_schema: JsonMap::new(),
            manual_stock_total: 0,
            manual_stock_locked: 0,
            manual_stock_sold: 0,
            payment_channel_ids: String::new(),
            is_affiliate_enabled: false,
            auto_stock_available: 0,
            auto_stock_total: 0,
            auto_stock_locked: 0,
            auto_stock_sold: 0,
            is_mapped: false,
            is_active: true,
            sort_order: 0,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
            category: None,
            skus,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    fn input(code: &str, price: i64, stock: i32, active: Option<bool>) -> SkuInput {
        SkuInput {
            sku_code: code.into(),
            price_amount: Amount::from(price),
            cost_price_amount: Amount::from(price / 2),
            manual_stock_total: stock,
            is_active: active,
            ..SkuInput::default()
        }
    }

    #[test]
    fn payment_channel_ids_roundtrip_and_discard_invalid() {
        assert_eq!(encode_payment_channel_ids(&[3, 7]), "[3,7]");
        assert_eq!(decode_payment_channel_ids("[3,7]"), vec![3, 7]);
        assert_eq!(decode_payment_channel_ids("[0,3,0,7]"), vec![3, 7]);
        for raw in ["", "[]", "invalid", "[0]", "garbage"] {
            assert!(decode_payment_channel_ids(raw).is_empty(), "{raw}");
        }
        assert_eq!(unique_positive_ids(&[1, 0, 2, 1, -3]), vec![1, 2]);
        assert_eq!(encode_payment_channel_ids(&[]), "");
    }

    // ORD-10
    #[test]
    fn purchase_quantity_limits() {
        assert_eq!(
            validate_purchase_quantity(2, 5, 0).unwrap_err().key(),
            keys::ORDER_ITEM_INVALID
        );
        assert_eq!(
            validate_purchase_quantity(2, 5, 1).unwrap_err().key(),
            keys::MIN_PURCHASE_NOT_MET
        );
        assert!(validate_purchase_quantity(2, 5, 2).is_ok());
        assert!(validate_purchase_quantity(2, 5, 5).is_ok());
        assert_eq!(
            validate_purchase_quantity(2, 5, 6).unwrap_err().key(),
            keys::MAX_PURCHASE_EXCEEDED
        );
        assert_eq!(
            validate_purchase_quantity(0, 2, 3).unwrap_err().key(),
            keys::MAX_PURCHASE_EXCEEDED
        );
        assert!(validate_purchase_quantity(0, 0, 999).is_ok());
        assert!(validate_purchase_quantity(-3, -1, 1).is_ok());
        assert_eq!(normalize_purchase_limit(-1), 0);
        assert_eq!(normalize_purchase_limit(3), 3);
    }

    #[test]
    fn enum_normalizers_default_and_reject() {
        assert_eq!(PurchaseType::parse_input(""), Some(PurchaseType::Member));
        assert_eq!(
            PurchaseType::parse_input("guest"),
            Some(PurchaseType::Guest)
        );
        assert_eq!(PurchaseType::parse_input("invalid"), None);
        assert_eq!(
            FulfillmentType::parse_input(""),
            Some(FulfillmentType::Manual)
        );
        assert_eq!(
            FulfillmentType::parse_input("UPSTREAM"),
            Some(FulfillmentType::Upstream)
        );
        assert_eq!(FulfillmentType::parse_input("invalid"), None);
    }

    #[test]
    fn legacy_default_sku_stock_policy() {
        let p = product(vec![sku(1, "DEFAULT", true), sku(2, "SECOND", true)]);
        let mut legacy = sku(1, DEFAULT_SKU_CODE, true);
        legacy.manual_stock_total = -2;
        assert!(should_enforce_manual_sku_stock(&p, &legacy));
        assert_eq!(manual_sku_available(&legacy), 0);
        let mut unlimited = sku(2, "SECOND", true);
        unlimited.manual_stock_total = -1;
        assert!(!should_enforce_manual_sku_stock(&p, &unlimited));
        assert!(manual_sku_available(&unlimited) > 0);
        let single = product(vec![sku(1, "DEFAULT", true)]);
        assert!(!should_enforce_manual_sku_stock(&single, &legacy));
    }

    // ORD-06
    #[test]
    fn sku_normalization_aggregates_active_rows() {
        let rows = normalize_sku_inputs(
            &[
                input("A", 30, 5, None),
                input("B", 20, 3, Some(true)),
                input("C", 10, 9, Some(false)),
            ],
            FulfillmentType::Manual,
            None,
        )
        .unwrap();
        assert_eq!(rows.min_price, Amount::from(20));
        assert_eq!(rows.min_cost, Amount::from(10));
        assert_eq!(rows.manual_stock_total, 8);

        let unlimited = normalize_sku_inputs(
            &[input("A", 30, -1, None), input("B", 20, 3, None)],
            FulfillmentType::Manual,
            None,
        )
        .unwrap();
        assert_eq!(unlimited.manual_stock_total, MANUAL_STOCK_UNLIMITED);

        let auto =
            normalize_sku_inputs(&[input("A", 30, 7, None)], FulfillmentType::Auto, None).unwrap();
        assert_eq!(auto.manual_stock_total, 0);
        assert_eq!(auto.rows[0].fields.manual_stock_total, 0);
    }

    #[test]
    fn sku_normalization_rejections() {
        let key = |inputs: &[SkuInput]| {
            normalize_sku_inputs(inputs, FulfillmentType::Manual, None)
                .unwrap_err()
                .key()
                .to_owned()
        };
        assert_eq!(key(&[]), "error.bad_request");
        assert_eq!(key(&[input(" ", 1, 0, None)]), "error.bad_request");
        assert_eq!(
            key(&[input("A", 1, 0, None), input("a", 1, 0, None)]),
            "error.bad_request"
        );
        assert_eq!(key(&[input("A", 0, 0, None)]), keys::PRICE_INVALID);
        assert_eq!(key(&[input("A", 1, -2, None)]), keys::MANUAL_STOCK_INVALID);
        assert_eq!(key(&[input("A", 1, 0, Some(false))]), "error.bad_request");
        let existing: HashSet<Id> = [5].into_iter().collect();
        let foreign = SkuInput {
            id: 9,
            ..input("A", 1, 0, None)
        };
        assert!(
            normalize_sku_inputs(&[foreign], FulfillmentType::Manual, Some(&existing)).is_err()
        );
    }

    fn norm(id: Id, code: &str, active: bool) -> NormalizedSku {
        NormalizedSku {
            id,
            fields: SkuFields {
                sku_code: code.into(),
                spec_values: JsonMap::new(),
                price_amount: Amount::from(1),
                cost_price_amount: Amount::ZERO,
                manual_stock_total: 0,
                is_active: active,
                sort_order: 0,
            },
        }
    }

    // ORD-11
    #[test]
    fn multi_sync_updates_creates_and_hard_deletes() {
        let existing = [sku(1, "A", true), sku(2, "B", true), sku(3, "C", true)];
        let ops = plan_multi_sku_sync(
            &existing,
            &[norm(1, "A1", true), norm(0, "b", true), norm(0, "D", true)],
        )
        .unwrap();
        assert!(matches!(&ops[0], SkuOp::Update { id: 1, fields } if fields.sku_code == "A1"));
        // matched by code keeps the stored code spelling
        assert!(matches!(&ops[1], SkuOp::Update { id: 2, fields } if fields.sku_code == "B"));
        assert!(matches!(&ops[2], SkuOp::Create(f) if f.sku_code == "D"));
        assert_eq!(ops[3], SkuOp::Delete(3));
        assert!(plan_multi_sku_sync(&existing, &[norm(99, "X", true)]).is_err());
    }

    // ORD-11 ③ / ORD-06
    #[test]
    fn single_sync_keeps_one_default_row() {
        let ops = plan_single_sku_sync(&[], Amount::from(9), Amount::ZERO, 5);
        assert!(
            matches!(&ops[0], SkuOp::Create(f) if f.sku_code == DEFAULT_SKU_CODE && f.manual_stock_total == 5)
        );
        let existing = [
            sku(1, "X", false),
            sku(2, "DEFAULT", true),
            sku(3, "Y", true),
        ];
        let ops = plan_single_sku_sync(&existing, Amount::from(9), Amount::ZERO, -1);
        assert!(
            matches!(&ops[0], SkuOp::Update { id: 2, fields } if fields.is_active && fields.manual_stock_total == -1)
        );
        assert_eq!(&ops[1..], &[SkuOp::Delete(1), SkuOp::Delete(3)]);
        assert_eq!(
            pick_single_mode_target(&[sku(1, "X", false), sku(2, "Y", true)]),
            Some(1)
        );
        assert_eq!(
            pick_single_mode_target(&[sku(1, "X", false), sku(2, "DEFAULT", false)]),
            Some(1)
        );
        assert_eq!(pick_single_mode_target(&[sku(1, "X", false)]), Some(0));
    }

    // DLV-04
    #[test]
    fn detects_skus_losing_activation() {
        let existing = [sku(1, "A", true), sku(2, "B", true), sku(3, "C", false)];
        let rows = [norm(1, "A", false), norm(0, "c", true)];
        assert_eq!(skus_losing_activation(&existing, &rows), vec![1, 2]);
        assert!(
            skus_losing_activation(&existing, &[norm(1, "A", true), norm(2, "B", true)]).is_empty()
        );
    }

    #[test]
    fn upstream_admin_display() {
        let mut p = product(vec![sku(1, "A", true), sku(2, "B", true)]);
        p.fulfillment_type = FulfillmentType::Upstream;
        let mapping = UpstreamMapping {
            local_product_id: 1,
            upstream_fulfillment_type: "auto".into(),
            skus: vec![
                UpstreamSkuMapping {
                    local_sku_id: 1,
                    upstream_stock: 4,
                    upstream_is_active: true,
                },
                UpstreamSkuMapping {
                    local_sku_id: 2,
                    upstream_stock: 6,
                    upstream_is_active: false,
                },
            ],
        };
        apply_upstream_admin_display(&mut p, &mapping);
        assert_eq!(p.fulfillment_type, FulfillmentType::Auto);
        assert_eq!(p.auto_stock_available, 4);
        assert_eq!(p.skus[0].auto_stock_available, 4);
        assert_eq!(p.skus[1].auto_stock_available, 0);
    }

    #[test]
    fn product_json_shape() {
        let p = product(vec![sku(1, "DEFAULT", true)]);
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["price_amount"], "10.00");
        assert_eq!(v["fulfillment_type"], "manual");
        assert_eq!(v["payment_channel_ids"], "");
        assert_eq!(v["category"]["id"], 0);
        assert_eq!(v["skus"][0]["sku_code"], "DEFAULT");
        assert!(v["wholesale_prices"].as_array().unwrap().is_empty());
        let empty = serde_json::to_value(product(vec![])).unwrap();
        assert!(empty.get("skus").is_none());
    }
}
