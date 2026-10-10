//! Optional reverse-proxy certificate domain reads (RSL-03, RSL-06).

mod reseller_common;

use axum::http::StatusCode;
use reseller_common::{App, config, data};
use serde_json::json;

const ALLOW: &str = "/api/v1/internal/reseller-domains/allow";
const LIST: &str = "/api/v1/internal/reseller-domains";

#[tokio::test]
async fn domain_setup_exposes_optional_shared_target_to_console_user() {
    let mut cfg = config();
    cfg.reseller.cname_target = "connect.example.com".into();
    let app = App::with(cfg).await;
    let (_, user_token, _) = app.reseller("setup@example.test", "0").await;
    assert_eq!(
        data(
            &app.call(
                "GET",
                "/api/v1/reseller/domain-setup",
                None,
                Some(&user_token)
            )
            .await
        ),
        &json!({"cname_target": "connect.example.com", "review_mode": "manual"})
    );
    assert_ne!(
        app.raw("GET", "/api/v1/reseller/domain-setup", None, None, &[])
            .await
            .1["status_code"],
        0
    );
}

#[tokio::test]
async fn admin_can_set_cdn_cname_target_and_review_mode() {
    let app = App::with(config()).await;
    let (_, user_token, _) = app.reseller("cdn-setup@example.test", "0").await;
    assert_eq!(
        data(
            &app.admin_call(
                "GET",
                "/api/v1/admin/settings?key=reseller_domain_review_config",
                None
            )
            .await
        ),
        &json!({"mode": "manual", "cname_target": ""})
    );
    let denied = app
        .admin_call(
            "PUT",
            "/api/v1/admin/settings",
            Some(json!({
                "key": "reseller_domain_review_config",
                "value": {"mode": "auto", "cname_target": ""}
            })),
        )
        .await;
    assert_ne!(denied["status_code"], 0);
    assert_eq!(
        data(
            &app.admin_call(
                "PUT",
                "/api/v1/admin/settings",
                Some(json!({
                    "key": "reseller_domain_review_config",
                    "value": {"mode": "auto", "cname_target": " Edge.CDN.Example.COM. "}
                }))
            )
            .await
        ),
        &json!({"mode": "auto", "cname_target": "edge.cdn.example.com"})
    );
    assert_eq!(
        data(
            &app.call(
                "GET",
                "/api/v1/reseller/domain-setup",
                None,
                Some(&user_token)
            )
            .await
        ),
        &json!({"cname_target": "edge.cdn.example.com", "review_mode": "auto"})
    );
}

#[tokio::test]
async fn disabled_reseller_needs_no_proxy_domain_configuration() {
    let mut cfg = config();
    cfg.reseller.enabled = false;
    cfg.reseller.domain_sync_token = "test-sync-token".into();
    let app = App::with(cfg).await;
    assert_eq!(
        app.raw("GET", "/health", None, None, &[]).await.0,
        StatusCode::OK
    );
    assert_eq!(
        app.raw("GET", "/api/v1/public/config", None, None, &[])
            .await
            .0,
        StatusCode::OK
    );
    assert_eq!(
        app.raw(
            "GET",
            &format!("{ALLOW}?domain=shop.example.com"),
            None,
            None,
            &[]
        )
        .await
        .0,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        app.raw("GET", LIST, None, Some("test-sync-token"), &[])
            .await
            .0,
        StatusCode::NOT_FOUND
    );
}

// RSL-03: only active, verified domains on active profiles may get certificates.
#[tokio::test]
async fn proxy_eligibility_follows_domain_and_profile_state() {
    let mut cfg = config();
    cfg.reseller.domain_sync_token = "test-sync-token".into();
    let app = App::with(cfg).await;
    let (_, user_token, pid) = app.reseller("edge@example.test", "0").await;
    let system = app.system_domain(pid, "edge").await;
    let submitted = app
        .call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "Edge.Custom.test"})),
            Some(&user_token),
        )
        .await;
    let domain_id = data(&submitted)["id"].as_i64().expect("domain id");

    assert_eq!(
        app.raw("GET", &format!("{ALLOW}?domain={system}"), None, None, &[])
            .await
            .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        app.raw(
            "GET",
            &format!("{ALLOW}?domain=EDGE.CUSTOM.TEST."),
            None,
            None,
            &[]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        app.raw(
            "GET",
            &format!("{ALLOW}?domain=main.example.com"),
            None,
            None,
            &[]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    for invalid in [
        "",
        "localhost",
        "edge.custom.test:443",
        "not-a-domain",
        "evil/custom.test",
    ] {
        let url = format!("{ALLOW}?domain={invalid}");
        assert_ne!(
            app.raw("GET", &url, None, None, &[]).await.0,
            StatusCode::NO_CONTENT,
            "{invalid}"
        );
    }
    assert_eq!(
        app.raw("GET", ALLOW, None, None, &[]).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        app.raw("GET", LIST, None, None, &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        app.raw("GET", LIST, None, Some("wrong"), &[]).await.0,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        data(
            &app.raw("GET", LIST, None, Some("test-sync-token"), &[])
                .await
                .1
        ),
        &json!({"domains": [system]})
    );

    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/domains/{domain_id}/approve"),
            None,
        )
        .await,
    );
    assert_eq!(
        app.raw(
            "GET",
            &format!("{ALLOW}?domain=EDGE.CUSTOM.TEST."),
            None,
            None,
            &[]
        )
        .await
        .0,
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        data(
            &app.raw("GET", LIST, None, Some("test-sync-token"), &[])
                .await
                .1
        ),
        &json!({"domains": ["edge.custom.test", "edge.shop.example.com"]})
    );

    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/domains/{domain_id}/disable"),
            None,
        )
        .await,
    );
    assert_eq!(
        app.raw(
            "GET",
            &format!("{ALLOW}?domain=edge.custom.test"),
            None,
            None,
            &[]
        )
        .await
        .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        data(
            &app.raw("GET", LIST, None, Some("test-sync-token"), &[])
                .await
                .1
        ),
        &json!({"domains": [system]})
    );
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/profiles/{pid}/disable"),
            Some(json!({"reason": "test"})),
        )
        .await,
    );
    assert_eq!(
        app.raw("GET", &format!("{ALLOW}?domain={system}"), None, None, &[])
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        data(
            &app.raw("GET", LIST, None, Some("test-sync-token"), &[])
                .await
                .1
        ),
        &json!({"domains": []})
    );
}

#[tokio::test]
async fn inventory_is_off_without_a_sync_token() {
    let app = App::new().await;
    assert_eq!(
        app.raw("GET", LIST, None, None, &[]).await.0,
        StatusCode::NOT_FOUND
    );
}
