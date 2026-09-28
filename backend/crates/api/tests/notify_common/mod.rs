//! Helpers shared by the `notify_*` integration tests: an app whose outbound
//! Telegram / Feishu / callback traffic goes to a local mock server, and a
//! signed channel API client.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::any;
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_infra::notify::safe_http::AddressPolicy;
use zs_infra::wire::notify::Adapters;

use crate::common::{TestApp, data, test_config};

/// One request received by the mock server.
#[derive(Debug, Clone)]
pub struct Hit {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: HeaderMap,
    pub body: String,
}

impl Hit {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// Mock of Telegram, Feishu and the bot callback endpoint.
#[derive(Clone, Default)]
pub struct Mock {
    pub hits: Arc<Mutex<Vec<Hit>>>,
}

impl Mock {
    pub fn hits(&self) -> Vec<Hit> {
        self.hits.lock().unwrap().clone()
    }

    pub fn paths(&self) -> Vec<String> {
        self.hits().into_iter().map(|h| h.path).collect()
    }
}

async fn mock_handler(State(mock): State<Mock>, req: Request) -> axum::response::Response {
    let (parts, body) = req.into_parts();
    let bytes: Bytes = body
        .collect()
        .await
        .map(|b| b.to_bytes())
        .unwrap_or_default();
    let hit = Hit {
        method: parts.method.to_string(),
        path: parts.uri.path().to_owned(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
        headers: parts.headers.clone(),
        body: String::from_utf8_lossy(&bytes).into_owned(),
    };
    mock.hits.lock().unwrap().push(hit.clone());
    let path = hit.path.as_str();
    if path.starts_with("/open-apis/auth/") {
        return axum::Json(
            json!({"code": 0, "msg": "ok", "tenant_access_token": "t-mock", "expire": 7200}),
        )
        .into_response();
    }
    if path.starts_with("/open-apis/im/") {
        return axum::Json(json!({"code": 0, "msg": "success"})).into_response();
    }
    if path.starts_with("/bot") {
        // Chat 666 has blocked the bot.
        if hit.body.contains("\"666\"") || hit.body.contains("\r\n666\r\n") {
            return axum::Json(
                json!({"ok": false, "description": "Forbidden: bot was blocked by the user"}),
            )
            .into_response();
        }
        return axum::Json(json!({"ok": true, "result": {}})).into_response();
    }
    if path.starts_with("/internal/") {
        return (StatusCode::OK, axum::Json(json!({"ok": true}))).into_response();
    }
    StatusCode::NOT_FOUND.into_response()
}

/// Starts the mock server; returns its base URL.
pub async fn start_mock(mock: &Mock) -> String {
    let app = Router::new()
        .fallback(any(mock_handler))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

/// Test app with the default admin logged in; outbound traffic goes to `base`.
pub async fn app_with(base: &str, callback_policy: AddressPolicy) -> TestApp {
    let cfg = test_config();
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let mut services = zs_infra::wire::services(&ctx);
    services.notify = zs_infra::wire::notify::build_with(
        &ctx,
        &Adapters {
            telegram_base: base.to_owned(),
            feishu_base: base.to_owned(),
            callback_policy,
            broadcast_interval: Duration::ZERO,
        },
    );
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
    let mut t = TestApp {
        router: app.router,
        db,
        services,
        admin_token: None,
    };
    t.admin_token = Some(t.login("admin", "Admin12345").await);
    t
}

/// Writes a raw setting.
pub async fn set_setting(app: &TestApp, key: &str, value: Value) {
    use zs_domain::settings::SettingsStore;
    zs_infra::db::repo::settings::SeaSettingsStore::new(app.db.clone())
        .set(key, &value)
        .await
        .unwrap();
}

/// Creates a channel client through the admin API; returns `(id, key, secret)`.
pub async fn channel_client(
    app: &TestApp,
    bot_token: &str,
    callback_url: &str,
) -> (i64, String, String) {
    let res = app
        .post(
            "/api/v1/admin/channel-clients",
            json!({"name": "Bot", "channel_type": "telegram_bot", "bot_token": bot_token, "callback_url": callback_url}),
        )
        .await;
    let d = data(&res);
    (
        d["id"].as_i64().unwrap(),
        d["channel_key"].as_str().unwrap().to_owned(),
        d["channel_secret"].as_str().unwrap().to_owned(),
    )
}

/// A channel API response.
#[derive(Debug)]
pub struct ChannelRes {
    pub status: u16,
    pub body: Value,
}

/// Sends a request to `/api/v1/channel{path}` with explicit auth headers.
pub async fn channel_raw(
    app: &TestApp,
    method: &str,
    path_and_query: &str,
    body: Option<Value>,
    headers: &[(&str, String)],
) -> ChannelRes {
    let uri = format!("/api/v1/channel{path_and_query}");
    let mut req = Request::builder().method(method).uri(&uri);
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let req = match body {
        Some(b) => req
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(b.to_string()))
            .unwrap(),
        None => req.body(Body::empty()).unwrap(),
    };
    let res = app.router.clone().oneshot(req).await.unwrap();
    let status = res.status().as_u16();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    ChannelRes {
        status,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

/// Signed headers for a channel request (`timestamp_offset` shifts the clock).
pub fn signed_headers(
    key: &str,
    secret: &str,
    method: &str,
    path: &str,
    body: &[u8],
    timestamp_offset: i64,
) -> Vec<(&'static str, String)> {
    let ts = chrono::Utc::now().timestamp() + timestamp_offset;
    let signature = zs_shared::sign::sign(secret, method, path, ts, body);
    vec![
        ("Dujiao-Next-Channel-Key", key.to_owned()),
        ("Dujiao-Next-Channel-Timestamp", ts.to_string()),
        ("Dujiao-Next-Channel-Signature", signature),
    ]
}

/// Sends a correctly signed channel request.
pub async fn channel(
    app: &TestApp,
    creds: &(i64, String, String),
    method: &str,
    path_and_query: &str,
    body: Option<Value>,
) -> ChannelRes {
    let path = format!(
        "/api/v1/channel{}",
        path_and_query.split('?').next().unwrap_or_default()
    );
    let raw = body.as_ref().map(ToString::to_string).unwrap_or_default();
    let headers = signed_headers(&creds.1, &creds.2, method, &path, raw.as_bytes(), 0);
    channel_raw(app, method, path_and_query, body, &headers).await
}

/// Asserts a channel success envelope and returns `data`.
pub fn ok_data(res: &ChannelRes) -> Value {
    assert_eq!(res.status, 200, "unexpected {res:?}");
    assert_eq!(res.body["status_code"], 0, "unexpected {res:?}");
    assert_eq!(res.body["msg"], "success");
    assert!(res.body["request_id"].is_string());
    res.body["data"].clone()
}
