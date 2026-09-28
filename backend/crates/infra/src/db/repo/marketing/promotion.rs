//! [`PromotionRepo`] backed by `promotions`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set,
};
use zs_domain::marketing::promotion::{
    Promotion, PromotionFields, PromotionFilter, PromotionRepo, SCOPE_PRODUCT,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::promotions;
use crate::db::repo::catalog::sql::ilike_sql;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`PromotionRepo`].
#[derive(Debug, Clone)]
pub struct SeaPromotionRepo {
    db: DatabaseConnection,
}

impl SeaPromotionRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: promotions::Model) -> Promotion {
    Promotion {
        id: m.id,
        name: m.name,
        scope_type: m.scope_type,
        scope_ref_id: m.scope_ref_id,
        kind: m.type_,
        value: Amount::new(m.value),
        min_amount: Amount::new(m.min_amount),
        starts_at: m.starts_at,
        ends_at: m.ends_at,
        is_active: m.is_active,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn apply_fields(model: &mut promotions::ActiveModel, f: &PromotionFields, now: DateTime<Utc>) {
    model.name = Set(f.name.clone());
    model.scope_type = Set(SCOPE_PRODUCT.to_owned());
    model.scope_ref_id = Set(f.scope_ref_id);
    model.type_ = Set(f.kind.as_str().to_owned());
    model.value = Set(f.value.decimal());
    model.min_amount = Set(f.min_amount.decimal());
    model.starts_at = Set(f.starts_at);
    model.ends_at = Set(f.ends_at);
    model.is_active = Set(f.is_active);
    model.updated_at = Set(now);
}

#[async_trait]
impl PromotionRepo for SeaPromotionRepo {
    async fn get(&self, id: Id) -> Result<Option<Promotion>> {
        Ok(promotions::Entity::find_by_id(id)
            .filter(promotions::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn list_effective(&self, product_id: Id, now: DateTime<Utc>) -> Result<Vec<Promotion>> {
        Ok(promotions::Entity::find()
            .filter(promotions::Column::DeletedAt.is_null())
            .filter(promotions::Column::ScopeType.eq(SCOPE_PRODUCT))
            .filter(promotions::Column::ScopeRefId.eq(product_id))
            .filter(promotions::Column::IsActive.eq(true))
            .filter(
                Condition::any()
                    .add(promotions::Column::StartsAt.is_null())
                    .add(promotions::Column::StartsAt.lte(now)),
            )
            .filter(
                Condition::any()
                    .add(promotions::Column::EndsAt.is_null())
                    .add(promotions::Column::EndsAt.gte(now)),
            )
            .order_by_asc(promotions::Column::MinAmount)
            .order_by_asc(promotions::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn list(&self, filter: &PromotionFilter, page: PageRequest) -> Result<Page<Promotion>> {
        let mut cond = Condition::all().add(promotions::Column::DeletedAt.is_null());
        if filter.id > 0 {
            cond = cond.add(promotions::Column::Id.eq(filter.id));
        }
        let name = filter.name.trim();
        if !name.is_empty() {
            cond = cond.add(ilike_sql("promotions.name", name));
        }
        if filter.scope_ref_id > 0 {
            cond = cond.add(promotions::Column::ScopeRefId.eq(filter.scope_ref_id));
        }
        if let Some(active) = filter.is_active {
            cond = cond.add(promotions::Column::IsActive.eq(active));
        }
        let q = promotions::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let items = q
            .order_by_desc(promotions::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(to_domain)
            .collect();
        Ok(Page { items, total })
    }

    async fn create(&self, fields: &PromotionFields, now: DateTime<Utc>) -> Result<Promotion> {
        let mut model = promotions::ActiveModel {
            created_at: Set(now),
            ..Default::default()
        };
        apply_fields(&mut model, fields, now);
        Ok(to_domain(model.insert(&self.db).await.dom()?))
    }

    async fn update(&self, id: Id, fields: &PromotionFields, now: DateTime<Utc>) -> Result<()> {
        let mut model = promotions::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        apply_fields(&mut model, fields, now);
        model.update(&self.db).await.dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()> {
        promotions::Entity::update_many()
            .col_expr(promotions::Column::DeletedAt, Expr::value(now))
            .filter(promotions::Column::Id.eq(id))
            .filter(promotions::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
