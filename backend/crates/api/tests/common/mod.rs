//! Integration-test harness: in-memory SQLite + the real router.

#![allow(
    dead_code,
    reason = "shared by several test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;
use zs_app::config::Config;

pub struct TestApp {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub admin_token: Option<String>,
}

pub fn test_config() -> Config {
    let mut cfg = Config::default();
    cfg.app.secret_key = "test-app-secret-0123456789abcdef".into();
    cfg.jwt.secret = "test-admin-jwt-secret-0123456789".into();
    cfg.user_jwt.secret = "test-user-jwt-secret-0123456789a".into();
    cfg.database.url = "sqlite::memory:".into();
    cfg.database.max_connections = 1;
    cfg.bootstrap.default_admin_username = "admin".into();
    cfg.bootstrap.default_admin_password = "Admin12345".into();
    // Tests share one client IP; keep the in-process login limiter out of the way.
    cfg.security.login_rate_limit.max_attempts = 1000;
    cfg
}

impl TestApp {
    /// App with the default super admin logged in (`admin_token` set).
    pub async fn new() -> Self {
        let mut app = Self::anonymous().await;
        app.admin_token = Some(app.login("admin", "Admin12345").await);
        app
    }

    /// App without any token.
    pub async fn anonymous() -> Self {
        Self::with_config(test_config()).await
    }

    /// App without any token, built from `cfg`.
    pub async fn with_config(cfg: Config) -> Self {
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        let services = zs_infra::wire::services(&ctx);
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
        Self {
            router: app.router,
            db,
            services,
            admin_token: None,
        }
    }

    /// Logs in an administrator and returns the access token.
    pub async fn login(&self, username: &str, password: &str) -> String {
        let body = serde_json::json!({"username": username, "password": password});
        let res = self
            .call("POST", "/api/v1/admin/login", Some(body), None)
            .await;
        data(&res)["token"].as_str().unwrap().to_owned()
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Value {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.router.clone().oneshot(req).await.unwrap();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    pub async fn get(&self, uri: &str) -> Value {
        self.call("GET", uri, None, self.admin_token.as_deref())
            .await
    }

    pub async fn post(&self, uri: &str, body: Value) -> Value {
        self.call("POST", uri, Some(body), self.admin_token.as_deref())
            .await
    }

    pub async fn put(&self, uri: &str, body: Value) -> Value {
        self.call("PUT", uri, Some(body), self.admin_token.as_deref())
            .await
    }

    pub async fn patch(&self, uri: &str, body: Value) -> Value {
        self.call("PATCH", uri, Some(body), self.admin_token.as_deref())
            .await
    }

    pub async fn delete(&self, uri: &str) -> Value {
        self.call("DELETE", uri, None, self.admin_token.as_deref())
            .await
    }
}

/// Asserts a success envelope and returns `data`.
pub fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    assert_eq!(v["msg"], "success");
    &v["data"]
}
