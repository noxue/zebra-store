//! [`SupplyDesk`]: the protocol-neutral provider core. Every supplier protocol we
//! serve (legacy `/upstream`, zebra-store `/zs`, and the compat facades for
//! acg-faka / mcy OpenApi) answers the same questions about an authenticated API
//! buyer ([`Caller`]): who is the site, what is the buyer's balance, which products
//! can the buyer purchase and at which price (member / wholesale / promotion through
//! the storefront pricing engine of the order group), and "place this order, paid from
//! the buyer's wallet, idempotent on the buyer's request number".
//!
//! The desk speaks domain types only; wire formats belong to the facades.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value;
use zs_domain::catalog::category::Category;
use zs_domain::catalog::product::{FulfillmentType, Product, validate_purchase_quantity};
use zs_domain::integration::downstream::OrderRefRepo;
use zs_domain::integration::protocol::{ProductQuery, RemoteProduct};
use zs_domain::integration::provide::{UNLIMITED_STOCK, valid_request_no};
use zs_domain::integration::supplier::{
    Buyer, MemberPricing, PlaceOutcome, PlaceUpstreamOrder, PriceLine, SiteInfo, SupplierCatalog,
    UpstreamOrderError, UpstreamOrderView, UpstreamOrdering, to_remote_product,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::integration::credential::Caller;

/// Default currency when the site has none configured.
pub const DEFAULT_CURRENCY: &str = "CNY";
/// Page size used to walk the whole catalog for full-tree protocols.
const CATALOG_PAGE: i64 = 100;
/// Locale used for texts of protocols without localization (acg-faka, mcy).
pub const TEXT_LOCALE: &str = "zh-CN";

/// Which products a protocol may offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryPolicy {
    /// Every sellable product (protocols with order lookup / callbacks).
    Any,
    /// Only products delivered inside the order request (local card secrets): for
    /// protocols whose order response is the buyer's final delivery (acg-faka and mcy
    /// `trade` responses are never followed up by the caller).
    Synchronous,
}

impl DeliveryPolicy {
    /// True when `product` may be offered under this policy.
    pub fn offers(self, product: &Product) -> bool {
        match self {
            Self::Any => true,
            Self::Synchronous => product.fulfillment_type == FulfillmentType::Auto,
        }
    }
}

/// A SKU as offered to one buyer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfferSku {
    pub sku_id: Id,
    /// Display name (spec values joined, else the SKU code).
    pub name: String,
    /// Retail unit price (list price of the SKU).
    pub retail: Amount,
    /// The buyer's unit price (storefront pricing engine at the minimum quantity).
    pub price: Amount,
    /// Sellable stock ([`UNLIMITED_STOCK`] when unlimited).
    pub stock: i64,
}

/// A product as offered to one buyer.
#[derive(Debug, Clone)]
pub struct Offer {
    pub product: Product,
    /// Projection shared by the legacy and Zebra Store protocols.
    pub remote: RemoteProduct,
    /// Active SKUs (at least one).
    pub skus: Vec<OfferSku>,
}

impl Offer {
    pub fn title(&self) -> String {
        localized(&self.product.title)
    }

    /// Rich content when present, else the short description.
    pub fn description(&self) -> String {
        let content = localized(&self.product.content);
        if content.trim().is_empty() {
            localized(&self.product.description)
        } else {
            content
        }
    }

    /// First image as an absolute URL (`base` = site origin), empty when none.
    pub fn cover(&self, base: &str) -> String {
        self.product
            .images
            .iter()
            .find(|i| !i.trim().is_empty())
            .map(|i| absolute_url(base, i))
            .unwrap_or_default()
    }

    /// Sum of the SKU stocks (saturating at [`UNLIMITED_STOCK`]).
    pub fn stock(&self) -> i64 {
        self.skus
            .iter()
            .map(|s| s.stock)
            .sum::<i64>()
            .min(UNLIMITED_STOCK)
    }

    /// The single SKU of a product without variants (one active SKU without spec
    /// values), which protocols may address without naming it.
    pub fn default_sku(&self) -> Option<&OfferSku> {
        match self.skus.as_slice() {
            [only] => self
                .product
                .skus
                .iter()
                .find(|s| s.id == only.sku_id)
                .filter(|s| s.spec_values.is_empty())
                .map(|_| only),
            _ => None,
        }
    }

    pub fn sku(&self, sku_id: Id) -> Option<&OfferSku> {
        self.skus.iter().find(|s| s.sku_id == sku_id)
    }
}

/// Offers of one category.
#[derive(Debug, Clone)]
pub struct OfferCategory {
    pub category: Category,
    pub offers: Vec<Offer>,
}

/// Price of a purchase for the buyer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Quote {
    pub unit_price: Amount,
    pub total: Amount,
}

/// A buyer's purchase request.
#[derive(Debug, Clone, Default)]
pub struct Purchase {
    pub sku_id: Id,
    pub quantity: i32,
    /// The buyer's own order number (idempotency key, 1–64 visible characters).
    pub request_no: String,
    pub client_ip: String,
}

/// An order of the buyer as seen by the facades.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeskOrder {
    pub order_id: Id,
    pub order_no: String,
    pub status: String,
    /// Paid (not pending payment, not canceled).
    pub paid: bool,
    pub amount: Amount,
    pub currency: String,
    /// Delivered content (card secrets joined by `\n`), `None` while undelivered.
    pub contents: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    /// Answered from an existing order of the same request number.
    pub replayed: bool,
    /// SKU stock after the purchase (placement only).
    pub stock_left: Option<i64>,
}

/// Why a desk operation was refused (facades map these to their wire texts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeskError {
    /// Unknown product / SKU.
    NotFound,
    /// Delisted, category disabled, SKU disabled, or not offered by the policy.
    Unavailable,
    /// Outside the product's purchase quantity limits.
    QuantityLimit,
    OutOfStock,
    InsufficientBalance,
    /// Malformed request (message for the caller).
    Invalid(String),
    /// Priced at zero or less: refused so a misconfigured price cannot give goods
    /// away (acg-faka `Order.php:875-889`, PRV-05).
    PriceInvalid,
    OrderNotFound,
    /// The order exists but failed (payment failed / canceled).
    OrderFailed(String),
    Internal,
}

fn internal(error: &zs_domain::Error) -> DeskError {
    tracing::error!(%error, "supply desk failed");
    DeskError::Internal
}

fn order_error(e: UpstreamOrderError) -> DeskError {
    match e {
        UpstreamOrderError::InsufficientBalance => DeskError::InsufficientBalance,
        UpstreamOrderError::InsufficientStock => DeskError::OutOfStock,
        UpstreamOrderError::ProductUnavailable | UpstreamOrderError::SkuUnavailable => {
            DeskError::Unavailable
        }
        UpstreamOrderError::InvalidItem => DeskError::QuantityLimit,
        UpstreamOrderError::ManualFormInvalid(m) => DeskError::Invalid(m),
        UpstreamOrderError::NotFound => DeskError::OrderNotFound,
        UpstreamOrderError::CancelNotAllowed | UpstreamOrderError::DuplicateDownstreamNo => {
            DeskError::OrderFailed("order state conflict".into())
        }
        UpstreamOrderError::Internal(error) => internal(&error),
    }
}

/// Text of a localized JSON map: zh-CN, then en-US / zh-TW, then any value.
pub fn localized(map: &serde_json::Map<String, Value>) -> String {
    for locale in [TEXT_LOCALE, "en-US", "zh-TW"] {
        if let Some(s) = map.get(locale).and_then(Value::as_str)
            && !s.trim().is_empty()
        {
            return s.to_owned();
        }
    }
    map.values()
        .filter_map(Value::as_str)
        .find(|s| !s.trim().is_empty())
        .unwrap_or_default()
        .to_owned()
}

/// `base` + a site-relative path; absolute URLs are kept.
pub fn absolute_url(base: &str, path: &str) -> String {
    let path = path.trim();
    if path.starts_with("http://") || path.starts_with("https://") || base.is_empty() {
        return path.to_owned();
    }
    let base = base.trim_end_matches('/');
    if path.starts_with('/') {
        format!("{base}{path}")
    } else {
        format!("{base}/{path}")
    }
}

/// Locales of a `LocalizedText` object (`{"zh-CN","zh-TW","en-US"}`).
const LOCALES: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];

/// Display name of a SKU: its spec values (`a / b`), else its code.
///
/// `spec_values` is either one localized value (`{"zh-CN":…,"en-US":…}`, what the
/// admin form and the seed data store) or a `{spec name: value}` map; like the
/// storefront (`utils/sku.ts` `isLocalizedObject`), an object whose keys are all
/// locale codes is ONE value, not three (PRV-08: acg-faka race names used to read
/// `空月祝福 / 空月祝福 / Welkin`).
fn sku_name(spec_values: &serde_json::Map<String, Value>, code: &str) -> String {
    if !spec_values.is_empty() && spec_values.keys().all(|k| LOCALES.contains(&k.as_str())) {
        let name = localized(spec_values);
        let name = name.trim();
        return if name.is_empty() {
            code.trim().to_owned()
        } else {
            name.to_owned()
        };
    }
    let parts: Vec<String> = spec_values
        .values()
        .filter_map(|v| match v {
            Value::String(s) => Some(s.trim().to_owned()),
            Value::Object(m) => Some(localized(m)),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        code.trim().to_owned()
    } else {
        parts.join(" / ")
    }
}

fn paid_status(status: &str) -> bool {
    !matches!(status.trim(), "pending_payment" | "canceled" | "")
}

/// The provider core (cheap to clone).
#[derive(Clone)]
pub struct SupplyDesk {
    catalog: Arc<dyn SupplierCatalog>,
    ordering: Arc<dyn UpstreamOrdering>,
    refs: Arc<dyn OrderRefRepo>,
}

impl std::fmt::Debug for SupplyDesk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SupplyDesk")
    }
}

impl SupplyDesk {
    pub fn new(
        catalog: Arc<dyn SupplierCatalog>,
        ordering: Arc<dyn UpstreamOrdering>,
        refs: Arc<dyn OrderRefRepo>,
    ) -> Self {
        Self {
            catalog,
            ordering,
            refs,
        }
    }

    // ------------------------------------------------------------------ site / buyer

    /// Site identity with the default currency applied.
    pub async fn site(&self) -> Result<SiteInfo> {
        let mut site = self.catalog.site_info().await?;
        if site.currency.trim().is_empty() {
            DEFAULT_CURRENCY.clone_into(&mut site.currency);
        }
        Ok(site)
    }

    /// Site currency, [`DEFAULT_CURRENCY`] when unknown or unreadable.
    pub async fn currency(&self) -> String {
        match self.catalog.site_info().await {
            Ok(s) if !s.currency.trim().is_empty() => s.currency,
            _ => DEFAULT_CURRENCY.to_owned(),
        }
    }

    /// Public base URL: the brand URL, else the request's own origin.
    pub async fn base_url(&self, origin: &str) -> String {
        let brand = self.site().await.map(|s| s.site_url).unwrap_or_default();
        let url = if brand.trim().is_empty() {
            origin
        } else {
            brand.as_str()
        };
        url.trim().trim_end_matches('/').to_owned()
    }

    /// The buyer (balance, member level); `None` when the user vanished.
    pub async fn buyer(&self, caller: &Caller) -> Option<Buyer> {
        self.catalog.buyer(caller.user_id).await.ok().flatten()
    }

    /// Wallet balance of the buyer.
    pub async fn balance(&self, caller: &Caller) -> Result<Amount> {
        Ok(self
            .catalog
            .buyer(caller.user_id)
            .await?
            .map_or(Amount::ZERO, |b| b.balance))
    }

    // ------------------------------------------------------------------ projection

    /// Projects products for the caller (member prices, effective types, real stock).
    pub async fn project(
        &self,
        caller: &Caller,
        products: &[Product],
    ) -> Result<Vec<RemoteProduct>> {
        let ids: Vec<Id> = products.iter().map(|p| p.id).collect();
        let mapped: Vec<Id> = products
            .iter()
            .filter(|p| p.fulfillment_type == FulfillmentType::Upstream)
            .map(|p| p.id)
            .collect();
        let mappings: HashMap<Id, _> = if mapped.is_empty() {
            HashMap::new()
        } else {
            self.catalog
                .upstream_mappings(&mapped)
                .await
                .unwrap_or_else(|error| {
                    tracing::warn!(%error, "resolve upstream mappings failed");
                    Vec::new()
                })
                .into_iter()
                .map(|m| (m.local_product_id, m))
                .collect()
        };
        let buyer = self.buyer(caller).await;
        let level = buyer.as_ref().and_then(|b| b.member_level.as_ref());
        let prices = match level {
            Some(l) if l.is_active && !ids.is_empty() => self
                .catalog
                .member_prices(l.id, &ids)
                .await
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let member = MemberPricing {
            level,
            prices: &prices,
        };
        Ok(products
            .iter()
            .map(|p| to_remote_product(p, mappings.get(&p.id), member))
            .collect())
    }

    // ------------------------------------------------------------------ offers

    /// The buyer's unit price of one SKU at the product's minimum quantity; the
    /// projected (member) price when the pricing engine refuses the line (e.g. sold
    /// out), so listings still show a price.
    async fn unit_price(
        &self,
        caller: &Caller,
        product: &Product,
        sku_id: Id,
        fallback: Amount,
    ) -> Amount {
        let quantity = product.min_purchase_quantity.max(1);
        match self
            .ordering
            .price_lines(
                caller.user_id,
                &[PriceLine {
                    product_id: product.id,
                    sku_id,
                    quantity,
                }],
            )
            .await
        {
            Ok(lines) => lines.first().map_or(fallback, |l| l.unit_price),
            Err(UpstreamOrderError::Internal(error)) => {
                tracing::warn!(%error, sku_id, "offer pricing failed");
                fallback
            }
            Err(_) => fallback,
        }
    }

    async fn offer_of(
        &self,
        caller: &Caller,
        product: Product,
        remote: RemoteProduct,
    ) -> Option<Offer> {
        let mut skus = Vec::with_capacity(remote.skus.len());
        for rs in &remote.skus {
            let Some(sku) = product.skus.iter().find(|s| s.id == rs.id) else {
                continue;
            };
            let projected: Amount = rs.price_amount.parse().unwrap_or(sku.price_amount);
            let price = self.unit_price(caller, &product, sku.id, projected).await;
            skus.push(OfferSku {
                sku_id: sku.id,
                name: sku_name(&sku.spec_values, &sku.sku_code),
                retail: sku.price_amount,
                price,
                stock: if rs.stock_quantity < 0 {
                    UNLIMITED_STOCK
                } else {
                    rs.stock_quantity.min(UNLIMITED_STOCK)
                },
            });
        }
        (!skus.is_empty()).then_some(Offer {
            product,
            remote,
            skus,
        })
    }

    async fn active_categories(&self) -> Result<HashMap<Id, Category>> {
        Ok(self
            .catalog
            .categories()
            .await?
            .into_iter()
            .filter(|c| c.is_active)
            .map(|c| (c.id, c))
            .collect())
    }

    /// Every product the buyer can purchase under `policy`, grouped by category
    /// (categories in catalog order, empty ones skipped).
    pub async fn offers(
        &self,
        caller: &Caller,
        policy: DeliveryPolicy,
    ) -> Result<Vec<OfferCategory>> {
        let categories = self.catalog.categories().await?;
        let active: HashMap<Id, &Category> = categories
            .iter()
            .filter(|c| c.is_active)
            .map(|c| (c.id, c))
            .collect();
        let mut by_category: HashMap<Id, Vec<Offer>> = HashMap::new();
        let mut page = 1;
        let mut seen: u64 = 0;
        loop {
            let result = self
                .catalog
                .products(&ProductQuery {
                    page,
                    page_size: CATALOG_PAGE,
                    updated_after: None,
                    include_inactive: false,
                    cursor: None,
                })
                .await?;
            if result.items.is_empty() {
                break;
            }
            seen += result.items.len() as u64;
            let wanted: Vec<Product> = result
                .items
                .into_iter()
                .filter(|p| p.is_active && policy.offers(p) && active.contains_key(&p.category_id))
                .collect();
            let projected = self.project(caller, &wanted).await?;
            for (mut product, remote) in wanted.into_iter().zip(projected) {
                let category_id = product.category_id;
                if product.category.is_none() {
                    product.category = active.get(&category_id).map(|c| (*c).clone());
                }
                if let Some(offer) = self.offer_of(caller, product, remote).await {
                    by_category.entry(category_id).or_default().push(offer);
                }
            }
            if seen >= result.total {
                break;
            }
            page += 1;
        }
        Ok(categories
            .into_iter()
            .filter_map(|c| {
                let offers = by_category.remove(&c.id)?;
                Some(OfferCategory {
                    category: c,
                    offers,
                })
            })
            .collect())
    }

    async fn offer_checked(
        &self,
        caller: &Caller,
        product: Product,
        policy: DeliveryPolicy,
    ) -> std::result::Result<Offer, DeskError> {
        if !product.is_active || !policy.offers(&product) {
            return Err(DeskError::Unavailable);
        }
        let mut categories = self.active_categories().await.map_err(|e| internal(&e))?;
        let Some(category) = categories.remove(&product.category_id) else {
            return Err(DeskError::Unavailable);
        };
        let mut product = product;
        product.category.get_or_insert(category);
        let remote = self
            .project(caller, std::slice::from_ref(&product))
            .await
            .map_err(|e| internal(&e))?
            .pop()
            .ok_or(DeskError::Internal)?;
        self.offer_of(caller, product, remote)
            .await
            .ok_or(DeskError::Unavailable)
    }

    /// One product offered to the buyer (delisted / not offered → refused).
    pub async fn offer(
        &self,
        caller: &Caller,
        product_id: Id,
        policy: DeliveryPolicy,
    ) -> std::result::Result<Offer, DeskError> {
        if product_id <= 0 {
            return Err(DeskError::NotFound);
        }
        let product = self
            .catalog
            .product(product_id)
            .await
            .map_err(|e| internal(&e))?
            .ok_or(DeskError::NotFound)?;
        self.offer_checked(caller, product, policy).await
    }

    /// The offer containing `sku_id` (the SKU must be active).
    pub async fn offer_of_sku(
        &self,
        caller: &Caller,
        sku_id: Id,
        policy: DeliveryPolicy,
    ) -> std::result::Result<Offer, DeskError> {
        if sku_id <= 0 {
            return Err(DeskError::NotFound);
        }
        let (product, sku_active) = self
            .catalog
            .product_of_sku(sku_id)
            .await
            .map_err(|e| internal(&e))?
            .ok_or(DeskError::NotFound)?;
        if !sku_active {
            return Err(DeskError::Unavailable);
        }
        let offer = self.offer_checked(caller, product, policy).await?;
        if offer.sku(sku_id).is_none() {
            return Err(DeskError::Unavailable);
        }
        Ok(offer)
    }

    // ------------------------------------------------------------------ pricing

    /// Availability of `quantity` units of `sku` in `offer` (quantity limits, stock).
    pub fn check(offer: &Offer, sku_id: Id, quantity: i32) -> std::result::Result<(), DeskError> {
        let sku = offer.sku(sku_id).ok_or(DeskError::Unavailable)?;
        if validate_purchase_quantity(
            offer.product.min_purchase_quantity,
            offer.product.max_purchase_quantity,
            quantity,
        )
        .is_err()
        {
            return Err(DeskError::QuantityLimit);
        }
        if sku.stock < UNLIMITED_STOCK && sku.stock < i64::from(quantity) {
            return Err(DeskError::OutOfStock);
        }
        Ok(())
    }

    /// Prices `quantity` units of a SKU for the buyer with the storefront pricing
    /// engine (member / wholesale / promotion); no stock is reserved, no price locked.
    pub async fn quote(
        &self,
        caller: &Caller,
        offer: &Offer,
        sku_id: Id,
        quantity: i32,
    ) -> std::result::Result<Quote, DeskError> {
        Self::check(offer, sku_id, quantity)?;
        if offer
            .sku(sku_id)
            .is_none_or(|s| !s.price.is_positive() || !s.retail.is_positive())
        {
            return Err(DeskError::PriceInvalid);
        }
        let lines = self
            .ordering
            .price_lines(
                caller.user_id,
                &[PriceLine {
                    product_id: offer.product.id,
                    sku_id,
                    quantity,
                }],
            )
            .await
            .map_err(order_error)?;
        let line = lines.first().ok_or(DeskError::Internal)?;
        if !line.total_price.is_positive() {
            return Err(DeskError::PriceInvalid);
        }
        Ok(Quote {
            unit_price: line.unit_price,
            total: line.total_price,
        })
    }

    // ------------------------------------------------------------------ orders

    fn order_of(view: &UpstreamOrderView, replayed: bool) -> DeskOrder {
        let contents = view
            .fulfillment
            .as_ref()
            .filter(|f| f.status == "delivered")
            .map(|f| f.payload.clone());
        DeskOrder {
            order_id: view.order_id,
            order_no: view.order_no.clone(),
            status: view.status.clone(),
            paid: paid_status(&view.status),
            amount: view.amount,
            currency: view.currency.clone(),
            contents,
            created_at: None,
            replayed,
            stock_left: None,
        }
    }

    /// The buyer's order by id (`None` when missing or someone else's).
    pub async fn order(
        &self,
        caller: &Caller,
        order_id: Id,
    ) -> std::result::Result<DeskOrder, DeskError> {
        let view = self
            .ordering
            .get(caller.user_id, order_id)
            .await
            .map_err(|e| internal(&e))?
            .ok_or(DeskError::OrderNotFound)?;
        Ok(Self::order_of(&view, false))
    }

    /// The buyer's order placed with `request_no` through this credential.
    pub async fn order_by_request(
        &self,
        caller: &Caller,
        request_no: &str,
    ) -> std::result::Result<Option<DeskOrder>, DeskError> {
        let Some(r) = self
            .refs
            .find_by_downstream_no(caller.credential_id, request_no)
            .await
            .map_err(|e| internal(&e))?
        else {
            return Ok(None);
        };
        let mut order = self.order(caller, r.order_id).await?;
        order.replayed = true;
        Ok(Some(order))
    }

    /// Delivers a paid, undelivered order inline, then re-reads it.
    async fn settle(
        &self,
        caller: &Caller,
        mut order: DeskOrder,
    ) -> std::result::Result<DeskOrder, DeskError> {
        if order.paid && order.contents.is_none() {
            if let Err(error) = self.ordering.deliver_now(order.order_id).await {
                tracing::warn!(%error, order_id = order.order_id, "inline delivery failed");
            }
            let replayed = order.replayed;
            order = self.order(caller, order.order_id).await?;
            order.replayed = replayed;
        }
        Ok(order)
    }

    /// The order already placed with `request_no` (delivered inline when still
    /// undelivered); an unpaid / canceled one is [`DeskError::OrderFailed`].
    pub async fn resume(
        &self,
        caller: &Caller,
        request_no: &str,
    ) -> std::result::Result<Option<DeskOrder>, DeskError> {
        match self.order_by_request(caller, request_no).await? {
            Some(o) if !o.paid => Err(DeskError::OrderFailed(format!(
                "order {} was not paid",
                o.order_no
            ))),
            Some(o) => Ok(Some(self.settle(caller, o).await?)),
            None => Ok(None),
        }
    }

    /// Places an order paid from the buyer's wallet and delivers it inline when the
    /// product delivers automatically. A request number already used by this
    /// credential answers the existing order (`replayed = true`) and never charges
    /// again (UPS-10); a failed earlier attempt stays failed.
    pub async fn place(
        &self,
        caller: &Caller,
        purchase: &Purchase,
        policy: DeliveryPolicy,
    ) -> std::result::Result<DeskOrder, DeskError> {
        let request_no = purchase.request_no.trim();
        if !valid_request_no(request_no) {
            return Err(DeskError::Invalid(
                "request number must be 1-64 visible characters".into(),
            ));
        }
        if let Some(existing) = self.resume(caller, request_no).await? {
            return Ok(existing);
        }
        let offer = self.offer_of_sku(caller, purchase.sku_id, policy).await?;
        let quote = self
            .quote(caller, &offer, purchase.sku_id, purchase.quantity)
            .await?;
        let balance = self.balance(caller).await.map_err(|e| internal(&e))?;
        if balance < quote.total {
            return Err(DeskError::InsufficientBalance);
        }
        let placed = self
            .ordering
            .place(&PlaceUpstreamOrder {
                user_id: caller.user_id,
                credential_id: caller.credential_id,
                product_id: offer.product.id,
                sku_id: purchase.sku_id,
                quantity: purchase.quantity,
                manual_form_data: None,
                downstream_order_no: request_no.to_owned(),
                trace_id: String::new(),
                callback_url: String::new(),
                client_ip: purchase.client_ip.clone(),
            })
            .await;
        let order_id = match placed {
            Ok(PlaceOutcome::Placed(s)) => s.order_id,
            Ok(PlaceOutcome::PaymentFailed { .. }) => return Err(DeskError::InsufficientBalance),
            Err(UpstreamOrderError::DuplicateDownstreamNo) => {
                // A concurrent request with the same number won the unique index.
                return self
                    .resume(caller, request_no)
                    .await?
                    .ok_or(DeskError::Internal);
            }
            Err(e) => return Err(order_error(e)),
        };
        let order = self.order(caller, order_id).await?;
        let mut order = self.settle(caller, order).await?;
        order.stock_left = match self.catalog.product_of_sku(purchase.sku_id).await {
            Ok(Some((product, _))) => self
                .project(caller, std::slice::from_ref(&product))
                .await
                .ok()
                .and_then(|mut p| p.pop())
                .and_then(|p| p.skus.into_iter().find(|s| s.id == purchase.sku_id))
                .map(|s| {
                    if s.stock_quantity < 0 {
                        UNLIMITED_STOCK
                    } else {
                        s.stock_quantity
                    }
                }),
            _ => None,
        };
        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn texts_and_urls() {
        let m = json!({"en-US": "Card", "zh-CN": "月卡"});
        assert_eq!(localized(m.as_object().unwrap()), "月卡");
        let m = json!({"en-US": "Card", "zh-CN": " "});
        assert_eq!(localized(m.as_object().unwrap()), "Card");
        assert_eq!(localized(&serde_json::Map::new()), "");
        assert_eq!(
            absolute_url("https://a.com/", "/uploads/x.png"),
            "https://a.com/uploads/x.png"
        );
        assert_eq!(
            absolute_url("https://a.com", "uploads/x.png"),
            "https://a.com/uploads/x.png"
        );
        assert_eq!(
            absolute_url("https://a.com", "https://cdn/x.png"),
            "https://cdn/x.png"
        );
    }

    #[test]
    fn sku_names() {
        let spec = json!({"时长": "月卡", "区服": {"zh-CN": "亚服"}});
        assert_eq!(sku_name(spec.as_object().unwrap(), "S1"), "月卡 / 亚服");
        assert_eq!(sku_name(&serde_json::Map::new(), " DEFAULT "), "DEFAULT");
    }

    // PRV-08: a localized spec value (what the admin form stores) is one name, not
    // `空月祝福 / 空月祝福 / Welkin`.
    #[test]
    fn prv08_localized_spec_value_is_one_name() {
        let spec = json!({"zh-CN": "空月祝福", "zh-TW": "空月祝福", "en-US": "Welkin"});
        assert_eq!(sku_name(spec.as_object().unwrap(), "month"), "空月祝福");
        let spec = json!({"en-US": "Welkin", "zh-CN": ""});
        assert_eq!(sku_name(spec.as_object().unwrap(), "month"), "Welkin");
        let spec = json!({"zh-CN": " "});
        assert_eq!(sku_name(spec.as_object().unwrap(), "month"), "month");
        // A locale key mixed with a spec name is still a spec map.
        let spec = json!({"zh-CN": "月卡", "区服": "亚服"});
        assert_eq!(sku_name(spec.as_object().unwrap(), "S1"), "月卡 / 亚服");
    }

    #[test]
    fn paid_statuses() {
        assert!(paid_status("paid"));
        assert!(paid_status("completed"));
        assert!(paid_status("refunded"));
        assert!(!paid_status("pending_payment"));
        assert!(!paid_status("canceled"));
    }
}
