//! Product categories (two levels: root categories and their children).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::i18n::LocalizedText;

use crate::{Id, Result};

/// A product category as returned by the admin API.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Category {
    pub id: Id,
    /// Parent category id; `0` for a root category.
    pub parent_id: Id,
    pub slug: String,
    pub name: LocalizedText,
    pub icon: String,
    pub sort_order: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

impl Category {
    pub fn is_root(&self) -> bool {
        self.parent_id == 0
    }
}

/// Fields written on create/update.
#[derive(Debug, Clone)]
pub struct CategoryInput {
    pub parent_id: Id,
    pub slug: String,
    pub name: LocalizedText,
    pub icon: String,
    pub sort_order: i32,
}

/// Persistence port for categories. Soft-deleted rows are never returned.
#[async_trait]
pub trait CategoryRepo: Send + Sync {
    /// All categories ordered by `sort_order DESC, id ASC`.
    async fn list(&self, active_only: bool) -> Result<Vec<Category>>;
    async fn get(&self, id: Id) -> Result<Option<Category>>;
    async fn count_by_slug(&self, slug: &str, exclude_id: Option<Id>) -> Result<u64>;
    async fn count_children(&self, id: Id) -> Result<u64>;
    async fn count_products(&self, id: Id) -> Result<u64>;
    async fn create(&self, input: &CategoryInput) -> Result<Category>;
    async fn update(&self, id: Id, input: &CategoryInput) -> Result<()>;
    async fn set_active(&self, id: Id, active: bool) -> Result<()>;
    async fn delete(&self, id: Id) -> Result<()>;
}
