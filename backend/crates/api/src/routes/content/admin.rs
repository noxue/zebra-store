//! Admin endpoints for posts, post categories, banners, media and uploads.

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::handler::Handler;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_domain::content::banner::{Banner, BannerInput, BannerQuery};
use zs_domain::content::media::{Media, MediaQuery};
use zs_domain::content::post::{Post, PostCategory, PostCategoryInput};
use zs_domain::{Error, Id};
use zs_shared::page::Pagination;

use super::PageQuery;
use zs_app::content::post::PostInput;

use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req, req_min1, req_ptr};
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/admin")
        .get("/posts", list_posts)
        .post("/posts", create_post)
        .put("/posts/{id}", update_post)
        .delete("/posts/{id}", delete_post)
        .get("/posts/{id}/products", post_products)
        .get("/post-categories", list_categories)
        .post("/post-categories", create_category)
        .put("/post-categories/{id}", update_category)
        .delete("/post-categories/{id}", delete_category)
        .patch("/post-categories/{id}/status", patch_category_status)
        .get("/banners", list_banners)
        .get("/banners/{id}", get_banner)
        .post("/banners", create_banner)
        .put("/banners/{id}", update_banner)
        .delete("/banners/{id}", delete_banner)
        .get("/media", list_media)
        .post("/media/batch-delete", batch_delete_media)
        .put("/media/{id}", rename_media)
        .delete("/media/{id}", delete_media)
        // The multipart body is size-checked while streaming (config.upload.max_size).
        .post("/upload", upload.layer(DefaultBodyLimit::disable()))
}

// --- posts -------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct PostsQuery {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    search: String,
    #[serde(flatten)]
    page: PageQuery,
}

async fn list_posts(
    State(s): State<AppState>,
    Query(q): Query<PostsQuery>,
) -> ApiResult<Paged<Post>> {
    let req = q.page.request();
    let page = s
        .svc
        .content
        .posts
        .list_admin(&q.kind, &q.search, req)
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

#[derive(Debug, Deserialize)]
struct PostRequest {
    slug: String,
    #[serde(rename = "type")]
    kind: String,
    title: serde_json::Map<String, Value>,
    #[serde(default)]
    summary: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    content: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    is_published: Option<bool>,
    #[serde(default)]
    product_ids: Option<Vec<Id>>,
    #[serde(default)]
    category_id: Option<Id>,
}

/// Original `CreatePostRequest` (create and update).
impl BindRules for PostRequest {
    const FIELDS: &'static [BindField] = &[
        req("slug", "Slug"),
        req("type", "Type"),
        req("title", "TitleJSON"),
    ];
}

impl PostRequest {
    fn into_input(self) -> ApiResult<PostInput> {
        if self.slug.is_empty() || self.kind.is_empty() {
            return Err(Error::invalid().into());
        }
        let obj = |m: Option<serde_json::Map<String, Value>>| m.map_or(Value::Null, Value::Object);
        Ok(PostInput {
            slug: self.slug,
            kind: self.kind,
            title: Value::Object(self.title),
            summary: obj(self.summary),
            content: obj(self.content),
            thumbnail: self.thumbnail,
            is_published: self.is_published,
            product_ids: self.product_ids,
            category_id: self.category_id,
        })
    }
}

async fn create_post(
    State(s): State<AppState>,
    Bind(r): Bind<PostRequest>,
) -> ApiResult<Data<Post>> {
    ok(s.svc.content.posts.create(r.into_input()?).await?)
}

async fn update_post(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(r): Bind<PostRequest>,
) -> ApiResult<Data<Post>> {
    ok(s.svc.content.posts.update(id, r.into_input()?).await?)
}

async fn delete_post(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<()>> {
    s.svc.content.posts.delete(id).await?;
    ok(())
}

/// Product reference used to pre-fill the post editor.
#[derive(Debug, Serialize)]
struct PostProductRef {
    id: Id,
    slug: String,
    title: Value,
    #[serde(skip_serializing_if = "String::is_empty")]
    image: String,
}

async fn post_products(
    State(s): State<AppState>,
    PathId(id): PathId,
) -> ApiResult<Data<Vec<PostProductRef>>> {
    let products = s.svc.content.posts.related_products(id).await?;
    ok(products
        .into_iter()
        .map(|p| PostProductRef {
            id: p.id,
            slug: p.slug,
            title: p.title,
            image: p.images.into_iter().next().unwrap_or_default(),
        })
        .collect())
}

// --- post categories -----------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct CategoryListQuery {
    #[serde(default)]
    tree: Option<String>,
    #[serde(default)]
    parent_id: Option<String>,
}

async fn list_categories(
    State(s): State<AppState>,
    Query(q): Query<CategoryListQuery>,
) -> ApiResult<Data<Vec<PostCategory>>> {
    let svc = &s.svc.content.post_categories;
    if q.tree.as_deref() == Some("1") {
        return ok(svc.tree().await?);
    }
    let parent = q.parent_id.as_deref().and_then(|p| p.parse::<Id>().ok());
    ok(svc.list(parent).await?)
}

#[derive(Debug, Deserialize)]
struct CategoryRequest {
    #[serde(default)]
    name: Option<serde_json::Map<String, Value>>,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    parent_id: Option<Id>,
    #[serde(default)]
    sort_order: i32,
    #[serde(default)]
    icon: String,
}

impl BindRules for CategoryRequest {
    const FIELDS: &'static [BindField] = &[req("name", "NameJSON"), req("slug", "Slug")];
}

async fn create_category(
    State(s): State<AppState>,
    Bind(r): Bind<CategoryRequest>,
) -> ApiResult<Data<PostCategory>> {
    let Some(name) = r.name else {
        return Err(Error::invalid().into());
    };
    if r.slug.is_empty() {
        return Err(Error::invalid().into());
    }
    let input = PostCategoryInput {
        name: Value::Object(name),
        slug: r.slug,
        parent_id: r.parent_id,
        sort_order: r.sort_order,
        icon: r.icon,
    };
    ok(s.svc.content.post_categories.create(input).await?)
}

async fn update_category(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(r): Body<CategoryRequest>,
) -> ApiResult<Data<PostCategory>> {
    ok(s.svc
        .content
        .post_categories
        .update(
            id,
            r.name.map(Value::Object),
            r.slug,
            r.parent_id,
            r.sort_order,
            r.icon,
        )
        .await?)
}

async fn delete_category(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<()>> {
    s.svc.content.post_categories.delete(id).await?;
    ok(())
}

#[derive(Debug, Deserialize)]
struct StatusRequest {
    is_active: bool,
}

impl BindRules for StatusRequest {
    const FIELDS: &'static [BindField] = &[req_ptr("is_active", "IsActive")];
}

async fn patch_category_status(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(r): Bind<StatusRequest>,
) -> ApiResult<Data<PostCategory>> {
    ok(s.svc
        .content
        .post_categories
        .set_active(id, r.is_active)
        .await?)
}

// --- banners -------------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct BannersQuery {
    #[serde(default)]
    position: String,
    #[serde(default)]
    search: String,
    #[serde(default)]
    is_active: Option<String>,
    #[serde(flatten)]
    page: PageQuery,
}

async fn list_banners(
    State(s): State<AppState>,
    Query(q): Query<BannersQuery>,
) -> ApiResult<Paged<Banner>> {
    let is_active = match q.is_active.as_deref().map(str::trim) {
        None | Some("") => None,
        Some("1" | "t" | "T" | "true" | "TRUE" | "True") => Some(true),
        Some("0" | "f" | "F" | "false" | "FALSE" | "False") => Some(false),
        Some(_) => return Err(Error::invalid().into()),
    };
    let req = q.page.request();
    let page = s
        .svc
        .content
        .banners
        .list_admin(BannerQuery {
            page: req,
            position: q.position,
            search: q.search,
            is_active,
        })
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

async fn get_banner(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Banner>> {
    ok(s.svc.content.banners.get(id).await?)
}

#[derive(Debug, Deserialize)]
struct BannerRequest {
    name: String,
    #[serde(default)]
    position: String,
    #[serde(default)]
    title: Option<Value>,
    #[serde(default)]
    subtitle: Option<Value>,
    image: String,
    #[serde(default)]
    mobile_image: String,
    #[serde(default)]
    link_type: String,
    #[serde(default)]
    link_value: String,
    #[serde(default)]
    open_in_new_tab: Option<bool>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    start_at: String,
    #[serde(default)]
    end_at: String,
    #[serde(default)]
    sort_order: i32,
}

/// Original `BannerUpsertRequest` (create and update).
impl BindRules for BannerRequest {
    const FIELDS: &'static [BindField] = &[req("name", "Name"), req("image", "Image")];
}

fn parse_time(raw: &str) -> ApiResult<Option<DateTime<Utc>>> {
    if raw.is_empty() {
        return Ok(None);
    }
    DateTime::parse_from_rfc3339(raw)
        .map(|t| Some(t.with_timezone(&Utc)))
        .map_err(|_| Error::invalid().into())
}

impl BannerRequest {
    fn into_input(self) -> ApiResult<BannerInput> {
        if self.name.is_empty() || self.image.is_empty() {
            return Err(Error::invalid().into());
        }
        Ok(BannerInput {
            start_at: parse_time(&self.start_at)?,
            end_at: parse_time(&self.end_at)?,
            name: self.name,
            position: self.position,
            title: self.title,
            subtitle: self.subtitle,
            image: self.image,
            mobile_image: self.mobile_image,
            link_type: self.link_type,
            link_value: self.link_value,
            open_in_new_tab: self.open_in_new_tab,
            is_active: self.is_active,
            sort_order: self.sort_order,
        })
    }
}

async fn create_banner(
    State(s): State<AppState>,
    Bind(r): Bind<BannerRequest>,
) -> ApiResult<Data<Banner>> {
    ok(s.svc.content.banners.create(r.into_input()?).await?)
}

async fn update_banner(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(r): Bind<BannerRequest>,
) -> ApiResult<Data<Banner>> {
    ok(s.svc.content.banners.update(id, r.into_input()?).await?)
}

async fn delete_banner(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.content.banners.delete(id).await?;
    ok(json!({"deleted": true}))
}

// --- media & upload -------------------------------------------------------------

#[derive(Debug, Default, Deserialize)]
struct MediaListQuery {
    #[serde(default)]
    scene: String,
    #[serde(default)]
    search: String,
    #[serde(flatten)]
    page: PageQuery,
}

#[derive(Debug, Serialize)]
struct MediaList {
    items: Vec<Media>,
    total: u64,
}

async fn list_media(
    State(s): State<AppState>,
    Query(q): Query<MediaListQuery>,
) -> ApiResult<Data<MediaList>> {
    let page = s
        .svc
        .content
        .media
        .list(&MediaQuery {
            page: q.page.request(),
            scene: q.scene,
            search: q.search,
        })
        .await
        .map_err(|e| e.or_internal("error.internal"))?;
    ok(MediaList {
        items: page.items,
        total: page.total,
    })
}

#[derive(Debug, Deserialize)]
struct RenameRequest {
    name: String,
}

async fn rename_media(
    State(s): State<AppState>,
    PathId(id): PathId,
    body: axum::body::Bytes,
) -> ApiResult<Data<()>> {
    let req: RenameRequest = serde_json::from_slice(&body)
        .ok()
        .filter(|r: &RenameRequest| !r.name.is_empty())
        .ok_or_else(|| Error::bad_request("error.invalid_params"))?;
    s.svc
        .content
        .media
        .rename(id, &req.name)
        .await
        .map_err(|e| e.or_internal("error.internal"))?;
    ok(())
}

#[derive(Debug, Deserialize)]
struct BatchDeleteRequest {
    ids: Vec<Id>,
}

impl BindRules for BatchDeleteRequest {
    const FIELDS: &'static [BindField] = &[req_min1("ids", "IDs")];
}

async fn batch_delete_media(
    State(s): State<AppState>,
    Bind(r): Bind<BatchDeleteRequest>,
) -> ApiResult<Data<Value>> {
    if r.ids.is_empty() {
        return Err(Error::invalid().into());
    }
    let (success, failed) = s.svc.content.media.batch_delete(&r.ids).await;
    ok(json!({"total": r.ids.len(), "success_count": success, "failed_ids": failed}))
}

async fn delete_media(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<()>> {
    s.svc
        .content
        .media
        .delete(id)
        .await
        .map_err(|e| e.or_internal("error.internal"))?;
    ok(())
}

/// `POST /admin/upload` (multipart `file` + `scene`).
async fn upload(State(s): State<AppState>, mut form: Multipart) -> ApiResult<Data<Value>> {
    let upload = &s.svc.content.upload;
    let mut scene = String::from("common");
    let mut file: Option<(String, Vec<u8>)> = None;
    while let Some(mut field) = form
        .next_field()
        .await
        .map_err(|_| Error::bad_request("error.file_missing"))?
    {
        match field.name() {
            Some("scene") => {
                scene = field.text().await.unwrap_or_default();
            }
            Some("file") if file.is_none() => {
                let name = field.file_name().unwrap_or_default().to_owned();
                let mut buf = Vec::new();
                // Stream with an early size check instead of buffering unbounded bodies.
                while let Some(chunk) = field
                    .chunk()
                    .await
                    .map_err(|_| Error::bad_request("error.file_missing"))?
                {
                    if (buf.len() + chunk.len()) as u64 > upload.max_size() {
                        return Err(upload.too_large().into());
                    }
                    buf.extend_from_slice(&chunk);
                }
                file = Some((name, buf));
            }
            _ => {}
        }
    }
    let Some((name, bytes)) = file else {
        return Err(Error::bad_request("error.file_missing").into());
    };
    if scene.trim().is_empty() {
        scene = "common".into();
    }
    let stored = upload.save(&name, &bytes, &scene).await?;
    let media_id = match s.svc.content.media.record(&stored, &scene).await {
        Ok(m) => m.id,
        Err(error) => {
            tracing::warn!(%error, url = %stored.url, "upload_record_media_failed");
            0
        }
    };
    ok(json!({
        "url": stored.url,
        "filename": stored.filename,
        "size": stored.size,
        "media_id": media_id,
    }))
}
