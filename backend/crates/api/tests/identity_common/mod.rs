//! Test harness of the identity integration tests (in-memory SQLite, real router,
//! configurable limits, raw HTTP status access).

#![allow(
    dead_code,
    reason = "shared by several identity test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test harness: failures should abort the test"
)]

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use http_body_util::BodyExt;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_app::config::Config;
use zs_domain::settings::SettingsStore;

pub struct App {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub cfg: Config,
    pub admin: String,
}

pub fn config() -> Config {
    let mut cfg = Config::default();
    cfg.app.secret_key = "test-app-secret-0123456789abcdef".into();
    cfg.jwt.secret = "test-admin-jwt-secret-0123456789".into();
    cfg.user_jwt.secret = "test-user-jwt-secret-0123456789a".into();
    cfg.database.url = "sqlite::memory:".into();
    // DB-01: a single connection exposes any query issued outside a held transaction.
    cfg.database.max_connections = 1;
    cfg.bootstrap.default_admin_username = "admin".into();
    cfg.bootstrap.default_admin_password = "Admin12345".into();
    // Generous limits; rate-limit tests use their own configuration.
    cfg.security.login_rate_limit.max_attempts = 1000;
    cfg
}

impl App {
    pub async fn new() -> Self {
        Self::with(config()).await
    }

    /// App with `cfg`; logs the bootstrap admin in when it exists.
    pub async fn with(cfg: Config) -> Self {
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        let services = zs_infra::wire::services(&ctx);
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg.clone()));
        let mut me = Self {
            router: app.router,
            db,
            services,
            cfg,
            admin: String::new(),
        };
        let res = me
            .call(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": me.cfg.bootstrap.default_admin_username, "password": me.cfg.bootstrap.default_admin_password})),
                None,
            )
            .await;
        if res["status_code"] == 0 {
            me.admin = res["data"]["token"].as_str().unwrap().to_owned();
        }
        me
    }

    pub async fn raw(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
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
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Value {
        self.raw(method, uri, body, token).await.1
    }

    pub async fn admin_call(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        self.call(method, uri, body, Some(&self.admin)).await
    }

    pub async fn set_setting(&self, key: &str, value: Value) {
        zs_infra::db::repo::settings::SeaSettingsStore::new(self.db.clone())
            .set(key, &value)
            .await
            .unwrap();
    }

    /// Latest verification code sent to `email` for `purpose`.
    pub async fn last_code(&self, email: &str, purpose: &str) -> String {
        use zs_infra::db::entity::email_verify_codes as c;
        c::Entity::find()
            .filter(c::Column::Email.eq(email))
            .filter(c::Column::Purpose.eq(purpose))
            .order_by_desc(c::Column::Id)
            .one(&self.db)
            .await
            .unwrap()
            .expect("a code was sent")
            .code
    }

    /// Count of verification codes stored for `email`.
    pub async fn code_count(&self, email: &str) -> usize {
        use zs_infra::db::entity::email_verify_codes as c;
        c::Entity::find()
            .filter(c::Column::Email.eq(email))
            .all(&self.db)
            .await
            .unwrap()
            .len()
    }

    /// Registers a user through the API (email verification disabled) and returns its token.
    pub async fn register(&self, email: &str, password: &str) -> String {
        self.set_setting(
            "registration_config",
            json!({"registration_enabled": true, "email_verification_enabled": false}),
        )
        .await;
        let res = self
            .call(
                "POST",
                "/api/v1/auth/register",
                Some(json!({"email": email, "password": password, "agreement_accepted": true})),
                None,
            )
            .await;
        data(&res)["token"].as_str().unwrap().to_owned()
    }

    /// Logs a user in (no 2FA) and returns the token.
    pub async fn user_login(&self, email: &str, password: &str) -> String {
        let res = self
            .call(
                "POST",
                "/api/v1/auth/login",
                Some(json!({"email": email, "password": password})),
                None,
            )
            .await;
        data(&res)["token"].as_str().unwrap().to_owned()
    }

    /// Creates a non-super admin through the API and returns `(id, token)`.
    pub async fn create_admin(
        &self,
        username: &str,
        password: &str,
        roles: &[&str],
    ) -> (i64, String) {
        let res = self
            .admin_call(
                "POST",
                "/api/v1/admin/authz/admins",
                Some(json!({"username": username, "password": password})),
            )
            .await;
        let id = data(&res)["id"].as_i64().unwrap();
        let res = self
            .admin_call(
                "PUT",
                &format!("/api/v1/admin/authz/admins/{id}/roles"),
                Some(json!({"roles": roles})),
            )
            .await;
        data(&res);
        let login = self
            .call(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": username, "password": password})),
                None,
            )
            .await;
        (id, data(&login)["token"].as_str().unwrap().to_owned())
    }
}

/// Current TOTP code of a base32 secret.
pub fn totp(secret: &str) -> String {
    zs_app::identity::totp::code_at(secret, Utc::now()).unwrap()
}

/// A 6-digit code that is not valid right now.
pub fn wrong_totp(secret: &str) -> String {
    let now = Utc::now();
    let valid: Vec<String> = [-30, 0, 30]
        .iter()
        .map(|d| {
            zs_app::identity::totp::code_at(secret, now + chrono::Duration::seconds(*d)).unwrap()
        })
        .collect();
    (0..1_000_000)
        .map(|n| format!("{n:06}"))
        .find(|c| !valid.contains(c))
        .unwrap()
}

/// Asserts a success envelope and returns `data`.
pub fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    assert_eq!(v["msg"], "success");
    &v["data"]
}

/// Asserts an error envelope with `code` and zh-CN message `msg`.
pub fn err(v: &Value, code: i64, msg: &str) {
    assert_eq!(v["status_code"], code, "unexpected envelope {v}");
    assert_eq!(v["msg"], msg, "unexpected envelope {v}");
}
