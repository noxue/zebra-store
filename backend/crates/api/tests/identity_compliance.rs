//! Compliance acknowledgement endpoints and the [C] route gate.

#![expect(
    clippy::unwrap_used,
    reason = "integration test helpers abort on failure"
)]

mod identity_common;

use axum::body::Body;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::{Next, from_fn, from_fn_with_state};
use axum::routing::get;
use http_body_util::BodyExt;
use identity_common::{App, data, err};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_api::middleware::compliance::{ComplianceAcked, require};
use zs_app::identity::admin_auth::AdminPrincipal;

const S1: &str = "我已阅读并理解上述合规声明提醒";
const S2: &str = "知悉相关法律风险";
const S3: &str = "并确认自行承担部署运营和收费行为产生的法律责任";

/// A router with a gated route, authenticated as an admin with `is_super`.
fn gated(app: &App, is_super: bool) -> axum::Router {
    let state = zs_api::AppState::new(app.services.clone(), app.cfg.clone());
    let as_admin = move |mut req: Request, next: Next| async move {
        req.extensions_mut().insert(AdminPrincipal {
            id: 1,
            username: "x".into(),
            is_super,
        });
        next.run(req).await
    };
    axum::Router::new()
        .route("/layer", get(|| async { "ok" }))
        .route_layer(from_fn_with_state(state.clone(), require))
        .route("/extractor", get(|_: ComplianceAcked| async { "ok" }))
        .layer(from_fn(zs_api::response::render_errors))
        .layer(from_fn(as_admin))
        .with_state(state)
}

async fn get_json(router: &axum::Router, uri: &str) -> (StatusCode, Value) {
    let res = router
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or_else(|_| json!(String::from_utf8_lossy(&bytes))),
    )
}

// ADM-05: gate messages, exact text, super-admin only, idempotent.
#[tokio::test]
async fn acknowledge_and_gate() {
    let app = App::new().await;
    let st = app
        .admin_call("GET", "/api/v1/admin/compliance/status", None)
        .await;
    assert_eq!(data(&st), &json!({"acknowledged": false}));

    for (is_super, msg) in [
        (true, "compliance_required"),
        (false, "compliance_required_by_super_admin"),
    ] {
        let router = gated(&app, is_super);
        for uri in ["/layer", "/extractor"] {
            let (status, body) = get_json(&router, uri).await;
            assert_eq!(status, StatusCode::OK);
            err(&body, 403, msg);
        }
    }

    let (_, ops) = app
        .create_admin("sysop", "Sysop1234", &["system_admin"])
        .await;
    let body = json!({"segment1": S1, "segment2": S2, "segment3": S3});
    err(
        &app.call(
            "POST",
            "/api/v1/admin/compliance/acknowledge",
            Some(body.clone()),
            Some(&ops),
        )
        .await,
        403,
        "compliance.error.super_admin_required",
    );
    let short = json!({"segment1": S1, "segment2": S2, "segment3": &S3[..S3.len() - 3]});
    err(
        &app.admin_call("POST", "/api/v1/admin/compliance/acknowledge", Some(short))
            .await,
        400,
        "compliance.error.text_mismatch",
    );
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/compliance/acknowledge",
            Some(json!({"segment1": S1})),
        )
        .await,
        400,
        // Original `RespondBindError`: every missing required field, in struct order.
        "Segment2: 不能为空; Segment3: 不能为空",
    );
    let ok = app
        .admin_call(
            "POST",
            "/api/v1/admin/compliance/acknowledge",
            Some(body.clone()),
        )
        .await;
    assert_eq!(data(&ok), &json!({"already_acknowledged": false}));
    let again = app
        .admin_call("POST", "/api/v1/admin/compliance/acknowledge", Some(body))
        .await;
    assert_eq!(data(&again), &json!({"already_acknowledged": true}));

    let st = app
        .admin_call("GET", "/api/v1/admin/compliance/status", None)
        .await;
    let d = data(&st);
    assert_eq!(d["acknowledged"], true);
    assert_eq!(d["acknowledged_by_username"], "admin");
    assert_eq!(d["version"], "v1");
    assert!(d["acknowledged_at"].as_str().unwrap().ends_with('Z'));

    let router = gated(&app, false);
    assert_eq!(get_json(&router, "/layer").await.1, json!("ok"));
    assert_eq!(get_json(&router, "/extractor").await.1, json!("ok"));

    // The flag is restored from storage on start-up.
    let fresh = zs_app::identity::compliance::ComplianceService::new(
        std::sync::Arc::new(zs_infra::db::repo::settings::SeaSettingsStore::new(
            app.db.clone(),
        )),
        std::sync::Arc::new(zs_shared::clock::SystemClock),
    );
    assert!(!fresh.is_acknowledged());
    fresh.load().await.unwrap();
    assert!(fresh.is_acknowledged());
}
