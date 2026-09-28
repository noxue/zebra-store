//! Category use cases.

use std::sync::Arc;

use zs_domain::catalog::category::{Category, CategoryInput, CategoryRepo};
use zs_domain::{Error, Id, Result};

const NOT_FOUND: &str = "error.category_not_found";
const PARENT_INVALID: &str = "error.category_parent_invalid";
const IN_USE: &str = "error.category_in_use";

/// Category queries and admin commands.
#[derive(Clone)]
pub struct CategoryService {
    repo: Arc<dyn CategoryRepo>,
}

impl std::fmt::Debug for CategoryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CategoryService")
    }
}

impl CategoryService {
    pub fn new(repo: Arc<dyn CategoryRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self) -> Result<Vec<Category>> {
        self.repo
            .list(false)
            .await
            .map_err(|e| e.or_internal("error.category_fetch_failed"))
    }

    pub async fn list_active(&self) -> Result<Vec<Category>> {
        self.repo
            .list(true)
            .await
            .map_err(|e| e.or_internal("error.category_fetch_failed"))
    }

    pub async fn create(&self, input: CategoryInput) -> Result<Category> {
        self.validate_parent(None, input.parent_id).await?;
        if self.repo.count_by_slug(&input.slug, None).await? > 0 {
            return Err(Error::bad_request("error.slug_exists"));
        }
        self.repo
            .create(&input)
            .await
            .map_err(|e| e.or_internal("error.category_create_failed"))
    }

    pub async fn update(&self, id: Id, input: CategoryInput) -> Result<Category> {
        let current = self.get(id).await?;
        self.validate_parent(Some(&current), input.parent_id)
            .await?;
        if self.repo.count_by_slug(&input.slug, Some(id)).await? > 0 {
            return Err(Error::bad_request("error.slug_used"));
        }
        self.repo
            .update(id, &input)
            .await
            .map_err(|e| e.or_internal("error.category_update_failed"))?;
        Ok(Category {
            parent_id: input.parent_id,
            slug: input.slug,
            name: input.name,
            icon: input.icon,
            sort_order: input.sort_order,
            ..current
        })
    }

    pub async fn set_active(&self, id: Id, active: bool) -> Result<Category> {
        let mut category = self.get(id).await?;
        if category.is_active != active {
            self.repo
                .set_active(id, active)
                .await
                .map_err(|e| e.or_internal("error.category_update_failed"))?;
            category.is_active = active;
        }
        Ok(category)
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        self.get(id).await?;
        if self.repo.count_children(id).await? > 0 || self.repo.count_products(id).await? > 0 {
            return Err(Error::bad_request(IN_USE));
        }
        self.repo
            .delete(id)
            .await
            .map_err(|e| e.or_internal("error.category_delete_failed"))
    }

    async fn get(&self, id: Id) -> Result<Category> {
        self.repo
            .get(id)
            .await?
            .ok_or_else(|| Error::not_found(NOT_FOUND))
    }

    /// A parent must be an existing root category; a root with children cannot become a child.
    async fn validate_parent(&self, current: Option<&Category>, parent_id: Id) -> Result<()> {
        if parent_id == 0 {
            return Ok(());
        }
        if current.is_some_and(|c| c.id == parent_id) {
            return Err(Error::bad_request(PARENT_INVALID));
        }
        match self.repo.get(parent_id).await? {
            Some(parent) if parent.is_root() => {}
            _ => return Err(Error::bad_request(PARENT_INVALID)),
        }
        if let Some(current) = current
            && current.is_root()
            && self.repo.count_children(current.id).await? > 0
        {
            return Err(Error::bad_request(PARENT_INVALID));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::Mutex;
    use zs_shared::i18n::LocalizedText;

    #[derive(Default)]
    struct MemRepo {
        rows: Mutex<Vec<Category>>,
        products: Mutex<Vec<Id>>,
    }

    #[async_trait]
    impl CategoryRepo for MemRepo {
        async fn list(&self, active_only: bool) -> Result<Vec<Category>> {
            let rows = self.rows.lock().unwrap();
            Ok(rows
                .iter()
                .filter(|c| !active_only || c.is_active)
                .cloned()
                .collect())
        }
        async fn get(&self, id: Id) -> Result<Option<Category>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        async fn count_by_slug(&self, slug: &str, exclude: Option<Id>) -> Result<u64> {
            let rows = self.rows.lock().unwrap();
            Ok(rows
                .iter()
                .filter(|c| c.slug == slug && Some(c.id) != exclude)
                .count() as u64)
        }
        async fn count_children(&self, id: Id) -> Result<u64> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.parent_id == id)
                .count() as u64)
        }
        async fn count_products(&self, id: Id) -> Result<u64> {
            Ok(self
                .products
                .lock()
                .unwrap()
                .iter()
                .filter(|p| **p == id)
                .count() as u64)
        }
        async fn create(&self, input: &CategoryInput) -> Result<Category> {
            let mut rows = self.rows.lock().unwrap();
            let c = Category {
                id: rows.len() as Id + 1,
                parent_id: input.parent_id,
                slug: input.slug.clone(),
                name: input.name.clone(),
                icon: input.icon.clone(),
                sort_order: input.sort_order,
                is_active: true,
                created_at: Utc::now(),
            };
            rows.push(c.clone());
            Ok(c)
        }
        async fn update(&self, id: Id, input: &CategoryInput) -> Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(c) = rows.iter_mut().find(|c| c.id == id) {
                c.parent_id = input.parent_id;
                c.slug = input.slug.clone();
            }
            Ok(())
        }
        async fn set_active(&self, id: Id, active: bool) -> Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(c) = rows.iter_mut().find(|c| c.id == id) {
                c.is_active = active;
            }
            Ok(())
        }
        async fn delete(&self, id: Id) -> Result<()> {
            self.rows.lock().unwrap().retain(|c| c.id != id);
            Ok(())
        }
    }

    fn input(slug: &str, parent_id: Id) -> CategoryInput {
        CategoryInput {
            parent_id,
            slug: slug.into(),
            name: LocalizedText::single(slug),
            icon: String::new(),
            sort_order: 0,
        }
    }

    fn service() -> (CategoryService, Arc<MemRepo>) {
        let repo = Arc::new(MemRepo::default());
        (CategoryService::new(repo.clone()), repo)
    }

    #[tokio::test]
    async fn rejects_duplicate_slug_with_operation_specific_keys() {
        let (svc, _) = service();
        let a = svc.create(input("a", 0)).await.unwrap();
        svc.create(input("b", 0)).await.unwrap();
        assert_eq!(
            svc.create(input("a", 0)).await.unwrap_err().key(),
            "error.slug_exists"
        );
        assert_eq!(
            svc.update(a.id, input("b", 0)).await.unwrap_err().key(),
            "error.slug_used"
        );
    }

    #[tokio::test]
    async fn only_two_levels_allowed() {
        let (svc, _) = service();
        let root = svc.create(input("root", 0)).await.unwrap();
        let child = svc.create(input("child", root.id)).await.unwrap();
        let err = svc.create(input("grandchild", child.id)).await.unwrap_err();
        assert_eq!(err.key(), PARENT_INVALID);
        // a root that has children cannot be moved under another root
        let other = svc.create(input("other", 0)).await.unwrap();
        let err = svc
            .update(root.id, input("root", other.id))
            .await
            .unwrap_err();
        assert_eq!(err.key(), PARENT_INVALID);
    }

    #[tokio::test]
    async fn delete_blocked_by_children_or_products() {
        let (svc, repo) = service();
        let root = svc.create(input("root", 0)).await.unwrap();
        svc.create(input("child", root.id)).await.unwrap();
        assert_eq!(svc.delete(root.id).await.unwrap_err().key(), IN_USE);
        let lone = svc.create(input("lone", 0)).await.unwrap();
        repo.products.lock().unwrap().push(lone.id);
        assert_eq!(svc.delete(lone.id).await.unwrap_err().key(), IN_USE);
        assert!(svc.delete(999).await.unwrap_err().is_not_found());
    }
}
