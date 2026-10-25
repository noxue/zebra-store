//! AUTH-03 / RISK-06: personal machine credentials keep realm, RBAC and revocation checks.
mod identity_common;
use identity_common::{App, config, data};
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{extra::personal_api_keys, users};

async fn exchange(app: &App, path: &str, key: &str) -> Value {
    app.call("POST", path, Some(json!({"api_key":key})), None)
        .await
}
#[tokio::test]
async fn admin_key_lifecycle_revokes_derived_tokens_and_preserves_browser_session() {
    let app = App::new().await;
    let uri = "/api/v1/admin/api-key";
    assert!(data(&app.admin_call("GET", uri, None).await).is_null());
    let created = app.admin_call("POST", uri, None).await;
    let key = data(&created)["api_key"].as_str().unwrap();
    assert_eq!(key.len(), 47);
    assert!(key.starts_with("zsa_"));
    assert_eq!(app.admin_call("POST", uri, None).await["status_code"], 400);
    let status = app.admin_call("GET", uri, None).await;
    assert!(data(&status).get("api_key").is_none());
    assert!(data(&status).get("key_hash").is_none());
    let row = personal_api_keys::Entity::find()
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.key_hash, zs_shared::crypto::sha256_hex(key.as_bytes()));
    assert!(!format!("{row:?}").contains(key));
    let session = exchange(&app, "/api/v1/admin/api-key/token", key).await;
    let token = data(&session)["token"].as_str().unwrap();
    assert_eq!(
        app.call("GET", "/api/v1/admin/categories", None, Some(token))
            .await["status_code"],
        0
    );
    assert_eq!(
        exchange(&app, "/api/v1/auth/api-key/token", key).await["status_code"],
        401
    );
    assert_eq!(
        app.call("GET", "/api/v1/me", None, Some(token)).await["status_code"],
        401
    );
    let rotated = app.admin_call("PUT", uri, None).await;
    let new_key = data(&rotated)["api_key"].as_str().unwrap();
    assert_ne!(key, new_key);
    assert_eq!(
        exchange(&app, "/api/v1/admin/api-key/token", key).await["status_code"],
        401
    );
    assert_eq!(
        app.call("GET", uri, None, Some(token)).await["status_code"],
        401
    );
    let session = exchange(&app, "/api/v1/admin/api-key/token", new_key).await;
    let new_token = data(&session)["token"].as_str().unwrap();
    data(&app.admin_call("DELETE", uri, None).await);
    assert_eq!(
        app.call("GET", uri, None, Some(new_token)).await["status_code"],
        401
    );
    assert_eq!(
        exchange(&app, "/api/v1/admin/api-key/token", new_key).await["status_code"],
        401
    );
    assert!(data(&app.admin_call("GET", uri, None).await).is_null());
    assert_eq!(app.admin_call("PUT", uri, None).await["status_code"], 400);
}
#[tokio::test]
async fn user_keys_are_private_and_cannot_cross_realms_or_disabled_accounts() {
    let app = App::new().await;
    let a = app.register("apikey-a@example.com", "User12345").await;
    let b = app.register("apikey-b@example.com", "User12345").await;
    let uri = "/api/v1/me/api-key";
    assert_eq!(app.call("POST", uri, None, None).await["status_code"], 401);
    let created = app.call("POST", uri, None, Some(&a)).await;
    let key = data(&created)["api_key"].as_str().unwrap();
    assert!(data(&app.call("GET", uri, None, Some(&b)).await).is_null());
    data(&app.call("DELETE", uri, None, Some(&b)).await);
    let session = exchange(&app, "/api/v1/auth/api-key/token", key).await;
    let token = data(&session)["token"].as_str().unwrap();
    assert_eq!(
        data(&app.call("GET", "/api/v1/me", None, Some(token)).await)["email"],
        "apikey-a@example.com"
    );
    assert_eq!(
        app.call("GET", "/api/v1/admin/categories", None, Some(token))
            .await["status_code"],
        401
    );
    assert_eq!(
        exchange(&app, "/api/v1/admin/api-key/token", key).await["status_code"],
        401
    );
    let owner = data(&app.call("GET", uri, None, Some(&a)).await)["owner_id"]
        .as_i64()
        .unwrap();
    let mut user = users::Entity::find_by_id(owner)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    user.status = Set("disabled".into());
    user.update(&app.db).await.unwrap();
    assert_eq!(
        exchange(&app, "/api/v1/auth/api-key/token", key).await["status_code"],
        401
    );
    assert_eq!(
        app.call("GET", "/api/v1/me", None, Some(token)).await["status_code"],
        401
    );
}
#[tokio::test]
async fn user_rotation_and_delete_revoke_only_the_key_sessions() {
    let app = App::new().await;
    let browser = app.register("apikey-rotate@example.com", "User12345").await;
    let uri = "/api/v1/me/api-key";
    let created = app.call("POST", uri, None, Some(&browser)).await;
    let key = data(&created)["api_key"].as_str().unwrap();
    let session = exchange(&app, "/api/v1/auth/api-key/token", key).await;
    let token = data(&session)["token"].as_str().unwrap();
    let rotated = app.call("PUT", uri, None, Some(&browser)).await;
    let new_key = data(&rotated)["api_key"].as_str().unwrap();
    assert_eq!(
        exchange(&app, "/api/v1/auth/api-key/token", key).await["status_code"],
        401
    );
    assert_eq!(
        app.call("GET", "/api/v1/me", None, Some(token)).await["status_code"],
        401
    );
    let session = exchange(&app, "/api/v1/auth/api-key/token", new_key).await;
    let token = data(&session)["token"].as_str().unwrap();
    data(&app.call("DELETE", uri, None, Some(&browser)).await);
    assert_eq!(
        app.call("GET", "/api/v1/me", None, Some(token)).await["status_code"],
        401
    );
    assert_eq!(
        exchange(&app, "/api/v1/auth/api-key/token", new_key).await["status_code"],
        401
    );
    assert_eq!(
        app.call("GET", "/api/v1/me", None, Some(&browser)).await["status_code"],
        0
    );
}
#[tokio::test]
async fn key_exchange_is_rate_limited_and_never_accepts_password_or_bearer_as_key() {
    let mut cfg = config();
    cfg.security.login_rate_limit.max_attempts = 2;
    let app = App::with(cfg).await;
    for key in ["invalid", app.admin.as_str()] {
        assert_eq!(
            exchange(&app, "/api/v1/admin/api-key/token", key).await["status_code"],
            401
        );
    }
    let (status, body) = app
        .raw(
            "POST",
            "/api/v1/admin/api-key/token",
            Some(json!({"api_key":"invalid"})),
            None,
        )
        .await;
    assert_eq!(status, axum::http::StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(body["status_code"], 429);
}

#[tokio::test]
async fn admin_key_inherits_current_rbac_without_privilege_escalation() {
    let app = App::new().await;
    let account = app
        .admin_call(
            "POST",
            "/api/v1/admin/authz/admins",
            Some(json!({"username":"keyauditor","password":"User12345"})),
        )
        .await;
    let id = data(&account)["id"].as_i64().unwrap();
    data(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{id}/roles"),
            Some(json!({"roles":["readonly_auditor"]})),
        )
        .await,
    );
    let login = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username":"keyauditor","password":"User12345"})),
            None,
        )
        .await;
    let browser = data(&login)["token"].as_str().unwrap();
    let created = app
        .call("POST", "/api/v1/admin/api-key", None, Some(browser))
        .await;
    let key = data(&created)["api_key"].as_str().unwrap();
    let session = exchange(&app, "/api/v1/admin/api-key/token", key).await;
    let token = data(&session)["token"].as_str().unwrap();
    assert_eq!(
        app.call("GET", "/api/v1/admin/dashboard/overview", None, Some(token))
            .await["status_code"],
        0
    );
    assert_eq!(
        app.call(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"slug":"cannot-create","name":{"zh-CN":"禁止"}})),
            Some(token)
        )
        .await["status_code"],
        403
    );
    data(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/authz/admins/{id}/roles"),
            Some(json!({"roles":[]})),
        )
        .await,
    );
    assert_eq!(
        app.call("GET", "/api/v1/admin/dashboard/overview", None, Some(token))
            .await["status_code"],
        403
    );
    assert_eq!(
        app.call("GET", "/api/v1/admin/api-key", None, Some(token))
            .await["status_code"],
        0
    );
}

#[tokio::test]
async fn concurrent_generation_keeps_exactly_one_usable_key() {
    let app = App::new().await;
    let (a, b) = tokio::join!(
        app.admin_call("POST", "/api/v1/admin/api-key", None),
        app.admin_call("POST", "/api/v1/admin/api-key", None)
    );
    assert_eq!([&a, &b].iter().filter(|v| v["status_code"] == 0).count(), 1);
    let row = personal_api_keys::Entity::find()
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(row.len(), 1);
}
