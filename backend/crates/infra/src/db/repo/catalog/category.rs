//! [`CategoryRepo`] backed by the `categories` table.

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, Set,
};
use zs_domain::catalog::category::{Category, CategoryInput, CategoryRepo};
use zs_domain::{Id, Result};

use crate::db::entity::{categories, products};
use crate::db::repo::support::{DbResultExt, from_json, now, to_json};

/// SeaORM implementation of [`CategoryRepo`].
#[derive(Debug, Clone)]
pub struct SeaCategoryRepo {
    db: DatabaseConnection,
}

impl SeaCategoryRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: categories::Model) -> Category {
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

fn alive() -> sea_orm::Condition {
    sea_orm::Condition::all().add(categories::Column::DeletedAt.is_null())
}

#[async_trait]
impl CategoryRepo for SeaCategoryRepo {
    async fn list(&self, active_only: bool) -> Result<Vec<Category>> {
        let mut q = categories::Entity::find().filter(alive());
        if active_only {
            q = q.filter(categories::Column::IsActive.eq(true));
        }
        let rows = q
            .order_by_desc(categories::Column::SortOrder)
            .order_by_asc(categories::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_domain).collect())
    }

    async fn get(&self, id: Id) -> Result<Option<Category>> {
        let row = categories::Entity::find_by_id(id)
            .filter(alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn count_by_slug(&self, slug: &str, exclude_id: Option<Id>) -> Result<u64> {
        let mut q = categories::Entity::find()
            .filter(alive())
            .filter(categories::Column::Slug.eq(slug));
        if let Some(id) = exclude_id {
            q = q.filter(categories::Column::Id.ne(id));
        }
        q.count(&self.db).await.dom()
    }

    async fn count_children(&self, id: Id) -> Result<u64> {
        categories::Entity::find()
            .filter(alive())
            .filter(categories::Column::ParentId.eq(id))
            .count(&self.db)
            .await
            .dom()
    }

    async fn count_products(&self, id: Id) -> Result<u64> {
        products::Entity::find()
            .filter(products::Column::DeletedAt.is_null())
            .filter(products::Column::CategoryId.eq(id))
            .count(&self.db)
            .await
            .dom()
    }

    async fn create(&self, input: &CategoryInput) -> Result<Category> {
        let model = categories::ActiveModel {
            parent_id: Set(input.parent_id),
            slug: Set(input.slug.clone()),
            name_json: Set(to_json(&input.name)?),
            icon: Set(input.icon.clone()),
            sort_order: Set(input.sort_order),
            is_active: Set(true),
            created_at: Set(now()),
            ..Default::default()
        };
        Ok(to_domain(model.insert(&self.db).await.dom()?))
    }

    async fn update(&self, id: Id, input: &CategoryInput) -> Result<()> {
        categories::ActiveModel {
            id: Set(id),
            parent_id: Set(input.parent_id),
            slug: Set(input.slug.clone()),
            name_json: Set(to_json(&input.name)?),
            icon: Set(input.icon.clone()),
            sort_order: Set(input.sort_order),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn set_active(&self, id: Id, active: bool) -> Result<()> {
        categories::Entity::update_many()
            .col_expr(categories::Column::IsActive, Expr::value(active))
            .filter(categories::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id) -> Result<()> {
        categories::Entity::update_many()
            .col_expr(categories::Column::DeletedAt, Expr::value(now()))
            .filter(categories::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
