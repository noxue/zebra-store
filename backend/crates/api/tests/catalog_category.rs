//! Category endpoints follow the original contract.

mod common;

use common::{TestApp, data};
use serde_json::json;

#[tokio::test]
async fn category_crud_contract() {
    let app = TestApp::new().await;

    let created = app
        .post(
            "/api/v1/admin/categories",
            json!({"name": {"zh-CN": "游戏点卡"}, "slug": "game-cards", "sort_order": 1}),
        )
        .await;
    let c = data(&created);
    assert_eq!(c["slug"], "game-cards");
    assert_eq!(c["parent_id"], 0);
    assert_eq!(c["is_active"], true);
    assert_eq!(c["name"]["zh-CN"], "游戏点卡");
    let id = c["id"].as_i64().unwrap();

    // duplicate slug → 400 with translated message, HTTP 200 envelope
    let dup = app
        .post(
            "/api/v1/admin/categories",
            json!({"name": {"zh-CN": "x"}, "slug": "game-cards"}),
        )
        .await;
    assert_eq!(dup["status_code"], 400);
    assert_eq!(dup["msg"], "Slug 已存在");
    assert!(dup["data"]["request_id"].is_string());

    // child + public list excludes inactive
    let child = app
        .post(
            "/api/v1/admin/categories",
            json!({"name": {"zh-CN": "子"}, "slug": "child", "parent_id": id}),
        )
        .await;
    let child_id = data(&child)["id"].as_i64().unwrap();
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{child_id}/active"),
            json!({"is_active": false}),
        )
        .await,
    );
    let public = app.get("/api/v1/public/categories").await;
    let list = data(&public).as_array().unwrap();
    assert_eq!(list.len(), 1);
    assert!(
        list[0].get("is_active").is_none(),
        "public shape hides is_active"
    );

    // in-use delete rejected, then allowed after removing child
    assert_eq!(
        app.delete(&format!("/api/v1/admin/categories/{id}")).await["status_code"],
        400
    );
    data(
        &app.delete(&format!("/api/v1/admin/categories/{child_id}"))
            .await,
    );
    let deleted = app.delete(&format!("/api/v1/admin/categories/{id}")).await;
    assert!(data(&deleted).is_null());

    // malformed body
    let bad = app
        .call(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"slug": 5})),
            app.admin_token.as_deref(),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    assert_eq!(bad["msg"], "请求参数错误");
}
