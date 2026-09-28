//! The upstream API we serve to downstream shops (`/api/v1/upstream/*`): catalog
//! projection rules and ports.
//!
//! [`UpstreamOrdering`] is **implemented by the order group**; until then the
//! wiring uses a stub that answers `internal_error`.

use std::collections::HashMap;

use async_trait::async_trait;
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::Page;

use super::mapping::effective_fulfillment_type;
use super::protocol::{ProductQuery, RemoteFulfillment, RemoteProduct, RemoteSku, RemoteTier};
use crate::catalog::category::Category;
use crate::catalog::product::{FulfillmentType, JsonMap, Product, ProductSku, UpstreamMapping};
use crate::catalog::stock::StockPolicy;
use crate::marketing::member_level::{MemberLevel, MemberLevelPrice, resolve_member_price};
use crate::{Id, Result};

/// Largest `page_size` of `/upstream/products` (original bound 50, UPS-17).
pub const PRODUCTS_MAX_PAGE_SIZE: i64 = 50;
/// Upstream API rate limit: requests per window per `IP|API key` (original 60 / 60 s, block 30 s).
pub const RATE_LIMIT_MAX: i64 = 60;
pub const RATE_LIMIT_WINDOW_SECS: i64 = 60;
pub const RATE_LIMIT_BLOCK_SECS: i64 = 30;
/// `last_used_at` is written at most once per minute (original throttle).
pub const TOUCH_THROTTLE_SECS: i64 = 60;
/// Largest accepted request body (original `MaxBytesReader` 10 MB).
pub const MAX_BODY_BYTES: usize = 10 << 20;
/// Largest accepted supplier callback body (UPS-02).
pub const MAX_CALLBACK_BODY_BYTES: usize = 1 << 20;
/// Protocol version reported by `/upstream/ping`.
pub const PROTOCOL_VERSION: &str = "1.0";

/// The API buyer (credential owner).
#[derive(Debug, Clone, PartialEq)]
pub struct Buyer {
    pub user_id: Id,
    pub balance: Amount,
    pub member_level: Option<MemberLevel>,
}

/// Site identity reported by ping.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SiteInfo {
    pub site_name: String,
    pub currency: String,
    /// `site_config.brand.site_url` (may be empty).
    pub site_url: String,
}

/// Reads needed to serve the upstream API (consumer-defined; implemented on the
/// catalog / identity / wallet / marketing tables).
#[async_trait]
pub trait SupplierCatalog: Send + Sync {
    async fn buyer(&self, user_id: Id) -> Result<Option<Buyer>>;
    async fn site_info(&self) -> Result<SiteInfo>;
    async fn categories(&self) -> Result<Vec<Category>>;
    /// Products (active + active category unless `include_inactive`), SKUs attached,
    /// auto stock applied; `updated_after` filters on `products.updated_at`.
    async fn products(&self, query: &ProductQuery) -> Result<Page<Product>>;
    /// Any live product (inactive included, UPS-14) with SKUs and auto stock.
    async fn product(&self, id: Id) -> Result<Option<Product>>;
    /// Live products (inactive included) with `id > after_id` by id, SKUs and auto
    /// stock attached, plus the total count of live products (cursor listing).
    async fn products_after(&self, after_id: Id, limit: u64) -> Result<(Vec<Product>, u64)>;
    /// Product (any state) owning a live SKU, with the SKU's active flag.
    async fn product_of_sku(&self, sku_id: Id) -> Result<Option<(Product, bool)>>;
    async fn upstream_mappings(&self, product_ids: &[Id]) -> Result<Vec<UpstreamMapping>>;
    async fn member_prices(
        &self,
        level_id: Id,
        product_ids: &[Id],
    ) -> Result<Vec<MemberLevelPrice>>;
}

/// Member pricing context of a buyer.
#[derive(Debug, Clone, Copy)]
pub struct MemberPricing<'a> {
    pub level: Option<&'a MemberLevel>,
    pub prices: &'a [MemberLevelPrice],
}

impl MemberPricing<'_> {
    /// `(price_amount, original_price, member_price)` texts: when the member price is
    /// lower, `price_amount` is the member price and the original is reported.
    fn apply(&self, product_id: Id, sku_id: Id, base: Amount) -> (String, String, String) {
        let member = resolve_member_price(self.level, self.prices, product_id, sku_id, base).price;
        if member < base {
            (member.to_string(), base.to_string(), member.to_string())
        } else {
            (base.to_string(), String::new(), String::new())
        }
    }
}

fn sku_stock(product: &Product, sku: &ProductSku, upstream: &HashMap<Id, i32>) -> i64 {
    match product.fulfillment_type {
        FulfillmentType::Manual => i64::from(sku.manual_stock_total),
        FulfillmentType::Upstream => i64::from(upstream.get(&sku.id).copied().unwrap_or(0)),
        FulfillmentType::Auto => sku.auto_stock_available,
    }
}

/// Projects a local product into the supplier wire format (original
/// `toUpstreamProductWithMemberPrice`): active SKUs only, real stock quantity with the
/// upstream low-stock threshold, member prices, and the *effective* fulfillment type of
/// mapped products so multi-level chains see `auto` / `manual` (UPS-19).
pub fn to_remote_product(
    product: &Product,
    mapping: Option<&UpstreamMapping>,
    member: MemberPricing<'_>,
) -> RemoteProduct {
    let upstream_stock: HashMap<Id, i32> = mapping
        .map(|m| {
            m.skus
                .iter()
                .map(|s| {
                    let stock = if s.upstream_is_active {
                        s.upstream_stock
                    } else {
                        0
                    };
                    (s.local_sku_id, stock)
                })
                .collect()
        })
        .unwrap_or_default();
    let skus = product
        .skus
        .iter()
        .filter(|s| s.is_active)
        .map(|s| {
            let quantity = sku_stock(product, s, &upstream_stock);
            let (price, original, member_price) = member.apply(product.id, s.id, s.price_amount);
            RemoteSku {
                id: s.id,
                sku_code: s.sku_code.clone(),
                spec_values: s.spec_values.clone(),
                price_amount: price,
                original_price: original,
                member_price,
                stock_status: StockPolicy::UPSTREAM.status(quantity).as_str().to_owned(),
                stock_quantity: quantity,
                is_active: s.is_active,
            }
        })
        .collect();
    let fulfillment_type = match (product.is_mapped, product.fulfillment_type, mapping) {
        (_, FulfillmentType::Upstream, Some(m)) => {
            effective_fulfillment_type(&m.upstream_fulfillment_type).to_owned()
        }
        // Never expose `upstream` (UPS-19): unmapped upstream products count as manual.
        (_, FulfillmentType::Upstream, None) => "manual".to_owned(),
        (_, t, _) => t.as_str().to_owned(),
    };
    let (price, original, member_price) = member.apply(product.id, 0, product.price_amount);
    RemoteProduct {
        id: product.id,
        slug: product.slug.clone(),
        seo_meta: product.seo_meta.clone(),
        title: product.title.clone(),
        description: product.description.clone(),
        content: product.content.clone(),
        images: product.images.clone(),
        tags: product.tags.clone(),
        price_amount: price,
        original_price: original,
        member_price,
        wholesale_prices: product
            .wholesale_prices
            .iter()
            .map(|t| RemoteTier {
                sku_id: t.sku_id,
                sku_code: t.sku_code.clone(),
                min_quantity: i64::from(t.min_quantity),
                unit_price: t.unit_price.to_string(),
            })
            .collect(),
        currency: String::new(),
        fulfillment_type,
        manual_form_schema: Some(product.manual_form_schema.clone()),
        is_active: product.is_active,
        category_id: product.category_id,
        skus,
        created_at: Some(product.created_at),
        updated_at: Some(product.updated_at),
    }
}

// ---------------------------------------------------------------------------
// Ordering port (order group)
// ---------------------------------------------------------------------------

/// An order placed by a downstream shop through `POST /upstream/orders`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaceUpstreamOrder {
    pub user_id: Id,
    pub credential_id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    /// Manual form answers (only for manual products).
    pub manual_form_data: Option<JsonMap>,
    /// Buyer's own order number (may be empty).
    pub downstream_order_no: String,
    pub trace_id: String,
    /// Already validated (http/https, no internal literal addresses).
    pub callback_url: String,
    pub client_ip: String,
}

/// Summary returned by create / cancel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamOrderSummary {
    pub order_id: Id,
    pub order_no: String,
    pub status: String,
    pub amount: Amount,
    pub currency: String,
}

/// Result of placing an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaceOutcome {
    /// Created and paid from the buyer's wallet.
    Placed(UpstreamOrderSummary),
    /// Created, wallet payment failed, order already canceled (stock released).
    PaymentFailed {
        order_id: Id,
        order_no: String,
        message: String,
    },
}

/// Business failures mapped to the protocol's `error_code`s.
#[derive(Debug)]
pub enum UpstreamOrderError {
    /// HTTP 402 `insufficient_balance`.
    InsufficientBalance,
    /// HTTP 409 `insufficient_stock`.
    InsufficientStock,
    ProductUnavailable,
    SkuUnavailable,
    InvalidItem,
    ManualFormInvalid(String),
    /// The `(credential, downstream_order_no)` pair already has an order (unique index
    /// race): the caller answers with the existing order.
    DuplicateDownstreamNo,
    NotFound,
    CancelNotAllowed,
    Internal(crate::Error),
}

impl From<crate::Error> for UpstreamOrderError {
    fn from(e: crate::Error) -> Self {
        Self::Internal(e)
    }
}

/// An order item as reported to the buyer.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UpstreamOrderItem {
    pub product_id: Id,
    pub sku_id: Id,
    pub title: JsonMap,
    pub quantity: i32,
    pub original_unit_price: Amount,
    pub unit_price: Amount,
    pub original_total_price: Amount,
    pub total_price: Amount,
    /// Effective type (`auto` / `manual`), never `upstream` (UPS-19).
    pub fulfillment_type: String,
}

/// Order detail as reported to the buyer.
#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamOrderView {
    pub order_id: Id,
    pub order_no: String,
    pub status: String,
    pub amount: Amount,
    pub refunded_amount: Amount,
    pub currency: String,
    /// Local refund records of the order.
    pub refund_records: Vec<JsonMap>,
    /// Delivered fulfillment of the order or of its first delivered child (UPS-12).
    pub fulfillment: Option<RemoteFulfillment>,
    pub items: Vec<UpstreamOrderItem>,
}

/// A line to price for a quote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PriceLine {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
}

/// Price of a line for the buyer (member / wholesale / promotion applied).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinePrice {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub unit_price: Amount,
    pub total_price: Amount,
}

/// A line of a multi-item API order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaceLine {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub manual_form_data: Option<JsonMap>,
    /// Locked unit price (quote): the buyer never pays more than this.
    pub price_cap: Option<Amount>,
}

/// A multi-item order placed by an API buyer (one parent + one child per line).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaceUpstreamLines {
    pub user_id: Id,
    pub credential_id: Id,
    pub lines: Vec<PlaceLine>,
    pub downstream_order_no: String,
    pub trace_id: String,
    pub client_ip: String,
}

/// One line of an API order with its own status and delivery.
#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamOrderLine {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub unit_price: Amount,
    pub total_price: Amount,
    pub status: String,
    /// Delivered fulfillment of the line (effective type, never `upstream`).
    pub fulfillment: Option<RemoteFulfillment>,
}

/// An API order with per-line status (zebra-store order object).
#[derive(Debug, Clone, PartialEq)]
pub struct UpstreamOrderDetail {
    pub order_id: Id,
    pub order_no: String,
    pub user_id: Id,
    pub status: String,
    pub total: Amount,
    pub currency: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub lines: Vec<UpstreamOrderLine>,
}

fn unsupported() -> crate::Error {
    crate::Error::internal_msg("operation not supported by this ordering port")
}

/// Ordering on behalf of API buyers (**implemented by the order group**).
#[async_trait]
pub trait UpstreamOrdering: Send + Sync {
    /// Prices lines for the buyer with the order group's pricing (preview, no stock
    /// reservation); fails like checkout for unavailable lines.
    async fn price_lines(
        &self,
        _user_id: Id,
        _lines: &[PriceLine],
    ) -> Result<Vec<LinePrice>, UpstreamOrderError> {
        Err(UpstreamOrderError::Internal(unsupported()))
    }

    /// Multi-item [`Self::place`]: one parent order with a child per line, paid from
    /// the buyer's wallet; locked prices cap the line prices.
    async fn place_lines(
        &self,
        _req: &PlaceUpstreamLines,
    ) -> Result<PlaceOutcome, UpstreamOrderError> {
        Err(UpstreamOrderError::Internal(unsupported()))
    }

    /// The buyer's root order with per-line status and deliveries.
    async fn detail(&self, _user_id: Id, _order_id: Id) -> Result<Option<UpstreamOrderDetail>> {
        Ok(None)
    }

    /// Creates the order (risk control skipped), inserts the `downstream_order_refs`
    /// row in the same transaction (see `zs_infra::db::repo::integration::downstream::
    /// insert_ref_in`; the unique `(api_credential_id, downstream_order_no)` index turns
    /// a concurrent duplicate into [`UpstreamOrderError::DuplicateDownstreamNo`],
    /// UPS-10), then pays it from the buyer's wallet; a failed payment cancels the
    /// order and yields [`PlaceOutcome::PaymentFailed`].
    async fn place(&self, req: &PlaceUpstreamOrder) -> Result<PlaceOutcome, UpstreamOrderError>;

    /// The buyer's order (`None` when missing or owned by someone else).
    async fn get(&self, user_id: Id, order_id: Id) -> Result<Option<UpstreamOrderView>>;

    /// Cancels an unpaid order of the buyer.
    async fn cancel(
        &self,
        user_id: Id,
        order_id: Id,
    ) -> Result<UpstreamOrderSummary, UpstreamOrderError>;

    /// Runs the automatic (card secret) fulfillment of a paid API order right away
    /// instead of waiting for the queued `order:auto_fulfill` job, for protocols whose
    /// order response is the final delivery (acg-faka / mcy `trade`). Idempotent: an
    /// already delivered or non-auto order is left as is (the queued job then skips).
    async fn deliver_now(&self, _order_id: Id) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::catalog::product::{PurchaseType, UpstreamSkuMapping};
    use crate::catalog::stock::StockDisplayMode;

    fn sku(id: Id, price: &str, active: bool) -> ProductSku {
        ProductSku {
            id,
            product_id: 1,
            sku_code: format!("S{id}"),
            spec_values: JsonMap::new(),
            price_amount: price.parse().unwrap(),
            cost_price_amount: Amount::ZERO,
            manual_stock_total: 5,
            manual_stock_locked: 4,
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

    fn product(kind: FulfillmentType, mapped: bool) -> Product {
        Product {
            id: 1,
            category_id: 2,
            slug: "p".into(),
            seo_meta: JsonMap::new(),
            title: JsonMap::new(),
            description: JsonMap::new(),
            content: JsonMap::new(),
            instructions: JsonMap::new(),
            price_amount: "10".parse().unwrap(),
            cost_price_amount: "1".parse().unwrap(),
            wholesale_prices: Vec::new(),
            images: Vec::new(),
            tags: Vec::new(),
            purchase_type: PurchaseType::Member,
            min_purchase_quantity: 0,
            max_purchase_quantity: 0,
            stock_display_mode: StockDisplayMode::Exact,
            fulfillment_type: kind,
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
            is_mapped: mapped,
            is_active: true,
            sort_order: 0,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
            category: None,
            skus: vec![sku(11, "10", true), sku(12, "20", false)],
        }
    }

    const NO_MEMBER: MemberPricing<'static> = MemberPricing {
        level: None,
        prices: &[],
    };

    // Manual stock is the remaining stock; inactive SKUs are hidden; no cost price.
    #[test]
    fn manual_product_projection() {
        let r = to_remote_product(&product(FulfillmentType::Manual, false), None, NO_MEMBER);
        assert_eq!(r.skus.len(), 1);
        assert_eq!(r.skus[0].stock_quantity, 5);
        assert_eq!(r.skus[0].stock_status, "low_stock");
        assert_eq!(r.skus[0].price_amount, "10.00");
        assert_eq!(r.fulfillment_type, "manual");
        let json = serde_json::to_value(&r).unwrap();
        assert!(json.get("cost_price_amount").is_none());
    }

    // UPS-19 (1): a mapped product exposes the supplier's type and synced stock.
    #[test]
    fn ups19_mapped_product_uses_effective_type_and_upstream_stock() {
        let mapping = UpstreamMapping {
            local_product_id: 1,
            upstream_fulfillment_type: "auto".into(),
            skus: vec![UpstreamSkuMapping {
                local_sku_id: 11,
                upstream_stock: 42,
                upstream_is_active: true,
            }],
        };
        let r = to_remote_product(
            &product(FulfillmentType::Upstream, true),
            Some(&mapping),
            NO_MEMBER,
        );
        assert_eq!(r.fulfillment_type, "auto");
        assert_eq!(r.skus[0].stock_quantity, 42);
        assert_eq!(r.skus[0].stock_status, "in_stock");
        // unmapped SKU of an upstream product → out of stock
        let r = to_remote_product(&product(FulfillmentType::Upstream, true), None, NO_MEMBER);
        assert_eq!(r.fulfillment_type, "manual");
        assert_eq!(r.skus[0].stock_quantity, 0);
    }
}
