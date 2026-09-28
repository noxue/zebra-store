//! Helpers shared by the `content_*` integration tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![expect(clippy::unwrap_used, reason = "test harness")]

use std::path::PathBuf;

use axum::body::{Body, Bytes};
use axum::http::{HeaderMap, Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

use crate::common::{TestApp, test_config};

/// A unique temporary upload root.
pub fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zs-content-{name}-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Logged-in app whose uploads go to `upload_dir`.
pub async fn app_with_uploads(upload_dir: &str) -> TestApp {
    let mut cfg = test_config();
    cfg.upload.dir = upload_dir.to_owned();
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let services = zs_infra::wire::services(&ctx);
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
    let mut app = TestApp {
        router: app.router,
        db,
        services,
        admin_token: None,
    };
    app.admin_token = Some(app.login("admin", "Admin12345").await);
    app
}

/// Sends a raw request and returns status, headers and body bytes.
pub async fn raw(router: &axum::Router, req: Request<Body>) -> (StatusCode, HeaderMap, Bytes) {
    let res = router.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, headers, bytes)
}

/// GET with extra headers, returning the raw response.
pub async fn get_raw(
    router: &axum::Router,
    uri: &str,
    headers: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Bytes) {
    let mut req = Request::builder().method("GET").uri(uri);
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    raw(router, req.body(Body::empty()).unwrap()).await
}

/// `POST /api/v1/admin/upload` with a multipart body.
pub async fn upload(app: &TestApp, filename: &str, bytes: &[u8], scene: Option<&str>) -> Value {
    upload_with_token(app, filename, bytes, scene, app.admin_token.as_deref()).await
}

pub async fn upload_with_token(
    app: &TestApp,
    filename: &str,
    bytes: &[u8],
    scene: Option<&str>,
    token: Option<&str>,
) -> Value {
    upload_at(app, "/api/v1/admin/upload", filename, bytes, scene, token).await
}

/// Upload to `uri` (e.g. with `?lang=en-US`).
pub async fn upload_at(
    app: &TestApp,
    uri: &str,
    filename: &str,
    bytes: &[u8],
    scene: Option<&str>,
    token: Option<&str>,
) -> Value {
    const BOUNDARY: &str = "zebraBoundary7MA4YWxkTrZu0gW";
    let mut body = Vec::new();
    if let Some(scene) = scene {
        body.extend_from_slice(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"scene\"\r\n\r\n{scene}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!(
            "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: application/octet-stream\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{BOUNDARY}--\r\n").as_bytes());
    let mut req = Request::builder().method("POST").uri(uri).header(
        header::CONTENT_TYPE,
        format!("multipart/form-data; boundary={BOUNDARY}"),
    );
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    let (_, _, bytes) = raw(&app.router, req.body(Body::from(body)).unwrap()).await;
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

/// A minimal PNG header of the given size (enough for dimension parsing).
pub fn png(width: u32, height: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    v.extend_from_slice(&width.to_be_bytes());
    v.extend_from_slice(&height.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}
