//! Admin login, token validation and RBAC.

mod common;

use common::{TestApp, data};
use serde_json::json;
use zs_domain::identity::admin::NewAdmin;

#[tokio::test]
async fn login_contract_and_errors() {
    let app = TestApp::anonymous().await;
    let ok = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username":"admin","password":"Admin12345"})),
            None,
        )
        .await;
    let d = data(&ok);
    assert_eq!(d["requires_totp"], false);
    assert_eq!(d["user"]["username"], "admin");
    assert!(d["expires_at"].as_str().unwrap().ends_with('Z'));

    let bad = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username":"admin","password":"nope"})),
            None,
        )
        .await;
    assert_eq!(bad["status_code"], 401);
    assert_eq!(bad["msg"], "用户名或密码错误");

    let missing = app
        .call("GET", "/api/v1/admin/categories", None, None)
        .await;
    assert_eq!(missing["status_code"], 401);
    assert_eq!(missing["msg"], "缺少 Authorization header");

    let invalid = app
        .call("GET", "/api/v1/admin/categories", None, Some("garbage"))
        .await;
    assert_eq!(invalid["status_code"], 401);
}

#[tokio::test]
async fn password_change_revokes_old_tokens() {
    let app = TestApp::new().await;
    let token = app.admin_token.clone().unwrap();
    let weak = app
        .put(
            "/api/v1/admin/password",
            json!({"old_password":"Admin12345","new_password":"short"}),
        )
        .await;
    assert_eq!(weak["status_code"], 400);
    assert_eq!(weak["msg"], "密码长度至少 8 位");
    let res = app
        .put(
            "/api/v1/admin/password",
            json!({"old_password":"Admin12345","new_password":"NewPass123"}),
        )
        .await;
    data(&res);
    let after = app
        .call("GET", "/api/v1/admin/authz/me", None, Some(&token))
        .await;
    assert_eq!(after["status_code"], 401);
    assert!(!app.login("admin", "NewPass123").await.is_empty());
}

#[tokio::test]
async fn rbac_limits_non_super_admins() {
    let app = TestApp::new().await;
    let hash = bcrypt::hash("Ops12345", 4).unwrap();
    let ops = app
        .services
        .identity
        .admin_auth
        .repo()
        .create(&NewAdmin {
            username: "ops".into(),
            password_hash: hash,
            is_super: false,
        })
        .await
        .unwrap();
    app.services
        .identity
        .authz
        .set_admin_roles(ops.id, &["operations".into()])
        .await
        .unwrap();
    let token = app.login("ops", "Ops12345").await;

    let me = app
        .call("GET", "/api/v1/admin/authz/me", None, Some(&token))
        .await;
    let d = data(&me);
    assert_eq!(d["is_super"], false);
    assert_eq!(d["roles"], json!(["role:operations"]));
    assert!(
        d["policies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["object"] == "/admin/categories/:id")
    );

    // operations may manage categories …
    let created = app
        .call(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"slug":"x","name":{"zh-CN":"x"}})),
            Some(&token),
        )
        .await;
    data(&created);
    // … and inherits readonly_auditor, but has no other rights.
    app.services
        .identity
        .authz
        .set_admin_roles(ops.id, &["readonly_auditor".into()])
        .await
        .unwrap();
    let denied = app
        .call("GET", "/api/v1/admin/categories", None, Some(&token))
        .await;
    assert_eq!(denied["status_code"], 403);
    assert_eq!(denied["msg"], "无权限访问");
}

/// QA-A03 (live QA I-3): the admin login limiter counts only failures, per
/// (account, IP); successful logins never lock a shared IP out, and a success
/// resets the account's counter.
#[tokio::test]
async fn qa_a03_login_limiter_counts_failures_per_account() {
    let mut cfg = common::test_config();
    cfg.security.login_rate_limit.max_attempts = 3;
    let app = TestApp::with_config(cfg).await;
    app.services
        .identity
        .admin_auth
        .repo()
        .create(&NewAdmin {
            username: "ops".into(),
            password_hash: bcrypt::hash("Ops123456", 4).unwrap(),
            is_super: false,
        })
        .await
        .unwrap();
    let login = |user: &'static str, pwd: &'static str| {
        app.call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": user, "password": pwd})),
            None,
        )
    };
    // many successful logins from the same IP are fine
    for _ in 0..6 {
        assert_eq!(login("admin", "Admin12345").await["status_code"], 0);
    }
    // two failures, then a success resets the counter
    for _ in 0..2 {
        assert_eq!(login("admin", "bad").await["status_code"], 401);
    }
    assert_eq!(login("admin", "Admin12345").await["status_code"], 0);
    for _ in 0..3 {
        assert_eq!(login("admin", "bad").await["status_code"], 401);
    }
    // the account is now blocked from this IP, even with the right password …
    let blocked = login("admin", "Admin12345").await;
    assert_eq!(blocked["status_code"], 429, "{blocked}");
    // … but a colleague behind the same IP can still log in
    assert_eq!(login("ops", "Ops123456").await["status_code"], 0);
}
