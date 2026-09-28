//! [`PostRepo`] and [`PostCategoryRepo`] backed by `posts`, `post_products`,
//! `post_categories` and (read-only) `products`.

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde_json::Value;
use zs_domain::content::post::{
    Post, PostCategory, PostCategoryInput, PostCategoryRepo, PostOrder, PostQuery, PostRepo,
    RelatedPost, RelatedProduct,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

use super::search::localized_like;
use crate::db::entity::{post_categories, post_products, posts, products};
use crate::db::repo::support::{DbResultExt, from_json, now};

/// SeaORM implementation of [`PostRepo`].
#[derive(Debug, Clone)]
pub struct SeaPostRepo {
    db: DatabaseConnection,
}

impl SeaPostRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn json_or_null(v: Option<sea_orm::JsonValue>) -> Value {
    v.unwrap_or(Value::Null)
}

fn null_to_none(v: &Value) -> Option<sea_orm::JsonValue> {
    (!v.is_null()).then(|| v.clone())
}

fn to_post(m: posts::Model) -> Post {
    Post {
        id: m.id,
        slug: m.slug,
        kind: m.type_,
        title: json_or_null(m.title_json),
        summary: json_or_null(m.summary_json),
        content: json_or_null(m.content_json),
        thumbnail: m.thumbnail,
        category_id: m.category_id,
        is_published: m.is_published,
        published_at: m.published_at,
        created_at: m.created_at,
    }
}

fn alive() -> sea_orm::Condition {
    sea_orm::Condition::all().add(posts::Column::DeletedAt.is_null())
}

/// Replaces the ordered product relations of a post (inside the caller's transaction).
async fn set_related<C: ConnectionTrait>(conn: &C, post_id: Id, product_ids: &[Id]) -> Result<()> {
    post_products::Entity::delete_many()
        .filter(post_products::Column::PostId.eq(post_id))
        .exec(conn)
        .await
        .dom()?;
    let mut seen = Vec::new();
    let mut rows = Vec::new();
    for (index, product_id) in product_ids.iter().enumerate() {
        if *product_id <= 0 || seen.contains(product_id) {
            continue;
        }
        seen.push(*product_id);
        rows.push(post_products::ActiveModel {
            post_id: Set(post_id),
            product_id: Set(*product_id),
            sort: Set(i32::try_from(index).unwrap_or(i32::MAX)),
            created_at: Set(now()),
            ..Default::default()
        });
    }
    if !rows.is_empty() {
        post_products::Entity::insert_many(rows)
            .exec(conn)
            .await
            .dom()?;
    }
    Ok(())
}

#[async_trait]
impl PostRepo for SeaPostRepo {
    async fn list(&self, query: &PostQuery) -> Result<Page<Post>> {
        let mut q = posts::Entity::find().filter(alive());
        if query.only_published {
            q = q.filter(posts::Column::IsPublished.eq(true));
        }
        if !query.kind.is_empty() {
            q = q.filter(posts::Column::Type.eq(query.kind.clone()));
        }
        if !query.search.trim().is_empty() {
            q = q.filter(localized_like(
                self.db.get_database_backend(),
                &["slug"],
                &["title_json"],
                &query.search,
            ));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        q = match query.order {
            PostOrder::CreatedDesc => q.order_by_desc(posts::Column::CreatedAt),
            PostOrder::PublishedDesc => q
                .order_by_desc(posts::Column::PublishedAt)
                .order_by_desc(posts::Column::CreatedAt),
        };
        let rows = q
            .order_by_desc(posts::Column::Id)
            .offset(query.page.offset())
            .limit(query.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_post).collect(),
            total,
        })
    }

    async fn get(&self, id: Id) -> Result<Option<Post>> {
        let row = posts::Entity::find_by_id(id)
            .filter(alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_post))
    }

    async fn get_by_slug(&self, slug: &str, only_published: bool) -> Result<Option<Post>> {
        let mut q = posts::Entity::find()
            .filter(alive())
            .filter(posts::Column::Slug.eq(slug));
        if only_published {
            q = q.filter(posts::Column::IsPublished.eq(true));
        }
        Ok(q.one(&self.db).await.dom()?.map(to_post))
    }

    async fn count_by_slug(&self, slug: &str, exclude_id: Option<Id>) -> Result<u64> {
        let mut q = posts::Entity::find()
            .filter(alive())
            .filter(posts::Column::Slug.eq(slug));
        if let Some(id) = exclude_id {
            q = q.filter(posts::Column::Id.ne(id));
        }
        q.count(&self.db).await.dom()
    }

    async fn create(&self, post: &Post, product_ids: Option<&[Id]>) -> Result<Post> {
        let txn = self.db.begin().await.dom()?;
        let model = posts::ActiveModel {
            slug: Set(post.slug.clone()),
            type_: Set(post.kind.clone()),
            title_json: Set(null_to_none(&post.title)),
            summary_json: Set(null_to_none(&post.summary)),
            content_json: Set(null_to_none(&post.content)),
            thumbnail: Set(post.thumbnail.clone()),
            category_id: Set(post.category_id),
            is_published: Set(post.is_published),
            published_at: Set(post.published_at),
            created_at: Set(post.created_at),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        if let Some(ids) = product_ids {
            set_related(&txn, model.id, ids).await?;
        }
        txn.commit().await.dom()?;
        Ok(to_post(model))
    }

    async fn update(&self, post: &Post, product_ids: Option<&[Id]>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        posts::ActiveModel {
            id: Set(post.id),
            slug: Set(post.slug.clone()),
            type_: Set(post.kind.clone()),
            title_json: Set(null_to_none(&post.title)),
            summary_json: Set(null_to_none(&post.summary)),
            content_json: Set(null_to_none(&post.content)),
            thumbnail: Set(post.thumbnail.clone()),
            category_id: Set(post.category_id),
            is_published: Set(post.is_published),
            published_at: Set(post.published_at),
            ..Default::default()
        }
        .update(&txn)
        .await
        .dom()?;
        if let Some(ids) = product_ids {
            set_related(&txn, post.id, ids).await?;
        }
        txn.commit().await.dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id) -> Result<()> {
        posts::Entity::update_many()
            .col_expr(posts::Column::DeletedAt, Expr::value(now()))
            .filter(posts::Column::Id.eq(id))
            .filter(alive())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn related_products(&self, post_id: Id) -> Result<Vec<RelatedProduct>> {
        let relations = post_products::Entity::find()
            .filter(post_products::Column::PostId.eq(post_id))
            .order_by_asc(post_products::Column::Sort)
            .order_by_asc(post_products::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = relations.iter().map(|r| r.product_id).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut by_id: HashMap<Id, products::Model> = products::Entity::find()
            .filter(products::Column::Id.is_in(ids.clone()))
            .filter(products::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        Ok(ids
            .iter()
            .filter_map(|id| by_id.remove(id))
            .map(|p| RelatedProduct {
                id: p.id,
                slug: p.slug,
                title: json_or_null(p.title_json),
                price_amount: Amount::new(p.price_amount),
                images: from_json(p.images),
                is_active: p.is_active,
            })
            .collect())
    }

    async fn posts_for_product(
        &self,
        product_id: Id,
        kind: &str,
        only_published: bool,
        limit: u64,
    ) -> Result<Vec<RelatedPost>> {
        let relations = post_products::Entity::find()
            .filter(post_products::Column::ProductId.eq(product_id))
            .order_by_asc(post_products::Column::Sort)
            .order_by_asc(post_products::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = relations.iter().map(|r| r.post_id).collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut q = posts::Entity::find()
            .filter(alive())
            .filter(posts::Column::Id.is_in(ids.clone()));
        if !kind.is_empty() {
            q = q.filter(posts::Column::Type.eq(kind));
        }
        if only_published {
            q = q.filter(posts::Column::IsPublished.eq(true));
        }
        let mut by_id: HashMap<Id, posts::Model> = q
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|p| (p.id, p))
            .collect();
        let take = if limit == 0 {
            usize::MAX
        } else {
            usize::try_from(limit).unwrap_or(usize::MAX)
        };
        Ok(ids
            .iter()
            .filter_map(|id| by_id.remove(id))
            .take(take)
            .map(|p| RelatedPost {
                id: p.id,
                slug: p.slug,
                kind: p.type_,
                title: json_or_null(p.title_json),
                summary: json_or_null(p.summary_json),
                thumbnail: p.thumbnail,
                published_at: p.published_at,
            })
            .collect())
    }
}

/// SeaORM implementation of [`PostCategoryRepo`].
#[derive(Debug, Clone)]
pub struct SeaPostCategoryRepo {
    db: DatabaseConnection,
}

impl SeaPostCategoryRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_category(m: post_categories::Model) -> PostCategory {
    PostCategory {
        id: m.id,
        parent_id: m.parent_id,
        slug: m.slug,
        name: json_or_null(m.name_json),
        icon: m.icon,
        is_active: m.is_active,
        sort_order: m.sort_order,
        created_at: m.created_at,
        updated_at: m.updated_at,
        children: Vec::new(),
    }
}

fn category_alive() -> sea_orm::Condition {
    sea_orm::Condition::all().add(post_categories::Column::DeletedAt.is_null())
}

#[async_trait]
impl PostCategoryRepo for SeaPostCategoryRepo {
    async fn list(&self, parent_id: Option<Id>, active_only: bool) -> Result<Vec<PostCategory>> {
        let mut q = post_categories::Entity::find().filter(category_alive());
        if let Some(pid) = parent_id {
            q = q.filter(post_categories::Column::ParentId.eq(pid));
        }
        if active_only {
            q = q.filter(post_categories::Column::IsActive.eq(true));
        }
        let rows = q
            .order_by_asc(post_categories::Column::SortOrder)
            .order_by_asc(post_categories::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().map(to_category).collect())
    }

    async fn get(&self, id: Id) -> Result<Option<PostCategory>> {
        let row = post_categories::Entity::find_by_id(id)
            .filter(category_alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_category))
    }

    async fn create(&self, input: &PostCategoryInput) -> Result<PostCategory> {
        let ts = now();
        let model = post_categories::ActiveModel {
            parent_id: Set(input.parent_id),
            slug: Set(input.slug.clone()),
            name_json: Set(null_to_none(&input.name)),
            icon: Set(input.icon.clone()),
            is_active: Set(true),
            sort_order: Set(input.sort_order),
            created_at: Set(ts),
            updated_at: Set(ts),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_category(model))
    }

    async fn update(&self, id: Id, input: &PostCategoryInput) -> Result<()> {
        post_categories::ActiveModel {
            id: Set(id),
            parent_id: Set(input.parent_id),
            slug: Set(input.slug.clone()),
            name_json: Set(null_to_none(&input.name)),
            icon: Set(input.icon.clone()),
            sort_order: Set(input.sort_order),
            updated_at: Set(now()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn set_active(&self, id: Id, active: bool) -> Result<()> {
        post_categories::Entity::update_many()
            .col_expr(post_categories::Column::IsActive, Expr::value(active))
            .col_expr(post_categories::Column::UpdatedAt, Expr::value(now()))
            .filter(post_categories::Column::Id.eq(id))
            .filter(category_alive())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id) -> Result<()> {
        post_categories::Entity::update_many()
            .col_expr(post_categories::Column::DeletedAt, Expr::value(now()))
            .filter(post_categories::Column::Id.eq(id))
            .filter(category_alive())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn count_by_slug(&self, slug: &str, exclude_id: Option<Id>) -> Result<u64> {
        let mut q = post_categories::Entity::find()
            .filter(category_alive())
            .filter(post_categories::Column::Slug.eq(slug));
        if let Some(id) = exclude_id {
            q = q.filter(post_categories::Column::Id.ne(id));
        }
        q.count(&self.db).await.dom()
    }

    async fn count_children(&self, id: Id) -> Result<u64> {
        post_categories::Entity::find()
            .filter(category_alive())
            .filter(post_categories::Column::ParentId.eq(id))
            .count(&self.db)
            .await
            .dom()
    }

    async fn count_posts(&self, id: Id) -> Result<u64> {
        posts::Entity::find()
            .filter(alive())
            .filter(posts::Column::CategoryId.eq(id))
            .count(&self.db)
            .await
            .dom()
    }
}
