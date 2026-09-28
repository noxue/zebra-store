//! Product endpoints: admin CRUD / batch operations and the public catalog.

use axum::extract::{Path, RawQuery, State};
use axum::http::HeaderMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_app::catalog::product::{AdminProductQuery, BatchOutcome};
use zs_domain::catalog::product::{JsonMap, Product, ProductInput, QuickUpdate, SkuInput};
use zs_domain::catalog::storefront::PublicProduct;
use zs_domain::catalog::wholesale::WholesalePriceInput;
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::Pagination;

use super::{PageQuery, parse_tri_state};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req, req_min1, req_ptr};
use crate::i18n;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::routes::order::common::TenantCtx;
use crate::state::AppState;

/// `/api/v1/public/*` product routes.
pub(super) fn public() -> Routes {
    Routes::new("/public")
        .get("/products", public_products)
        .get("/products/{slug}", public_product)
}

/// `/api/v1/admin/*` product routes.
pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/products", admin_products)
        .get("/products/{id}", admin_product)
        .post("/products", create_product)
        .put("/products/{id}", update_product)
        .patch("/products/{id}/wholesale-prices", update_wholesale_prices)
        .patch("/products/{id}", quick_update_product)
        .delete("/products/{id}", delete_product)
        .post("/products/batch-status", batch_status)
        .post("/products/batch-category", batch_category)
        .post("/products/batch-delete", batch_delete)
}

// ---------------------------------------------------------------- public

#[derive(Debug, Default, Deserialize)]
struct PublicListQuery {
    #[serde(flatten)]
    page: PageQuery,
    #[serde(default)]
    category_id: String,
    #[serde(default)]
    search: String,
}

async fn public_products(
    State(s): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Query(q): Query<PublicListQuery>,
) -> ApiResult<Paged<PublicProduct>> {
    let req = q.page.request();
    let page = s
        .svc
        .catalog
        .product
        .list_public_for(&tenant, &q.category_id, &q.search, req)
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

async fn public_product(
    State(s): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Path(slug): Path<String>,
) -> ApiResult<Data<PublicProduct>> {
    ok(s.svc.catalog.product.get_public_for(&tenant, &slug).await?)
}

// ---------------------------------------------------------------- admin reads

#[derive(Debug, Default, Deserialize)]
struct AdminListQuery {
    #[serde(flatten)]
    page: PageQuery,
    category_id: Option<String>,
    #[serde(default)]
    search: String,
    #[serde(default)]
    fulfillment_type: String,
    stock_status: Option<String>,
    /// Misspelled alias kept by the original.
    stock_staus: Option<String>,
    wholesale: Option<String>,
    has_wholesale_prices: Option<String>,
    is_active: Option<String>,
}

async fn admin_products(
    State(s): State<AppState>,
    Query(q): Query<AdminListQuery>,
) -> ApiResult<Paged<Product>> {
    let invalid = |_| Error::invalid();
    let mut wholesale = parse_tri_state(q.wholesale.as_deref()).map_err(invalid)?;
    if wholesale.is_none() {
        wholesale = parse_tri_state(q.has_wholesale_prices.as_deref()).map_err(invalid)?;
    }
    let is_active = parse_tri_state(q.is_active.as_deref()).map_err(invalid)?;
    // The original compares the raw string with `category_id`; a non-numeric value matches nothing.
    let category_id = q
        .category_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.parse::<Id>().unwrap_or(-1));
    let stock_status = q
        .stock_status
        .filter(|v| !v.is_empty())
        .or(q.stock_staus)
        .unwrap_or_default();
    let req = q.page.request();
    let page = s
        .svc
        .catalog
        .product
        .list_admin(AdminProductQuery {
            page: req,
            category_id,
            search: q.search,
            fulfillment_type: q.fulfillment_type,
            stock_status,
            has_wholesale_prices: wholesale,
            is_active,
        })
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

async fn admin_product(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Product>> {
    ok(s.svc.catalog.product.get_admin(id).await?)
}

// ---------------------------------------------------------------- writes

#[derive(Debug, Deserialize)]
struct SkuRequest {
    #[serde(default)]
    id: Id,
    #[serde(default)]
    sku_code: String,
    spec_values: Option<JsonMap>,
    #[serde(default)]
    price_amount: Amount,
    #[serde(default)]
    cost_price_amount: Amount,
    #[serde(default)]
    manual_stock_total: i32,
    is_active: Option<bool>,
    #[serde(default)]
    sort_order: i32,
}

#[derive(Debug, Deserialize)]
struct WholesalePriceRequest {
    #[serde(default)]
    sku_id: Id,
    #[serde(default)]
    sku_code: String,
    #[serde(default)]
    min_quantity: i32,
    #[serde(default)]
    unit_price: Amount,
}

impl From<WholesalePriceRequest> for WholesalePriceInput {
    fn from(r: WholesalePriceRequest) -> Self {
        Self {
            sku_id: r.sku_id,
            sku_code: r.sku_code.trim().to_owned(),
            min_quantity: r.min_quantity,
            unit_price: r.unit_price,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProductRequest {
    #[serde(default)]
    category_id: Id,
    #[serde(default)]
    slug: String,
    seo_meta: Option<JsonMap>,
    title: Option<JsonMap>,
    description: Option<JsonMap>,
    content: Option<JsonMap>,
    instructions: Option<JsonMap>,
    manual_form_schema: Option<JsonMap>,
    #[serde(default)]
    price_amount: Amount,
    #[serde(default)]
    cost_price_amount: Amount,
    wholesale_prices: Option<Vec<WholesalePriceRequest>>,
    #[serde(default)]
    images: Option<Vec<String>>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    purchase_type: String,
    min_purchase_quantity: Option<i32>,
    max_purchase_quantity: Option<i32>,
    #[serde(default)]
    stock_display_mode: String,
    #[serde(default)]
    fulfillment_type: String,
    manual_stock_total: Option<i32>,
    #[serde(default)]
    skus: Option<Vec<SkuRequest>>,
    #[serde(default)]
    payment_channel_ids: Option<Vec<Id>>,
    is_affiliate_enabled: Option<bool>,
    is_active: Option<bool>,
    #[serde(default)]
    sort_order: i32,
}

/// Original `CreateProductRequest` (create and update).
impl BindRules for ProductRequest {
    const FIELDS: &'static [BindField] = &[
        req("category_id", "CategoryID"),
        req("slug", "Slug"),
        req("title", "TitleJSON"),
        req("price_amount", "PriceAmount"),
    ];
}

impl ProductRequest {
    /// Mirrors the original `binding:"required"` rules (category, slug, title, non-zero price,
    /// SKU code and price).
    fn into_input(self) -> Result<ProductInput, Error> {
        if self.category_id <= 0
            || self.slug.is_empty()
            || self.title.is_none()
            || self.price_amount.is_zero()
        {
            return Err(Error::invalid());
        }
        let skus = self.skus.unwrap_or_default();
        if skus
            .iter()
            .any(|s| s.sku_code.is_empty() || s.price_amount.is_zero())
        {
            return Err(Error::invalid());
        }
        Ok(ProductInput {
            category_id: self.category_id,
            slug: self.slug,
            seo_meta: self.seo_meta,
            title: self.title,
            description: self.description,
            content: self.content,
            instructions: self.instructions,
            manual_form_schema: self.manual_form_schema,
            price_amount: self.price_amount,
            cost_price_amount: self.cost_price_amount,
            wholesale_prices: self
                .wholesale_prices
                .map(|v| v.into_iter().map(Into::into).collect()),
            images: self.images.unwrap_or_default(),
            tags: self.tags.unwrap_or_default(),
            purchase_type: self.purchase_type,
            min_purchase_quantity: self.min_purchase_quantity,
            max_purchase_quantity: self.max_purchase_quantity,
            stock_display_mode: self.stock_display_mode,
            fulfillment_type: self.fulfillment_type,
            manual_stock_total: self.manual_stock_total,
            skus: skus
                .into_iter()
                .map(|s| SkuInput {
                    id: s.id,
                    sku_code: s.sku_code,
                    spec_values: s.spec_values,
                    price_amount: s.price_amount,
                    cost_price_amount: s.cost_price_amount,
                    manual_stock_total: s.manual_stock_total,
                    is_active: s.is_active,
                    sort_order: s.sort_order,
                })
                .collect(),
            payment_channel_ids: self.payment_channel_ids.unwrap_or_default(),
            is_affiliate_enabled: self.is_affiliate_enabled,
            is_active: self.is_active,
            sort_order: self.sort_order,
        })
    }
}

async fn create_product(
    State(s): State<AppState>,
    Bind(req): Bind<ProductRequest>,
) -> ApiResult<Data<Product>> {
    ok(s.svc.catalog.product.create(req.into_input()?).await?)
}

async fn update_product(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<ProductRequest>,
) -> ApiResult<Data<Product>> {
    ok(s.svc.catalog.product.update(id, req.into_input()?).await?)
}

#[derive(Debug, Deserialize)]
struct WholesaleRequest {
    wholesale_prices: Option<Vec<WholesalePriceRequest>>,
}

impl BindRules for WholesaleRequest {
    const FIELDS: &'static [BindField] = &[req_ptr("wholesale_prices", "WholesalePrices")];
}

async fn update_wholesale_prices(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<WholesaleRequest>,
) -> ApiResult<Data<Product>> {
    let inputs: Vec<WholesalePriceInput> = req
        .wholesale_prices
        .ok_or_else(Error::invalid)?
        .into_iter()
        .map(Into::into)
        .collect();
    ok(s.svc
        .catalog
        .product
        .update_wholesale_prices(id, &inputs)
        .await?)
}

#[derive(Debug, Deserialize)]
struct QuickUpdateRequest {
    is_active: Option<bool>,
    sort_order: Option<i32>,
    category_id: Option<Id>,
}

async fn quick_update_product(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<QuickUpdateRequest>,
) -> ApiResult<Data<Product>> {
    let update = QuickUpdate {
        is_active: req.is_active,
        sort_order: req.sort_order,
        category_id: req.category_id,
    };
    if update.is_empty() || req.category_id.is_some_and(|c| c < 0) {
        return Err(Error::invalid().into());
    }
    ok(s.svc.catalog.product.quick_update(id, update).await?)
}

async fn delete_product(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<()>> {
    s.svc.catalog.product.delete(id).await?;
    ok(())
}

// ---------------------------------------------------------------- batch

fn require_ids(ids: Option<Vec<Id>>) -> Result<Vec<Id>, Error> {
    match ids {
        Some(ids) if !ids.is_empty() && ids.iter().all(|id| *id >= 0) => Ok(ids),
        _ => Err(Error::invalid()),
    }
}

#[derive(Debug, Deserialize)]
struct BatchStatusRequest {
    ids: Option<Vec<Id>>,
    #[serde(default)]
    is_active: bool,
}

impl BindRules for BatchStatusRequest {
    const FIELDS: &'static [BindField] = &[req_min1("ids", "IDs")];
}

#[derive(Debug, Serialize)]
struct FailedItem {
    id: Id,
    error_code: String,
    message: String,
}

fn failure_item(locale: &str, id: Id, err: &Error) -> FailedItem {
    let code = match err.key() {
        "error.product_category_invalid" => "product_category_invalid",
        "error.product_not_found" => "product_not_found",
        _ => "product_update_failed",
    };
    FailedItem {
        id,
        error_code: code.to_owned(),
        message: i18n::translate(locale, &format!("error.{code}")),
    }
}

async fn batch_status(
    State(s): State<AppState>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
    Bind(req): Bind<BatchStatusRequest>,
) -> ApiResult<Data<Value>> {
    let ids = require_ids(req.ids)?;
    let locale = i18n::resolve_locale(query.as_deref(), &headers);
    let BatchOutcome {
        total,
        success_count,
        failures,
    } = s
        .svc
        .catalog
        .product
        .batch_status(&ids, req.is_active)
        .await;
    let failed: Vec<FailedItem> = failures
        .iter()
        .map(|(id, e)| failure_item(locale, *id, e))
        .collect();
    ok(json!({"total": total, "success_count": success_count, "failed_items": failed}))
}

#[derive(Debug, Deserialize)]
struct BatchCategoryRequest {
    ids: Option<Vec<Id>>,
    #[serde(default)]
    category_id: Id,
}

impl BindRules for BatchCategoryRequest {
    const FIELDS: &'static [BindField] = &[req_min1("ids", "IDs")];
}

async fn batch_category(
    State(s): State<AppState>,
    Bind(req): Bind<BatchCategoryRequest>,
) -> ApiResult<Data<Value>> {
    let ids = require_ids(req.ids)?;
    if req.category_id < 0 {
        return Err(Error::invalid().into());
    }
    let outcome = s
        .svc
        .catalog
        .product
        .batch_category(&ids, req.category_id)
        .await;
    ok(json!({"total": outcome.total, "success_count": outcome.success_count}))
}

#[derive(Debug, Deserialize)]
struct BatchIdsRequest {
    ids: Option<Vec<Id>>,
}

impl BindRules for BatchIdsRequest {
    const FIELDS: &'static [BindField] = &[req_min1("ids", "IDs")];
}

async fn batch_delete(
    State(s): State<AppState>,
    Bind(req): Bind<BatchIdsRequest>,
) -> ApiResult<Data<Value>> {
    let ids = require_ids(req.ids)?;
    let outcome = s.svc.catalog.product.batch_delete(&ids).await;
    let failed: Option<Vec<Id>> = (!outcome.failures.is_empty())
        .then(|| outcome.failures.iter().map(|(id, _)| *id).collect());
    ok(
        json!({"total": outcome.total, "success_count": outcome.success_count, "failed_ids": failed}),
    )
}
