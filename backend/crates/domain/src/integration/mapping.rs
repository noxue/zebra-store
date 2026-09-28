//! Product mappings (local product ↔ supplier product) and the pure plans of
//! import / sync (original `catalog/mapping`, UPS-04/06/08/13/14/20).

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::connection::SiteConnection;
use super::pricing::{Pricing, parse_upstream_price};
use super::protocol::{RemoteProduct, RemoteSku, RemoteTier};
use crate::catalog::product::{DEFAULT_SKU_CODE, JsonMap, Product, keys as product_keys};
use crate::catalog::wholesale::{WholesalePriceInput, WholesaleTier, normalize_wholesale_prices};
use crate::{Error, Id, Result};

/// Supplier product state of a mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamStatus {
    Active,
    Inactive,
    Deleted,
}

impl UpstreamStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "active" => Some(Self::Active),
            "inactive" => Some(Self::Inactive),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        Self::parse(raw).unwrap_or(Self::Active)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
            Self::Deleted => "deleted",
        }
    }
}

/// A product mapping (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductMapping {
    pub id: Id,
    pub connection_id: Id,
    pub local_product_id: Id,
    pub upstream_product_id: Id,
    pub upstream_fulfillment_type: String,
    pub upstream_status: UpstreamStatus,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<SiteConnection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub product: Option<Product>,
}

/// A SKU mapping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkuMapping {
    pub id: Id,
    pub product_mapping_id: Id,
    pub local_sku_id: Id,
    pub upstream_sku_id: Id,
    pub upstream_price: Amount,
    /// `-1` unlimited.
    pub upstream_stock: i32,
    pub upstream_is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_synced_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Display type of a supplier fulfillment type: `auto`, anything else `manual`.
pub fn effective_fulfillment_type(raw: &str) -> &'static str {
    if raw.trim() == "auto" {
        "auto"
    } else {
        "manual"
    }
}

/// Supplier stock as stored locally (`-1` unlimited, clamped to `i32`).
pub fn stock_of(quantity: i64) -> i32 {
    if quantity < 0 {
        -1
    } else {
        i32::try_from(quantity).unwrap_or(i32::MAX)
    }
}

// ---------------------------------------------------------------------------
// Local price pairs
// ---------------------------------------------------------------------------

/// Local selling and cost price derived from one supplier price.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PricePair {
    pub price: Amount,
    pub cost: Amount,
    /// The supplier price itself (stored on the SKU mapping).
    pub upstream: Amount,
}

/// Prices of a supplier price text; `None` when unparseable or when the local price
/// would not be positive (UPS-08: never write a zero price).
pub fn price_pair(raw: &str, pricing: &Pricing) -> Option<PricePair> {
    let upstream = parse_upstream_price(raw)?;
    let price = pricing.local_price(upstream);
    (price > Decimal::ZERO).then(|| PricePair {
        price: Amount::new(price),
        cost: Amount::new(pricing.cost_price(upstream)),
        upstream: Amount::new(upstream),
    })
}

// ---------------------------------------------------------------------------
// Wholesale tiers (UPS-04, UPS-13)
// ---------------------------------------------------------------------------

/// Local SKU identity used to translate supplier tiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalSkuRef {
    pub id: Id,
    pub code: String,
}

/// Supplier SKU id / code → local SKU.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WholesaleIndex {
    by_upstream_id: HashMap<Id, LocalSkuRef>,
    by_code: HashMap<String, LocalSkuRef>,
}

impl WholesaleIndex {
    /// Builds the index: SKU mappings first, then SKU codes (case-insensitive), then
    /// a single-SKU ↔ single-SKU pairing (original `buildUpstreamWholesaleSKUIndex`).
    pub fn build(local: &[LocalSkuRef], remote: &[RemoteSku], mappings: &[(Id, Id)]) -> Self {
        let mut index = Self::default();
        let by_id: HashMap<Id, &LocalSkuRef> = local.iter().map(|s| (s.id, s)).collect();
        let mut by_code_local: HashMap<String, &LocalSkuRef> = HashMap::new();
        for sku in local {
            let code = sku.code.trim();
            if !code.is_empty() {
                let r = LocalSkuRef {
                    id: sku.id,
                    code: code.to_owned(),
                };
                index.by_code.insert(code.to_lowercase(), r);
                by_code_local.insert(code.to_lowercase(), sku);
            }
        }
        for (local_id, upstream_id) in mappings {
            if let Some(sku) = by_id.get(local_id) {
                index.by_upstream_id.insert(
                    *upstream_id,
                    LocalSkuRef {
                        id: sku.id,
                        code: sku.code.trim().to_owned(),
                    },
                );
            }
        }
        for up in remote {
            if index.by_upstream_id.contains_key(&up.id) {
                continue;
            }
            let found = by_code_local
                .get(&up.sku_code.trim().to_lowercase())
                .copied()
                .or_else(|| (local.len() == 1 && remote.len() == 1).then(|| &local[0]));
            if let Some(sku) = found {
                index.by_upstream_id.insert(
                    up.id,
                    LocalSkuRef {
                        id: sku.id,
                        code: sku.code.trim().to_owned(),
                    },
                );
            }
        }
        index
    }

    fn is_empty(&self) -> bool {
        self.by_upstream_id.is_empty() && self.by_code.is_empty()
    }

    /// Local scope `(sku_id, sku_code)` of a supplier tier; `None` = drop the tier.
    fn scope(&self, tier: &RemoteTier) -> Option<(Id, String)> {
        let code = tier.sku_code.trim();
        if !code.is_empty() {
            if let Some(r) = self.by_code.get(&code.to_lowercase()) {
                if tier.sku_id > 0
                    && let Some(id_ref) = self.by_upstream_id.get(&tier.sku_id)
                    && id_ref.id != r.id
                {
                    return None;
                }
                return Some((r.id, r.code.clone()));
            }
            if !self.is_empty() {
                return None;
            }
            return Some((0, code.to_owned()));
        }
        if tier.sku_id > 0 {
            return self
                .by_upstream_id
                .get(&tier.sku_id)
                .map(|r| (r.id, r.code.clone()));
        }
        Some((0, String::new()))
    }
}

/// Converts supplier tiers into local tiers: invalid or unmappable tiers are dropped
/// with a warning, prices go through the connection pricing; an inconsistent set
/// yields an empty list (callers then keep the local configuration, UPS-13).
pub fn convert_tiers(
    tiers: &[RemoteTier],
    pricing: &Pricing,
    index: &WholesaleIndex,
) -> Vec<WholesaleTier> {
    let mut inputs = Vec::new();
    for tier in tiers {
        let Ok(min_quantity) = i32::try_from(tier.min_quantity) else {
            continue;
        };
        let Some(pair) = price_pair(&tier.unit_price, pricing) else {
            continue;
        };
        if min_quantity <= 0 || !pair.upstream.is_positive() {
            continue;
        }
        let Some((sku_id, sku_code)) = index.scope(tier) else {
            continue;
        };
        inputs.push(WholesalePriceInput {
            sku_id,
            sku_code,
            min_quantity,
            unit_price: pair.price,
        });
    }
    normalize_wholesale_prices(&inputs).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Content images
// ---------------------------------------------------------------------------

/// Markdown `![..](url)` and HTML `<img src="url">` references.
static IMAGE_REF: std::sync::LazyLock<Option<regex::Regex>> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r#"!\[[^\]]*\]\(([^)\s]+)[^)]*\)|<img[^>]+src=["']([^"']+)["']"#).ok()
});

/// Unique image URLs referenced by localized content, skipping local uploads.
pub fn content_image_urls(content: &JsonMap) -> Vec<String> {
    let Some(re) = IMAGE_REF.as_ref() else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for text in content.values().filter_map(serde_json::Value::as_str) {
        for cap in re.captures_iter(text) {
            let url = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str().to_owned())
                .unwrap_or_default();
            if url.is_empty() || url.starts_with("/uploads/") {
                continue;
            }
            if seen.insert(url.clone()) {
                out.push(url);
            }
        }
    }
    out
}

/// Replaces image URLs in every localized text.
pub fn replace_content_urls(content: &JsonMap, replacements: &HashMap<String, String>) -> JsonMap {
    content
        .iter()
        .map(|(lang, v)| {
            let v = match v.as_str() {
                Some(text) => {
                    let mut text = text.to_owned();
                    for (from, to) in replacements {
                        if from != to {
                            text = text.replace(from.as_str(), to);
                        }
                    }
                    serde_json::Value::String(text)
                }
                None => v.clone(),
            };
            (lang.clone(), v)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Import plan
// ---------------------------------------------------------------------------

/// A local SKU to create for a supplier SKU (`upstream_sku_id` 0 = default SKU, no mapping).
#[derive(Debug, Clone, PartialEq)]
pub struct NewLocalSku {
    pub upstream_sku_id: Id,
    pub sku_code: String,
    pub spec_values: JsonMap,
    pub price: Amount,
    pub cost: Amount,
    pub is_active: bool,
    pub upstream_price: Amount,
    pub upstream_stock: i32,
    pub upstream_is_active: bool,
}

/// Everything the import transaction writes (UPS-20: one transaction).
#[derive(Debug, Clone, PartialEq)]
pub struct ImportPlan {
    pub connection_id: Id,
    pub upstream_product_id: Id,
    pub category_id: Id,
    pub slug: String,
    pub seo_meta: JsonMap,
    pub title: JsonMap,
    pub description: JsonMap,
    pub content: JsonMap,
    pub manual_form_schema: JsonMap,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub price: Amount,
    pub cost: Amount,
    pub skus: Vec<NewLocalSku>,
    pub upstream_fulfillment_type: String,
    pub tiers: Vec<RemoteTier>,
    pub pricing: Pricing,
}

/// Builds the import plan of an active supplier product (images / content already
/// localised by the caller). SKUs with an invalid price are not created (UPS-08);
/// the product is refused when no positive local price can be derived.
pub fn plan_import(
    remote: &RemoteProduct,
    pricing: Pricing,
    connection_id: Id,
    category_id: Id,
    slug: String,
) -> Result<ImportPlan> {
    let invalid_price = || Error::bad_request(product_keys::PRICE_INVALID);
    let mut skus = Vec::with_capacity(remote.skus.len());
    for up in &remote.skus {
        let Some(pair) = price_pair(&up.price_amount, &pricing) else {
            continue;
        };
        skus.push(NewLocalSku {
            upstream_sku_id: up.id,
            sku_code: up.sku_code.clone(),
            spec_values: up.spec_values.clone(),
            price: pair.price,
            cost: pair.cost,
            is_active: up.is_active,
            upstream_price: pair.upstream,
            upstream_stock: stock_of(up.stock_quantity),
            upstream_is_active: up.is_active,
        });
    }
    if !remote.skus.is_empty() && skus.is_empty() {
        return Err(invalid_price());
    }
    let product_pair = price_pair(&remote.price_amount, &pricing);
    let (price, cost) = match product_pair {
        Some(p) => (p.price, p.cost),
        None => skus
            .iter()
            .min_by_key(|s| s.price)
            .map(|s| (s.price, s.cost))
            .ok_or_else(invalid_price)?,
    };
    if skus.is_empty() {
        skus.push(NewLocalSku {
            upstream_sku_id: 0,
            sku_code: DEFAULT_SKU_CODE.to_owned(),
            spec_values: JsonMap::new(),
            price,
            cost,
            is_active: true,
            upstream_price: Amount::ZERO,
            upstream_stock: 0,
            upstream_is_active: true,
        });
    }
    Ok(ImportPlan {
        connection_id,
        upstream_product_id: remote.id,
        category_id,
        slug,
        seo_meta: remote.seo_meta.clone(),
        title: remote.title.clone(),
        description: remote.description.clone(),
        content: remote.content.clone(),
        manual_form_schema: remote.manual_form_schema.clone().unwrap_or_default(),
        images: remote.images.clone(),
        tags: remote.tags.clone(),
        price,
        cost,
        skus,
        upstream_fulfillment_type: effective_fulfillment_type(&remote.fulfillment_type).to_owned(),
        tiers: remote.wholesale_prices.clone(),
        pricing,
    })
}

/// Default slug of an imported product (UPS-14: never empty).
pub fn default_slug(connection_id: Id, upstream_product_id: Id, now: DateTime<Utc>) -> String {
    format!(
        "upstream-{connection_id}-{upstream_product_id}-{}",
        now.timestamp_millis()
    )
}

// ---------------------------------------------------------------------------
// Sync plan
// ---------------------------------------------------------------------------

/// Update of one SKU mapping row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuMappingChange {
    pub id: Id,
    /// `None` keeps the stored supplier price (unparseable price, UPS-08).
    pub upstream_price: Option<Amount>,
    pub upstream_stock: i32,
    pub upstream_is_active: bool,
}

/// Update of one local SKU.
#[derive(Debug, Clone, PartialEq)]
pub struct LocalSkuChange {
    pub sku_id: Id,
    pub spec_values: Option<JsonMap>,
    pub is_active: bool,
    /// `(price, cost)` when prices follow the supplier.
    pub price: Option<(Amount, Amount)>,
}

/// Everything one product sync writes.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncPlan {
    pub manual_form_schema: Option<JsonMap>,
    pub mapping_changes: Vec<SkuMappingChange>,
    pub local_changes: Vec<LocalSkuChange>,
    pub new_skus: Vec<NewLocalSku>,
    /// Recompute product price / cost from the active SKUs.
    pub recalc_product_price: bool,
    pub upstream_fulfillment_type: String,
    pub tiers: Vec<RemoteTier>,
}

/// Aligns the local SKUs with an *active* supplier product (UPS-14 (2)): removed
/// SKUs are disabled, new ones created, existing ones updated; prices only follow
/// with `auto_sync_price` and never become zero (UPS-08).
pub fn plan_sync(
    existing: &[SkuMapping],
    remote: &RemoteProduct,
    pricing: &Pricing,
    auto_sync_price: bool,
) -> SyncPlan {
    let remote_by_id: HashMap<Id, &RemoteSku> = remote.skus.iter().map(|s| (s.id, s)).collect();
    let known: HashSet<Id> = existing.iter().map(|m| m.upstream_sku_id).collect();
    let mut mapping_changes = Vec::new();
    let mut local_changes = Vec::new();
    for m in existing {
        let Some(up) = remote_by_id.get(&m.upstream_sku_id) else {
            mapping_changes.push(SkuMappingChange {
                id: m.id,
                upstream_price: None,
                upstream_stock: 0,
                upstream_is_active: false,
            });
            local_changes.push(LocalSkuChange {
                sku_id: m.local_sku_id,
                spec_values: None,
                is_active: false,
                price: None,
            });
            continue;
        };
        let upstream = parse_upstream_price(&up.price_amount);
        mapping_changes.push(SkuMappingChange {
            id: m.id,
            upstream_price: upstream.map(Amount::new),
            upstream_stock: stock_of(up.stock_quantity),
            upstream_is_active: up.is_active,
        });
        if upstream.is_none() {
            // Only stock / availability follow an unparseable price.
            continue;
        }
        let price = if auto_sync_price {
            price_pair(&up.price_amount, pricing).map(|p| (p.price, p.cost))
        } else {
            None
        };
        local_changes.push(LocalSkuChange {
            sku_id: m.local_sku_id,
            spec_values: Some(up.spec_values.clone()),
            is_active: up.is_active,
            price,
        });
    }
    let new_skus = remote
        .skus
        .iter()
        .filter(|s| !known.contains(&s.id))
        .filter_map(|up| {
            let pair = price_pair(&up.price_amount, pricing)?;
            Some(NewLocalSku {
                upstream_sku_id: up.id,
                sku_code: up.sku_code.clone(),
                spec_values: up.spec_values.clone(),
                price: pair.price,
                cost: pair.cost,
                is_active: up.is_active,
                upstream_price: pair.upstream,
                upstream_stock: stock_of(up.stock_quantity),
                upstream_is_active: up.is_active,
            })
        })
        .collect();
    SyncPlan {
        manual_form_schema: remote.manual_form_schema.clone(),
        mapping_changes,
        local_changes,
        new_skus,
        recalc_product_price: auto_sync_price,
        upstream_fulfillment_type: effective_fulfillment_type(&remote.fulfillment_type).to_owned(),
        tiers: remote.wholesale_prices.clone(),
    }
}

/// Lowest price / cost of the active SKUs (original `recalcProductPrice`).
pub fn lowest_prices(skus: &[(Amount, Amount, bool)]) -> Option<(Amount, Amount)> {
    let active: Vec<_> = skus.iter().filter(|s| s.2).collect();
    let price = active.iter().map(|s| s.0).min()?;
    let cost = active.iter().map(|s| s.1).min()?;
    Some((price, cost))
}

/// Forced full-sync interval: `max(24h, 3 × sync interval)` (UPS-06).
pub fn full_sync_interval(sync_interval: Duration) -> Duration {
    let floor = Duration::hours(24);
    let scaled = sync_interval * 3;
    if scaled > floor { scaled } else { floor }
}

/// Outcome of a paginated catalog fetch (UPS-06/17): products are only presumed
/// deleted when the fetch was complete *and* the supplier echoed `includes_inactive`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FetchOutcome {
    pub complete: bool,
    pub includes_inactive: bool,
}

impl FetchOutcome {
    pub fn may_mark_deleted(self, full_sync: bool) -> bool {
        full_sync && self.complete && self.includes_inactive
    }
}

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

/// Stores images downloaded from a supplier (validated like uploads, recorded in the
/// media library with scene `upstream`).
#[async_trait]
pub trait ImageStore: Send + Sync {
    /// Returns the local `/uploads/...` URL.
    async fn store(&self, filename: &str, bytes: &[u8]) -> Result<String>;
}

/// Admin list filter.
#[derive(Debug, Clone, Default)]
pub struct MappingFilter {
    pub page: PageRequest,
    pub connection_id: Id,
    pub upstream_status: Option<UpstreamStatus>,
    /// `Some(true)` active mappings, `Some(false)` inactive.
    pub active: Option<bool>,
    pub search: String,
}

/// Persistence of mappings plus the atomic catalog writes of import / sync.
#[async_trait]
pub trait MappingRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<ProductMapping>>;
    async fn get_by_upstream(
        &self,
        connection_id: Id,
        upstream_product_id: Id,
    ) -> Result<Option<ProductMapping>>;
    /// Page with `connection` and `product` (all SKUs) attached.
    async fn list(&self, filter: &MappingFilter) -> Result<Page<ProductMapping>>;
    async fn list_active(&self) -> Result<Vec<ProductMapping>>;
    async fn list_active_by_connection(&self, connection_id: Id) -> Result<Vec<ProductMapping>>;
    async fn mapped_upstream_ids(&self, connection_id: Id) -> Result<Vec<Id>>;
    async fn sku_mappings(&self, mapping_id: Id) -> Result<Vec<SkuMapping>>;
    async fn sku_mapping_by_local_sku(&self, local_sku_id: Id) -> Result<Option<SkuMapping>>;
    async fn set_active(&self, id: Id, active: bool, now: DateTime<Utc>) -> Result<bool>;
    /// Removes the mapping and its SKU mappings; the local product loses `is_mapped`
    /// and, when it was `upstream`, becomes an inactive `manual` product.
    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()>;
    /// True for 0 or an existing leaf category.
    async fn category_assignable(&self, category_id: Id) -> Result<bool>;
    /// Finds (by slug), restores or creates a category; returns its id.
    async fn ensure_category(
        &self,
        slug: &str,
        name: &JsonMap,
        parent_id: Id,
        now: DateTime<Utc>,
    ) -> Result<Id>;
    /// Product + SKUs + wholesale tiers + mapping + SKU mappings in one transaction (UPS-20).
    async fn import(&self, plan: &ImportPlan, now: DateTime<Utc>) -> Result<ProductMapping>;
    /// Applies a sync of an active supplier product in one transaction.
    async fn apply_sync(
        &self,
        mapping: &ProductMapping,
        remote: &RemoteProduct,
        pricing: &Pricing,
        auto_sync_price: bool,
        now: DateTime<Utc>,
    ) -> Result<()>;
    /// Supplier product delisted/deleted: local product and SKUs off, SKU mappings
    /// inactive with zero stock; `deleted` also disables the mapping (UPS-14 (1)).
    async fn mark_unavailable(
        &self,
        mapping: &ProductMapping,
        status: UpstreamStatus,
        now: DateTime<Utc>,
    ) -> Result<()>;
    /// Recomputes local prices of every active mapping of a connection from the stored
    /// supplier prices; returns the number of products updated.
    async fn reapply_pricing(
        &self,
        connection_id: Id,
        pricing: &Pricing,
        now: DateTime<Utc>,
    ) -> Result<u64>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn amt(s: &str) -> Amount {
        s.parse().unwrap()
    }

    fn rsku(id: Id, code: &str, price: &str, stock: i64, active: bool) -> RemoteSku {
        RemoteSku {
            id,
            sku_code: code.into(),
            price_amount: price.into(),
            stock_quantity: stock,
            is_active: active,
            ..RemoteSku::default()
        }
    }

    fn smap(id: Id, local: Id, up: Id, price: &str) -> SkuMapping {
        SkuMapping {
            id,
            product_mapping_id: 1,
            local_sku_id: local,
            upstream_sku_id: up,
            upstream_price: amt(price),
            upstream_stock: 5,
            upstream_is_active: true,
            stock_synced_at: None,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        }
    }

    fn tier(sku_id: Id, code: &str, qty: i64, price: &str) -> RemoteTier {
        RemoteTier {
            sku_id,
            sku_code: code.into(),
            min_quantity: qty,
            unit_price: price.into(),
        }
    }

    fn local(id: Id, code: &str) -> LocalSkuRef {
        LocalSkuRef {
            id,
            code: code.into(),
        }
    }

    // UPS-04: supplier SKU ids go through the mapping; unmappable tiers are dropped.
    #[test]
    fn ups04_tiers_are_translated_to_local_skus() {
        let pricing = Pricing::default();
        let remote = vec![rsku(5, "A", "10", 1, true), rsku(6, "B", "10", 1, true)];
        let index = WholesaleIndex::build(&[local(12, "A"), local(13, "B")], &remote, &[(12, 5)]);
        let out = convert_tiers(&[tier(5, "", 3, "8")], &pricing, &index);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].sku_id, 12);
        assert_eq!(out[0].sku_code, "A");
        // unknown code → dropped
        assert!(convert_tiers(&[tier(0, "ZZ", 3, "8")], &pricing, &index).is_empty());
        // id and code point at different local SKUs → dropped
        assert!(convert_tiers(&[tier(5, "B", 3, "8")], &pricing, &index).is_empty());
        // unmapped id → dropped
        assert!(convert_tiers(&[tier(99, "", 3, "8")], &pricing, &index).is_empty());
        // single ↔ single pairing
        let single =
            WholesaleIndex::build(&[local(20, "DEFAULT")], &[rsku(7, "X", "1", 1, true)], &[]);
        let out = convert_tiers(&[tier(7, "", 2, "0.5")], &pricing, &single);
        assert_eq!(out[0].sku_id, 20);
    }

    // UPS-13: invalid tiers are skipped individually.
    #[test]
    fn ups13_invalid_tiers_are_skipped() {
        let pricing = Pricing::default();
        let out = convert_tiers(
            &[
                tier(0, "", 5, "0"),
                tier(0, "", 0, "5"),
                tier(0, "", 10, "4"),
                tier(0, "", 3, "x"),
            ],
            &pricing,
            &WholesaleIndex::default(),
        );
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].min_quantity, 10);
        assert_eq!(out[0].unit_price, amt("4.00"));
    }

    fn remote(skus: Vec<RemoteSku>) -> RemoteProduct {
        RemoteProduct {
            id: 9,
            price_amount: "10".into(),
            fulfillment_type: "auto".into(),
            is_active: true,
            skus,
            ..RemoteProduct::default()
        }
    }

    // UPS-08 (1): an unparseable supplier price only syncs stock; UPS-14 (2): SKU diff.
    #[test]
    fn ups08_ups14_sync_plan() {
        let pricing = Pricing {
            exchange_rate: d("2"),
            ..Pricing::default()
        };
        let existing = vec![
            smap(1, 11, 101, "5"),
            smap(2, 12, 102, "5"),
            smap(3, 13, 103, "5"),
        ];
        let r = remote(vec![
            rsku(101, "A", "abc", 7, true),
            rsku(102, "B", "6", 3, true),
            rsku(104, "D", "4", -1, true),
            rsku(105, "E", "bad", 1, true),
        ]);
        let plan = plan_sync(&existing, &r, &pricing, true);
        // 101: price kept, stock follows, no local change
        assert_eq!(plan.mapping_changes[0].upstream_price, None);
        assert_eq!(plan.mapping_changes[0].upstream_stock, 7);
        assert!(!plan.local_changes.iter().any(|c| c.sku_id == 11));
        // 102: price follows (auto sync) through the exchange rate
        let c12 = plan.local_changes.iter().find(|c| c.sku_id == 12).unwrap();
        assert_eq!(c12.price, Some((amt("12.00"), amt("12.00"))));
        // 103: removed upstream → local SKU disabled, stock 0
        let c13 = plan.local_changes.iter().find(|c| c.sku_id == 13).unwrap();
        assert!(!c13.is_active);
        assert_eq!(plan.mapping_changes[2].upstream_stock, 0);
        assert!(!plan.mapping_changes[2].upstream_is_active);
        // 104 created, 105 (bad price) skipped
        assert_eq!(plan.new_skus.len(), 1);
        assert_eq!(plan.new_skus[0].upstream_sku_id, 104);
        assert_eq!(plan.new_skus[0].upstream_stock, -1);
        assert_eq!(plan.upstream_fulfillment_type, "auto");
        assert!(plan.recalc_product_price);

        // without auto sync the local price never changes
        let plan = plan_sync(&existing, &r, &pricing, false);
        assert!(plan.local_changes.iter().all(|c| c.price.is_none()));
        assert!(!plan.recalc_product_price);
    }

    // UPS-08 (2): an invalid SKU price is not imported at zero.
    #[test]
    fn ups08_import_skips_invalid_skus() {
        let pricing = Pricing {
            exchange_rate: d("7.2"),
            ..Pricing::default()
        };
        let r = remote(vec![
            rsku(1, "A", "1.00", 3, true),
            rsku(2, "B", "N/A", 3, true),
        ]);
        let plan = plan_import(&r, pricing, 1, 0, "s".into()).unwrap();
        assert_eq!(plan.skus.len(), 1);
        assert_eq!(plan.skus[0].price, amt("7.20"));
        assert_eq!(plan.skus[0].cost, amt("7.20"));
        assert_eq!(plan.price, amt("72.00"));
        assert_eq!(plan.upstream_fulfillment_type, "auto");

        let bad = RemoteProduct {
            price_amount: "x".into(),
            ..remote(vec![rsku(2, "B", "N/A", 3, true)])
        };
        let err = plan_import(&bad, pricing, 1, 0, "s".into()).unwrap_err();
        assert_eq!(err.key(), product_keys::PRICE_INVALID);

        // no SKUs → one DEFAULT SKU at the product price
        let plan = plan_import(&remote(vec![]), Pricing::default(), 1, 0, "s".into()).unwrap();
        assert_eq!(plan.skus.len(), 1);
        assert_eq!(plan.skus[0].sku_code, DEFAULT_SKU_CODE);
        assert_eq!(plan.skus[0].upstream_sku_id, 0);
    }

    // UPS-06: full-sync interval follows the sync interval with a 24h floor.
    #[test]
    fn ups06_full_sync_interval() {
        assert_eq!(full_sync_interval(Duration::hours(12)), Duration::hours(36));
        assert_eq!(full_sync_interval(Duration::hours(6)), Duration::hours(24));
        assert_eq!(
            full_sync_interval(Duration::minutes(5)),
            Duration::hours(24)
        );
    }

    // UPS-06 / UPS-14: deletion is only inferred from a complete, inactive-aware full sync.
    #[test]
    fn ups06_deletion_inference() {
        let ok = FetchOutcome {
            complete: true,
            includes_inactive: true,
        };
        assert!(ok.may_mark_deleted(true));
        assert!(!ok.may_mark_deleted(false));
        assert!(
            !FetchOutcome {
                complete: false,
                includes_inactive: true
            }
            .may_mark_deleted(true)
        );
        assert!(
            !FetchOutcome {
                complete: true,
                includes_inactive: false
            }
            .may_mark_deleted(true)
        );
    }

    #[test]
    fn content_images_are_found_and_replaced() {
        let mut content = JsonMap::new();
        content.insert(
            "zh-CN".into(),
            serde_json::Value::String(
                "![a](https://s/a.png) <img class=\"x\" src='/b.jpg'> ![c](/uploads/c.png)".into(),
            ),
        );
        content.insert(
            "en-US".into(),
            serde_json::Value::String("![a](https://s/a.png)".into()),
        );
        let urls = content_image_urls(&content);
        assert_eq!(urls.len(), 2);
        assert!(
            urls.contains(&"https://s/a.png".to_owned()) && urls.contains(&"/b.jpg".to_owned())
        );
        let mut map = HashMap::new();
        map.insert("https://s/a.png".to_owned(), "/uploads/x.png".to_owned());
        let out = replace_content_urls(&content, &map);
        assert_eq!(out["en-US"], "![a](/uploads/x.png)");
    }

    #[test]
    fn lowest_prices_of_active_skus() {
        assert_eq!(
            lowest_prices(&[
                (amt("5"), amt("4"), true),
                (amt("3"), amt("9"), false),
                (amt("7"), amt("2"), true)
            ]),
            Some((amt("5"), amt("2")))
        );
        assert_eq!(lowest_prices(&[(amt("5"), amt("4"), false)]), None);
    }
}
