//! [`ProductRepo`] backed by `products` and `product_skus`.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::catalog::category::Category;
use zs_domain::catalog::product::{
    FulfillmentType, Product, ProductFilter, ProductRepo, ProductSavePlan, ProductSku,
    PurchaseType, QuickUpdate, SkuFields, SkuOp, SkuPlan, SkuScope, StockFilter,
    plan_multi_sku_sync, plan_single_sku_sync,
};
use zs_domain::catalog::stock::StockDisplayMode;
use zs_domain::catalog::wholesale::{SkuRef, WholesaleTier, normalize_wholesale_prices_for_skus};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

use super::sql::{
    SEARCH_LOCALES, backend_of, ilike_sql, json_array_nonempty, json_text, tombstone,
};
use crate::db::entity::{
    card_secret_batches, card_secrets, cart_items, categories, member_level_prices,
    product_mappings, product_skus, products, sku_mappings,
};
use crate::db::repo::support::{DbResultExt, from_json, now, to_json};

/// SeaORM implementation of [`ProductRepo`].
#[derive(Debug, Clone)]
pub struct SeaProductRepo {
    db: DatabaseConnection,
}

impl SeaProductRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// Maps a SKU row.
pub fn sku_to_domain(m: product_skus::Model) -> ProductSku {
    ProductSku {
        id: m.id,
        product_id: m.product_id,
        sku_code: m.sku_code,
        spec_values: from_json(m.spec_values_json),
        price_amount: Amount::new(m.price_amount),
        cost_price_amount: Amount::new(m.cost_price_amount),
        manual_stock_total: m.manual_stock_total,
        manual_stock_locked: m.manual_stock_locked,
        manual_stock_sold: m.manual_stock_sold,
        auto_stock_available: 0,
        auto_stock_total: 0,
        auto_stock_locked: 0,
        auto_stock_sold: 0,
        upstream_stock: 0,
        is_active: m.is_active,
        sort_order: m.sort_order,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn category_to_domain(m: categories::Model) -> Category {
    Category {
        id: m.id,
        parent_id: m.parent_id,
        slug: m.slug,
        name: from_json(m.name_json),
        icon: m.icon,
        sort_order: m.sort_order,
        is_active: m.is_active,
        created_at: m.created_at,
    }
}

/// Maps a product row (relations filled by the caller).
pub fn product_to_domain(m: products::Model) -> Product {
    Product {
        id: m.id,
        category_id: m.category_id,
        slug: m.slug,
        seo_meta: from_json(m.seo_meta_json),
        title: from_json(m.title_json),
        description: from_json(m.description_json),
        content: from_json(m.content_json),
        instructions: from_json(m.instructions_json),
        price_amount: Amount::new(m.price_amount),
        cost_price_amount: Amount::new(m.cost_price_amount),
        wholesale_prices: from_json::<Vec<WholesaleTier>>(m.wholesale_prices),
        images: from_json(m.images),
        tags: from_json(m.tags),
        purchase_type: PurchaseType::from_stored(&m.purchase_type),
        min_purchase_quantity: m.min_purchase_quantity,
        max_purchase_quantity: m.max_purchase_quantity,
        stock_display_mode: StockDisplayMode::from_stored(&m.stock_display_mode),
        fulfillment_type: FulfillmentType::from_stored(&m.fulfillment_type),
        manual_form_schema: from_json(m.manual_form_schema_json),
        manual_stock_total: m.manual_stock_total,
        manual_stock_locked: m.manual_stock_locked,
        manual_stock_sold: m.manual_stock_sold,
        payment_channel_ids: m.payment_channel_ids,
        is_affiliate_enabled: m.is_affiliate_enabled,
        auto_stock_available: 0,
        auto_stock_total: 0,
        auto_stock_locked: 0,
        auto_stock_sold: 0,
        is_mapped: m.is_mapped,
        is_active: m.is_active,
        sort_order: m.sort_order,
        created_at: m.created_at,
        updated_at: m.updated_at,
        category: None,
        skus: Vec::new(),
    }
}

/// `EXISTS` an active, non-deleted category for the product row.
const ACTIVE_CATEGORY_SQL: &str = "EXISTS (SELECT 1 FROM categories c WHERE c.id = products.category_id \
     AND c.is_active = true AND c.deleted_at IS NULL)";

// Stock sub-expressions (ORD-12), identical to the original `applyStockStatusFilter`.
const MANUAL_ACTIVE_SKU: &str = "EXISTS (SELECT 1 FROM product_skus ps WHERE ps.product_id = products.id \
     AND ps.is_active = true AND ps.deleted_at IS NULL)";
const MANUAL_UNLIMITED_SKU: &str = "EXISTS (SELECT 1 FROM product_skus ps WHERE ps.product_id = products.id \
     AND ps.is_active = true AND ps.deleted_at IS NULL AND ps.manual_stock_total = -1)";
const MANUAL_REMAINING: &str = "COALESCE((SELECT SUM(CASE WHEN ps.manual_stock_total > 0 THEN ps.manual_stock_total \
     ELSE 0 END) FROM product_skus ps WHERE ps.product_id = products.id AND ps.is_active = true \
     AND ps.deleted_at IS NULL), 0)";
const AUTO_AVAILABLE: &str = "COALESCE((SELECT COUNT(*) FROM card_secrets cs WHERE cs.product_id = products.id \
     AND cs.status = 'available' AND cs.deleted_at IS NULL), 0)";
const UPSTREAM_UNLIMITED: &str = "EXISTS (SELECT 1 FROM product_mappings pm JOIN sku_mappings sm \
     ON sm.product_mapping_id = pm.id AND sm.deleted_at IS NULL WHERE pm.local_product_id = products.id \
     AND pm.deleted_at IS NULL AND sm.upstream_stock = -1)";
const UPSTREAM_SUM: &str = "COALESCE((SELECT SUM(CASE WHEN sm.upstream_stock > 0 THEN sm.upstream_stock ELSE 0 END) \
     FROM product_mappings pm JOIN sku_mappings sm ON sm.product_mapping_id = pm.id AND sm.deleted_at IS NULL \
     WHERE pm.local_product_id = products.id AND pm.deleted_at IS NULL), 0)";

fn stock_condition(filter: StockFilter, threshold: i64) -> String {
    let threshold = Ord::max(threshold, 0);
    match filter {
        StockFilter::Low => format!(
            "((products.fulfillment_type = 'manual' AND ((({MANUAL_ACTIVE_SKU}) AND NOT ({MANUAL_UNLIMITED_SKU}) \
             AND ({MANUAL_REMAINING}) <= 0) OR (NOT ({MANUAL_ACTIVE_SKU}) AND products.manual_stock_total = 0))) \
             OR (products.fulfillment_type = 'auto' AND ({AUTO_AVAILABLE}) >= 0 AND ({AUTO_AVAILABLE}) <= {threshold}) \
             OR (products.fulfillment_type = 'upstream' AND NOT ({UPSTREAM_UNLIMITED}) AND ({UPSTREAM_SUM}) = 0))"
        ),
        StockFilter::Normal => format!(
            "((products.fulfillment_type = 'manual' AND ((({MANUAL_ACTIVE_SKU}) AND NOT ({MANUAL_UNLIMITED_SKU}) \
             AND ({MANUAL_REMAINING}) > 0) OR (NOT ({MANUAL_ACTIVE_SKU}) AND products.manual_stock_total > 0))) \
             OR (products.fulfillment_type = 'auto' AND ({AUTO_AVAILABLE}) > {threshold}) \
             OR (products.fulfillment_type = 'upstream' AND NOT ({UPSTREAM_UNLIMITED}) AND ({UPSTREAM_SUM}) > 0))"
        ),
        StockFilter::Unlimited => format!(
            "((products.fulfillment_type = 'manual' AND (({MANUAL_UNLIMITED_SKU}) OR (NOT ({MANUAL_ACTIVE_SKU}) \
             AND products.manual_stock_total = -1))) \
             OR (products.fulfillment_type = 'upstream' AND ({UPSTREAM_UNLIMITED})))"
        ),
    }
}

fn search_condition<C: ConnectionTrait>(conn: &C, search: &str) -> Condition {
    let backend = backend_of(conn);
    let mut any = Condition::any().add(ilike_sql("products.slug", search));
    for column in ["products.title_json", "products.description_json"] {
        for locale in SEARCH_LOCALES {
            any = any.add(ilike_sql(json_text(backend, column, locale), search));
        }
    }
    let sku_match = Query::select()
        .expr(Expr::val(1))
        .from(product_skus::Entity)
        .and_where(
            Expr::col((product_skus::Entity, product_skus::Column::ProductId))
                .equals((products::Entity, products::Column::Id)),
        )
        .and_where(Expr::col((product_skus::Entity, product_skus::Column::DeletedAt)).is_null())
        .and_where(ilike_sql("product_skus.sku_code", search))
        .to_owned();
    any = any.add(Expr::exists(sku_match));
    if let Ok(id) = search.parse::<Id>()
        && id > 0
    {
        any = any.add(products::Column::Id.eq(id));
    }
    any
}

fn filter_condition<C: ConnectionTrait>(conn: &C, filter: &ProductFilter) -> Condition {
    let mut cond = Condition::all().add(products::Column::DeletedAt.is_null());
    if filter.only_active {
        cond = cond
            .add(products::Column::IsActive.eq(true))
            .add(Expr::cust(ACTIVE_CATEGORY_SQL));
    } else if let Some(active) = filter.is_active {
        cond = cond.add(products::Column::IsActive.eq(active));
    }
    match (&filter.category_ids, filter.category_id) {
        (Some(ids), _) if ids.is_empty() => cond = cond.add(Expr::cust("1 = 0")),
        (Some(ids), _) => cond = cond.add(products::Column::CategoryId.is_in(ids.clone())),
        (None, Some(id)) => cond = cond.add(products::Column::CategoryId.eq(id)),
        (None, None) => {}
    }
    if !filter.exclude_ids.is_empty() {
        cond = cond.add(products::Column::Id.is_not_in(filter.exclude_ids.clone()));
    }
    if !filter.fulfillment_type.is_empty() {
        cond = cond.add(products::Column::FulfillmentType.eq(filter.fulfillment_type.clone()));
    }
    if !filter.search.is_empty() {
        cond = cond.add(search_condition(conn, &filter.search));
    }
    if let Some(stock) = filter.stock {
        cond = cond.add(Expr::cust(stock_condition(
            stock,
            filter.low_stock_threshold,
        )));
    }
    if let Some(has) = filter.has_wholesale_prices {
        let nonempty = json_array_nonempty(backend_of(conn), "products.wholesale_prices");
        cond = cond.add(if has {
            Expr::cust(nonempty)
        } else {
            Expr::cust(format!("NOT ({nonempty})"))
        });
    }
    cond
}

/// Loads categories and SKUs for a page of products.
async fn attach_relations<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<products::Model>,
    scope: SkuScope,
) -> Result<Vec<Product>> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<Id> = rows.iter().map(|r| r.id).collect();
    let category_ids: Vec<Id> = rows.iter().map(|r| r.category_id).collect();
    let cats: HashMap<Id, Category> = categories::Entity::find()
        .filter(categories::Column::DeletedAt.is_null())
        .filter(categories::Column::Id.is_in(category_ids))
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|c| (c.id, category_to_domain(c)))
        .collect();
    let mut sku_q = product_skus::Entity::find()
        .filter(product_skus::Column::DeletedAt.is_null())
        .filter(product_skus::Column::ProductId.is_in(ids));
    if scope == SkuScope::Active {
        sku_q = sku_q.filter(product_skus::Column::IsActive.eq(true));
    }
    let mut skus: HashMap<Id, Vec<ProductSku>> = HashMap::new();
    for s in sku_q
        .order_by_desc(product_skus::Column::SortOrder)
        .order_by_asc(product_skus::Column::Id)
        .all(conn)
        .await
        .dom()?
    {
        skus.entry(s.product_id).or_default().push(sku_to_domain(s));
    }
    Ok(rows
        .into_iter()
        .map(|m| {
            let mut p = product_to_domain(m);
            p.category = cats.get(&p.category_id).cloned();
            p.skus = skus.remove(&p.id).unwrap_or_default();
            p
        })
        .collect())
}

/// Loads one product with relations on any connection.
pub(crate) async fn load_product<C: ConnectionTrait>(
    conn: &C,
    id: Id,
    scope: SkuScope,
    public: bool,
) -> Result<Option<Product>> {
    let mut q = products::Entity::find_by_id(id).filter(products::Column::DeletedAt.is_null());
    if public {
        q = q
            .filter(products::Column::IsActive.eq(true))
            .filter(Expr::cust(ACTIVE_CATEGORY_SQL));
    }
    let Some(row) = q.one(conn).await.dom()? else {
        return Ok(None);
    };
    Ok(attach_relations(conn, vec![row], scope).await?.pop())
}

async fn list_skus_in<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    active_only: bool,
) -> Result<Vec<ProductSku>> {
    let mut q = product_skus::Entity::find()
        .filter(product_skus::Column::DeletedAt.is_null())
        .filter(product_skus::Column::ProductId.eq(product_id));
    if active_only {
        q = q.filter(product_skus::Column::IsActive.eq(true));
    }
    Ok(q.order_by_desc(product_skus::Column::SortOrder)
        .order_by_asc(product_skus::Column::Id)
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(sku_to_domain)
        .collect())
}

async fn apply_sku_ops<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    ops: &[SkuOp],
    at: DateTime<Utc>,
) -> Result<()> {
    for op in ops {
        match op {
            SkuOp::Update { id, fields } => {
                product_skus::ActiveModel {
                    id: Set(*id),
                    sku_code: Set(fields.sku_code.clone()),
                    spec_values_json: Set(to_json(&fields.spec_values)?),
                    price_amount: Set(fields.price_amount.decimal()),
                    cost_price_amount: Set(fields.cost_price_amount.decimal()),
                    manual_stock_total: Set(fields.manual_stock_total),
                    is_active: Set(fields.is_active),
                    sort_order: Set(fields.sort_order),
                    updated_at: Set(at),
                    ..Default::default()
                }
                .update(conn)
                .await
                .dom()?;
            }
            SkuOp::Create(fields) => {
                // ORD-11: soft-deleted rows would still hold the (product_id, sku_code) key.
                product_skus::Entity::delete_many()
                    .filter(product_skus::Column::ProductId.eq(product_id))
                    .filter(product_skus::Column::SkuCode.eq(fields.sku_code.clone()))
                    .filter(product_skus::Column::DeletedAt.is_not_null())
                    .exec(conn)
                    .await
                    .dom()?;
                new_sku(product_id, fields, at)?.insert(conn).await.dom()?;
            }
            SkuOp::Delete(id) => {
                product_skus::Entity::delete_by_id(*id)
                    .exec(conn)
                    .await
                    .dom()?;
            }
        }
    }
    Ok(())
}

fn new_sku(
    product_id: Id,
    fields: &SkuFields,
    at: DateTime<Utc>,
) -> Result<product_skus::ActiveModel> {
    Ok(product_skus::ActiveModel {
        product_id: Set(product_id),
        sku_code: Set(fields.sku_code.clone()),
        spec_values_json: Set(to_json(&fields.spec_values)?),
        price_amount: Set(fields.price_amount.decimal()),
        cost_price_amount: Set(fields.cost_price_amount.decimal()),
        manual_stock_total: Set(fields.manual_stock_total),
        manual_stock_locked: Set(0),
        manual_stock_sold: Set(0),
        is_active: Set(fields.is_active),
        sort_order: Set(fields.sort_order),
        created_at: Set(at),
        updated_at: Set(at),
        ..Default::default()
    })
}

fn product_model(plan: &ProductSavePlan, at: DateTime<Utc>) -> Result<products::ActiveModel> {
    let f = &plan.fields;
    let mut model = products::ActiveModel {
        category_id: Set(f.category_id),
        slug: Set(f.slug.clone()),
        seo_meta_json: Set(to_json(&f.seo_meta)?),
        title_json: Set(to_json(&f.title)?),
        description_json: Set(to_json(&f.description)?),
        content_json: Set(to_json(&f.content)?),
        instructions_json: Set(to_json(&f.instructions)?),
        price_amount: Set(f.price_amount.decimal()),
        cost_price_amount: Set(f.cost_price_amount.decimal()),
        images: Set(to_json(&f.images)?),
        tags: Set(to_json(&f.tags)?),
        purchase_type: Set(f.purchase_type.as_str().to_owned()),
        min_purchase_quantity: Set(f.min_purchase_quantity),
        max_purchase_quantity: Set(f.max_purchase_quantity),
        stock_display_mode: Set(f.stock_display_mode.as_str().to_owned()),
        fulfillment_type: Set(f.fulfillment_type.as_str().to_owned()),
        manual_form_schema_json: Set(to_json(&f.manual_form_schema)?),
        manual_stock_total: Set(f.manual_stock_total),
        payment_channel_ids: Set(f.payment_channel_ids.clone()),
        is_affiliate_enabled: Set(f.is_affiliate_enabled),
        is_active: Set(f.is_active),
        sort_order: Set(f.sort_order),
        updated_at: Set(at),
        ..Default::default()
    };
    match plan.id {
        Some(id) => model.id = Set(id),
        None => {
            model.wholesale_prices = Set(to_json(&Vec::<WholesaleTier>::new())?);
            model.manual_stock_locked = Set(0);
            model.manual_stock_sold = Set(0);
            model.is_mapped = Set(false);
            model.created_at = Set(at);
        }
    }
    Ok(model)
}

#[async_trait]
impl ProductRepo for SeaProductRepo {
    async fn list(&self, filter: &ProductFilter) -> Result<Page<Product>> {
        let query = products::Entity::find().filter(filter_condition(&self.db, filter));
        let total = query.clone().count(&self.db).await.dom()?;
        let rows = query
            .order_by_desc(products::Column::SortOrder)
            .order_by_desc(products::Column::CreatedAt)
            .order_by_desc(products::Column::Id)
            .offset(filter.page.offset())
            .limit(filter.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        let scope = if filter.only_active {
            SkuScope::Active
        } else {
            SkuScope::All
        };
        Ok(Page {
            items: attach_relations(&self.db, rows, scope).await?,
            total,
        })
    }

    async fn get(&self, id: Id, skus: SkuScope) -> Result<Option<Product>> {
        load_product(&self.db, id, skus, false).await
    }

    async fn get_public_by_slug(&self, slug: &str) -> Result<Option<Product>> {
        let row = products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::Slug.eq(slug))
            .filter(products::Column::IsActive.eq(true))
            .filter(Expr::cust(ACTIVE_CATEGORY_SQL))
            .one(&self.db)
            .await
            .dom()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(attach_relations(&self.db, vec![row], SkuScope::Active)
            .await?
            .pop())
    }

    async fn count_by_slug(&self, slug: &str, exclude: Option<Id>) -> Result<u64> {
        let mut q = products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::Slug.eq(slug));
        if let Some(id) = exclude {
            q = q.filter(products::Column::Id.ne(id));
        }
        q.count(&self.db).await.dom()
    }

    async fn list_skus(&self, product_id: Id, active_only: bool) -> Result<Vec<ProductSku>> {
        list_skus_in(&self.db, product_id, active_only).await
    }

    async fn get_sku(&self, id: Id) -> Result<Option<ProductSku>> {
        Ok(product_skus::Entity::find_by_id(id)
            .filter(product_skus::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(sku_to_domain))
    }

    async fn get_sku_by_code(&self, product_id: Id, code: &str) -> Result<Option<ProductSku>> {
        Ok(product_skus::Entity::find()
            .filter(product_skus::Column::DeletedAt.is_null())
            .filter(product_skus::Column::ProductId.eq(product_id))
            .filter(product_skus::Column::SkuCode.eq(code.trim()))
            .one(&self.db)
            .await
            .dom()?
            .map(sku_to_domain))
    }

    async fn save(&self, plan: &ProductSavePlan) -> Result<Id> {
        let at = now();
        let txn = self.db.begin().await.dom()?;
        let model = product_model(plan, at)?;
        let id = match plan.id {
            Some(id) => {
                model.update(&txn).await.dom()?;
                id
            }
            None => model.insert(&txn).await.dom()?.id,
        };
        let existing = list_skus_in(&txn, id, false).await?;
        let ops = match &plan.skus {
            SkuPlan::Multi(rows) => plan_multi_sku_sync(&existing, rows)?,
            SkuPlan::Single {
                price,
                cost,
                manual_stock_total,
            } => plan_single_sku_sync(&existing, *price, *cost, *manual_stock_total),
        };
        apply_sku_ops(&txn, id, &ops, at).await?;
        if let Some(inputs) = &plan.wholesale {
            // PRC-02: tiers are validated against the SKUs as persisted above.
            let refs: Vec<SkuRef> = list_skus_in(&txn, id, false)
                .await?
                .into_iter()
                .map(|s| SkuRef {
                    id: s.id,
                    code: s.sku_code,
                })
                .collect();
            let tiers = normalize_wholesale_prices_for_skus(inputs, &refs)?;
            products::Entity::update_many()
                .col_expr(
                    products::Column::WholesalePrices,
                    Expr::value(to_json(&tiers)?),
                )
                .filter(products::Column::Id.eq(id))
                .exec(&txn)
                .await
                .dom()?;
        }
        txn.commit().await.dom()?;
        Ok(id)
    }

    async fn quick_update(&self, id: Id, update: &QuickUpdate) -> Result<()> {
        let mut q = products::Entity::update_many()
            .col_expr(products::Column::UpdatedAt, Expr::value(now()))
            .filter(products::Column::Id.eq(id))
            .filter(products::Column::DeletedAt.is_null());
        if let Some(v) = update.is_active {
            q = q.col_expr(products::Column::IsActive, Expr::value(v));
        }
        if let Some(v) = update.sort_order {
            q = q.col_expr(products::Column::SortOrder, Expr::value(v));
        }
        if let Some(v) = update.category_id {
            q = q.col_expr(products::Column::CategoryId, Expr::value(v));
        }
        q.exec(&self.db).await.dom()?;
        Ok(())
    }

    async fn set_wholesale_prices(&self, id: Id, tiers: &[WholesaleTier]) -> Result<()> {
        products::Entity::update_many()
            .col_expr(
                products::Column::WholesalePrices,
                Expr::value(to_json(&tiers)?),
            )
            .col_expr(products::Column::UpdatedAt, Expr::value(now()))
            .filter(products::Column::Id.eq(id))
            .filter(products::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn delete_cascade(&self, id: Id) -> Result<()> {
        let at = now();
        let txn = self.db.begin().await.dom()?;
        card_secrets::Entity::update_many()
            .col_expr(card_secrets::Column::DeletedAt, Expr::value(at))
            .col_expr(card_secrets::Column::UpdatedAt, Expr::value(at))
            .filter(card_secrets::Column::ProductId.eq(id))
            .filter(card_secrets::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        card_secret_batches::Entity::update_many()
            .col_expr(card_secret_batches::Column::DeletedAt, Expr::value(at))
            .col_expr(card_secret_batches::Column::UpdatedAt, Expr::value(at))
            .filter(card_secret_batches::Column::ProductId.eq(id))
            .filter(card_secret_batches::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        product_skus::Entity::update_many()
            .col_expr(product_skus::Column::DeletedAt, Expr::value(at))
            .filter(product_skus::Column::ProductId.eq(id))
            .filter(product_skus::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        member_level_prices::Entity::update_many()
            .col_expr(member_level_prices::Column::DeletedAt, Expr::value(at))
            .filter(member_level_prices::Column::ProductId.eq(id))
            .filter(member_level_prices::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        cart_items::Entity::update_many()
            .col_expr(cart_items::Column::DeletedAt, Expr::value(at))
            .filter(cart_items::Column::ProductId.eq(id))
            .filter(cart_items::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        let mapping_ids: Vec<Id> = product_mappings::Entity::find()
            .select_only()
            .column(product_mappings::Column::Id)
            .filter(product_mappings::Column::LocalProductId.eq(id))
            .filter(product_mappings::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&txn)
            .await
            .dom()?;
        if !mapping_ids.is_empty() {
            sku_mappings::Entity::update_many()
                .col_expr(sku_mappings::Column::DeletedAt, Expr::value(at))
                .filter(sku_mappings::Column::ProductMappingId.is_in(mapping_ids.clone()))
                .filter(sku_mappings::Column::DeletedAt.is_null())
                .exec(&txn)
                .await
                .dom()?;
            product_mappings::Entity::update_many()
                .col_expr(product_mappings::Column::DeletedAt, Expr::value(at))
                .filter(product_mappings::Column::Id.is_in(mapping_ids))
                .exec(&txn)
                .await
                .dom()?;
        }
        // Free the unique slug so it can be reused (ORD-11 principle for unique keys).
        let slug: Option<String> = products::Entity::find_by_id(id)
            .select_only()
            .column(products::Column::Slug)
            .into_tuple()
            .one(&txn)
            .await
            .dom()?;
        let mut q = products::Entity::update_many()
            .col_expr(products::Column::DeletedAt, Expr::value(at))
            .filter(products::Column::Id.eq(id))
            .filter(products::Column::DeletedAt.is_null());
        if let Some(slug) = slug {
            q = q.col_expr(products::Column::Slug, Expr::value(tombstone(&slug, id)));
        }
        q.exec(&txn).await.dom()?;
        txn.commit().await.dom()?;
        Ok(())
    }
}
