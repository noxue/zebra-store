//! Posts, post categories and banners (admin + public) follow the original contract.

#![expect(clippy::unwrap_used, reason = "test helpers abort on failure")]

mod common;

use chrono::{Duration, Utc};
use common::{TestApp, data};
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{categories, products};

async fn product(app: &TestApp, slug: &str, active: bool) -> i64 {
    let cat = categories::ActiveModel {
        parent_id: Set(0),
        slug: Set(format!("cat-{slug}")),
        name_json: Set(Some(json!({"zh-CN": slug}))),
        icon: Set(String::new()),
        sort_order: Set(0),
        is_active: Set(true),
        created_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    products::ActiveModel {
        category_id: Set(cat.id),
        slug: Set(slug.into()),
        title_json: Set(Some(json!({"zh-CN": slug}))),
        price_amount: Set(Decimal::new(1230, 2)),
        cost_price_amount: Set(Decimal::ZERO),
        images: Set(Some(json!([format!("/uploads/product/{slug}.png")]))),
        payment_channel_ids: Set(String::new()),
        is_active: Set(active),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id
}

fn id(v: &Value) -> i64 {
    data(v)["id"].as_i64().unwrap()
}

#[tokio::test]
async fn post_crud_publication_and_public_views() {
    let app = TestApp::new().await;
    let p1 = product(&app, "p1", true).await;
    let p2 = product(&app, "p2", false).await;

    // draft blog with related products (order kept, duplicates/zero dropped)
    let created = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "hello", "type": "blog", "title": {"zh-CN": "你好 guide"},
                   "summary": {"zh-CN": "s"}, "content": {"zh-CN": "<p>c</p>"},
                   "product_ids": [p2, 0, p1, p2]}),
        )
        .await;
    let d = data(&created);
    assert_eq!(d["type"], "blog");
    assert_eq!(d["is_published"], false);
    assert!(d["published_at"].is_null());
    assert!(d["category_id"].is_null());
    assert!(d.get("deleted_at").is_none());
    let post_id = id(&created);

    let refs = app
        .get(&format!("/api/v1/admin/posts/{post_id}/products"))
        .await;
    let refs = data(&refs).as_array().unwrap().clone();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0]["id"], p2);
    assert_eq!(refs[1]["id"], p1);
    assert_eq!(refs[1]["image"], "/uploads/product/p1.png");

    // drafts are not public
    let v = app
        .call("GET", "/api/v1/public/posts/hello", None, None)
        .await;
    assert_eq!(v["status_code"], 404);
    assert_eq!(v["msg"], "文章不存在");

    // first publication stamps published_at (original fix 8dd3ca52)
    let body = json!({"slug": "hello", "type": "blog", "title": {"zh-CN": "你好 guide"}, "is_published": true});
    let updated = app
        .put(&format!("/api/v1/admin/posts/{post_id}"), body.clone())
        .await;
    let published_at = data(&updated)["published_at"].clone();
    assert!(published_at.is_string());
    // re-publishing keeps the original timestamp; product relations untouched without product_ids
    let again = app
        .put(&format!("/api/v1/admin/posts/{post_id}"), body)
        .await;
    assert_eq!(data(&again)["published_at"], published_at);

    let v = app
        .call("GET", "/api/v1/public/posts/hello", None, None)
        .await;
    let d = data(&v);
    assert!(d.get("is_published").is_none());
    assert!(d.get("created_at").is_none());
    assert!(d.get("thumbnail").is_none(), "empty thumbnail omitted");
    // only active related products are shown, with money strings
    let related = d["related_products"].as_array().unwrap();
    assert_eq!(related.len(), 1);
    assert_eq!(related[0]["slug"], "p1");
    assert_eq!(related[0]["price_amount"], "12.30");

    // created as published → published_at set; notice cannot have a category
    let notice = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "n1", "type": "notice", "title": {"zh-CN": "公告"}, "is_published": true}),
        )
        .await;
    assert!(data(&notice)["published_at"].is_string());
    let bad = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "n2", "type": "notice", "title": {"zh-CN": "x"}, "category_id": 5}),
        )
        .await;
    assert_eq!(bad["msg"], "公告不支持设置文章分类");
    let bad = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "x", "type": "page", "title": {"zh-CN": "x"}}),
        )
        .await;
    assert_eq!(bad["msg"], "文章类型不合法");
    let dup = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "hello", "type": "blog", "title": {"zh-CN": "x"}}),
        )
        .await;
    assert_eq!(dup["msg"], "Slug 已存在");
    let dup = app
        .put(
            &format!("/api/v1/admin/posts/{}", id(&notice)),
            json!({"slug": "hello", "type": "notice", "title": {"zh-CN": "x"}}),
        )
        .await;
    assert_eq!(dup["msg"], "Slug 已被其他资源使用");

    // public list: published only, type filter, search, pagination envelope
    let list = app
        .call(
            "GET",
            "/api/v1/public/posts?type=blog&search=guide&page=1&page_size=10",
            None,
            None,
        )
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(data(&list)[0]["slug"], "hello");
    let all = app.call("GET", "/api/v1/public/posts", None, None).await;
    assert_eq!(all["pagination"]["total"], 2);
    let admin = app.get("/api/v1/admin/posts?type=notice").await;
    assert_eq!(admin["pagination"]["total"], 1);
    assert_eq!(data(&admin)[0]["is_published"], true);

    // delete
    assert!(data(&app.delete(&format!("/api/v1/admin/posts/{post_id}")).await).is_null());
    let v = app.delete(&format!("/api/v1/admin/posts/{post_id}")).await;
    assert_eq!(v["status_code"], 404);
}

#[tokio::test]
async fn post_categories_tree_status_and_assignment() {
    let app = TestApp::new().await;
    let root = app
        .post(
            "/api/v1/admin/post-categories",
            json!({"name": {"zh-CN": "教程"}, "slug": "guides", "sort_order": 1}),
        )
        .await;
    let root_id = id(&root);
    assert!(data(&root)["parent_id"].is_null());
    assert_eq!(data(&root)["is_active"], true);
    let child = app
        .post(
            "/api/v1/admin/post-categories",
            json!({"name": {"zh-CN": "入门"}, "slug": "start", "parent_id": root_id}),
        )
        .await;
    let child_id = id(&child);
    let bad = app
        .post(
            "/api/v1/admin/post-categories",
            json!({"name": {"zh-CN": "x"}, "slug": "deep", "parent_id": child_id}),
        )
        .await;
    assert_eq!(bad["msg"], "父分类不合法，仅支持最多两级分类");

    let tree = app.get("/api/v1/admin/post-categories?tree=1").await;
    let tree = data(&tree).as_array().unwrap().clone();
    assert_eq!(tree.len(), 1);
    assert_eq!(tree[0]["children"][0]["slug"], "start");
    let flat = app
        .get(&format!(
            "/api/v1/admin/post-categories?parent_id={root_id}"
        ))
        .await;
    assert_eq!(data(&flat).as_array().unwrap().len(), 1);

    // a parent with children cannot hold posts
    let bad = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "a", "type": "blog", "title": {"zh-CN": "a"}, "category_id": root_id}),
        )
        .await;
    assert_eq!(
        bad["msg"],
        "当前文章分类不可直接挂载文章，请选择有效的末级分类"
    );
    let post = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "a", "type": "blog", "title": {"zh-CN": "a"}, "category_id": child_id}),
        )
        .await;
    let post_id = id(&post);

    // original fix dab47887: a disabled category cannot be newly assigned, but is kept
    let v = app
        .patch(
            &format!("/api/v1/admin/post-categories/{child_id}/status"),
            json!({"is_active": false}),
        )
        .await;
    assert_eq!(data(&v)["is_active"], false);
    let bad = app
        .post(
            "/api/v1/admin/posts",
            json!({"slug": "b", "type": "blog", "title": {"zh-CN": "b"}, "category_id": child_id}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    let kept = app
        .put(
            &format!("/api/v1/admin/posts/{post_id}"),
            json!({"slug": "a", "type": "blog", "title": {"zh-CN": "a2"}, "category_id": child_id}),
        )
        .await;
    assert_eq!(data(&kept)["category_id"], child_id);

    // public list only has active categories, flat DTO shape
    let public = app
        .call("GET", "/api/v1/public/post-categories", None, None)
        .await;
    let public = data(&public).as_array().unwrap().clone();
    assert_eq!(public.len(), 1);
    assert_eq!(public[0]["parent_id"], 0);
    assert!(public[0].get("is_active").is_none());

    // in-use deletion refused; update keeps name when omitted
    let v = app
        .delete(&format!("/api/v1/admin/post-categories/{child_id}"))
        .await;
    assert_eq!(v["msg"], "该分类存在子分类或文章，无法删除");
    let v = app
        .put(
            &format!("/api/v1/admin/post-categories/{root_id}"),
            json!({"slug": "", "sort_order": 3}),
        )
        .await;
    assert_eq!(data(&v)["slug"], "guides");
    assert_eq!(data(&v)["name"]["zh-CN"], "教程");
    assert_eq!(data(&v)["sort_order"], 3);
    let v = app
        .put("/api/v1/admin/post-categories/9999", json!({"slug": "zz"}))
        .await;
    assert_eq!(v["msg"], "文章分类不存在");
}

#[tokio::test]
async fn banners_admin_and_public_window() {
    let app = TestApp::new().await;
    let past = (Utc::now() - Duration::days(2)).to_rfc3339();
    let future = (Utc::now() + Duration::days(2)).to_rfc3339();

    let live = app
        .post(
            "/api/v1/admin/banners",
            json!({"name": "Hero", "image": "/uploads/banner/a.png", "title": {"zh-CN": " 春季 "},
                   "link_type": "external", "link_value": "https://example.com", "sort_order": 1,
                   "start_at": past}),
        )
        .await;
    let d = data(&live);
    assert_eq!(d["position"], "home_hero");
    assert_eq!(
        d["title"],
        json!({"zh-CN": "春季", "zh-TW": "", "en-US": ""})
    );
    assert_eq!(d["is_active"], true);
    let live_id = id(&live);

    // scheduled for the future, and one inactive
    app.post(
        "/api/v1/admin/banners",
        json!({"name": "Later", "image": "/b.png", "start_at": future}),
    )
    .await;
    app.post(
        "/api/v1/admin/banners",
        json!({"name": "Off", "image": "/c.png", "is_active": false}),
    )
    .await;
    let expired = app
        .post(
            "/api/v1/admin/banners",
            json!({"name": "Old", "image": "/d.png", "end_at": past}),
        )
        .await;
    assert_eq!(data(&expired)["is_active"], true);

    let bad = app
        .post(
            "/api/v1/admin/banners",
            json!({"name": "x", "image": "/x.png", "link_type": "internal"}),
        )
        .await;
    assert_eq!(bad["msg"], "Banner 参数不合法");
    let bad = app
        .post(
            "/api/v1/admin/banners",
            json!({"name": "x", "image": "/x.png", "start_at": "tomorrow"}),
        )
        .await;
    assert_eq!(bad["msg"], "请求参数错误");

    // CNT-13: public list = active + inside window, without admin fields
    let v = app
        .call("GET", "/api/v1/public/banners?limit=abc", None, None)
        .await;
    let list = data(&v).as_array().unwrap().clone();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], live_id);
    assert_eq!(list[0]["link_value"], "https://example.com");
    for hidden in [
        "name",
        "is_active",
        "start_at",
        "end_at",
        "sort_order",
        "created_at",
    ] {
        assert!(list[0].get(hidden).is_none(), "{hidden} must be hidden");
    }
    assert!(list[0].get("mobile_image").is_none());

    // admin list / filter / get / update / delete
    let v = app.get("/api/v1/admin/banners?is_active=false").await;
    assert_eq!(v["pagination"]["total"], 1);
    let v = app.get("/api/v1/admin/banners?is_active=maybe").await;
    assert_eq!(v["status_code"], 400);
    let v = app.get("/api/v1/admin/banners?search=hero").await;
    assert_eq!(v["pagination"]["total"], 1);
    let v = app
        .put(
            &format!("/api/v1/admin/banners/{live_id}"),
            json!({"name": "Hero2", "image": "/uploads/banner/a.png", "link_type": "none", "link_value": "x"}),
        )
        .await;
    assert_eq!(data(&v)["name"], "Hero2");
    assert_eq!(data(&v)["link_value"], "");
    assert_eq!(data(&v)["is_active"], true);
    let v = app.get(&format!("/api/v1/admin/banners/{live_id}")).await;
    assert_eq!(data(&v)["name"], "Hero2");
    let v = app
        .delete(&format!("/api/v1/admin/banners/{live_id}"))
        .await;
    assert_eq!(data(&v), &json!({"deleted": true}));
    let v = app.get(&format!("/api/v1/admin/banners/{live_id}")).await;
    assert_eq!(v["msg"], "Banner 不存在");
}
