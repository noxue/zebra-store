//! Storefront content endpoints under `/api/v1/public`.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zs_domain::Id;
use zs_domain::content::banner::{Banner, POSITION_HOME_HERO};
use zs_domain::content::post::{POST_TYPE_BLOG, Post, PostCategory, RelatedProduct};
use zs_domain::content::public::Tenant;
use zs_shared::money::Amount;
use zs_shared::page::Pagination;

use super::PageQuery;
use crate::extract::Query;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Default and maximum `limit` of `/public/banners`.
const BANNER_LIMIT_DEFAULT: u64 = 10;
const BANNER_LIMIT_MAX: u64 = 50;

pub(super) fn routes() -> Routes {
    Routes::new("/public")
        .get("/config", config)
        .get("/posts", posts)
        .get("/posts/{slug}", post_by_slug)
        .get("/banners", banners)
        .get("/post-categories", post_categories)
}

/// Request host without port, lower-cased.
pub(crate) fn request_host(headers: &HeaderMap) -> String {
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if let Some(end) = host.strip_prefix('[').and_then(|h| h.find(']')) {
        return host[..end + 2].to_owned();
    }
    host.split(':').next().unwrap_or_default().to_owned()
}

async fn config(
    State(s): State<AppState>,
    headers: HeaderMap,
    resolved: Option<axum::Extension<crate::middleware::tenant::Tenant>>,
) -> ApiResult<Data<Value>> {
    // The reseller tenant middleware resolves the host; without it (feature off)
    // the request is served as the main shop.
    let tenant = match resolved {
        Some(axum::Extension(t)) => t.to_content(),
        None => Tenant {
            reseller_id: None,
            host: request_host(&headers),
        },
    };
    ok(s.svc.content.public_config.get(&tenant).await?)
}

/// Public post shape (`presenter.PostResp`).
#[derive(Debug, Serialize)]
struct PostResp {
    id: Id,
    slug: String,
    #[serde(rename = "type")]
    kind: String,
    title: Value,
    summary: Value,
    content: Value,
    #[serde(skip_serializing_if = "String::is_empty")]
    thumbnail: String,
    published_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    related_products: Vec<RelatedProductCard>,
}

impl From<Post> for PostResp {
    fn from(p: Post) -> Self {
        Self {
            id: p.id,
            slug: p.slug,
            kind: p.kind,
            title: p.title,
            summary: p.summary,
            content: p.content,
            thumbnail: p.thumbnail,
            published_at: p.published_at,
            related_products: Vec::new(),
        }
    }
}

/// Product card under a blog post (active products only).
#[derive(Debug, Serialize)]
struct RelatedProductCard {
    id: Id,
    slug: String,
    title: Value,
    price_amount: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    image: String,
}

fn cards(products: Vec<RelatedProduct>) -> Vec<RelatedProductCard> {
    products
        .into_iter()
        .filter(|p| p.is_active)
        .map(|p| RelatedProductCard {
            id: p.id,
            slug: p.slug,
            title: p.title,
            price_amount: p.price_amount,
            image: p.images.into_iter().next().unwrap_or_default(),
        })
        .collect()
}

#[derive(Debug, Default, Deserialize)]
struct PostsQuery {
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    search: String,
    #[serde(flatten)]
    page: PageQuery,
}

async fn posts(
    State(s): State<AppState>,
    Query(q): Query<PostsQuery>,
) -> ApiResult<Paged<PostResp>> {
    let req = q.page.request();
    let page = s
        .svc
        .content
        .posts
        .list_public(&q.kind, &q.search, req)
        .await?;
    let pagination = Pagination::new(req, page.total);
    Ok(Paged(
        page.items.into_iter().map(PostResp::from).collect(),
        pagination,
    ))
}

async fn post_by_slug(
    State(s): State<AppState>,
    Path(slug): Path<String>,
) -> ApiResult<Data<PostResp>> {
    let post = s.svc.content.posts.public_by_slug(&slug).await?;
    let blog = post.kind == POST_TYPE_BLOG;
    let id = post.id;
    let mut resp = PostResp::from(post);
    if blog && let Ok(products) = s.svc.content.posts.related_products(id).await {
        resp.related_products = cards(products);
    }
    ok(resp)
}

/// Public banner shape (`presenter.BannerResp`): no admin-only fields.
#[derive(Debug, Serialize)]
struct BannerResp {
    id: Id,
    position: String,
    title: Value,
    subtitle: Value,
    image: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    mobile_image: String,
    link_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    link_value: String,
    open_in_new_tab: bool,
}

impl From<Banner> for BannerResp {
    fn from(b: Banner) -> Self {
        Self {
            id: b.id,
            position: b.position,
            title: b.title,
            subtitle: b.subtitle,
            image: b.image,
            mobile_image: b.mobile_image,
            link_type: b.link_type,
            link_value: b.link_value,
            open_in_new_tab: b.open_in_new_tab,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct BannerQuery {
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    limit: Option<String>,
}

async fn banners(
    State(s): State<AppState>,
    Query(q): Query<BannerQuery>,
) -> ApiResult<Data<Vec<BannerResp>>> {
    let limit = q
        .limit
        .as_deref()
        .and_then(|l| l.trim().parse::<i64>().ok())
        .filter(|l| *l > 0)
        .map_or(BANNER_LIMIT_DEFAULT, |l| {
            u64::try_from(l).unwrap_or(BANNER_LIMIT_DEFAULT)
        })
        .min(BANNER_LIMIT_MAX);
    let position = q.position.unwrap_or_else(|| POSITION_HOME_HERO.to_owned());
    let list = s.svc.content.banners.list_public(&position, limit).await?;
    ok(list.into_iter().map(BannerResp::from).collect())
}

/// Public category shape (`PostCategoryDTO`).
#[derive(Debug, Serialize)]
struct PostCategoryDto {
    id: Id,
    parent_id: Id,
    slug: String,
    name: Value,
    icon: String,
    sort_order: i32,
}

impl From<PostCategory> for PostCategoryDto {
    fn from(c: PostCategory) -> Self {
        Self {
            id: c.id,
            parent_id: c.parent_id.unwrap_or(0),
            slug: c.slug,
            name: c.name,
            icon: c.icon,
            sort_order: c.sort_order,
        }
    }
}

async fn post_categories(State(s): State<AppState>) -> ApiResult<Data<Vec<PostCategoryDto>>> {
    let list = s.svc.content.post_categories.list_active().await?;
    ok(list.into_iter().map(PostCategoryDto::from).collect())
}
