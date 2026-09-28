//! Post and post category use cases (port of `post_service.go` / `post_category_service.go`).

use std::sync::Arc;

use serde_json::Value;
use zs_domain::content::post::{
    POST_TYPE_BLOG, POST_TYPE_NOTICE, Post, PostCategory, PostCategoryInput, PostCategoryRepo,
    PostOrder, PostQuery, PostRepo, RelatedPost, RelatedProduct, category_tree,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

const POST_NOT_FOUND: &str = "error.post_not_found";
const POST_FETCH_FAILED: &str = "error.post_fetch_failed";
const CATEGORY_NOT_FOUND: &str = "error.post_category_not_found";
const CATEGORY_FETCH_FAILED: &str = "error.post_category_fetch_failed";
const CATEGORY_INVALID: &str = "error.post_category_invalid";
const PARENT_INVALID: &str = "error.category_parent_invalid";

/// Create/update payload of a post.
#[derive(Debug, Clone)]
pub struct PostInput {
    pub slug: String,
    pub kind: String,
    pub title: Value,
    pub summary: Value,
    pub content: Value,
    pub thumbnail: String,
    pub is_published: Option<bool>,
    pub product_ids: Option<Vec<Id>>,
    pub category_id: Option<Id>,
}

/// Post queries and admin commands.
#[derive(Clone)]
pub struct PostService {
    posts: Arc<dyn PostRepo>,
    categories: Arc<dyn PostCategoryRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for PostService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PostService")
    }
}

fn normalize_category(id: Option<Id>) -> Option<Id> {
    id.filter(|v| *v != 0)
}

impl PostService {
    pub fn new(
        posts: Arc<dyn PostRepo>,
        categories: Arc<dyn PostCategoryRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            posts,
            categories,
            clock,
        }
    }

    /// Published posts, newest publication first.
    pub async fn list_public(
        &self,
        kind: &str,
        search: &str,
        page: PageRequest,
    ) -> Result<Page<Post>> {
        self.posts
            .list(&PostQuery {
                page,
                kind: kind.to_owned(),
                search: search.to_owned(),
                only_published: true,
                order: PostOrder::PublishedDesc,
            })
            .await
            .map_err(|e| e.or_internal(POST_FETCH_FAILED))
    }

    pub async fn list_admin(
        &self,
        kind: &str,
        search: &str,
        page: PageRequest,
    ) -> Result<Page<Post>> {
        self.posts
            .list(&PostQuery {
                page,
                kind: kind.to_owned(),
                search: search.to_owned(),
                only_published: false,
                order: PostOrder::CreatedDesc,
            })
            .await
            .map_err(|e| e.or_internal(POST_FETCH_FAILED))
    }

    pub async fn public_by_slug(&self, slug: &str) -> Result<Post> {
        self.posts
            .get_by_slug(slug, true)
            .await
            .map_err(|e| e.or_internal(POST_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(POST_NOT_FOUND))
    }

    pub async fn create(&self, input: PostInput) -> Result<Post> {
        check_kind(&input.kind)?;
        let category_id = normalize_category(input.category_id);
        self.validate_category(&input.kind, category_id, None)
            .await?;
        if self.posts.count_by_slug(&input.slug, None).await? > 0 {
            return Err(Error::bad_request("error.slug_exists"));
        }
        let is_published = input.is_published.unwrap_or(false);
        let now = self.clock.now();
        let post = Post {
            id: 0,
            slug: input.slug,
            kind: input.kind,
            title: input.title,
            summary: input.summary,
            content: input.content,
            thumbnail: input.thumbnail,
            category_id,
            is_published,
            // published_at is set when created as published (original 8dd3ca52).
            published_at: is_published.then_some(now),
            created_at: now,
        };
        self.posts
            .create(&post, input.product_ids.as_deref())
            .await
            .map_err(|e| e.or_internal("error.post_create_failed"))
    }

    pub async fn update(&self, id: Id, input: PostInput) -> Result<Post> {
        check_kind(&input.kind)?;
        let mut post = self.get(id).await?;
        let category_id = normalize_category(input.category_id);
        self.validate_category(&input.kind, category_id, post.category_id)
            .await?;
        if self.posts.count_by_slug(&input.slug, Some(id)).await? > 0 {
            return Err(Error::bad_request("error.slug_used"));
        }
        post.slug = input.slug;
        post.kind = input.kind;
        post.title = input.title;
        post.summary = input.summary;
        post.content = input.content;
        post.thumbnail = input.thumbnail;
        post.category_id = category_id;
        if let Some(publish) = input.is_published {
            // First publication stamps published_at; re-publishing keeps it.
            if publish && !post.is_published && post.published_at.is_none() {
                post.published_at = Some(self.clock.now());
            }
            post.is_published = publish;
        }
        self.posts
            .update(&post, input.product_ids.as_deref())
            .await
            .map_err(|e| e.or_internal("error.post_update_failed"))?;
        Ok(post)
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        self.get(id).await?;
        self.posts
            .delete(id)
            .await
            .map_err(|e| e.or_internal("error.post_delete_failed"))
    }

    pub async fn related_products(&self, post_id: Id) -> Result<Vec<RelatedProduct>> {
        self.posts
            .related_products(post_id)
            .await
            .map_err(|e| e.or_internal(POST_FETCH_FAILED))
    }

    /// Published blog posts linked to a product (for product pages).
    pub async fn posts_for_product(&self, product_id: Id, limit: u64) -> Result<Vec<RelatedPost>> {
        self.posts
            .posts_for_product(product_id, POST_TYPE_BLOG, true, limit)
            .await
    }

    async fn get(&self, id: Id) -> Result<Post> {
        self.posts
            .get(id)
            .await
            .map_err(|e| e.or_internal(POST_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(POST_NOT_FOUND))
    }

    /// Notices take no category; blog posts need an existing leaf category that is
    /// active, unless the post already had it (a later-disabled category is kept).
    async fn validate_category(
        &self,
        kind: &str,
        id: Option<Id>,
        current: Option<Id>,
    ) -> Result<()> {
        if kind == POST_TYPE_NOTICE {
            if id.is_some() {
                return Err(Error::bad_request("error.post_notice_category_unsupported"));
            }
            return Ok(());
        }
        let Some(id) = id else { return Ok(()) };
        let category = self
            .categories
            .get(id)
            .await?
            .ok_or_else(|| Error::bad_request(CATEGORY_INVALID))?;
        let unchanged = current == Some(id);
        if !category.is_active && !unchanged {
            return Err(Error::bad_request(CATEGORY_INVALID));
        }
        if self.categories.count_children(id).await? > 0 && !unchanged {
            return Err(Error::bad_request(CATEGORY_INVALID));
        }
        Ok(())
    }
}

fn check_kind(kind: &str) -> Result<()> {
    if kind == POST_TYPE_BLOG || kind == POST_TYPE_NOTICE {
        Ok(())
    } else {
        Err(Error::bad_request("error.post_type_invalid"))
    }
}

/// Post category queries and admin commands.
#[derive(Clone)]
pub struct PostCategoryService {
    repo: Arc<dyn PostCategoryRepo>,
}

impl std::fmt::Debug for PostCategoryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PostCategoryService")
    }
}

impl PostCategoryService {
    pub fn new(repo: Arc<dyn PostCategoryRepo>) -> Self {
        Self { repo }
    }

    pub async fn list(&self, parent_id: Option<Id>) -> Result<Vec<PostCategory>> {
        self.repo
            .list(parent_id, false)
            .await
            .map_err(|e| e.or_internal(CATEGORY_FETCH_FAILED))
    }

    /// Every category as a two-level tree (inactive included).
    pub async fn tree(&self) -> Result<Vec<PostCategory>> {
        Ok(category_tree(self.list(None).await?))
    }

    pub async fn list_active(&self) -> Result<Vec<PostCategory>> {
        self.repo
            .list(None, true)
            .await
            .map_err(|e| e.or_internal(CATEGORY_FETCH_FAILED))
    }

    pub async fn create(&self, mut input: PostCategoryInput) -> Result<PostCategory> {
        input.parent_id = normalize_category(input.parent_id);
        self.validate_parent(None, input.parent_id).await?;
        if self.repo.count_by_slug(&input.slug, None).await? > 0 {
            return Err(Error::bad_request("error.slug_exists"));
        }
        self.repo
            .create(&input)
            .await
            .map_err(|e| e.or_internal("error.post_category_create_failed"))
    }

    /// Update; an absent name or empty slug keeps the current value.
    pub async fn update(
        &self,
        id: Id,
        name: Option<Value>,
        slug: String,
        parent_id: Option<Id>,
        sort_order: i32,
        icon: String,
    ) -> Result<PostCategory> {
        let mut category = self.get(id).await?;
        let parent_id = normalize_category(parent_id);
        self.validate_parent(Some(&category), parent_id).await?;
        if self.repo.count_by_slug(&slug, Some(id)).await? > 0 {
            return Err(Error::bad_request("error.slug_used"));
        }
        if let Some(name) = name {
            category.name = name;
        }
        if !slug.is_empty() {
            category.slug = slug;
        }
        category.parent_id = parent_id;
        category.sort_order = sort_order;
        category.icon = icon;
        let input = PostCategoryInput {
            name: category.name.clone(),
            slug: category.slug.clone(),
            parent_id,
            sort_order,
            icon: category.icon.clone(),
        };
        self.repo
            .update(id, &input)
            .await
            .map_err(|e| e.or_internal("error.post_category_update_failed"))?;
        Ok(category)
    }

    pub async fn set_active(&self, id: Id, active: bool) -> Result<PostCategory> {
        let mut category = self.get(id).await?;
        if category.is_active != active {
            self.repo
                .set_active(id, active)
                .await
                .map_err(|e| e.or_internal("error.post_category_update_failed"))?;
            category.is_active = active;
        }
        Ok(category)
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        self.get(id).await?;
        if self.repo.count_children(id).await? > 0 || self.repo.count_posts(id).await? > 0 {
            return Err(Error::bad_request("error.post_category_in_use"));
        }
        self.repo
            .delete(id)
            .await
            .map_err(|e| e.or_internal("error.post_category_delete_failed"))
    }

    async fn get(&self, id: Id) -> Result<PostCategory> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(CATEGORY_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(CATEGORY_NOT_FOUND))
    }

    /// The parent must be an existing root; a root with children cannot move under another.
    async fn validate_parent(
        &self,
        current: Option<&PostCategory>,
        parent_id: Option<Id>,
    ) -> Result<()> {
        let Some(parent_id) = parent_id else {
            return Ok(());
        };
        if current.is_some_and(|c| c.id == parent_id) {
            return Err(Error::bad_request(PARENT_INVALID));
        }
        match self.repo.get(parent_id).await? {
            Some(parent) if parent.is_root() => {}
            _ => return Err(Error::bad_request(PARENT_INVALID)),
        }
        if let Some(c) = current
            && c.is_root()
            && self.repo.count_children(c.id).await? > 0
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
    use chrono::{TimeZone, Utc};
    use serde_json::json;
    use std::sync::Mutex;
    use zs_domain::content::post::RelatedPost;
    use zs_shared::clock::FixedClock;

    #[derive(Default)]
    struct Mem {
        posts: Mutex<Vec<Post>>,
        cats: Mutex<Vec<PostCategory>>,
    }

    #[async_trait]
    impl PostRepo for Mem {
        async fn list(&self, _: &PostQuery) -> Result<Page<Post>> {
            let items = self.posts.lock().unwrap().clone();
            let total = items.len() as u64;
            Ok(Page { items, total })
        }
        async fn get(&self, id: Id) -> Result<Option<Post>> {
            Ok(self
                .posts
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.id == id)
                .cloned())
        }
        async fn get_by_slug(&self, slug: &str, _: bool) -> Result<Option<Post>> {
            Ok(self
                .posts
                .lock()
                .unwrap()
                .iter()
                .find(|p| p.slug == slug)
                .cloned())
        }
        async fn count_by_slug(&self, slug: &str, ex: Option<Id>) -> Result<u64> {
            let posts = self.posts.lock().unwrap();
            Ok(posts
                .iter()
                .filter(|p| p.slug == slug && Some(p.id) != ex)
                .count() as u64)
        }
        async fn create(&self, post: &Post, _: Option<&[Id]>) -> Result<Post> {
            let mut posts = self.posts.lock().unwrap();
            let mut p = post.clone();
            p.id = posts.len() as Id + 1;
            posts.push(p.clone());
            Ok(p)
        }
        async fn update(&self, post: &Post, _: Option<&[Id]>) -> Result<()> {
            let mut posts = self.posts.lock().unwrap();
            if let Some(p) = posts.iter_mut().find(|p| p.id == post.id) {
                *p = post.clone();
            }
            Ok(())
        }
        async fn delete(&self, id: Id) -> Result<()> {
            self.posts.lock().unwrap().retain(|p| p.id != id);
            Ok(())
        }
        async fn related_products(&self, _: Id) -> Result<Vec<RelatedProduct>> {
            Ok(Vec::new())
        }
        async fn posts_for_product(
            &self,
            _: Id,
            _: &str,
            _: bool,
            _: u64,
        ) -> Result<Vec<RelatedPost>> {
            Ok(Vec::new())
        }
    }

    #[async_trait]
    impl PostCategoryRepo for Mem {
        async fn list(&self, _: Option<Id>, _: bool) -> Result<Vec<PostCategory>> {
            Ok(self.cats.lock().unwrap().clone())
        }
        async fn get(&self, id: Id) -> Result<Option<PostCategory>> {
            Ok(self
                .cats
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        async fn create(&self, _: &PostCategoryInput) -> Result<PostCategory> {
            Err(Error::internal_msg("unused"))
        }
        async fn update(&self, _: Id, _: &PostCategoryInput) -> Result<()> {
            Ok(())
        }
        async fn set_active(&self, id: Id, active: bool) -> Result<()> {
            if let Some(c) = self.cats.lock().unwrap().iter_mut().find(|c| c.id == id) {
                c.is_active = active;
            }
            Ok(())
        }
        async fn delete(&self, _: Id) -> Result<()> {
            Ok(())
        }
        async fn count_by_slug(&self, _: &str, _: Option<Id>) -> Result<u64> {
            Ok(0)
        }
        async fn count_children(&self, id: Id) -> Result<u64> {
            let cats = self.cats.lock().unwrap();
            Ok(cats.iter().filter(|c| c.parent_id == Some(id)).count() as u64)
        }
        async fn count_posts(&self, _: Id) -> Result<u64> {
            Ok(0)
        }
    }

    fn category(id: Id, parent: Option<Id>, active: bool) -> PostCategory {
        PostCategory {
            id,
            parent_id: parent,
            slug: format!("c{id}"),
            name: json!({}),
            icon: String::new(),
            is_active: active,
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            children: Vec::new(),
        }
    }

    fn input(slug: &str, published: Option<bool>, category: Option<Id>) -> PostInput {
        PostInput {
            slug: slug.into(),
            kind: POST_TYPE_BLOG.into(),
            title: json!({"zh-CN": slug}),
            summary: Value::Null,
            content: Value::Null,
            thumbnail: String::new(),
            is_published: published,
            product_ids: None,
            category_id: category,
        }
    }

    fn service(repo: Arc<Mem>) -> PostService {
        let at = Utc.with_ymd_and_hms(2026, 4, 20, 12, 0, 0).unwrap();
        PostService::new(repo.clone(), repo, Arc::new(FixedClock(at)))
    }

    // Original fix 8dd3ca52: published_at is stamped on first publication only.
    #[tokio::test]
    async fn published_at_set_once() {
        let repo = Arc::new(Mem::default());
        let svc = service(repo.clone());
        let draft = svc.create(input("a", None, None)).await.unwrap();
        assert_eq!(draft.published_at, None);
        let published = svc
            .update(draft.id, input("a", Some(true), None))
            .await
            .unwrap();
        let first = published.published_at.unwrap();
        let unpublished = svc
            .update(draft.id, input("a", Some(false), None))
            .await
            .unwrap();
        assert!(!unpublished.is_published);
        assert_eq!(unpublished.published_at, Some(first));
        let created = svc.create(input("b", Some(true), None)).await.unwrap();
        assert!(created.published_at.is_some());
    }

    // Original fix dab47887: inactive categories cannot be newly assigned but are kept.
    #[tokio::test]
    async fn inactive_category_kept_but_not_assignable() {
        let repo = Arc::new(Mem::default());
        repo.cats.lock().unwrap().push(category(1, None, true));
        let svc = service(repo.clone());
        let post = svc.create(input("a", None, Some(1))).await.unwrap();
        repo.cats.lock().unwrap()[0].is_active = false;
        assert_eq!(
            svc.create(input("b", None, Some(1)))
                .await
                .unwrap_err()
                .key(),
            CATEGORY_INVALID
        );
        let kept = svc
            .update(post.id, input("a", None, Some(1)))
            .await
            .unwrap();
        assert_eq!(kept.category_id, Some(1));
        // category id 0 means "none"
        let none = svc
            .update(post.id, input("a", None, Some(0)))
            .await
            .unwrap();
        assert_eq!(none.category_id, None);
    }
}
