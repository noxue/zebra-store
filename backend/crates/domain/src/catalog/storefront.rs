//! Storefront (public) product view: stock aggregation, masking and price decoration.
//!
//! Whitelisted DTO (MISC-02): no cost price, no instructions, masked stock outside
//! `exact` mode (ORD-08).

use std::collections::HashMap;

use serde::Serialize;
use zs_shared::i18n::LocalizedText;
use zs_shared::money::Amount;

use super::card_secret::{SecretStatus, StockCount};
use super::category::Category;
use super::product::{
    FulfillmentType, JsonMap, Product, ProductSku, RelatedPost, UpstreamMapping,
    upstream_display_type,
};
use super::stock::{
    MANUAL_STOCK_UNLIMITED, StockDisplayMode, StockPolicy, StockStatus, mask_sold, mask_stock,
    stock_quantity,
};
use super::wholesale::{WholesaleTier, public_tiers};
use crate::Id;
use crate::marketing::member_level::{MemberLevel, MemberLevelPrice, resolve_member_price};
use crate::marketing::promotion::{Promotion, apply_promotion};
use crate::reseller::pricing::{DisplayPrices, PricedProduct, PricedSku};

/// Fills product/SKU auto stock from grouped card-secret counts (DLV-08).
///
/// Legacy secrets with `sku_id = 0` are attributed once: to the active DEFAULT SKU, else
/// the first active SKU, else the DEFAULT SKU, else the first SKU.
pub fn apply_auto_stock_counts(products: &mut [Product], counts: &[StockCount]) {
    let mut map: HashMap<Id, HashMap<Id, HashMap<SecretStatus, i64>>> = HashMap::new();
    for c in counts {
        if let Some(status) = SecretStatus::parse(&c.status) {
            *map.entry(c.product_id)
                .or_default()
                .entry(c.sku_id)
                .or_default()
                .entry(status)
                .or_default() += c.total;
        }
    }
    for product in products.iter_mut().filter(|p| p.is_auto()) {
        let Some(by_sku) = map.get(&product.id) else {
            continue;
        };
        let get = |sku: Id, s: SecretStatus| {
            by_sku
                .get(&sku)
                .and_then(|m| m.get(&s))
                .copied()
                .unwrap_or(0)
        };
        let sum = |s: SecretStatus| {
            by_sku
                .values()
                .map(|m| m.get(&s).copied().unwrap_or(0))
                .sum::<i64>()
        };
        let (available, locked, used) = (
            sum(SecretStatus::Available),
            sum(SecretStatus::Reserved),
            sum(SecretStatus::Used),
        );
        product.auto_stock_available = available;
        product.auto_stock_total = available + locked;
        product.auto_stock_locked = locked;
        product.auto_stock_sold = used;

        let legacy_target = legacy_stock_target(&product.skus);
        let has_legacy = by_sku.contains_key(&0);
        for (idx, sku) in product.skus.iter_mut().enumerate() {
            let mut available = get(sku.id, SecretStatus::Available);
            let mut locked = get(sku.id, SecretStatus::Reserved);
            let mut used = get(sku.id, SecretStatus::Used);
            if has_legacy && Some(idx) == legacy_target {
                available += get(0, SecretStatus::Available);
                locked += get(0, SecretStatus::Reserved);
                used += get(0, SecretStatus::Used);
            }
            sku.auto_stock_available = available;
            sku.auto_stock_total = available + locked;
            sku.auto_stock_locked = locked;
            sku.auto_stock_sold = used;
        }
    }
}

fn legacy_stock_target(skus: &[ProductSku]) -> Option<usize> {
    if skus.is_empty() {
        return None;
    }
    skus.iter()
        .position(|s| s.is_active && s.is_default_code())
        .or_else(|| skus.iter().position(|s| s.is_active))
        .or_else(|| skus.iter().position(ProductSku::is_default_code))
        .or(Some(0))
}

/// Public category (no status/timestamps).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublicCategory {
    pub id: Id,
    pub parent_id: Id,
    pub slug: String,
    pub name: LocalizedText,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
    pub sort_order: i32,
}

impl From<&Category> for PublicCategory {
    fn from(c: &Category) -> Self {
        Self {
            id: c.id,
            parent_id: c.parent_id,
            slug: c.slug.clone(),
            name: c.name.clone(),
            icon: c.icon.clone(),
            sort_order: c.sort_order,
        }
    }
}

/// Promotion rule shown to buyers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PromotionRule {
    pub id: Id,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Amount,
    pub min_amount: Amount,
}

/// Level-specific price shown to buyers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemberPriceView {
    pub member_level_id: Id,
    pub sku_id: Id,
    pub price_amount: Amount,
}

/// Public SKU.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublicSku {
    pub id: Id,
    pub sku_code: String,
    pub spec_values: JsonMap,
    pub price_amount: Amount,
    pub manual_stock_total: i64,
    pub manual_stock_sold: i32,
    pub auto_stock_available: i64,
    pub upstream_stock: i64,
    pub stock_status: StockStatus,
    pub stock_display_mode: StockDisplayMode,
    pub stock_display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_range_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_range_max: Option<i32>,
    pub stock_quantity_hidden: bool,
    pub is_sold_out: bool,
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_price_amount: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_price_amount: Option<Amount>,
}

/// Public product (list and detail).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublicProduct {
    pub id: Id,
    pub category_id: Id,
    pub slug: String,
    pub seo_meta: JsonMap,
    pub title: JsonMap,
    pub description: JsonMap,
    pub content: JsonMap,
    pub price_amount: Amount,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub wholesale_prices: Vec<WholesaleTier>,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub purchase_type: super::product::PurchaseType,
    pub min_purchase_quantity: i32,
    pub max_purchase_quantity: i32,
    pub stock_display_mode: StockDisplayMode,
    pub stock_display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_range_min: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_range_max: Option<i32>,
    pub stock_quantity_hidden: bool,
    pub fulfillment_type: FulfillmentType,
    pub manual_form_schema: JsonMap,
    pub manual_stock_available: i64,
    pub auto_stock_available: i64,
    pub stock_status: StockStatus,
    pub is_sold_out: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub payment_channel_ids: Vec<Id>,
    pub category: PublicCategory,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub skus: Vec<PublicSku>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<Id>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub promotion_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub promotion_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_price_amount: Option<Amount>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub promotion_rules: Vec<PromotionRule>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub member_prices: Vec<MemberPriceView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub related_posts: Vec<RelatedPost>,
}

/// Pricing context of one product.
#[derive(Debug, Clone, Copy, Default)]
pub struct Decoration<'a> {
    /// Effective promotions of the product sorted by `min_amount ASC`.
    pub promotions: &'a [Promotion],
    /// Level prices of the product.
    pub level_prices: &'a [MemberLevelPrice],
    /// Active levels (member price lists only show active levels, PRC-07).
    pub active_levels: &'a [MemberLevel],
    /// Viewer's level for per-SKU `member_price_amount` (the original public API passes none).
    pub viewer_level: Option<&'a MemberLevel>,
    /// Upstream mapping of an `upstream` product.
    pub upstream: Option<&'a UpstreamMapping>,
}

/// Stock state computed before masking.
#[derive(Debug, Clone, Copy)]
struct StockState {
    manual_available: i64,
    auto_available: i64,
    status: StockStatus,
    sold_out: bool,
}

fn product_stock(product: &mut Product, upstream: Option<&UpstreamMapping>) -> StockState {
    let policy = StockPolicy::STOREFRONT;
    let mut state = StockState {
        manual_available: 0,
        auto_available: 0,
        status: StockStatus::InStock,
        sold_out: false,
    };
    match product.fulfillment_type {
        FulfillmentType::Upstream => upstream_stock(product, upstream, state),
        FulfillmentType::Manual => {
            let active: Vec<&ProductSku> = product.skus.iter().filter(|s| s.is_active).collect();
            let available = if !active.is_empty() {
                if active
                    .iter()
                    .any(|s| s.manual_stock_total == MANUAL_STOCK_UNLIMITED)
                {
                    return unlimited_manual(state);
                }
                active
                    .iter()
                    .map(|s| i64::from(s.manual_stock_total.max(0)))
                    .sum()
            } else if product.manual_stock_total == MANUAL_STOCK_UNLIMITED {
                return unlimited_manual(state);
            } else {
                i64::from(product.manual_stock_total.max(0))
            };
            state.manual_available = available;
            state.status = policy.status(available);
            state.sold_out = state.status == StockStatus::OutOfStock;
            state
        }
        FulfillmentType::Auto => {
            let available: i64 = product
                .skus
                .iter()
                .filter(|s| s.is_active)
                .map(|s| s.auto_stock_available)
                .sum();
            state.auto_available = available;
            state.status = policy.status(available);
            state.sold_out = state.status == StockStatus::OutOfStock;
            state
        }
    }
}

fn unlimited_manual(mut state: StockState) -> StockState {
    state.manual_available = i64::from(MANUAL_STOCK_UNLIMITED);
    state.status = StockStatus::Unlimited;
    state.sold_out = false;
    state
}

fn upstream_stock(
    product: &mut Product,
    mapping: Option<&UpstreamMapping>,
    mut state: StockState,
) -> StockState {
    let Some(mapping) = mapping else {
        // No mapping: show as manual and in stock to avoid a false sold-out.
        product.fulfillment_type = FulfillmentType::Manual;
        return state;
    };
    let display = upstream_display_type(mapping);
    product.fulfillment_type = display;
    if mapping.skus.is_empty() {
        return state;
    }
    let by_local: HashMap<Id, _> = mapping.skus.iter().map(|m| (m.local_sku_id, m)).collect();
    let mut unlimited = false;
    let mut total: i64 = 0;
    let mut any_active = false;
    for sku in &mut product.skus {
        let Some(m) = by_local.get(&sku.id).filter(|m| m.upstream_is_active) else {
            sku.upstream_stock = 0;
            continue;
        };
        any_active = true;
        sku.upstream_stock = m.upstream_stock;
        if display == FulfillmentType::Auto {
            sku.auto_stock_available = i64::from(m.upstream_stock);
        } else {
            sku.manual_stock_total = m.upstream_stock;
        }
        if m.upstream_stock == MANUAL_STOCK_UNLIMITED {
            unlimited = true;
        } else {
            total += i64::from(m.upstream_stock);
        }
    }
    if !any_active {
        state.status = StockStatus::OutOfStock;
        state.sold_out = true;
        return state;
    }
    if unlimited {
        if display == FulfillmentType::Auto {
            state.auto_available = -1;
        } else {
            state.manual_available = i64::from(MANUAL_STOCK_UNLIMITED);
        }
        state.status = StockStatus::Unlimited;
        return state;
    }
    if display == FulfillmentType::Auto {
        state.auto_available = total;
    } else {
        state.manual_available = total;
    }
    state.status = StockPolicy::STOREFRONT.status(total);
    state.sold_out = state.status == StockStatus::OutOfStock;
    state
}

/// Builds the public view of a product (active SKUs only are expected in `product.skus`).
///
/// Display price and product-level promotion come from the same SKU — the first active
/// SKU in `sort_order DESC, id ASC` order (PRC-13, PRC-14); SKU promotion prices use the
/// SKU's own price and are shown only when strictly lower (PRC-08).
pub fn decorate_public_product(mut product: Product, deco: &Decoration<'_>) -> PublicProduct {
    // LQA-I2: a buyer form exists for manual products and for upstream products whose
    // supplier asks for input (shown as `auto`); a local auto product never has one,
    // so the storefront can rely on the schema alone.
    let takes_form = product.fulfillment_type != FulfillmentType::Auto;
    let state = product_stock(&mut product, deco.upstream);
    let mode = product.stock_display_mode;
    let policy = StockPolicy::STOREFRONT;
    let display_sku = product.skus.iter().find(|s| s.is_active);
    let display_price = display_sku.map_or(product.price_amount, |s| s.price_amount);
    let display_sku_id = display_sku.map_or(0, |s| s.id);

    let promotion_rules = deco
        .promotions
        .iter()
        .map(|r| PromotionRule {
            id: r.id,
            name: r.name.trim().to_owned(),
            kind: r.kind.trim().to_owned(),
            value: r.value,
            min_amount: r.min_amount,
        })
        .collect();
    let member_prices = deco
        .level_prices
        .iter()
        .filter(|p| deco.active_levels.iter().any(|l| l.id == p.member_level_id))
        .map(|p| MemberPriceView {
            member_level_id: p.member_level_id,
            sku_id: p.sku_id,
            price_amount: p.price_amount,
        })
        .collect();

    let is_auto = product.fulfillment_type == FulfillmentType::Auto;
    let mut product_promotion: Option<(Promotion, Amount)> = None;
    let mut skus = Vec::with_capacity(product.skus.len());
    for sku in &product.skus {
        let mut promotion_price = None;
        let mut member_price = None;
        if sku.is_active {
            if deco.viewer_level.is_some() {
                let m = resolve_member_price(
                    deco.viewer_level,
                    deco.level_prices,
                    product.id,
                    sku.id,
                    sku.price_amount,
                );
                if m.price < sku.price_amount {
                    member_price = Some(m.price);
                }
            }
            if let Ok(Some((promo, price))) = apply_promotion(deco.promotions, sku.price_amount, 1)
                && price < sku.price_amount
            {
                promotion_price = Some(price);
                if display_sku_id != 0 && sku.id == display_sku_id {
                    product_promotion = Some((promo, price));
                }
            }
        }
        let quantity = stock_quantity(is_auto, sku.auto_stock_available, sku.manual_stock_total);
        let status = policy.status(quantity);
        let view = policy.display(mode, Some(status), quantity);
        skus.push(PublicSku {
            id: sku.id,
            sku_code: sku.sku_code.clone(),
            spec_values: sku.spec_values.clone(),
            price_amount: sku.price_amount,
            manual_stock_total: mask_stock(mode, i64::from(sku.manual_stock_total)),
            manual_stock_sold: mask_sold(mode, sku.manual_stock_sold),
            auto_stock_available: mask_stock(mode, sku.auto_stock_available),
            upstream_stock: mask_stock(mode, i64::from(sku.upstream_stock)),
            stock_status: status,
            stock_display_mode: view.mode,
            stock_display: view.display,
            stock_range_min: view.range_min,
            stock_range_max: view.range_max,
            stock_quantity_hidden: view.quantity_hidden,
            is_sold_out: status == StockStatus::OutOfStock,
            is_active: sku.is_active,
            promotion_price_amount: promotion_price,
            member_price_amount: member_price,
        });
    }

    let product_quantity = if state.status == StockStatus::Unlimited {
        i64::from(MANUAL_STOCK_UNLIMITED)
    } else {
        stock_quantity(
            is_auto,
            state.auto_available,
            i32::try_from(state.manual_available).unwrap_or(i32::MAX),
        )
    };
    let view = policy.display(mode, Some(state.status), product_quantity);
    let category = product
        .category
        .as_ref()
        .map(PublicCategory::from)
        .unwrap_or(PublicCategory {
            id: 0,
            parent_id: 0,
            slug: String::new(),
            name: LocalizedText::new(),
            icon: String::new(),
            sort_order: 0,
        });
    let (promotion_id, promotion_name, promotion_type, promotion_price_amount) =
        match product_promotion {
            Some((p, price)) => (
                Some(p.id),
                p.name.trim().to_owned(),
                p.kind.trim().to_owned(),
                Some(price),
            ),
            None => (None, String::new(), String::new(), None),
        };
    PublicProduct {
        id: product.id,
        category_id: product.category_id,
        slug: product.slug,
        seo_meta: product.seo_meta,
        title: product.title,
        description: product.description,
        content: product.content,
        price_amount: display_price,
        wholesale_prices: public_tiers(&product.wholesale_prices),
        images: product.images,
        tags: product.tags,
        purchase_type: product.purchase_type,
        min_purchase_quantity: product.min_purchase_quantity,
        max_purchase_quantity: product.max_purchase_quantity,
        stock_display_mode: view.mode,
        stock_display: view.display,
        stock_range_min: view.range_min,
        stock_range_max: view.range_max,
        stock_quantity_hidden: view.quantity_hidden,
        fulfillment_type: product.fulfillment_type,
        manual_form_schema: if takes_form {
            product.manual_form_schema
        } else {
            JsonMap::new()
        },
        manual_stock_available: mask_stock(mode, state.manual_available),
        auto_stock_available: mask_stock(mode, state.auto_available),
        stock_status: state.status,
        is_sold_out: state.sold_out,
        payment_channel_ids: super::product::decode_payment_channel_ids(
            &product.payment_channel_ids,
        ),
        category,
        skus,
        promotion_id,
        promotion_name,
        promotion_type,
        promotion_price_amount,
        promotion_rules,
        member_prices,
        related_posts: Vec::new(),
    }
}

/// The product as seen by the reseller pricing rules (active SKUs only).
pub fn priced_product(product: &Product) -> PricedProduct {
    PricedProduct {
        id: product.id,
        slug: product.slug.clone(),
        title: serde_json::Value::Object(product.title.clone()),
        price: product.price_amount,
        cost_price: product.cost_price_amount,
        is_active: product.is_active,
        skus: product
            .skus
            .iter()
            .filter(|s| s.is_active)
            .map(|s| PricedSku {
                id: s.id,
                sku_code: s.sku_code.clone(),
                spec_values: serde_json::Value::Object(s.spec_values.clone()),
                price: s.price_amount,
                cost_price: s.cost_price_amount,
                is_active: s.is_active,
            })
            .collect(),
    }
}

/// Reseller-site view of a public product (`decoratePublicProductForTenant`, LQA-R1):
/// the price is the reseller price the checkout charges (one pricing source,
/// [`crate::reseller::pricing::display_prices`]), hidden SKUs are dropped and no main-shop
/// discount is shown (wholesale tiers, promotions and member prices never apply on a
/// reseller site). `None` = the product is not listed on this site.
pub fn apply_reseller_display(
    mut view: PublicProduct,
    display: &DisplayPrices,
) -> Option<PublicProduct> {
    if !display.visible {
        return None;
    }
    let had_skus = !view.skus.is_empty();
    view.skus.retain(|s| {
        s.is_active
            && !display.hidden_sku_ids.contains(&s.id)
            && display.sku_prices.contains_key(&s.id)
    });
    if had_skus && view.skus.is_empty() {
        return None;
    }
    for sku in &mut view.skus {
        if let Some(price) = display.sku_prices.get(&sku.id) {
            sku.price_amount = *price;
        }
        sku.promotion_price_amount = None;
        sku.member_price_amount = None;
    }
    view.price_amount = display.display_price;
    view.wholesale_prices.clear();
    view.promotion_id = None;
    view.promotion_name.clear();
    view.promotion_type.clear();
    view.promotion_price_amount = None;
    view.promotion_rules.clear();
    view.member_prices.clear();
    Some(view)
}

#[cfg(test)]
mod tests {
    use super::super::product::UpstreamSkuMapping;
    use super::super::product::testkit::{product, sku};
    use super::*;
    use crate::marketing::member_level::testkit::{level, price};
    use crate::marketing::promotion::testkit::promo;

    fn amt(s: &str) -> Amount {
        s.parse().unwrap()
    }

    fn count(product_id: Id, sku_id: Id, status: &str, total: i64) -> StockCount {
        StockCount {
            product_id,
            sku_id,
            status: status.into(),
            total,
        }
    }

    // LQA-R1: reseller price replaces the base price everywhere, hidden SKUs vanish.
    #[test]
    fn reseller_display_overlays_price_and_hides() {
        let view = decorate_public_product(
            product(vec![sku(1, "A", true), sku(2, "B", true)]),
            &Decoration::default(),
        );
        assert_eq!(view.price_amount, amt("10.00"));
        let mut display = DisplayPrices {
            visible: true,
            product_id: 1,
            display_sku_id: 1,
            display_price: amt("11.00"),
            ..DisplayPrices::default()
        };
        display.sku_prices.insert(1, amt("11.00"));
        display.hidden_sku_ids.insert(2);
        let shown = apply_reseller_display(view.clone(), &display).unwrap();
        assert_eq!(shown.price_amount, amt("11.00"));
        assert_eq!(shown.skus.len(), 1);
        assert_eq!(shown.skus[0].id, 1);
        assert_eq!(shown.skus[0].price_amount, amt("11.00"));
        assert!(shown.wholesale_prices.is_empty() && shown.member_prices.is_empty());
        // Every SKU hidden, or the product rule hidden → not listed.
        let mut all_hidden = display.clone();
        all_hidden.sku_prices.clear();
        all_hidden.hidden_sku_ids.insert(1);
        assert!(apply_reseller_display(view.clone(), &all_hidden).is_none());
        assert!(apply_reseller_display(view, &DisplayPrices::default()).is_none());
    }

    // DLV-08 (1)
    #[test]
    fn legacy_stock_goes_to_default_sku_once() {
        let mut p = product(vec![
            sku(1, "A", true),
            sku(2, "B", true),
            sku(3, "DEFAULT", true),
            sku(4, "OFF", false),
        ]);
        p.fulfillment_type = FulfillmentType::Auto;
        let mut list = vec![p];
        apply_auto_stock_counts(
            &mut list,
            &[count(1, 0, "available", 5), count(1, 4, "available", 10)],
        );
        let p = &list[0];
        assert_eq!(p.skus[0].auto_stock_available, 0);
        assert_eq!(p.skus[2].auto_stock_available, 5);
        let public = decorate_public_product(
            Product {
                skus: p.skus.iter().filter(|s| s.is_active).cloned().collect(),
                ..p.clone()
            },
            &Decoration::default(),
        );
        // the inactive SKU's 10 secrets are not counted
        assert_eq!(public.auto_stock_available, 5);
    }

    // DLV-08 (2)
    #[test]
    fn per_sku_auto_stock_includes_legacy() {
        let mut p = product(vec![sku(1, "SKU1", true)]);
        p.fulfillment_type = FulfillmentType::Auto;
        let mut list = vec![p];
        apply_auto_stock_counts(
            &mut list,
            &[
                count(1, 1, "available", 3),
                count(1, 1, "reserved", 1),
                count(1, 0, "available", 2),
                count(1, 1, "used", 4),
            ],
        );
        let s = &list[0].skus[0];
        assert_eq!(
            (
                s.auto_stock_available,
                s.auto_stock_total,
                s.auto_stock_locked,
                s.auto_stock_sold
            ),
            (5, 6, 1, 4)
        );
        assert_eq!(list[0].auto_stock_total, 6);
    }

    // ORD-08
    #[test]
    fn masks_stock_outside_exact_mode() {
        let mut s = sku(1, "A", true);
        s.auto_stock_available = 37;
        let mut p = product(vec![s]);
        p.fulfillment_type = FulfillmentType::Auto;
        p.stock_display_mode = StockDisplayMode::Hidden;
        let v = decorate_public_product(p.clone(), &Decoration::default());
        assert_eq!(
            (v.auto_stock_available, v.stock_display.as_str()),
            (1, "hidden")
        );
        assert_eq!(v.skus[0].auto_stock_available, 1);
        p.stock_display_mode = StockDisplayMode::Range;
        let v = decorate_public_product(p.clone(), &Decoration::default());
        assert_eq!(
            (
                v.stock_display.as_str(),
                v.stock_range_min,
                v.stock_range_max
            ),
            ("range_21_50", Some(21), Some(50))
        );
        p.stock_display_mode = StockDisplayMode::Exact;
        let v = decorate_public_product(p.clone(), &Decoration::default());
        assert_eq!(v.auto_stock_available, 37);
        p.skus[0].auto_stock_available = 0;
        let v = decorate_public_product(p, &Decoration::default());
        assert_eq!(v.stock_status, StockStatus::OutOfStock);
        assert!(v.is_sold_out);
    }

    // ORD-06
    #[test]
    fn manual_stock_sums_active_skus_and_unlimited_wins() {
        let mut a = sku(1, "A", true);
        a.manual_stock_total = 3;
        let mut b = sku(2, "B", true);
        b.manual_stock_total = 4;
        let v =
            decorate_public_product(product(vec![a.clone(), b.clone()]), &Decoration::default());
        assert_eq!(
            (v.manual_stock_available, v.stock_status),
            (7, StockStatus::InStock)
        );
        b.manual_stock_total = -1;
        let v = decorate_public_product(product(vec![a, b]), &Decoration::default());
        assert_eq!(
            (v.manual_stock_available, v.stock_status),
            (-1, StockStatus::Unlimited)
        );
        let mut single = product(vec![]);
        single.manual_stock_total = 0;
        let v = decorate_public_product(single, &Decoration::default());
        assert!(v.is_sold_out);
    }

    // PRC-13 / PRC-14
    #[test]
    fn display_price_and_promotion_from_first_active_sku() {
        let mut a = sku(1, "A", true);
        a.price_amount = amt("89.90");
        let mut b = sku(2, "B", true);
        b.price_amount = amt("49.90");
        let mut p = product(vec![a, b]);
        p.price_amount = amt("59.90");
        let promotions = [promo(5, "fixed", 10, 0)];
        let deco = Decoration {
            promotions: &promotions,
            ..Decoration::default()
        };
        let v = decorate_public_product(p, &deco);
        assert_eq!(v.price_amount, amt("89.90"));
        assert_eq!(v.promotion_price_amount, Some(amt("79.90")));
        assert_eq!(v.promotion_id, Some(5));
        assert_eq!(v.skus[1].promotion_price_amount, Some(amt("39.90")));
        assert_eq!(v.promotion_rules.len(), 1);

        // SKU1 (display) has no discount, SKU2 has: product-level promotion stays empty
        let mut s1 = sku(1, "A", true);
        s1.price_amount = amt("10");
        let mut s2 = sku(2, "B", true);
        s2.price_amount = amt("100");
        let special = [promo(6, "special_price", 50, 0)];
        let deco = Decoration {
            promotions: &special,
            ..Decoration::default()
        };
        let v = decorate_public_product(product(vec![s1, s2]), &deco);
        assert_eq!(v.promotion_price_amount, None);
        assert_eq!(v.skus[1].promotion_price_amount, Some(amt("50")));
    }

    // PRC-07 ③ / MISC-02
    #[test]
    fn member_prices_only_for_active_levels_and_no_secrets() {
        let vip = level(1, 10, 90);
        let mut off = level(2, 20, 80);
        off.is_active = false;
        let prices = [price(1, 1, 0, "8"), price(2, 1, 0, "7")];
        let active = [vip.clone()];
        let deco = Decoration {
            level_prices: &prices,
            active_levels: &active,
            viewer_level: Some(&vip),
            ..Decoration::default()
        };
        let v = decorate_public_product(product(vec![sku(1, "A", true)]), &deco);
        assert_eq!(v.member_prices.len(), 1);
        assert_eq!(v.skus[0].member_price_amount, Some(amt("8")));
        let json = serde_json::to_value(&v).unwrap();
        assert!(json.get("cost_price_amount").is_none());
        assert!(json["skus"][0].get("cost_price_amount").is_none());
        assert!(json.get("instructions").is_none());
    }

    #[test]
    fn upstream_stock_view() {
        let mut p = product(vec![sku(1, "A", true), sku(2, "B", true)]);
        p.fulfillment_type = FulfillmentType::Upstream;
        let v = decorate_public_product(p.clone(), &Decoration::default());
        assert_eq!(
            (v.fulfillment_type, v.stock_status),
            (FulfillmentType::Manual, StockStatus::InStock)
        );
        let mapping = UpstreamMapping {
            local_product_id: 1,
            upstream_fulfillment_type: "manual".into(),
            skus: vec![
                UpstreamSkuMapping {
                    local_sku_id: 1,
                    upstream_stock: 0,
                    upstream_is_active: true,
                },
                UpstreamSkuMapping {
                    local_sku_id: 2,
                    upstream_stock: 0,
                    upstream_is_active: true,
                },
            ],
        };
        let deco = Decoration {
            upstream: Some(&mapping),
            ..Decoration::default()
        };
        let v = decorate_public_product(p.clone(), &deco);
        assert!(v.is_sold_out);
        let inactive = UpstreamMapping {
            skus: vec![UpstreamSkuMapping {
                local_sku_id: 1,
                upstream_stock: 9,
                upstream_is_active: false,
            }],
            ..mapping.clone()
        };
        let deco = Decoration {
            upstream: Some(&inactive),
            ..Decoration::default()
        };
        assert!(decorate_public_product(p.clone(), &deco).is_sold_out);
        let unlimited = UpstreamMapping {
            upstream_fulfillment_type: "auto".into(),
            skus: vec![UpstreamSkuMapping {
                local_sku_id: 1,
                upstream_stock: -1,
                upstream_is_active: true,
            }],
            ..mapping
        };
        let deco = Decoration {
            upstream: Some(&unlimited),
            ..Decoration::default()
        };
        let v = decorate_public_product(p, &deco);
        assert_eq!(
            (v.fulfillment_type, v.stock_status, v.auto_stock_available),
            (FulfillmentType::Auto, StockStatus::Unlimited, -1)
        );
    }
}
