//! [`MappingRepo`] on `product_mappings` / `sku_mappings`, plus the catalog writes of
//! import and sync, each in one transaction (UPS-20, DB-01: only the txn handle is
//! used inside).

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::catalog::product::{DEFAULT_SKU_CODE, JsonMap, SkuScope};
use zs_domain::catalog::wholesale::WholesaleTier;
use zs_domain::integration::mapping::{
    ImportPlan, LocalSkuRef, MappingFilter, MappingRepo, NewLocalSku, ProductMapping, SkuMapping,
    UpstreamStatus, WholesaleIndex, convert_tiers, lowest_prices, plan_sync,
};
use zs_domain::integration::pricing::Pricing;
use zs_domain::integration::protocol::{RemoteProduct, RemoteSku, RemoteTier};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

use super::connection::many_in as connections_in;
use crate::db::entity::{categories, product_mappings, product_skus, products, sku_mappings};
use crate::db::repo::catalog::product::load_product;
use crate::db::repo::catalog::sql::{SEARCH_LOCALES, backend_of, ilike_sql, json_text};
use crate::db::repo::support::{DbResultExt, to_json};

/// Suffixes tried when an auto-created category slug is taken (original 2..=10).
const CATEGORY_SLUG_RETRIES: u32 = 10;

/// SeaORM implementation of [`MappingRepo`].
#[derive(Debug, Clone)]
pub struct SeaMappingRepo {
    db: DatabaseConnection,
}

impl SeaMappingRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

pub(crate) fn to_mapping(m: product_mappings::Model) -> ProductMapping {
    ProductMapping {
        id: m.id,
        connection_id: m.connection_id,
        local_product_id: m.local_product_id,
        upstream_product_id: m.upstream_product_id,
        upstream_fulfillment_type: m.upstream_fulfillment_type,
        upstream_status: UpstreamStatus::from_stored(&m.upstream_status),
        is_active: m.is_active,
        last_synced_at: m.last_synced_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        connection: None,
        product: None,
    }
}

pub(crate) fn to_sku_mapping(m: sku_mappings::Model) -> SkuMapping {
    SkuMapping {
        id: m.id,
        product_mapping_id: m.product_mapping_id,
        local_sku_id: m.local_sku_id,
        upstream_sku_id: m.upstream_sku_id,
        upstream_price: Amount::new(m.upstream_price),
        upstream_stock: m.upstream_stock,
        upstream_is_active: m.upstream_is_active,
        stock_synced_at: m.stock_synced_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn live() -> Condition {
    Condition::all().add(product_mappings::Column::DeletedAt.is_null())
}

async fn sku_mappings_in<C: ConnectionTrait>(conn: &C, mapping_id: Id) -> Result<Vec<SkuMapping>> {
    Ok(sku_mappings::Entity::find()
        .filter(sku_mappings::Column::ProductMappingId.eq(mapping_id))
        .filter(sku_mappings::Column::DeletedAt.is_null())
        .order_by_asc(sku_mappings::Column::Id)
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(to_sku_mapping)
        .collect())
}

/// Every SKU code of a product, soft-deleted rows included (the unique index covers them).
async fn taken_codes<C: ConnectionTrait>(conn: &C, product_id: Id) -> Result<HashSet<String>> {
    Ok(product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|s| s.sku_code.to_lowercase())
        .collect())
}

/// A code unique within the product: the supplier code, else `SKU-{upstream id}`, with
/// a numeric suffix on collision.
fn unique_code(taken: &mut HashSet<String>, code: &str, upstream_id: Id) -> String {
    let base = if code.trim().is_empty() {
        format!("SKU-{upstream_id}")
    } else {
        code.trim().chars().take(56).collect()
    };
    let mut candidate = base.clone();
    let mut n = 2;
    while taken.contains(&candidate.to_lowercase()) {
        candidate = format!("{base}-{n}");
        n += 1;
    }
    taken.insert(candidate.to_lowercase());
    candidate
}

async fn insert_sku<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    sku: &NewLocalSku,
    code: String,
    now: DateTime<Utc>,
) -> Result<Id> {
    Ok(product_skus::ActiveModel {
        product_id: Set(product_id),
        sku_code: Set(code),
        spec_values_json: Set(to_json(&sku.spec_values)?),
        price_amount: Set(sku.price.decimal()),
        cost_price_amount: Set(sku.cost.decimal()),
        manual_stock_total: Set(0),
        manual_stock_locked: Set(0),
        manual_stock_sold: Set(0),
        is_active: Set(sku.is_active),
        sort_order: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?
    .id)
}

async fn insert_sku_mapping<C: ConnectionTrait>(
    conn: &C,
    mapping_id: Id,
    local_sku_id: Id,
    sku: &NewLocalSku,
    now: DateTime<Utc>,
) -> Result<()> {
    sku_mappings::ActiveModel {
        product_mapping_id: Set(mapping_id),
        local_sku_id: Set(local_sku_id),
        upstream_sku_id: Set(sku.upstream_sku_id),
        upstream_price: Set(sku.upstream_price.decimal()),
        upstream_stock: Set(sku.upstream_stock),
        upstream_is_active: Set(sku.upstream_is_active),
        stock_synced_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    Ok(())
}

/// Translates supplier tiers and stores them when the result is non-empty; an empty
/// supplier list or an empty conversion keeps the local configuration (UPS-04/13).
async fn sync_tiers<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    mapping_id: Id,
    tiers: &[RemoteTier],
    remote_skus: &[RemoteSku],
    pricing: &Pricing,
    now: DateTime<Utc>,
) -> Result<()> {
    if tiers.is_empty() {
        return Ok(());
    }
    let local: Vec<LocalSkuRef> = product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .filter(product_skus::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|s| LocalSkuRef {
            id: s.id,
            code: s.sku_code,
        })
        .collect();
    let pairs: Vec<(Id, Id)> = sku_mappings_in(conn, mapping_id)
        .await?
        .into_iter()
        .map(|m| (m.local_sku_id, m.upstream_sku_id))
        .collect();
    let index = WholesaleIndex::build(&local, remote_skus, &pairs);
    let converted: Vec<WholesaleTier> = convert_tiers(tiers, pricing, &index);
    if converted.is_empty() {
        tracing::warn!(
            product_id,
            tiers = tiers.len(),
            "upstream wholesale tiers dropped, local tiers kept"
        );
        return Ok(());
    }
    products::Entity::update_many()
        .col_expr(
            products::Column::WholesalePrices,
            Expr::value(to_json(&converted)?),
        )
        .col_expr(products::Column::UpdatedAt, Expr::value(now))
        .filter(products::Column::Id.eq(product_id))
        .exec(conn)
        .await
        .dom()?;
    Ok(())
}

/// Product price / cost = lowest active SKU price / cost.
async fn recalc_product_price<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    now: DateTime<Utc>,
) -> Result<()> {
    let skus: Vec<(Amount, Amount, bool)> = product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .filter(product_skus::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|s| {
            (
                Amount::new(s.price_amount),
                Amount::new(s.cost_price_amount),
                s.is_active,
            )
        })
        .collect();
    if let Some((price, cost)) = lowest_prices(&skus) {
        products::Entity::update_many()
            .col_expr(products::Column::PriceAmount, Expr::value(price.decimal()))
            .col_expr(
                products::Column::CostPriceAmount,
                Expr::value(cost.decimal()),
            )
            .col_expr(products::Column::UpdatedAt, Expr::value(now))
            .filter(products::Column::Id.eq(product_id))
            .exec(conn)
            .await
            .dom()?;
    }
    Ok(())
}

async fn touch_product<C: ConnectionTrait>(
    conn: &C,
    product_id: Id,
    now: DateTime<Utc>,
) -> Result<()> {
    products::Entity::update_many()
        .col_expr(products::Column::UpdatedAt, Expr::value(now))
        .filter(products::Column::Id.eq(product_id))
        .exec(conn)
        .await
        .dom()?;
    Ok(())
}

#[async_trait]
impl MappingRepo for SeaMappingRepo {
    async fn get(&self, id: Id) -> Result<Option<ProductMapping>> {
        Ok(product_mappings::Entity::find_by_id(id)
            .filter(live())
            .one(&self.db)
            .await
            .dom()?
            .map(to_mapping))
    }

    async fn get_by_upstream(
        &self,
        connection_id: Id,
        upstream_product_id: Id,
    ) -> Result<Option<ProductMapping>> {
        Ok(product_mappings::Entity::find()
            .filter(live())
            .filter(product_mappings::Column::ConnectionId.eq(connection_id))
            .filter(product_mappings::Column::UpstreamProductId.eq(upstream_product_id))
            .one(&self.db)
            .await
            .dom()?
            .map(to_mapping))
    }

    async fn list(&self, filter: &MappingFilter) -> Result<Page<ProductMapping>> {
        let mut q = product_mappings::Entity::find().filter(live());
        if filter.connection_id > 0 {
            q = q.filter(product_mappings::Column::ConnectionId.eq(filter.connection_id));
        }
        if let Some(s) = filter.upstream_status {
            q = q.filter(product_mappings::Column::UpstreamStatus.eq(s.as_str()));
        }
        if let Some(active) = filter.active {
            q = q.filter(product_mappings::Column::IsActive.eq(active));
        }
        let search = filter.search.trim();
        if !search.is_empty() {
            let backend = backend_of(&self.db);
            let mut any = Condition::any();
            for locale in SEARCH_LOCALES {
                any = any.add(ilike_sql(
                    json_text(backend, "products.title_json", locale),
                    search,
                ));
            }
            let ids: Vec<Id> = products::Entity::find()
                .select_only()
                .column(products::Column::Id)
                .filter(products::Column::DeletedAt.is_null())
                .filter(any)
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            q = q.filter(product_mappings::Column::LocalProductId.is_in(ids));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(product_mappings::Column::CreatedAt)
            .order_by_desc(product_mappings::Column::Id)
            .offset(filter.page.offset())
            .limit(filter.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        let conn_ids: Vec<Id> = rows.iter().map(|r| r.connection_id).collect();
        let conns: HashMap<Id, _> = connections_in(&self.db, &conn_ids)
            .await?
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        let mut items = Vec::with_capacity(rows.len());
        for row in rows {
            let mut m = to_mapping(row);
            m.connection = conns.get(&m.connection_id).cloned();
            m.product = load_product(&self.db, m.local_product_id, SkuScope::All, false).await?;
            items.push(m);
        }
        Ok(Page { items, total })
    }

    async fn list_active(&self) -> Result<Vec<ProductMapping>> {
        Ok(product_mappings::Entity::find()
            .filter(live())
            .filter(product_mappings::Column::IsActive.eq(true))
            .order_by_asc(product_mappings::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(to_mapping)
            .collect())
    }

    async fn list_active_by_connection(&self, connection_id: Id) -> Result<Vec<ProductMapping>> {
        Ok(product_mappings::Entity::find()
            .filter(live())
            .filter(product_mappings::Column::IsActive.eq(true))
            .filter(product_mappings::Column::ConnectionId.eq(connection_id))
            .order_by_asc(product_mappings::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(to_mapping)
            .collect())
    }

    async fn mapped_upstream_ids(&self, connection_id: Id) -> Result<Vec<Id>> {
        product_mappings::Entity::find()
            .select_only()
            .column(product_mappings::Column::UpstreamProductId)
            .filter(live())
            .filter(product_mappings::Column::ConnectionId.eq(connection_id))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()
    }

    async fn sku_mappings(&self, mapping_id: Id) -> Result<Vec<SkuMapping>> {
        sku_mappings_in(&self.db, mapping_id).await
    }

    async fn sku_mapping_by_local_sku(&self, local_sku_id: Id) -> Result<Option<SkuMapping>> {
        Ok(sku_mappings::Entity::find()
            .filter(sku_mappings::Column::LocalSkuId.eq(local_sku_id))
            .filter(sku_mappings::Column::DeletedAt.is_null())
            .order_by_desc(sku_mappings::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(to_sku_mapping))
    }

    async fn set_active(&self, id: Id, active: bool, now: DateTime<Utc>) -> Result<bool> {
        let res = product_mappings::Entity::update_many()
            .col_expr(product_mappings::Column::IsActive, Expr::value(active))
            .col_expr(product_mappings::Column::UpdatedAt, Expr::value(now))
            .filter(product_mappings::Column::Id.eq(id))
            .filter(live())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }

    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        let Some(m) = product_mappings::Entity::find_by_id(id)
            .filter(live())
            .one(&txn)
            .await
            .dom()?
        else {
            return Ok(());
        };
        sku_mappings::Entity::update_many()
            .col_expr(sku_mappings::Column::DeletedAt, Expr::value(now))
            .filter(sku_mappings::Column::ProductMappingId.eq(id))
            .filter(sku_mappings::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        // The local product loses the mapping; an `upstream` product becomes an
        // inactive `manual` one so nobody buys what cannot be delivered.
        products::Entity::update_many()
            .col_expr(products::Column::IsMapped, Expr::value(false))
            .col_expr(products::Column::UpdatedAt, Expr::value(now))
            .filter(products::Column::Id.eq(m.local_product_id))
            .exec(&txn)
            .await
            .dom()?;
        products::Entity::update_many()
            .col_expr(products::Column::FulfillmentType, Expr::value("manual"))
            .col_expr(products::Column::IsActive, Expr::value(false))
            .filter(products::Column::Id.eq(m.local_product_id))
            .filter(products::Column::FulfillmentType.eq("upstream"))
            .exec(&txn)
            .await
            .dom()?;
        product_mappings::Entity::update_many()
            .col_expr(product_mappings::Column::DeletedAt, Expr::value(now))
            .col_expr(product_mappings::Column::IsActive, Expr::value(false))
            .filter(product_mappings::Column::Id.eq(id))
            .exec(&txn)
            .await
            .dom()?;
        txn.commit().await.dom()
    }

    async fn category_assignable(&self, category_id: Id) -> Result<bool> {
        if category_id == 0 {
            return Ok(true);
        }
        let exists = categories::Entity::find_by_id(category_id)
            .filter(categories::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .is_some();
        if !exists {
            return Ok(false);
        }
        let children = categories::Entity::find()
            .filter(categories::Column::ParentId.eq(category_id))
            .filter(categories::Column::DeletedAt.is_null())
            .count(&self.db)
            .await
            .dom()?;
        Ok(children == 0)
    }

    async fn ensure_category(
        &self,
        slug: &str,
        name: &JsonMap,
        parent_id: Id,
        now: DateTime<Utc>,
    ) -> Result<Id> {
        let base = if slug.trim().is_empty() {
            format!("upstream-category-{}", now.timestamp_millis())
        } else {
            slug.trim().to_owned()
        };
        for n in 1..=CATEGORY_SLUG_RETRIES {
            let candidate = if n == 1 {
                base.clone()
            } else {
                format!("{base}-{n}")
            };
            let existing = categories::Entity::find()
                .filter(categories::Column::Slug.eq(&candidate))
                .one(&self.db)
                .await
                .dom()?;
            match existing {
                Some(c) if c.deleted_at.is_none() => {
                    if n == 1 {
                        return Ok(c.id);
                    }
                    // A suffixed live slug belongs to another category: keep trying.
                }
                Some(c) => {
                    categories::ActiveModel {
                        id: Set(c.id),
                        parent_id: Set(parent_id),
                        name_json: Set(Some(serde_json::Value::Object(name.clone()))),
                        is_active: Set(true),
                        deleted_at: Set(None),
                        ..Default::default()
                    }
                    .update(&self.db)
                    .await
                    .dom()?;
                    return Ok(c.id);
                }
                None => {
                    let row = categories::ActiveModel {
                        parent_id: Set(parent_id),
                        slug: Set(candidate),
                        name_json: Set(Some(serde_json::Value::Object(name.clone()))),
                        icon: Set(String::new()),
                        sort_order: Set(0),
                        is_active: Set(true),
                        created_at: Set(now),
                        deleted_at: Set(None),
                        ..Default::default()
                    }
                    .insert(&self.db)
                    .await
                    .dom()?;
                    return Ok(row.id);
                }
            }
        }
        Err(Error::internal_msg(format!(
            "slug conflict after retries: {base}"
        )))
    }

    async fn import(&self, plan: &ImportPlan, now: DateTime<Utc>) -> Result<ProductMapping> {
        let txn = self.db.begin().await.dom()?;
        let slug_taken = products::Entity::find()
            .filter(products::Column::Slug.eq(&plan.slug))
            .count(&txn)
            .await
            .dom()?;
        if slug_taken > 0 {
            return Err(Error::bad_request("error.slug_exists"));
        }
        let product = products::ActiveModel {
            category_id: Set(plan.category_id),
            slug: Set(plan.slug.clone()),
            seo_meta_json: Set(to_json(&plan.seo_meta)?),
            title_json: Set(to_json(&plan.title)?),
            description_json: Set(to_json(&plan.description)?),
            content_json: Set(to_json(&plan.content)?),
            instructions_json: Set(None),
            price_amount: Set(plan.price.decimal()),
            cost_price_amount: Set(plan.cost.decimal()),
            wholesale_prices: Set(to_json(&Vec::<WholesaleTier>::new())?),
            images: Set(to_json(&plan.images)?),
            tags: Set(to_json(&plan.tags)?),
            purchase_type: Set("member".into()),
            min_purchase_quantity: Set(0),
            max_purchase_quantity: Set(0),
            stock_display_mode: Set("exact".into()),
            fulfillment_type: Set("upstream".into()),
            manual_form_schema_json: Set(to_json(&plan.manual_form_schema)?),
            manual_stock_total: Set(0),
            manual_stock_locked: Set(0),
            manual_stock_sold: Set(0),
            payment_channel_ids: Set(String::new()),
            is_affiliate_enabled: Set(false),
            is_mapped: Set(true),
            // Imported products stay offline until an admin lists them.
            is_active: Set(false),
            sort_order: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        let mut taken = HashSet::new();
        let mut created = Vec::with_capacity(plan.skus.len());
        for sku in &plan.skus {
            let code = if sku.upstream_sku_id == 0 {
                taken.insert(DEFAULT_SKU_CODE.to_lowercase());
                DEFAULT_SKU_CODE.to_owned()
            } else {
                unique_code(&mut taken, &sku.sku_code, sku.upstream_sku_id)
            };
            let id = insert_sku(&txn, product.id, sku, code, now).await?;
            created.push((id, sku));
        }
        let mapping = product_mappings::ActiveModel {
            connection_id: Set(plan.connection_id),
            local_product_id: Set(product.id),
            upstream_product_id: Set(plan.upstream_product_id),
            upstream_fulfillment_type: Set(plan.upstream_fulfillment_type.clone()),
            upstream_status: Set(UpstreamStatus::Active.as_str().to_owned()),
            is_active: Set(true),
            last_synced_at: Set(Some(now)),
            created_at: Set(now),
            updated_at: Set(now),
            deleted_at: Set(None),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        for (local_id, sku) in &created {
            if sku.upstream_sku_id > 0 {
                insert_sku_mapping(&txn, mapping.id, *local_id, sku, now).await?;
            }
        }
        sync_tiers(
            &txn,
            product.id,
            mapping.id,
            &plan.tiers,
            &[],
            &plan.pricing,
            now,
        )
        .await?;
        txn.commit().await.dom()?;
        Ok(to_mapping(mapping))
    }

    async fn apply_sync(
        &self,
        mapping: &ProductMapping,
        remote: &RemoteProduct,
        pricing: &Pricing,
        auto_sync_price: bool,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        let existing = sku_mappings_in(&txn, mapping.id).await?;
        let plan = plan_sync(&existing, remote, pricing, auto_sync_price);
        let product_id = mapping.local_product_id;
        let product_live = products::Entity::find_by_id(product_id)
            .filter(products::Column::DeletedAt.is_null())
            .one(&txn)
            .await
            .dom()?
            .is_some();
        if product_live && let Some(schema) = &plan.manual_form_schema {
            products::Entity::update_many()
                .col_expr(
                    products::Column::ManualFormSchemaJson,
                    Expr::value(to_json(schema)?),
                )
                .filter(products::Column::Id.eq(product_id))
                .exec(&txn)
                .await
                .dom()?;
        }
        for c in &plan.mapping_changes {
            let mut upd = sku_mappings::Entity::update_many()
                .col_expr(
                    sku_mappings::Column::UpstreamStock,
                    Expr::value(c.upstream_stock),
                )
                .col_expr(
                    sku_mappings::Column::UpstreamIsActive,
                    Expr::value(c.upstream_is_active),
                )
                .col_expr(sku_mappings::Column::StockSyncedAt, Expr::value(now))
                .col_expr(sku_mappings::Column::UpdatedAt, Expr::value(now));
            if let Some(price) = c.upstream_price {
                upd = upd.col_expr(
                    sku_mappings::Column::UpstreamPrice,
                    Expr::value(price.decimal()),
                );
            }
            upd.filter(sku_mappings::Column::Id.eq(c.id))
                .exec(&txn)
                .await
                .dom()?;
        }
        for c in &plan.local_changes {
            let mut upd = product_skus::Entity::update_many()
                .col_expr(product_skus::Column::IsActive, Expr::value(c.is_active))
                .col_expr(product_skus::Column::UpdatedAt, Expr::value(now));
            if let Some(spec) = &c.spec_values {
                upd = upd.col_expr(
                    product_skus::Column::SpecValuesJson,
                    Expr::value(to_json(spec)?),
                );
            }
            if let Some((price, cost)) = c.price {
                upd = upd
                    .col_expr(
                        product_skus::Column::PriceAmount,
                        Expr::value(price.decimal()),
                    )
                    .col_expr(
                        product_skus::Column::CostPriceAmount,
                        Expr::value(cost.decimal()),
                    );
            }
            upd.filter(product_skus::Column::Id.eq(c.sku_id))
                .filter(product_skus::Column::DeletedAt.is_null())
                .exec(&txn)
                .await
                .dom()?;
        }
        if product_live && !plan.new_skus.is_empty() {
            let mut taken = taken_codes(&txn, product_id).await?;
            for sku in &plan.new_skus {
                let code = unique_code(&mut taken, &sku.sku_code, sku.upstream_sku_id);
                let id = insert_sku(&txn, product_id, sku, code, now).await?;
                insert_sku_mapping(&txn, mapping.id, id, sku, now).await?;
            }
        }
        if product_live {
            if plan.recalc_product_price {
                recalc_product_price(&txn, product_id, now).await?;
            }
            sync_tiers(
                &txn,
                product_id,
                mapping.id,
                &plan.tiers,
                &remote.skus,
                pricing,
                now,
            )
            .await?;
            // Stock/price changes refresh `updated_at` so our own downstream buyers'
            // incremental syncs see them (UPS-17).
            touch_product(&txn, product_id, now).await?;
        }
        product_mappings::Entity::update_many()
            .col_expr(
                product_mappings::Column::UpstreamFulfillmentType,
                Expr::value(plan.upstream_fulfillment_type.clone()),
            )
            .col_expr(
                product_mappings::Column::UpstreamStatus,
                Expr::value(UpstreamStatus::Active.as_str()),
            )
            .col_expr(product_mappings::Column::LastSyncedAt, Expr::value(now))
            .col_expr(product_mappings::Column::UpdatedAt, Expr::value(now))
            .filter(product_mappings::Column::Id.eq(mapping.id))
            .exec(&txn)
            .await
            .dom()?;
        txn.commit().await.dom()
    }

    async fn mark_unavailable(
        &self,
        mapping: &ProductMapping,
        status: UpstreamStatus,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        products::Entity::update_many()
            .col_expr(products::Column::IsActive, Expr::value(false))
            .col_expr(products::Column::UpdatedAt, Expr::value(now))
            .filter(products::Column::Id.eq(mapping.local_product_id))
            .filter(products::Column::IsActive.eq(true))
            .exec(&txn)
            .await
            .dom()?;
        let skus = sku_mappings_in(&txn, mapping.id).await?;
        sku_mappings::Entity::update_many()
            .col_expr(sku_mappings::Column::UpstreamIsActive, Expr::value(false))
            .col_expr(sku_mappings::Column::UpstreamStock, Expr::value(0))
            .col_expr(sku_mappings::Column::StockSyncedAt, Expr::value(now))
            .col_expr(sku_mappings::Column::UpdatedAt, Expr::value(now))
            .filter(sku_mappings::Column::ProductMappingId.eq(mapping.id))
            .filter(sku_mappings::Column::DeletedAt.is_null())
            .exec(&txn)
            .await
            .dom()?;
        let local_ids: Vec<Id> = skus.iter().map(|s| s.local_sku_id).collect();
        if !local_ids.is_empty() {
            product_skus::Entity::update_many()
                .col_expr(product_skus::Column::IsActive, Expr::value(false))
                .col_expr(product_skus::Column::UpdatedAt, Expr::value(now))
                .filter(product_skus::Column::Id.is_in(local_ids))
                .exec(&txn)
                .await
                .dom()?;
        }
        let mut upd = product_mappings::Entity::update_many()
            .col_expr(
                product_mappings::Column::UpstreamStatus,
                Expr::value(status.as_str()),
            )
            .col_expr(product_mappings::Column::LastSyncedAt, Expr::value(now))
            .col_expr(product_mappings::Column::UpdatedAt, Expr::value(now));
        if status == UpstreamStatus::Deleted {
            upd = upd.col_expr(product_mappings::Column::IsActive, Expr::value(false));
        }
        upd.filter(product_mappings::Column::Id.eq(mapping.id))
            .exec(&txn)
            .await
            .dom()?;
        txn.commit().await.dom()?;
        tracing::info!(
            mapping_id = mapping.id,
            local_product_id = mapping.local_product_id,
            status = status.as_str(),
            "upstream product unavailable"
        );
        Ok(())
    }

    async fn reapply_pricing(
        &self,
        connection_id: Id,
        pricing: &Pricing,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        let mappings = self.list_active_by_connection(connection_id).await?;
        let mut updated = 0;
        for m in mappings {
            let txn = self.db.begin().await.dom()?;
            for sm in sku_mappings_in(&txn, m.id).await? {
                let upstream = sm.upstream_price.decimal();
                let price = pricing.local_price(upstream);
                // Never write a zero price (UPS-05/08).
                if price <= rust_decimal::Decimal::ZERO {
                    continue;
                }
                product_skus::Entity::update_many()
                    .col_expr(product_skus::Column::PriceAmount, Expr::value(price))
                    .col_expr(
                        product_skus::Column::CostPriceAmount,
                        Expr::value(pricing.cost_price(upstream)),
                    )
                    .col_expr(product_skus::Column::UpdatedAt, Expr::value(now))
                    .filter(product_skus::Column::Id.eq(sm.local_sku_id))
                    .filter(product_skus::Column::DeletedAt.is_null())
                    .exec(&txn)
                    .await
                    .dom()?;
            }
            let live = products::Entity::find_by_id(m.local_product_id)
                .filter(products::Column::DeletedAt.is_null())
                .one(&txn)
                .await
                .dom()?
                .is_some();
            if live {
                recalc_product_price(&txn, m.local_product_id, now).await?;
                updated += 1;
            }
            txn.commit().await.dom()?;
        }
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sku_codes_are_unique_per_product() {
        let mut taken = HashSet::new();
        assert_eq!(unique_code(&mut taken, "A", 1), "A");
        assert_eq!(unique_code(&mut taken, "a", 2), "a-2");
        assert_eq!(unique_code(&mut taken, "", 9), "SKU-9");
        assert_eq!(unique_code(&mut taken, " ", 9), "SKU-9-2");
    }
}
