//! [`ProductSettingRepo`] and [`PricingRepo`]: `reseller_product_settings` with the
//! catalog tables they refer to.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde_json::Value;
use zs_domain::reseller::ports::{
    PricingRepo, ProductListFilter, ProductSettingRepo, ProductWithSettings, SettingFilter,
    SettingSummary,
};
use zs_domain::reseller::pricing::{PricedProduct, PricedSku};
use zs_domain::reseller::{PricingMode, ProductBrief, ProductSetting};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::{
    SeaResellerStore, keyword_profile_ids, load_profiles, setting_model, user_profile_ids,
};
use crate::db::entity::{
    categories, product_skus as skus, products, reseller_product_settings as settings,
    reseller_related_accounts as related,
};
use crate::db::repo::catalog::sql::{SEARCH_LOCALES, backend_of, ilike_sql, json_text};
use crate::db::repo::support::DbResultExt;

/// Which SKUs a priced product carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkuScope {
    Active,
    All,
}

fn sku_model(m: skus::Model) -> PricedSku {
    PricedSku {
        id: m.id,
        sku_code: m.sku_code,
        spec_values: m.spec_values_json.unwrap_or(Value::Null),
        price: Amount::new(m.price_amount),
        cost_price: Amount::new(m.cost_price_amount),
        is_active: m.is_active,
    }
}

fn product_model(m: products::Model, skus: Vec<PricedSku>) -> PricedProduct {
    PricedProduct {
        id: m.id,
        slug: m.slug,
        title: m.title_json.unwrap_or(Value::Null),
        price: Amount::new(m.price_amount),
        cost_price: Amount::new(m.cost_price_amount),
        is_active: m.is_active,
        skus,
    }
}

/// Attaches SKUs (`sort_order DESC, id ASC`) to product rows, keeping their order.
async fn with_skus<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<products::Model>,
    scope: SkuScope,
) -> Result<Vec<PricedProduct>> {
    let ids: Vec<Id> = rows.iter().map(|p| p.id).collect();
    let mut by_product: HashMap<Id, Vec<PricedSku>> = HashMap::new();
    if !ids.is_empty() {
        let mut q = skus::Entity::find()
            .filter(skus::Column::ProductId.is_in(ids))
            .filter(skus::Column::DeletedAt.is_null());
        if scope == SkuScope::Active {
            q = q.filter(skus::Column::IsActive.eq(true));
        }
        for s in q
            .order_by_desc(skus::Column::SortOrder)
            .order_by_asc(skus::Column::Id)
            .all(conn)
            .await
            .dom()?
        {
            by_product
                .entry(s.product_id)
                .or_default()
                .push(sku_model(s));
        }
    }
    Ok(rows
        .into_iter()
        .map(|p| {
            let s = by_product.remove(&p.id).unwrap_or_default();
            product_model(p, s)
        })
        .collect())
}

/// Live rules of a reseller for `product_ids` (`product_id, sku_id` order).
async fn rules_of<C: ConnectionTrait>(
    conn: &C,
    reseller_id: Id,
    product_ids: &[Id],
) -> Result<HashMap<Id, Vec<ProductSetting>>> {
    let mut out: HashMap<Id, Vec<ProductSetting>> = HashMap::new();
    if product_ids.is_empty() {
        return Ok(out);
    }
    for row in settings::Entity::find()
        .filter(settings::Column::ResellerId.eq(reseller_id))
        .filter(settings::Column::ProductId.is_in(product_ids.to_vec()))
        .filter(settings::Column::DeletedAt.is_null())
        .order_by_asc(settings::Column::ProductId)
        .order_by_asc(settings::Column::SkuId)
        .all(conn)
        .await
        .dom()?
    {
        out.entry(row.product_id)
            .or_default()
            .push(setting_model(row));
    }
    Ok(out)
}

/// Product ids that have a live rule of the reseller (optionally only hidden rules).
fn rule_products(reseller_id: Id, hidden_only: bool) -> sea_orm::sea_query::SelectStatement {
    let mut q = Query::select()
        .column(settings::Column::ProductId)
        .from(settings::Entity)
        .and_where(settings::Column::ResellerId.eq(reseller_id))
        .and_where(settings::Column::DeletedAt.is_null())
        .to_owned();
    if hidden_only {
        q.and_where(settings::Column::IsListed.eq(false));
    }
    q
}

/// Slug / localized title / description / numeric id search.
fn product_keyword<C: ConnectionTrait>(conn: &C, keyword: &str) -> Condition {
    let backend = backend_of(conn);
    let mut cond = Condition::any().add(ilike_sql("products.slug", keyword));
    for column in ["products.title_json", "products.description_json"] {
        for locale in SEARCH_LOCALES {
            cond = cond.add(ilike_sql(json_text(backend, column, locale), keyword));
        }
    }
    if let Ok(id) = keyword.parse::<Id>()
        && id > 0
    {
        cond = cond.add(products::Column::Id.eq(id));
    }
    cond
}

#[async_trait]
impl ProductSettingRepo for SeaResellerStore {
    async fn list_products(
        &self,
        f: &ProductListFilter,
        page: PageRequest,
    ) -> Result<Page<ProductWithSettings>> {
        if f.reseller_id == 0 {
            return Ok(Page {
                items: Vec::new(),
                total: 0,
            });
        }
        let active_categories = Query::select()
            .column(categories::Column::Id)
            .from(categories::Entity)
            .and_where(categories::Column::IsActive.eq(true))
            .and_where(categories::Column::DeletedAt.is_null())
            .to_owned();
        let mut cond = Condition::all()
            .add(products::Column::DeletedAt.is_null())
            .add(products::Column::IsActive.eq(true))
            .add(products::Column::CategoryId.in_subquery(active_categories));
        if let Some(cid) = f.category_id.filter(|c| *c > 0) {
            cond = cond.add(products::Column::CategoryId.eq(cid));
        }
        let keyword = f.keyword.trim();
        if !keyword.is_empty() {
            cond = cond.add(product_keyword(&self.db, keyword));
        }
        match f.configured.as_str() {
            "configured" => {
                cond =
                    cond.add(products::Column::Id.in_subquery(rule_products(f.reseller_id, false)))
            }
            "unconfigured" => {
                cond = cond
                    .add(products::Column::Id.not_in_subquery(rule_products(f.reseller_id, false)));
            }
            _ => {}
        }
        match f.listed.as_str() {
            "hidden" => {
                cond =
                    cond.add(products::Column::Id.in_subquery(rule_products(f.reseller_id, true)))
            }
            "listed" => {
                cond = cond
                    .add(products::Column::Id.not_in_subquery(rule_products(f.reseller_id, true)))
            }
            _ => {}
        }
        let q = products::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(products::Column::SortOrder)
            .order_by_desc(products::Column::CreatedAt)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        let items = with_skus(&self.db, rows, SkuScope::Active).await?;
        let ids: Vec<Id> = items.iter().map(|p| p.id).collect();
        let mut rules = rules_of(&self.db, f.reseller_id, &ids).await?;
        let items = items
            .into_iter()
            .map(|p| {
                let r = rules.remove(&p.id).unwrap_or_default();
                (p, r)
            })
            .collect();
        Ok(Page { items, total })
    }

    async fn product_with_settings(
        &self,
        reseller_id: Id,
        product_id: Id,
    ) -> Result<Option<ProductWithSettings>> {
        let Some(row) = products::Entity::find_by_id(product_id)
            .filter(products::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let product = with_skus(&self.db, vec![row], SkuScope::Active)
            .await?
            .pop();
        let mut rules = rules_of(&self.db, reseller_id, &[product_id]).await?;
        Ok(product.map(|p| (p, rules.remove(&product_id).unwrap_or_default())))
    }

    async fn product_for_save(&self, product_id: Id) -> Result<Option<PricedProduct>> {
        let Some(row) = products::Entity::find_by_id(product_id)
            .filter(products::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        Ok(with_skus(&self.db, vec![row], SkuScope::All).await?.pop())
    }

    async fn save_settings(
        &self,
        reseller_id: Id,
        product_id: Id,
        rows: &[ProductSetting],
        now: DateTime<Utc>,
    ) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        for row in rows {
            let existing = settings::Entity::find()
                .filter(settings::Column::ResellerId.eq(reseller_id))
                .filter(settings::Column::ProductId.eq(product_id))
                .filter(settings::Column::SkuId.eq(row.sku_id))
                .lock_exclusive()
                .one(&txn)
                .await
                .dom()?;
            let mut model = settings::ActiveModel {
                reseller_id: Set(reseller_id),
                product_id: Set(product_id),
                sku_id: Set(row.sku_id),
                is_listed: Set(row.is_listed),
                pricing_mode: Set(row.pricing_mode.as_str().to_owned()),
                markup_percent: Set(row.markup_percent.decimal()),
                fixed_markup_amount: Set(row.fixed_markup_amount.decimal()),
                fixed_price_amount: Set(row.fixed_price_amount.decimal()),
                sort_order: Set(row.sort_order),
                updated_at: Set(now),
                deleted_at: Set(None),
                ..Default::default()
            };
            match existing {
                Some(e) => {
                    model.id = Set(e.id);
                    model.update(&txn).await.dom()?;
                }
                None => {
                    model.created_at = Set(now);
                    model.insert(&txn).await.dom()?;
                }
            }
        }
        txn.commit().await.dom()
    }

    async fn delete_setting(
        &self,
        reseller_id: Id,
        product_id: Id,
        sku_id: Id,
        now: DateTime<Utc>,
    ) -> Result<()> {
        settings::Entity::update_many()
            .col_expr(settings::Column::DeletedAt, Expr::value(now))
            .col_expr(settings::Column::UpdatedAt, Expr::value(now))
            .filter(settings::Column::ResellerId.eq(reseller_id))
            .filter(settings::Column::ProductId.eq(product_id))
            .filter(settings::Column::SkuId.eq(sku_id))
            .filter(settings::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn list_settings(
        &self,
        f: &SettingFilter,
        page: PageRequest,
    ) -> Result<Page<ProductSetting>> {
        let mut cond = Condition::all().add(settings::Column::DeletedAt.is_null());
        if let Some(rid) = f.reseller_id {
            cond = cond.add(settings::Column::ResellerId.eq(rid));
        }
        if let Some(pid) = f.product_id {
            cond = cond.add(settings::Column::ProductId.eq(pid));
        }
        if let Some(uid) = f.user_id {
            cond = cond
                .add(settings::Column::ResellerId.is_in(user_profile_ids(&self.db, uid).await?));
        }
        if !f.pricing_mode.trim().is_empty() {
            cond = cond.add(settings::Column::PricingMode.eq(f.pricing_mode.trim()));
        }
        match f.listed.trim().to_lowercase().as_str() {
            "hidden" => cond = cond.add(settings::Column::IsListed.eq(false)),
            "listed" => cond = cond.add(settings::Column::IsListed.eq(true)),
            _ => {}
        }
        let keyword = f.keyword.trim();
        if !keyword.is_empty() {
            let slugs = Query::select()
                .column(products::Column::Id)
                .from(products::Entity)
                .cond_where(ilike_sql("slug", keyword))
                .to_owned();
            cond = cond.add(
                Condition::any()
                    .add(settings::Column::ProductId.in_subquery(slugs))
                    .add(
                        settings::Column::ResellerId
                            .is_in(keyword_profile_ids(&self.db, keyword).await?),
                    ),
            );
        }
        let q = settings::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows: Vec<ProductSetting> = q
            .order_by_desc(settings::Column::UpdatedAt)
            .order_by_desc(settings::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(setting_model)
            .collect();
        let profiles = load_profiles(
            &self.db,
            &rows.iter().map(|r| r.reseller_id).collect::<Vec<_>>(),
        )
        .await?;
        let product_ids: Vec<Id> = rows
            .iter()
            .map(|r| r.product_id)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let briefs: HashMap<Id, ProductBrief> = if product_ids.is_empty() {
            HashMap::new()
        } else {
            products::Entity::find()
                .filter(products::Column::Id.is_in(product_ids))
                .filter(products::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?
                .into_iter()
                .map(|p| {
                    (
                        p.id,
                        ProductBrief {
                            id: p.id,
                            slug: p.slug,
                            title: p.title_json.unwrap_or(Value::Null),
                            price_amount: Amount::new(p.price_amount),
                            is_active: p.is_active,
                        },
                    )
                })
                .collect()
        };
        let items = rows
            .into_iter()
            .map(|mut r| {
                r.profile = profiles.get(&r.reseller_id).cloned();
                r.product = briefs.get(&r.product_id).cloned();
                r
            })
            .collect();
        Ok(Page { items, total })
    }

    async fn summarize(&self, reseller_id: Id) -> Result<SettingSummary> {
        let rows = settings::Entity::find()
            .filter(settings::Column::ResellerId.eq(reseller_id))
            .filter(settings::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?;
        let configured: HashSet<Id> = rows.iter().map(|r| r.product_id).collect();
        let hidden: HashSet<Id> = rows
            .iter()
            .filter(|r| !r.is_listed)
            .map(|r| r.product_id)
            .collect();
        Ok(SettingSummary {
            configured_products: configured.len() as i64,
            hidden_products: hidden.len() as i64,
            sku_overrides: rows.iter().filter(|r| r.sku_id > 0).count() as i64,
            pricing_overrides: rows
                .iter()
                .filter(|r| r.pricing_mode != PricingMode::Inherit.as_str())
                .count() as i64,
        })
    }
}

#[async_trait]
impl PricingRepo for SeaResellerStore {
    async fn settings_for_pricing(
        &self,
        reseller_id: Id,
        product_ids: &[Id],
        sku_ids: &[Id],
    ) -> Result<Vec<ProductSetting>> {
        let product_ids: Vec<Id> = product_ids
            .iter()
            .copied()
            .filter(|i| *i > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if reseller_id == 0 || product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let sku_ids: Vec<Id> = sku_ids
            .iter()
            .copied()
            .filter(|i| *i > 0)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let sku_cond = if sku_ids.is_empty() {
            Condition::all().add(settings::Column::SkuId.eq(0))
        } else {
            Condition::any()
                .add(settings::Column::SkuId.eq(0))
                .add(settings::Column::SkuId.is_in(sku_ids))
        };
        Ok(settings::Entity::find()
            .filter(settings::Column::ResellerId.eq(reseller_id))
            .filter(settings::Column::ProductId.is_in(product_ids))
            .filter(settings::Column::DeletedAt.is_null())
            .filter(sku_cond)
            .order_by_asc(settings::Column::ProductId)
            .order_by_asc(settings::Column::SkuId)
            .order_by_asc(settings::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(setting_model)
            .collect())
    }

    async fn hidden_product_ids(&self, reseller_id: Id) -> Result<Vec<Id>> {
        let hidden_rules = settings::Entity::find()
            .filter(settings::Column::ResellerId.eq(reseller_id))
            .filter(settings::Column::IsListed.eq(false))
            .filter(settings::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?;
        let mut hidden: HashSet<Id> = hidden_rules
            .iter()
            .filter(|r| r.sku_id == 0 && r.product_id > 0)
            .map(|r| r.product_id)
            .collect();
        let hidden_skus: HashSet<(Id, Id)> = hidden_rules
            .iter()
            .filter(|r| r.sku_id > 0)
            .map(|r| (r.product_id, r.sku_id))
            .collect();
        let candidates: Vec<Id> = hidden_skus
            .iter()
            .map(|(p, _)| *p)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        if !candidates.is_empty() {
            let mut active: HashMap<Id, Vec<Id>> = HashMap::new();
            for s in skus::Entity::find()
                .filter(skus::Column::ProductId.is_in(candidates))
                .filter(skus::Column::IsActive.eq(true))
                .filter(skus::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?
            {
                active.entry(s.product_id).or_default().push(s.id);
            }
            for (product_id, sku_ids) in active {
                if sku_ids
                    .iter()
                    .all(|sid| hidden_skus.contains(&(product_id, *sid)))
                {
                    hidden.insert(product_id);
                }
            }
        }
        let mut ids: Vec<Id> = hidden.into_iter().collect();
        ids.sort_unstable();
        Ok(ids)
    }

    async fn is_related_account(&self, reseller_id: Id, user_id: Id) -> Result<bool> {
        if reseller_id == 0 || user_id == 0 {
            return Ok(false);
        }
        let n = related::Entity::find()
            .filter(related::Column::ResellerId.eq(reseller_id))
            .filter(related::Column::UserId.eq(user_id))
            .filter(related::Column::Status.eq(zs_domain::reseller::RELATED_ACCOUNT_ACTIVE))
            .filter(related::Column::DeletedAt.is_null())
            .count(&self.db)
            .await
            .dom()?;
        Ok(n > 0)
    }
}
