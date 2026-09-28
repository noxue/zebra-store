//! Response shapes fixed after the Go-vs-Rust contract diff
//! (`scripts/contract-diff`, report in `docs/reference/contract-diff-report.md`).

mod integration_common;

use integration_common::{IntApp, data, err};
use serde_json::json;

/// `GET /admin/api-credentials`: the original preloads the whole `User` model, so `user`
/// carries every JSON field of `users` (money as strings, nullable timestamps present).
#[tokio::test]
async fn admin_api_credential_user_is_full_user_model() {
    let app = IntApp::new().await;
    let (uid, token) = app.user("owner@example.com").await;
    app.call("POST", "/api/v1/api-credential/apply", None, Some(&token))
        .await;

    let list = app
        .admin("GET", "/api/v1/admin/api-credentials", None)
        .await;
    let user = &data(&list)[0]["user"];
    let mut keys: Vec<&str> = user
        .as_object()
        .expect("user object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "created_at",
            "display_name",
            "email",
            "email_verified_at",
            "id",
            "last_login_at",
            "locale",
            "member_level_id",
            "status",
            "total_recharged",
            "total_spent",
            "updated_at",
        ],
        "{user}"
    );
    assert_eq!(user["id"], uid);
    assert_eq!(user["locale"], "zh-CN");
    assert_eq!(user["total_recharged"], "0.00");
    assert_eq!(user["total_spent"], "0.00");
    assert!(user["created_at"].is_string(), "{user}");
}

/// Required-field failures name the Go struct fields like the original `RespondBindError`
/// (`validator` `required`: missing, null or Go zero value; all failures, struct order).
#[tokio::test]
async fn bind_errors_name_go_fields() {
    let app = IntApp::new().await;
    let login = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin"})),
            None,
        )
        .await;
    err(&login, 400, "Password: 不能为空");

    let category = app
        .admin(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"name": {}})),
        )
        .await;
    err(&category, 400, "Slug: 不能为空");

    let product = app
        .admin(
            "POST",
            "/api/v1/admin/products",
            Some(json!({"slug": "x", "category_id": 0})),
        )
        .await;
    err(
        &product,
        400,
        "CategoryID: 不能为空; TitleJSON: 不能为空; PriceAmount: 不能为空",
    );

    // Locale follows the request; type errors stay the generic bad request.
    let en = app
        .admin(
            "POST",
            "/api/v1/admin/categories?lang=en-US",
            Some(json!({"slug": ""})),
        )
        .await;
    err(&en, 400, "Slug: is required; NameJSON: is required");
    let typed = app
        .admin("POST", "/api/v1/admin/categories", Some(json!({"slug": 5})))
        .await;
    err(&typed, 400, "请求参数错误");
}
