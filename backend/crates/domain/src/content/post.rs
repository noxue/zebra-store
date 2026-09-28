//! Blog posts / notices, their categories and related products.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

pub const POST_TYPE_BLOG: &str = "blog";
pub const POST_TYPE_NOTICE: &str = "notice";

/// A post as returned by the admin API (`models.Post`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Post {
    pub id: Id,
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: Value,
    pub summary: Value,
    pub content: Value,
    pub thumbnail: String,
    pub category_id: Option<Id>,
    pub is_published: bool,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// Stable orderings accepted by [`PostRepo::list`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostOrder {
    CreatedDesc,
    PublishedDesc,
}

/// Post list filter.
#[derive(Debug, Clone)]
pub struct PostQuery {
    pub page: PageRequest,
    pub kind: String,
    pub search: String,
    pub only_published: bool,
    pub order: PostOrder,
}

/// Minimal product projection shown under a blog post.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatedProduct {
    pub id: Id,
    pub slug: String,
    pub title: Value,
    pub price_amount: Amount,
    pub images: Vec<String>,
    pub is_active: bool,
}

/// Minimal post projection shown on a product page (for the catalog group).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RelatedPost {
    pub id: Id,
    pub slug: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: Value,
    pub summary: Value,
    pub thumbnail: String,
    pub published_at: Option<DateTime<Utc>>,
}

/// Persistence port for posts and their product relations.
///
/// `create`/`update` write the post and (when `product_ids` is `Some`) replace its
/// ordered product relations in one transaction.
#[async_trait]
pub trait PostRepo: Send + Sync {
    async fn list(&self, query: &PostQuery) -> Result<Page<Post>>;
    async fn get(&self, id: Id) -> Result<Option<Post>>;
    async fn get_by_slug(&self, slug: &str, only_published: bool) -> Result<Option<Post>>;
    async fn count_by_slug(&self, slug: &str, exclude_id: Option<Id>) -> Result<u64>;
    async fn create(&self, post: &Post, product_ids: Option<&[Id]>) -> Result<Post>;
    async fn update(&self, post: &Post, product_ids: Option<&[Id]>) -> Result<()>;
    async fn delete(&self, id: Id) -> Result<()>;
    async fn related_products(&self, post_id: Id) -> Result<Vec<RelatedProduct>>;
    async fn posts_for_product(
        &self,
        product_id: Id,
        kind: &str,
        only_published: bool,
        limit: u64,
    ) -> Result<Vec<RelatedPost>>;
}

/// A two-level post category (`models.PostCategory`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PostCategory {
    pub id: Id,
    pub parent_id: Option<Id>,
    pub slug: String,
    pub name: Value,
    pub icon: String,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<PostCategory>,
}

impl PostCategory {
    pub fn is_root(&self) -> bool {
        self.parent_id.is_none_or(|p| p == 0)
    }
}

/// Fields written on category create/update.
#[derive(Debug, Clone)]
pub struct PostCategoryInput {
    pub name: Value,
    pub slug: String,
    pub parent_id: Option<Id>,
    pub sort_order: i32,
    pub icon: String,
}

/// Persistence port for post categories (ordered `sort_order ASC, id ASC`).
#[async_trait]
pub trait PostCategoryRepo: Send + Sync {
    async fn list(&self, parent_id: Option<Id>, active_only: bool) -> Result<Vec<PostCategory>>;
    async fn get(&self, id: Id) -> Result<Option<PostCategory>>;
    async fn create(&self, input: &PostCategoryInput) -> Result<PostCategory>;
    async fn update(&self, id: Id, input: &PostCategoryInput) -> Result<()>;
    async fn set_active(&self, id: Id, active: bool) -> Result<()>;
    async fn delete(&self, id: Id) -> Result<()>;
    async fn count_by_slug(&self, slug: &str, exclude_id: Option<Id>) -> Result<u64>;
    async fn count_children(&self, id: Id) -> Result<u64>;
    async fn count_posts(&self, id: Id) -> Result<u64>;
}

/// Builds the two-level tree (roots with children) from a flat ordered list.
pub fn category_tree(all: Vec<PostCategory>) -> Vec<PostCategory> {
    let (roots, children): (Vec<_>, Vec<_>) = all.into_iter().partition(PostCategory::is_root);
    roots
        .into_iter()
        .map(|mut root| {
            root.children = children
                .iter()
                .filter(|c| c.parent_id == Some(root.id))
                .cloned()
                .collect();
            root
        })
        .collect()
}
