//! Admin `/api/v1/admin/resellers/*`: review state machine, domains, site configs,
//! product settings, operations and permissions (RSL-03).

mod reseller_common;

use reseller_common::{App, config, data, err};
use serde_json::json;

#[tokio::test]
async fn requires_admin_and_compliance() {
    let app = App::new().await;
    assert_eq!(
        app.call("GET", "/api/v1/admin/resellers/profiles", None, None)
            .await["status_code"],
        401
    );
    let (_, user_token) = app.user("u@example.test").await;
    assert_eq!(
        app.call(
            "GET",
            "/api/v1/admin/resellers/profiles",
            None,
            Some(&user_token)
        )
        .await["status_code"],
        401
    );
    let list = app
        .admin_call("GET", "/api/v1/admin/resellers/withdraws", None)
        .await;
    assert_eq!(list["pagination"]["total"], 0);

    // Without the compliance acknowledgement the [C] finance routes are gated.
    let gated = App::without_ack(config()).await;
    for uri in [
        "/api/v1/admin/resellers/withdraws",
        "/api/v1/admin/resellers/ledger-entries",
        "/api/v1/admin/resellers/balance-accounts",
        "/api/v1/admin/resellers/operations/finance",
    ] {
        err(
            &gated.admin_call("GET", uri, None).await,
            403,
            "compliance_required",
        );
    }
    err(
        &gated
            .admin_call("POST", "/api/v1/admin/resellers/withdraws/1/pay", None)
            .await,
        403,
        "compliance_required",
    );
    // non-finance pages are not gated
    data(
        &gated
            .admin_call("GET", "/api/v1/admin/resellers/profiles", None)
            .await,
    );
}

// RSL-03: profile state machine through the admin API.
#[tokio::test]
async fn rsl03_profile_review_transitions() {
    let app = App::new().await;
    let (_, token) = app.user("rev@example.test").await;
    let pid = data(
        &app.call(
            "POST",
            "/api/v1/reseller/apply",
            Some(json!({"reason": "r"})),
            Some(&token),
        )
        .await,
    )["id"]
        .as_i64()
        .unwrap();
    let url = |a: &str| format!("/api/v1/admin/resellers/profiles/{pid}/{a}");

    // restore only from disabled
    err(
        &app.admin_call("POST", &url("restore"), Some(json!({})))
            .await,
        400,
        "请求参数错误",
    );
    // markup validation on approve
    err(
        &app.admin_call(
            "POST",
            &url("approve"),
            Some(json!({"default_markup_percent": "60", "max_markup_percent": "50"})),
        )
        .await,
        400,
        "请求参数错误",
    );
    err(
        &app.admin_call(
            "POST",
            &url("approve"),
            Some(json!({"default_markup_percent": "abc"})),
        )
        .await,
        400,
        "请求参数错误",
    );
    let approved = app
        .admin_call(
            "POST",
            &url("approve"),
            Some(json!({"default_markup_percent": "10", "max_markup_percent": "50"})),
        )
        .await;
    let a = data(&approved);
    assert_eq!(a["system_domain"], json!(null));
    assert_eq!(a["profile"]["status"], "active");
    assert_eq!(a["profile"]["default_markup_percent"], "10.00");
    assert_eq!(a["profile"]["user"]["email"], "rev@example.test");
    assert!(a["profile"]["reviewed_by"].is_number());

    err(
        &app.admin_call("POST", &url("reject"), Some(json!({"reason": "late"})))
            .await,
        400,
        "请求参数错误",
    );
    let disabled = app
        .admin_call("POST", &url("disable"), Some(json!({"reason": " abuse "})))
        .await;
    assert_eq!(data(&disabled)["status"], "disabled");
    assert_eq!(data(&disabled)["reject_reason"], "abuse");
    let restored = app
        .admin_call("POST", &url("restore"), Some(json!({})))
        .await;
    assert_eq!(data(&restored)["status"], "active");
    assert!(data(&restored).get("reject_reason").is_none());

    let updated = app
        .admin_call(
            "PUT",
            &format!("/api/v1/admin/resellers/profiles/{pid}"),
            Some(json!({"default_markup_percent": "5", "max_markup_percent": "20", "settlement_status": "frozen"})),
        )
        .await;
    assert_eq!(data(&updated)["settlement_status"], "frozen");
    assert_eq!(data(&updated)["max_markup_percent"], "20.00");
    err(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/resellers/profiles/{pid}"),
            Some(json!({"settlement_status": "x"})),
        )
        .await,
        400,
        "请求参数错误",
    );
    err(
        &app.admin_call(
            "POST",
            "/api/v1/admin/resellers/profiles/9999/disable",
            Some(json!({})),
        )
        .await,
        404,
        "请求参数错误",
    );

    let list = app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/profiles?status=active&keyword=rev@",
            None,
        )
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(data(&list)[0]["user"]["email"], "rev@example.test");
    let none = app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/profiles?status=rejected",
            None,
        )
        .await;
    assert_eq!(none["pagination"]["total"], 0);

    let detail = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/resellers/profiles/{pid}"),
            None,
        )
        .await;
    let d = data(&detail);
    assert_eq!(d["profile"]["id"], pid);
    assert_eq!(d["domains"], json!([]));
    assert_eq!(d["product_summary"]["configured_products"], 0);
    assert_eq!(d["finance_summary"]["recent_ledger_count"], 0);
    assert!(d.get("site_config").is_none());
    err(
        &app.admin_call("GET", "/api/v1/admin/resellers/profiles/9999", None)
            .await,
        404,
        "请求参数错误",
    );
}

// RSL-03: system subdomain rules and the dedicated missing-base error.
#[tokio::test]
async fn rsl03_system_subdomain() {
    let app = App::new().await;
    let (_, _, pid) = app.reseller("sys@example.test", "0").await;
    let url = format!("/api/v1/admin/resellers/profiles/{pid}/system-domain");
    let res = app
        .admin_call("PUT", &url, Some(json!({"subdomain": "hello"})))
        .await;
    let d = data(&res);
    assert_eq!(d["domain"], "hello.shop.example.com");
    assert_eq!(
        (
            d["type"].clone(),
            d["status"].clone(),
            d["is_primary"].clone()
        ),
        (json!("subdomain"), json!("active"), json!(true))
    );
    assert_eq!(d["verification_status"], "verified");
    // rename keeps a single system domain
    let res = app
        .admin_call("PUT", &url, Some(json!({"domain": "world"})))
        .await;
    assert_eq!(data(&res)["domain"], "world.shop.example.com");
    assert_eq!(data(&res)["id"], d["id"]);
    for bad in ["a.b", "-x", "main.example.com"] {
        err(
            &app.admin_call("PUT", &url, Some(json!({"subdomain": bad})))
                .await,
            400,
            "请求参数错误",
        );
    }
    // another reseller cannot take the same host
    let (_, _, other) = app.reseller("sys2@example.test", "0").await;
    err(
        &app.admin_call(
            "PUT",
            &format!("/api/v1/admin/resellers/profiles/{other}/system-domain"),
            Some(json!({"subdomain": "world"})),
        )
        .await,
        400,
        "请求参数错误",
    );

    let mut cfg = config();
    cfg.reseller.subdomain_base = String::new();
    let no_base = App::with(cfg).await;
    let (_, _, pid) = no_base.reseller("sys@example.test", "0").await;
    err(
        &no_base
            .admin_call(
                "PUT",
                &format!("/api/v1/admin/resellers/profiles/{pid}/system-domain"),
                Some(json!({"subdomain": "hello"})),
            )
            .await,
        400,
        "分销系统二级域名基础域名未配置，请先配置 reseller.subdomain_base",
    );
}

// RSL-03: domain approve / disable (primary promotion) / set-primary.
#[tokio::test]
async fn rsl03_domain_actions() {
    let app = App::new().await;
    let (_, token, pid) = app.reseller("dom@example.test", "0").await;
    let a = data(
        &app.call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "a.test"})),
            Some(&token),
        )
        .await,
    )["id"]
        .as_i64()
        .unwrap();
    let b = data(
        &app.call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "b.test"})),
            Some(&token),
        )
        .await,
    )["id"]
        .as_i64()
        .unwrap();
    let act = |id: i64, action: &str| format!("/api/v1/admin/resellers/domains/{id}/{action}");

    err(
        &app.admin_call("POST", &act(a, "set-primary"), None).await,
        400,
        "请求参数错误",
    );
    let ra = app.admin_call("POST", &act(a, "approve"), None).await;
    assert_eq!(
        (data(&ra)["status"].clone(), data(&ra)["is_primary"].clone()),
        (json!("active"), json!(true))
    );
    assert!(data(&ra)["verified_at"].is_string());
    let rb = app.admin_call("POST", &act(b, "approve"), None).await;
    assert_eq!(data(&rb)["is_primary"], false);
    err(
        &app.admin_call("POST", &act(b, "approve"), None).await,
        400,
        "请求参数错误",
    );

    // disabling the primary promotes the other live domain
    let da = app.admin_call("POST", &act(a, "disable"), None).await;
    assert_eq!(
        (data(&da)["status"].clone(), data(&da)["is_primary"].clone()),
        (json!("disabled"), json!(false))
    );
    let list = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/resellers/domains?reseller_id={pid}"),
            None,
        )
        .await;
    let rows = data(&list).as_array().unwrap().clone();
    let row_b = rows.iter().find(|r| r["id"] == b).unwrap();
    assert_eq!(row_b["is_primary"], true);
    assert_eq!(row_b["profile"]["user"]["email"], "dom@example.test");

    // re-approve a, then make it primary again
    data(&app.admin_call("POST", &act(a, "approve"), None).await);
    let sp = app.admin_call("POST", &act(a, "set-primary"), None).await;
    assert_eq!(data(&sp)["is_primary"], true);
    let list = app
        .admin_call("GET", "/api/v1/admin/resellers/domains?keyword=b.te", None)
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(data(&list)[0]["is_primary"], false);
    err(
        &app.admin_call("POST", &act(9999, "approve"), None).await,
        404,
        "请求参数错误",
    );
}

#[tokio::test]
async fn site_configs_admin() {
    let app = App::new().await;
    let (_, _, pid) = app.reseller("cfg@example.test", "0").await;
    let url = format!("/api/v1/admin/resellers/site-configs/{pid}");
    let empty = app.admin_call("GET", &url, None).await;
    let e = data(&empty);
    assert_eq!(e["id"], 0);
    assert_eq!(e["reseller_id"], pid);
    assert_eq!(e["footer_links"], json!([]));
    assert_eq!(e["profile"]["user"]["email"], "cfg@example.test");
    err(
        &app.admin_call("GET", "/api/v1/admin/resellers/site-configs/9999", None)
            .await,
        404,
        "请求参数错误",
    );

    let saved = app
        .admin_call("PUT", &url, Some(json!({"site_name": "Admin Set"})))
        .await;
    assert_eq!(data(&saved)["site_name"], "Admin Set");
    assert_eq!(data(&saved)["reseller_id"], pid);
    err(
        &app.admin_call("PUT", &url, Some(json!({"logo": "javascript:1"})))
            .await,
        400,
        "请求参数错误",
    );
    let list = app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/site-configs?keyword=admin",
            None,
        )
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(
        data(&app.admin_call("POST", &format!("{url}/reset"), None).await),
        &json!({"ok": true})
    );
    let list = app
        .admin_call("GET", "/api/v1/admin/resellers/site-configs", None)
        .await;
    assert_eq!(list["pagination"]["total"], 0);
    // saving again revives the soft-deleted row (live-row unique index)
    data(
        &app.admin_call("PUT", &url, Some(json!({"site_name": "Again"})))
            .await,
    );
    let list = app
        .admin_call("GET", "/api/v1/admin/resellers/site-configs", None)
        .await;
    assert_eq!(list["pagination"]["total"], 1);
}

#[tokio::test]
async fn product_settings_admin() {
    let app = App::new().await;
    let (_, _, pid) = app.reseller("ps@example.test", "0").await;
    let (product, skus) = app.product("adm", "50.00", &[("50.00", "0")]).await;
    let url = format!("/api/v1/admin/resellers/product-settings/{pid}/{product}");
    let body = json!({"settings": [{"sku_id": skus[0], "is_listed": true, "pricing_mode": "fixed_markup", "fixed_markup_amount": "5"}]});
    let saved = app.admin_call("PUT", &url, Some(body.clone())).await;
    assert_eq!(data(&saved)["skus"][0]["effective_price_amount"], "55.00");
    let preview = app
        .admin_call("POST", &format!("{url}/preview"), Some(body))
        .await;
    assert_eq!(
        data(&preview)["items"][1]["effective_price_amount"],
        "55.00"
    );
    let list = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/resellers/product-settings?reseller_id={pid}"),
            None,
        )
        .await;
    let row = &data(&list)[0];
    assert_eq!(row["pricing_mode"], "fixed_markup");
    assert_eq!(row["product"]["slug"], "adm");
    assert_eq!(row["profile"]["user"]["email"], "ps@example.test");
    let summary = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/resellers/profiles/{pid}"),
            None,
        )
        .await;
    assert_eq!(
        data(&summary)["product_summary"],
        json!({"configured_products": 1, "hidden_products": 0, "sku_overrides": 1, "pricing_overrides": 1})
    );
    assert_eq!(
        data(
            &app.admin_call("DELETE", &format!("{url}?sku_id={}", skus[0]), None)
                .await
        ),
        &json!({"deleted": true})
    );
    err(
        &app.admin_call(
            "GET",
            &format!("/api/v1/admin/resellers/product-settings/9999/{product}"),
            None,
        )
        .await,
        404,
        "资源不存在",
    );
    // inactive reseller → bad request
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/profiles/{pid}/disable"),
            Some(json!({})),
        )
        .await,
    );
    err(
        &app.admin_call("GET", &url, None).await,
        400,
        "请求参数错误",
    );
}

#[tokio::test]
async fn operations_overview_and_finance() {
    let app = App::new().await;
    let (ruid, _, pid) = app.reseller("ops@example.test", "0").await;
    app.user("pending@example.test").await;
    let (product, skus) = app.product("ops", "100.00", &[("100.00", "0")]).await;
    app.order(pid, ruid, 0, product, skus[0], "100.00", "120.00", "paid")
        .await;
    app.order(
        pid,
        ruid,
        ruid,
        product,
        skus[0],
        "100.00",
        "120.00",
        "completed",
    )
    .await;

    let res = app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/operations/overview?range=today",
            None,
        )
        .await;
    let o = data(&res);
    assert_eq!(o["range"], "today");
    assert_eq!(o["timezone"], "UTC");
    assert_eq!(o["lifecycle"]["profiles_active"], 1);
    assert_eq!(o["lifecycle"]["active_profiles_without_site_config"], 1);
    assert_eq!(o["orders"]["orders_total"], 2);
    assert_eq!(o["orders"]["paid_orders"], 2);
    assert_eq!(o["orders"]["self_dealing_blocked_orders"], 1);
    assert_eq!(
        o["orders"]["average_paid_orders_per_active_reseller"],
        "2.00"
    );
    assert_eq!(o["top_resellers"][0]["reseller_id"], pid);
    assert_eq!(o["top_resellers"][0]["email"], "ops@example.test");
    assert!(
        o["alerts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["type"] == "self_dealing_blocked_orders")
    );
    err(
        &app.admin_call(
            "GET",
            "/api/v1/admin/resellers/operations/overview?range=1y",
            None,
        )
        .await,
        400,
        "请求参数错误",
    );

    let fin = app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/operations/finance?range=7d",
            None,
        )
        .await;
    let f = data(&fin);
    assert_eq!(f["period_currency_rows"][0]["currency"], "CNY");
    assert_eq!(f["period_currency_rows"][0]["gmv_paid"], "240.00");
    assert_eq!(f["current_currency_rows"], json!([]));
}
