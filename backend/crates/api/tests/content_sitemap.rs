//! `/sitemap.xml` and `/robots.txt`.

#![expect(clippy::unwrap_used, reason = "test helpers abort on failure")]

mod common;
mod content_common;

use axum::http::StatusCode;
use chrono::Utc;
use common::TestApp;
use content_common::get_raw;
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use zs_infra::db::entity::{categories, products};

async fn seed_product(app: &TestApp, slug: &str, product_active: bool, category_active: bool) {
    let cat = categories::ActiveModel {
        parent_id: Set(0),
        slug: Set(format!("c-{slug}")),
        name_json: Set(Some(json!({"zh-CN": slug}))),
        icon: Set(String::new()),
        sort_order: Set(0),
        is_active: Set(category_active),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    products::ActiveModel {
        category_id: Set(cat.id),
        slug: Set(slug.into()),
        price_amount: Set(Decimal::ONE),
        cost_price_amount: Set(Decimal::ZERO),
        payment_channel_ids: Set(String::new()),
        is_active: Set(product_active),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
}

// SET-03: the configured site_url wins over forged forwarded headers; only indexable
// products and published posts are listed.
#[tokio::test]
async fn set_03_sitemap_uses_configured_url_and_filters() {
    let app = TestApp::new().await;
    seed_product(&app, "live", true, true).await;
    seed_product(&app, "off", false, true).await;
    seed_product(&app, "hidden-cat", true, false).await;
    app.post(
        "/api/v1/admin/posts",
        json!({"slug": "pub", "type": "blog", "title": {"zh-CN": "p"}, "is_published": true}),
    )
    .await;
    app.post(
        "/api/v1/admin/posts",
        json!({"slug": "draft", "type": "blog", "title": {"zh-CN": "d"}}),
    )
    .await;
    app.put(
        "/api/v1/admin/settings",
        json!({"key": "site_config", "value": {"brand": {"site_url": "https://shop.example/"}}}),
    )
    .await;

    let (status, h, body) = get_raw(
        &app.router,
        "/sitemap.xml",
        &[
            ("host", "evil.com"),
            ("x-forwarded-host", "evil.com"),
            ("x-forwarded-proto", "https"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h["content-type"], "application/xml; charset=utf-8");
    let xml = String::from_utf8(body.to_vec()).unwrap();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(xml.contains("<loc>https://shop.example/</loc>"));
    assert!(!xml.contains("evil.com"));
    assert!(xml.contains("https://shop.example/products/live"));
    assert!(!xml.contains("/products/off"));
    assert!(!xml.contains("/products/hidden-cat"));
    assert!(xml.contains("https://shop.example/categories/c-live"));
    assert!(!xml.contains("c-hidden-cat"));
    assert!(xml.contains("https://shop.example/blog/pub"));
    assert!(!xml.contains("/blog/draft"));

    let (_, h, body) = get_raw(&app.router, "/robots.txt", &[("host", "evil.com")]).await;
    assert_eq!(h["content-type"], "text/plain; charset=utf-8");
    let robots = String::from_utf8(body.to_vec()).unwrap();
    assert!(robots.starts_with("User-agent: *\nDisallow: /api/\n"));
    assert!(robots.ends_with("\nSitemap: https://shop.example/sitemap.xml\n"));
}

#[tokio::test]
async fn robots_without_site_url_has_no_sitemap_line() {
    let app = TestApp::new().await;
    let (_, _, body) = get_raw(
        &app.router,
        "/robots.txt",
        &[("host", "evil.com"), ("x-forwarded-host", "evil.com")],
    )
    .await;
    let robots = String::from_utf8(body.to_vec()).unwrap();
    assert!(!robots.contains("Sitemap:"));
    assert!(robots.contains("Disallow: /checkout\n"));
    let (status, _, body) = get_raw(
        &app.router,
        "/sitemap.xml",
        &[
            ("host", "shop.local:8080"),
            ("x-forwarded-host", "evil.com"),
        ],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let xml = String::from_utf8(body.to_vec()).unwrap();
    assert!(xml.contains("<loc>http://shop.local/</loc>"));
    assert!(!xml.contains("evil.com"));
}
