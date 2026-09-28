//! Tenant resolution middleware, console isolation and the `/public/config` overlay
//! (RSL-03, RSL-06, RSL-08).

mod reseller_common;

use axum::http::StatusCode;
use reseller_common::{App, BASE, config, data, err};
use serde_json::json;
use zs_domain::content::public::Tenant;

// RSL-06: host normalisation + unknown / unverified hosts are 404 "site unavailable".
#[tokio::test]
async fn rsl06_resolves_hosts_and_rejects_unknown_sites() {
    let app = App::new().await;
    let (_, _, pid) = app.reseller("r1@example.test", "0").await;
    let host = app.system_domain(pid, "hello").await;
    assert_eq!(host, format!("hello.{BASE}"));

    // Upper case, trailing dot and port resolve to the live reseller site.
    let (status, body) = app
        .raw(
            "GET",
            "/api/v1/public/config",
            None,
            None,
            &[("host", "Hello.SHOP.example.com.:8080")],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    data(&body);

    for unknown in ["nobody.shop.example.com", "unknown.test"] {
        let (status, body) = app
            .raw(
                "GET",
                "/api/v1/public/config",
                None,
                None,
                &[("host", unknown)],
            )
            .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(
            body,
            json!({"code": "not_found", "message": "site unavailable"})
        );
    }

    // A submitted (pending, unverified) custom domain does not serve traffic.
    let (_, token, _) = app.reseller("r2@example.test", "0").await;
    data(
        &app.call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "custom.test"})),
            Some(&token),
        )
        .await,
    );
    let (status, _) = app
        .raw(
            "GET",
            "/api/v1/public/config",
            None,
            None,
            &[("host", "custom.test")],
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Main hosts and requests without Host are the main shop.
    let (status, _) = app
        .raw(
            "GET",
            "/api/v1/public/config",
            None,
            None,
            &[("host", "localhost:5173")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

// RSL-03: disabling the profile turns its live domain unavailable immediately (cache invalidated).
#[tokio::test]
async fn rsl03_disabled_profile_domain_is_unavailable() {
    let app = App::new().await;
    let (_, _, pid) = app.reseller("r1@example.test", "0").await;
    let host = app.system_domain(pid, "shop1").await;
    let headers = [("host", host.as_str())];
    assert_eq!(
        app.raw("GET", "/api/v1/public/config", None, None, &headers)
            .await
            .0,
        StatusCode::OK
    );
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/profiles/{pid}/disable"),
            Some(json!({"reason": "abuse"})),
        )
        .await,
    );
    assert_eq!(
        app.raw("GET", "/api/v1/public/config", None, None, &headers)
            .await
            .0,
        StatusCode::NOT_FOUND
    );
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/profiles/{pid}/restore"),
            Some(json!({})),
        )
        .await,
    );
    assert_eq!(
        app.raw("GET", "/api/v1/public/config", None, None, &headers)
            .await
            .0,
        StatusCode::OK
    );
}

// RSL-03: the console refuses reseller-site hosts (403 envelope) and the handler does not run.
#[tokio::test]
async fn rsl03_console_only_on_main_site() {
    let app = App::new().await;
    let (_, _, pid) = app.reseller("owner@example.test", "0").await;
    let host = app.system_domain(pid, "owner").await;
    let (_, token) = app.user("buyer@example.test").await;

    let (status, body) = app
        .raw(
            "POST",
            "/api/v1/reseller/apply",
            Some(json!({"reason": "x"})),
            Some(&token),
            &[("host", &host)],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    err(&body, 403, "无权限访问");
    // nothing was created: the main site still shows no profile
    let snap = app
        .call("GET", "/api/v1/reseller/profile", None, Some(&token))
        .await;
    assert_eq!(data(&snap)["opened"], false);

    let (_, body) = app
        .raw(
            "GET",
            "/api/v1/reseller/profile",
            None,
            Some(&token),
            &[("host", "main.example.com")],
        )
        .await;
    assert_eq!(data(&body)["can_apply"], true);

    // Without a token the console is 401 even on a reseller host.
    let (_, body) = app
        .raw(
            "GET",
            "/api/v1/reseller/profile",
            None,
            None,
            &[("host", &host)],
        )
        .await;
    assert_eq!(body["status_code"], 401);
}

// RSL-06: X-Forwarded-Host is ignored unless `trusted_forwarded_host` is set.
#[tokio::test]
async fn rsl06_forwarded_host_requires_trust() {
    let app = App::new().await;
    let (_, token, pid) = app.reseller("owner@example.test", "0").await;
    let host = app.system_domain(pid, "fwd").await;
    let forged = [
        ("host", "main.example.com"),
        ("x-forwarded-host", host.as_str()),
    ];
    let (_, body) = app
        .raw(
            "GET",
            "/api/v1/reseller/profile",
            None,
            Some(&token),
            &forged,
        )
        .await;
    assert_eq!(
        data(&body)["opened"],
        true,
        "forged header must not switch tenant"
    );

    let mut cfg = config();
    cfg.reseller.trusted_forwarded_host = true;
    let trusted = App::with(cfg).await;
    let (_, token, pid) = trusted.reseller("owner@example.test", "0").await;
    let host = trusted.system_domain(pid, "fwd").await;
    let headers = [
        ("host", "internal.lan"),
        ("x-forwarded-host", host.as_str()),
    ];
    let (_, body) = trusted
        .raw(
            "GET",
            "/api/v1/reseller/profile",
            None,
            Some(&token),
            &headers,
        )
        .await;
    err(&body, 403, "无权限访问");
}

#[tokio::test]
async fn feature_disabled_means_main_shop_everywhere() {
    let mut cfg = config();
    cfg.reseller.enabled = false;
    let app = App::with(cfg).await;
    let (status, _) = app
        .raw(
            "GET",
            "/api/v1/public/config",
            None,
            None,
            &[("host", "whatever.test")],
        )
        .await;
    assert_eq!(status, StatusCode::OK);
}

// RSL-06 / RSL-08: the overlay is installed on the content public config and cached per tenant.
#[tokio::test]
async fn rsl06_rsl08_public_config_overlay_per_tenant() {
    let app = App::new().await;
    let (_, token, pid) = app.reseller("brand@example.test", "0").await;
    let saved = app
        .call(
            "PUT",
            "/api/v1/reseller/site-config",
            Some(json!({
                "site_name": "White Label",
                "announcement": {"enabled": true, "type": "success", "content": {"zh-CN": "<p>测试测试</p>"}},
                "support": {"telegram": "https://t.me/white"},
            })),
            Some(&token),
        )
        .await;
    assert_eq!(data(&saved)["site_name"], "White Label");

    let pc = &app.services.content.public_config;
    let main = pc
        .get(&Tenant {
            reseller_id: None,
            host: "main.example.com".into(),
        })
        .await
        .unwrap();
    let reseller = pc
        .get(&Tenant {
            reseller_id: Some(pid),
            host: "brand.shop.example.com".into(),
        })
        .await
        .unwrap();
    assert_eq!(main["tenant"]["mode"], "main");
    assert_ne!(main["brand"]["site_name"], "White Label");
    assert_eq!(reseller["tenant"]["mode"], "reseller");
    assert_eq!(reseller["brand"]["site_name"], "White Label");
    assert_eq!(reseller["contact"]["telegram"], "https://t.me/white");
    let a = &reseller["announcement"];
    assert_eq!(a["type"], "success");
    assert!(a.get("enabled").is_none());
    assert_eq!(a["version"].as_str().unwrap().len(), 8);

    // Disabling the announcement removes it (no main-site leak) after the cache is invalidated.
    data(
        &app.call(
            "PUT",
            "/api/v1/reseller/site-config",
            Some(json!({"site_name": "White Label", "announcement": {"enabled": false}})),
            Some(&token),
        )
        .await,
    );
    let reseller = pc
        .get(&Tenant {
            reseller_id: Some(pid),
            host: "brand.shop.example.com".into(),
        })
        .await
        .unwrap();
    assert!(reseller.get("announcement").is_none(), "{reseller}");
}

// RSL-06 / RSL-08 over HTTP: the `/public/config` route must pass the resolved tenant
// (a service-level test alone missed that the handler ignored the tenant).
#[tokio::test]
async fn rsl06_public_config_endpoint_serves_reseller_branding() {
    let app = App::new().await;
    let (_, token, pid) = app.reseller("http-brand@example.test", "0").await;
    let host = app.system_domain(pid, "httpbrand").await;
    data(
        &app.call(
            "PUT",
            "/api/v1/reseller/site-config",
            Some(json!({"site_name": "HTTP White Label"})),
            Some(&token),
        )
        .await,
    );

    let (status, body) = app
        .raw(
            "GET",
            "/api/v1/public/config",
            None,
            None,
            &[("host", host.as_str())],
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let d = data(&body);
    assert_eq!(d["tenant"]["mode"], "reseller");
    assert_eq!(d["brand"]["site_name"], "HTTP White Label");

    let (_, body) = app
        .raw(
            "GET",
            "/api/v1/public/config",
            None,
            None,
            &[("host", "localhost")],
        )
        .await;
    let d = data(&body);
    assert_eq!(d["tenant"]["mode"], "main");
    assert_ne!(d["brand"]["site_name"], "HTTP White Label");
}
