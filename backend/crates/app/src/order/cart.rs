//! Cart use cases (`modules/cart`).

use serde::Serialize;
use zs_domain::catalog::product::{
    FulfillmentType, Product, ProductSku, manual_sku_available, should_enforce_manual_sku_stock,
    validate_purchase_quantity,
};
use zs_domain::marketing::promotion::apply_promotion;
use zs_domain::order::model::{JsonMap, keys};
use zs_domain::order::ports::CartRow;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use super::OrderService;

/// Product summary of a cart line (`CartProductResp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CartProduct {
    pub slug: String,
    pub title: JsonMap,
    pub price_amount: Amount,
    pub images: Vec<String>,
    pub tags: Vec<String>,
    pub purchase_type: String,
    pub min_purchase_quantity: i32,
    pub max_purchase_quantity: i32,
    pub fulfillment_type: String,
    pub is_active: bool,
}

/// A cart line (`CartItemResp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CartItem {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub fulfillment_type: String,
    pub unit_price: Amount,
    pub original_price: Amount,
    pub currency: String,
    pub product: CartProduct,
}

/// Upstream delivery is shown as manual on the storefront (UPS-19).
fn shown_type(ft: FulfillmentType) -> String {
    match ft {
        FulfillmentType::Upstream => FulfillmentType::Manual.as_str().to_owned(),
        other => other.as_str().to_owned(),
    }
}

/// SKU of a cart line: explicit id among active SKUs, else the only active SKU.
fn cart_sku(product: &Product, sku_id: Id) -> Option<ProductSku> {
    let active: Vec<&ProductSku> = product.skus.iter().filter(|s| s.is_active).collect();
    if sku_id > 0 {
        return active.into_iter().find(|s| s.id == sku_id).cloned();
    }
    match active.as_slice() {
        [only] => Some((*only).clone()),
        _ => None,
    }
}

impl OrderService {
    /// `GET /cart`: unavailable lines (product gone, SKU gone, manual stock exhausted) are
    /// dropped from the cart.
    pub async fn cart(&self, user_id: Id) -> Result<Vec<CartItem>> {
        if user_id <= 0 {
            return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
        }
        let currency = self.site_currency().await;
        let now = self.deps.clock.now();
        let mut out = Vec::new();
        for row in self
            .deps
            .cart
            .list(user_id)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?
        {
            let product = self.deps.catalog.orderable_product(row.product_id).await?;
            let usable = product.and_then(|p| cart_sku(&p, row.sku_id).map(|s| (p, s)));
            let Some((product, sku)) = usable.filter(|(p, s)| {
                !(p.fulfillment_type == FulfillmentType::Manual
                    && should_enforce_manual_sku_stock(p, s)
                    && manual_sku_available(s) <= 0)
            }) else {
                self.deps
                    .cart
                    .delete(user_id, row.product_id, row.sku_id)
                    .await?;
                continue;
            };
            let promotions = self.deps.promotions.list_effective(product.id, now).await?;
            let unit = apply_promotion(&promotions, sku.price_amount, row.quantity.max(1))?
                .map_or(sku.price_amount, |(_, price)| price);
            let ft = shown_type(product.fulfillment_type);
            out.push(CartItem {
                product_id: row.product_id,
                sku_id: sku.id,
                quantity: row.quantity,
                fulfillment_type: ft.clone(),
                unit_price: unit,
                original_price: sku.price_amount,
                currency: currency.clone(),
                product: CartProduct {
                    slug: product.slug.clone(),
                    title: product.title.clone(),
                    price_amount: product.price_amount,
                    images: product.images.clone(),
                    tags: product.tags.clone(),
                    purchase_type: product.purchase_type.as_str().to_owned(),
                    min_purchase_quantity: product.min_purchase_quantity,
                    max_purchase_quantity: product.max_purchase_quantity,
                    fulfillment_type: ft,
                    is_active: product.is_active,
                },
            });
        }
        Ok(out)
    }

    /// `POST /cart/items` (`UpsertItem`): quantity limits and manual stock are enforced
    /// server side (ORD-10); upstream products cannot be put in the cart.
    pub async fn upsert_cart_item(
        &self,
        user_id: Id,
        product_id: Id,
        sku_id: Id,
        quantity: i32,
    ) -> Result<()> {
        if user_id <= 0 || product_id <= 0 || quantity <= 0 {
            return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
        }
        let product = self
            .deps
            .catalog
            .orderable_product(product_id)
            .await?
            .ok_or_else(|| Error::bad_request(keys::PRODUCT_NOT_AVAILABLE))?;
        validate_purchase_quantity(
            product.min_purchase_quantity,
            product.max_purchase_quantity,
            quantity,
        )?;
        let sku = cart_sku(&product, sku_id)
            .ok_or_else(|| Error::bad_request(keys::ORDER_ITEM_INVALID))?;
        if product.fulfillment_type == FulfillmentType::Upstream {
            return Err(Error::bad_request(keys::FULFILLMENT_INVALID));
        }
        if product.fulfillment_type == FulfillmentType::Manual
            && should_enforce_manual_sku_stock(&product, &sku)
            && manual_sku_available(&sku) < quantity
        {
            return Err(Error::bad_request(keys::MANUAL_STOCK_INSUFFICIENT));
        }
        self.deps
            .cart
            .upsert(
                user_id,
                &CartRow {
                    product_id,
                    sku_id: sku.id,
                    quantity,
                    fulfillment_type: product.fulfillment_type.as_str().to_owned(),
                },
                self.deps.clock.now(),
            )
            .await
            .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))
    }

    /// `DELETE /cart/items/:product_id?sku_id=` (a non-positive quantity upsert also removes).
    pub async fn remove_cart_item(&self, user_id: Id, product_id: Id, sku_id: Id) -> Result<()> {
        if user_id <= 0 || product_id <= 0 {
            return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
        }
        self.deps
            .cart
            .delete(user_id, product_id, sku_id)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))
    }
}
