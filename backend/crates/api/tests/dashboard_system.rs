//! `/admin/system/*` (offline version check, unsupported self-update),
//! the disabled ad proxy and the read-only auditor's access.

mod common;

use common::{TestApp, data};
use serde_json::json;

#[tokio::test]
async fn version_and_update_capability() {
    let app = TestApp::new().await;
    let check = app.get("/api/v1/admin/system/version/check").await;
    let c = data(&check);
    let version = c["current_version"].as_str().unwrap().to_owned();
    assert!(version.starts_with('v'));
    assert_eq!(c["current_version"], version.as_str());
    assert_eq!(c["latest_version"], version.as_str());
    assert_eq!(c["has_update"], false);

    let cap = app.get("/api/v1/admin/system/update/capability").await;
    let cap = data(&cap);
    assert_eq!(cap["capability"]["can_update"], false);
    assert_eq!(cap["capability"]["can_restart"], false);
    assert_eq!(cap["capability"]["block_reason"], "source_build");
    assert_eq!(cap["state"]["status"], "idle");
    let status = app.get("/api/v1/admin/system/update/status").await;
    assert_eq!(
        data(&status),
        &json!({"status": "idle", "stage": "idle", "percent": 0, "restart_required": false})
    );

    let start = app
        .post("/api/v1/admin/system/update/start", json!({}))
        .await;
    assert_eq!(start["status_code"], 400);
    assert_eq!(start["msg"], "当前部署方式不支持一键升级");
    let rollback = app
        .post(
            "/api/v1/admin/system/update/rollback",
            json!({"force": true}),
        )
        .await;
    assert_eq!(rollback["status_code"], 400);
    let restart = app.post("/api/v1/admin/system/restart", json!({})).await;
    assert_eq!(restart["status_code"], 400);
}

#[tokio::test]
async fn ad_proxy_is_a_silent_no_op() {
    let app = TestApp::new().await;
    let render = app
        .get("/api/v1/admin/ads/render/dashboard_sponsored?locale=zh-CN")
        .await;
    assert!(data(&render).is_null());
    let impression = app
        .post(
            "/api/v1/admin/ads/impression",
            json!({"impression_token": "x"}),
        )
        .await;
    assert!(data(&impression).is_null());
}

#[tokio::test]
async fn readonly_auditor_sees_dashboard_but_not_users() {
    let app = TestApp::new().await;
    let created = app
        .post(
            "/api/v1/admin/authz/admins",
            json!({"username": "auditor", "password": "Auditor123"}),
        )
        .await;
    let id = data(&created)["id"].as_i64().unwrap();
    data(
        &app.put(
            &format!("/api/v1/admin/authz/admins/{id}/roles"),
            json!({"roles": ["readonly_auditor"]}),
        )
        .await,
    );
    let token = app.login("auditor", "Auditor123").await;
    let (app, token) = (&app, token.as_str());
    let call = |uri: &'static str| app.call("GET", uri, None, Some(token));
    data(&call("/api/v1/admin/dashboard/overview?range=today").await);
    data(&call("/api/v1/admin/dashboard/inventory-alerts").await);
    data(&call("/api/v1/admin/ads/render/dashboard_sponsored").await);
    assert_eq!(call("/api/v1/admin/users").await["status_code"], 403);
    assert_eq!(
        call("/api/v1/admin/system/version/check").await["status_code"],
        403
    );
}
