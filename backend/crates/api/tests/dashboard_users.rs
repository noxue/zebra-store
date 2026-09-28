//! `/admin/users*`: list filters and whitelisted sorting, detail, edit,
//! batch status (token revocation), OAuth unbind and coupon usages.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

mod common;
mod dashboard_common;

use common::{TestApp, data};
use dashboard_common::{at, coupon, coupon_usage, oauth, product, setting, user, wallet};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use zs_infra::db::entity::users;

async fn set_user(app: &TestApp, id: i64, col: users::Column, value: sea_orm::Value) {
    users::Entity::update_many()
        .col_expr(col, Expr::value(value))
        .filter(users::Column::Id.eq(id))
        .exec(&app.db)
        .await
        .unwrap();
}

/// A (wallet 9, last login 09-10), B (wallet 10, never logged in), C (disabled, last login 09-05).
async fn seed(app: &TestApp) -> (i64, i64, i64) {
    let db = &app.db;
    let a = user(db, "a@example.com", "Passw0rd!", "2026-09-01T00:00:00Z").await;
    let b = user(db, "b@example.com", "Passw0rd!", "2026-09-02T00:00:00Z").await;
    let c = user(db, "c@example.com", "Passw0rd!", "2026-09-03T00:00:00Z").await;
    wallet(db, a, "9").await;
    wallet(db, b, "10").await;
    set_user(
        app,
        a,
        users::Column::LastLoginAt,
        at("2026-09-10T00:00:00Z").into(),
    )
    .await;
    set_user(
        app,
        c,
        users::Column::LastLoginAt,
        at("2026-09-05T00:00:00Z").into(),
    )
    .await;
    set_user(app, c, users::Column::Status, "disabled".into()).await;
    oauth(db, a, "telegram", "tg_alice").await;
    (a, b, c)
}

fn emails(v: &Value) -> Vec<String> {
    data(v)
        .as_array()
        .unwrap()
        .iter()
        .map(|u| u["email"].as_str().unwrap().to_owned())
        .collect()
}

async fn order_of(app: &TestApp, q: &str) -> Vec<String> {
    emails(&app.get(&format!("/api/v1/admin/users?{q}")).await)
}

async fn user_login(app: &TestApp, email: &str, password: &str) -> String {
    let v = app
        .call(
            "POST",
            "/api/v1/auth/login",
            Some(json!({"email": email, "password": password})),
            None,
        )
        .await;
    data(&v)["token"].as_str().unwrap().to_owned()
}

async fn me_status(app: &TestApp, token: &str) -> i64 {
    app.call("GET", "/api/v1/me", None, Some(token)).await["status_code"]
        .as_i64()
        .unwrap()
}

#[tokio::test]
async fn list_filters_sorting_and_shape() {
    let app = TestApp::new().await;
    seed(&app).await;
    let base = "/api/v1/admin/users";
    let v = app.get(base).await;
    assert_eq!(
        emails(&v),
        ["c@example.com", "b@example.com", "a@example.com"]
    );
    assert_eq!(
        v["pagination"],
        json!({"page": 1, "page_size": 20, "total": 3, "total_page": 1})
    );
    let a = &data(&v)[2];
    assert_eq!(a["wallet_balance"], "9.00");
    assert_eq!(data(&v)[0]["wallet_balance"], "0.00", "no wallet account");
    assert!(a.get("password_hash").is_none() && a.get("token_version").is_none());
    assert!(a.get("admin_note").is_none(), "empty admin_note omitted");
    assert!(a["email_verified_at"].is_string());

    assert_eq!(
        order_of(&app, "sort_by=wallet_balance").await,
        ["b@example.com", "a@example.com", "c@example.com"]
    );
    assert_eq!(
        order_of(&app, "sort_by=wallet_balance&sort_order=ASC").await,
        ["c@example.com", "a@example.com", "b@example.com"]
    );
    // Never-logged-in users stay last in both directions.
    assert_eq!(
        order_of(&app, "sort_by=last_login_at&sort_order=asc").await,
        ["c@example.com", "a@example.com", "b@example.com"]
    );
    assert_eq!(
        order_of(&app, "sort_by=last_login_at").await,
        ["a@example.com", "c@example.com", "b@example.com"]
    );
    assert_eq!(
        order_of(&app, "sort_by=created_at&sort_order=asc").await,
        ["a@example.com", "b@example.com", "c@example.com"]
    );
    // Injection attempts fall back to id DESC.
    assert_eq!(
        order_of(&app, "sort_by=id%3BDROP%20TABLE%20users").await,
        ["c@example.com", "b@example.com", "a@example.com"]
    );

    assert_eq!(
        order_of(&app, "keyword=tg_ali").await,
        ["a@example.com"],
        "matches OAuth usernames"
    );
    assert_eq!(order_of(&app, "keyword=b%40").await, ["b@example.com"]);
    assert_eq!(order_of(&app, "status=disabled").await, ["c@example.com"]);
    assert_eq!(
        order_of(
            &app,
            "created_from=2026-09-02T00:00:00Z&created_to=2026-09-02T23:00:00Z"
        )
        .await,
        ["b@example.com"]
    );
    assert_eq!(
        order_of(&app, "last_login_from=2026-09-06T00:00:00Z").await,
        ["a@example.com"]
    );

    let paged = app.get(&format!("{base}?page=2&page_size=1")).await;
    assert_eq!(emails(&paged), ["b@example.com"]);
    assert_eq!(paged["pagination"]["total_page"], 3);

    for (q, msg) in [
        ("user_id=abc", "无效的用户ID"),
        ("user_id=0", "无效的用户ID"),
        ("created_from=yesterday", "请求参数错误"),
    ] {
        let v = app.get(&format!("{base}?{q}")).await;
        assert_eq!(v["status_code"], 400, "{q}");
        assert_eq!(v["msg"], msg, "{q}");
    }
    let one = app
        .get(&format!("{base}?user_id={}", data(&v)[1]["id"]))
        .await;
    assert_eq!(emails(&one), ["b@example.com"]);
}

#[tokio::test]
async fn detail_and_not_found() {
    let app = TestApp::new().await;
    let (a, _, _) = seed(&app).await;
    let v = app.get(&format!("/api/v1/admin/users/{a}")).await;
    let d = data(&v);
    assert_eq!(d["email"], "a@example.com");
    assert_eq!(d["wallet_balance"], "9.00");
    let ids = d["oauth_identities"].as_array().unwrap();
    assert_eq!(ids.len(), 1);
    assert_eq!(ids[0]["provider"], "telegram");
    assert_eq!(ids[0]["username"], "tg_alice");
    assert!(ids[0].get("auth_at").is_none());

    let missing = app.get("/api/v1/admin/users/99999").await;
    assert_eq!(missing["status_code"], 404);
    assert_eq!(missing["msg"], "用户不存在");
    let bad = app.get("/api/v1/admin/users/abc").await;
    assert_eq!(bad["status_code"], 400);
    assert_eq!(bad["msg"], "无效的用户ID");
}

#[tokio::test]
async fn update_user_fields_and_errors() {
    let app = TestApp::new().await;
    let (a, _, _) = seed(&app).await;
    let uri = format!("/api/v1/admin/users/{a}");

    let nothing = app.put(&uri, json!({})).await;
    assert_eq!(nothing["status_code"], 400);
    assert_eq!(nothing["msg"], "请求参数错误");
    let same_status = app
        .put(&uri, json!({"status": "active", "nickname": "  "}))
        .await;
    assert_eq!(same_status["status_code"], 400, "no effective change");
    let taken = app.put(&uri, json!({"email": " B@Example.com "})).await;
    assert_eq!(taken["msg"], "邮箱已注册");
    let invalid = app.put(&uri, json!({"email": "not-an-email"})).await;
    assert_eq!(invalid["msg"], "邮箱格式不正确");
    let malformed = app.put(&uri, json!({"email_verified": "yes"})).await;
    assert_eq!(malformed["status_code"], 400);

    let v = app
        .put(
            &uri,
            json!({"email": " New@Example.com ", "nickname": " Alice ", "admin_note": "vip",
                   "email_verified": false, "locale": "en-US"}),
        )
        .await;
    let d = data(&v);
    assert_eq!(d["email"], "new@example.com");
    assert_eq!(d["display_name"], "Alice");
    assert_eq!(d["admin_note"], "vip");
    assert_eq!(d["locale"], "en-US");
    assert!(d["email_verified_at"].is_null());
    let detail = app.get(&uri).await;
    assert_eq!(data(&detail)["email"], "new@example.com");
    assert_eq!(data(&detail)["admin_note"], "vip");

    let missing = app
        .put("/api/v1/admin/users/99999", json!({"nickname": "x"}))
        .await;
    assert_eq!(missing["status_code"], 404);
}

#[tokio::test]
async fn password_change_and_disable_revoke_tokens() {
    let app = TestApp::new().await;
    let (a, _, _) = seed(&app).await;
    // Verified users can log in (email verification column seeded).
    set_user(
        &app,
        a,
        users::Column::EmailVerifiedAt,
        at("2026-09-01T00:00:00Z").into(),
    )
    .await;
    let token = user_login(&app, "a@example.com", "Passw0rd!").await;
    assert_eq!(me_status(&app, &token).await, 0);

    let uri = format!("/api/v1/admin/users/{a}");
    data(&app.put(&uri, json!({"password": "NewPassw0rd!"})).await);
    assert_eq!(
        me_status(&app, &token).await,
        401,
        "old token revoked by password change"
    );
    let token = user_login(&app, "a@example.com", "NewPassw0rd!").await;
    assert_eq!(me_status(&app, &token).await, 0);

    let before = users::Entity::find_by_id(a)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .token_version;
    let v = app.put(&uri, json!({"status": "DISABLED"})).await;
    assert_eq!(data(&v)["status"], "disabled");
    let after = users::Entity::find_by_id(a)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(after.token_version, before + 1);
    assert!(after.token_invalid_before.is_some());
    assert_eq!(me_status(&app, &token).await, 401);
}

#[tokio::test]
async fn batch_status() {
    let app = TestApp::new().await;
    let (a, b, c) = seed(&app).await;
    let token = user_login(&app, "b@example.com", "Passw0rd!").await;
    assert_eq!(me_status(&app, &token).await, 0);
    let before = users::Entity::find_by_id(b)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .token_version;

    let v = app
        .put(
            "/api/v1/admin/users/batch-status",
            json!({"user_ids": [a, b], "status": " Disabled"}),
        )
        .await;
    assert_eq!(data(&v), &json!({"updated": 2}));
    let row = users::Entity::find_by_id(b)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "disabled");
    assert_eq!(row.token_version, before + 1);
    assert_eq!(me_status(&app, &token).await, 401);

    let v = app
        .put(
            "/api/v1/admin/users/batch-status",
            json!({"user_ids": [c], "status": "active"}),
        )
        .await;
    data(&v);
    let row = users::Entity::find_by_id(c)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "active");
    assert_eq!(row.token_version, 0, "enabling does not revoke");

    for body in [
        json!({"user_ids": [], "status": "active"}),
        json!({"user_ids": [a], "status": "banned"}),
        json!({"user_ids": [a]}),
        json!({"status": "active"}),
    ] {
        let v = app
            .put("/api/v1/admin/users/batch-status", body.clone())
            .await;
        assert_eq!(v["status_code"], 400, "{body}");
    }
}

#[tokio::test]
async fn oauth_unbind_rules() {
    let app = TestApp::new().await;
    let (a, _, c) = seed(&app).await;
    // A has a local password: Telegram can be unbound, then it is gone.
    let v = app
        .delete(&format!("/api/v1/admin/users/{a}/oauth/telegram"))
        .await;
    assert_eq!(data(&v), &json!({"unbound": true}));
    let again = app
        .delete(&format!("/api/v1/admin/users/{a}/oauth/telegram"))
        .await;
    assert_eq!(again["msg"], "当前账号未绑定 Telegram");
    let google = app
        .delete(&format!("/api/v1/admin/users/{a}/oauth/google"))
        .await;
    assert_eq!(google["status_code"], 400);

    // D was created by Telegram login: no usable local password.
    let d = user(&app.db, "d@example.com", "unused", "2026-09-04T00:00:00Z").await;
    set_user(&app, d, users::Column::PasswordSetupRequired, true.into()).await;
    oauth(&app.db, d, "telegram", "tg_d").await;
    oauth(&app.db, d, "google", "g_d").await;
    let locked = app
        .delete(&format!("/api/v1/admin/users/{d}/oauth/telegram"))
        .await;
    assert_eq!(locked["status_code"], 400);
    assert_eq!(
        locked["msg"],
        "请先设置本地密码或绑定其他可用登录方式，再解绑 Telegram"
    );
    // Once Google login is configured it keeps the account reachable.
    setting(
        &app.db,
        "google_auth_config",
        json!({"enabled": true, "client_id": "cid.apps.googleusercontent.com"}),
    )
    .await;
    data(
        &app.delete(&format!("/api/v1/admin/users/{d}/oauth/telegram"))
            .await,
    );
    let last = app
        .delete(&format!("/api/v1/admin/users/{d}/oauth/google"))
        .await;
    assert_eq!(
        last["msg"],
        "请先设置本地密码或绑定其他可用登录方式，再解绑 Google"
    );

    oauth(&app.db, c, "telegram", "tg_c").await;
    let disabled = app
        .delete(&format!("/api/v1/admin/users/{c}/oauth/telegram"))
        .await;
    assert_eq!(disabled["msg"], "账号已禁用");
    let missing = app.delete("/api/v1/admin/users/99999/oauth/telegram").await;
    assert_eq!(missing["status_code"], 404);
    let bad = app.delete("/api/v1/admin/users/x/oauth/google").await;
    assert_eq!(bad["msg"], "无效的用户ID");
}

#[tokio::test]
async fn coupon_usages_with_scope_products() {
    let app = TestApp::new().await;
    let (a, b, _) = seed(&app).await;
    let p = product(&app.db, "scoped", "manual", 1).await;
    let x = coupon(&app.db, "SAVE5", "fixed", &format!("[{p}]")).await;
    let y = coupon(&app.db, "ALL10", "percent", "").await;
    coupon_usage(&app.db, x, a, 9, "1.5").await;
    coupon_usage(&app.db, y, a, 10, "2").await;
    coupon_usage(&app.db, y, b, 11, "3").await;

    let v = app
        .get(&format!("/api/v1/admin/users/{a}/coupon-usages"))
        .await;
    let rows = data(&v).as_array().unwrap();
    assert_eq!(v["pagination"]["total"], 2);
    assert_eq!(rows[0]["coupon_code"], "ALL10");
    assert_eq!(rows[0]["coupon_type"], "percent");
    assert!(rows[0]["scope_ref_ids"].is_null() && rows[0]["scope_products"].is_null());
    assert_eq!(rows[1]["coupon_code"], "SAVE5");
    assert_eq!(rows[1]["order_id"], 9);
    assert_eq!(rows[1]["discount_amount"], "1.50");
    assert_eq!(rows[1]["scope_ref_ids"], json!([p]));
    assert_eq!(
        rows[1]["scope_products"],
        json!([{"id": p, "title": {"zh-CN": "scoped"}}])
    );

    let paged = app
        .get(&format!(
            "/api/v1/admin/users/{a}/coupon-usages?page=2&page_size=1"
        ))
        .await;
    assert_eq!(data(&paged)[0]["coupon_code"], "SAVE5");
}
