//! Reseller console `/api/v1/reseller/*`: onboarding, domains, site config, product
//! pricing rules and sales orders (RSL-03, RSL-07, RSL-10).

mod reseller_common;

use reseller_common::{App, data, err};
use serde_json::json;

#[tokio::test]
async fn onboarding_flow_and_shapes() {
    let app = App::new().await;
    let (_, token) = app.user("apply@example.test").await;
    assert_eq!(
        app.call("GET", "/api/v1/reseller/profile", None, None)
            .await["status_code"],
        401
    );

    let snap = app
        .call("GET", "/api/v1/reseller/profile", None, Some(&token))
        .await;
    assert_eq!(
        data(&snap),
        &json!({"opened": false, "can_apply": true, "domains": []})
    );
    err(
        &app.call("GET", "/api/v1/reseller/domains", None, Some(&token))
            .await,
        400,
        "请求参数错误",
    );
    err(
        &app.call("GET", "/api/v1/reseller/site-config", None, Some(&token))
            .await,
        400,
        "请求参数错误",
    );

    let applied = app
        .call(
            "POST",
            "/api/v1/reseller/apply",
            Some(json!({"reason": " sell "})),
            Some(&token),
        )
        .await;
    let p = data(&applied);
    assert_eq!(p["status"], "pending_review");
    assert_eq!(p["apply_reason"], "sell");
    assert_eq!(p["default_markup_percent"], "0.00");
    assert!(p.get("reject_reason").is_none());
    let pid = p["id"].as_i64().unwrap();

    // Pending profiles cannot use the console features.
    err(
        &app.call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "a.test"})),
            Some(&token),
        )
        .await,
        400,
        "无权限访问",
    );
    err(
        &app.call("GET", "/api/v1/reseller/orders", None, Some(&token))
            .await,
        400,
        "无权限访问",
    );
    err(
        &app.call("GET", "/api/v1/reseller/ledger-entries", None, Some(&token))
            .await,
        400,
        "分销商资格未激活，暂时无法提现",
    );

    // Rejected → can re-apply.
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/profiles/{pid}/reject"),
            Some(json!({"reason": "no"})),
        )
        .await,
    );
    let snap = app
        .call("GET", "/api/v1/reseller/profile", None, Some(&token))
        .await;
    assert_eq!(data(&snap)["can_apply"], true);
    assert_eq!(data(&snap)["profile"]["reject_reason"], "no");
    let again = app
        .call(
            "POST",
            "/api/v1/reseller/apply",
            Some(json!({"reason": "again"})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&again)["status"], "pending_review");
    data(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/profiles/{pid}/approve"),
            Some(json!({"default_markup_percent": "5", "max_markup_percent": "50"})),
        )
        .await,
    );
    let snap = app
        .call("GET", "/api/v1/reseller/profile", None, Some(&token))
        .await;
    let d = data(&snap);
    assert_eq!(
        (d["opened"].clone(), d["can_apply"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(d["profile"]["status"], "active");
    assert_eq!(d["profile"]["max_markup_percent"], "50.00");
}

// RSL-03: custom domain validation, conflicts and main-host protection.
#[tokio::test]
async fn rsl03_custom_domains() {
    let app = App::new().await;
    let (_, token, _) = app.reseller("dom@example.test", "0").await;
    let res = app
        .call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "Shop.Custom.test"})),
            Some(&token),
        )
        .await;
    let d = data(&res);
    assert_eq!(d["domain"], "shop.custom.test");
    assert_eq!(d["type"], "custom");
    assert_eq!(d["status"], "pending_review");
    assert_eq!(d["verification_status"], "pending");
    assert_eq!(d["is_primary"], false);

    let (_, other, _) = app.reseller("dom2@example.test", "0").await;
    err(
        &app.call(
            "POST",
            "/api/v1/reseller/domains",
            Some(json!({"domain": "shop.custom.test"})),
            Some(&other),
        )
        .await,
        400,
        "该域名已被占用，请更换其他域名",
    );
    for (domain, msg) in [
        ("main.example.com", "不能使用主站域名作为分销域名"),
        ("x.shop.example.com", "不能使用主站域名作为分销域名"),
        ("https://evil.test", "域名格式无效，请填写正确的域名"),
    ] {
        err(
            &app.call(
                "POST",
                "/api/v1/reseller/domains",
                Some(json!({"domain": domain})),
                Some(&token),
            )
            .await,
            400,
            msg,
        );
    }
    let list = app
        .call("GET", "/api/v1/reseller/domains", None, Some(&token))
        .await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn site_config_roundtrip_and_field_errors() {
    let app = App::new().await;
    let (_, token, _) = app.reseller("site@example.test", "0").await;
    let snap = app
        .call("GET", "/api/v1/reseller/site-config", None, Some(&token))
        .await;
    assert_eq!(data(&snap), &json!({"opened": true, "can_edit": true}));
    let saved = app
        .call(
            "PUT",
            "/api/v1/reseller/site-config",
            Some(json!({
                "site_name": "Shop",
                "logo": "/uploads/reseller/a.png",
                "footer_links": [{"name": {"zh-CN": "帮助"}, "url": "https://help.test"}],
                "nav_config": {"builtin": {"blog": false}},
            })),
            Some(&token),
        )
        .await;
    let c = data(&saved);
    assert_eq!(c["footer_links"][0]["url"], "https://help.test");
    assert_eq!(
        c["nav_config"]["builtin"],
        json!({"blog": false, "notice": true, "about": true})
    );
    assert_eq!(
        c["support"],
        json!({"telegram": "", "whatsapp": "", "email": "", "support_url": ""})
    );
    let snap = app
        .call("GET", "/api/v1/reseller/site-config", None, Some(&token))
        .await;
    assert_eq!(data(&snap)["config"]["site_name"], "Shop");

    for (body, msg) in [
        (
            json!({"logo": "ftp://x"}),
            "图片地址无效，请重新上传或填写以 https:// 开头的完整链接",
        ),
        (
            json!({"support": {"telegram": "https://evil.test"}}),
            "Telegram 链接格式不正确，请使用 https://telegram.me/ 或 https://t.me/ 开头的链接",
        ),
        (
            json!({"support": {"email": "bad"}}),
            "客服邮箱格式不正确，请检查后重试",
        ),
        (
            json!({"footer_links": [{"url": "http://plain.test"}]}),
            "链接地址格式不正确，请使用 https:// 开头的完整链接",
        ),
    ] {
        err(
            &app.call(
                "PUT",
                "/api/v1/reseller/site-config",
                Some(body),
                Some(&token),
            )
            .await,
            400,
            msg,
        );
    }
}

#[tokio::test]
async fn upload_requires_active_profile() {
    let app = App::new().await;
    let (_, token) = app.user("up@example.test").await;
    // multipart body is not even read without a profile
    let res = app
        .call(
            "POST",
            "/api/v1/reseller/upload",
            Some(json!({})),
            Some(&token),
        )
        .await;
    err(&res, 400, "请求参数错误");
    data(
        &app.call(
            "POST",
            "/api/v1/reseller/apply",
            Some(json!({})),
            Some(&token),
        )
        .await,
    );
    let res = app
        .call(
            "POST",
            "/api/v1/reseller/upload",
            Some(json!({})),
            Some(&token),
        )
        .await;
    err(&res, 403, "无权限访问");

    // active reseller without a multipart body → file missing
    let (_, active, _) = app.reseller("up2@example.test", "0").await;
    let res = app
        .call(
            "POST",
            "/api/v1/reseller/upload",
            Some(json!({})),
            Some(&active),
        )
        .await;
    err(&res, 400, "未上传文件");
}

// RSL-07: price floor / cost / markup cap enforced on save; preview matches.
#[tokio::test]
async fn rsl07_product_settings_pricing() {
    let app = App::new().await;
    let (_, token, _) = app.reseller("price@example.test", "50").await;
    let (pid, skus) = app
        .product("p1", "100.00", &[("100.00", "90.00"), ("200.00", "0")])
        .await;
    let url = format!("/api/v1/reseller/product-settings/{pid}");
    let rule = |sku: i64, mode: &str, v: &str| json!({"settings": [{"sku_id": sku, "is_listed": true, "pricing_mode": mode, "fixed_price_amount": v, "markup_percent": v}]});

    err(
        &app.call(
            "PUT",
            &url,
            Some(rule(skus[0], "fixed_price", "99")),
            Some(&token),
        )
        .await,
        400,
        "分销商品价格配置不合法",
    );
    err(
        &app.call(
            "PUT",
            &url,
            Some(rule(skus[0], "fixed_price", "151")),
            Some(&token),
        )
        .await,
        400,
        "分销商品加价超过允许范围",
    );
    err(
        &app.call(
            "PUT",
            &url,
            Some(rule(skus[0], "unknown", "1")),
            Some(&token),
        )
        .await,
        400,
        "分销商品价格配置不合法",
    );
    err(
        &app.call(
            "PUT",
            &url,
            Some(rule(99_999, "inherit", "0")),
            Some(&token),
        )
        .await,
        400,
        "订单项不合法",
    );
    err(
        &app.call(
            "PUT",
            "/api/v1/reseller/product-settings/99999",
            Some(rule(0, "inherit", "0")),
            Some(&token),
        )
        .await,
        404,
        "资源不存在",
    );

    let saved = app
        .call(
            "PUT",
            &url,
            Some(rule(skus[0], "fixed_price", "120")),
            Some(&token),
        )
        .await;
    let d = data(&saved);
    assert_eq!(d["product"]["id"], pid);
    assert_eq!(d["product"]["price_amount"], "100.00");
    let sku0 = &d["skus"][0];
    assert_eq!(sku0["setting"]["pricing_mode"], "fixed_price");
    assert_eq!(sku0["setting"]["fixed_price_amount"], "120.00");
    assert_eq!(sku0["effective_price_amount"], "120.00");
    assert_eq!(sku0["setting"]["rule_source"], "sku");
    assert_eq!(d["skus"][1]["effective_price_amount"], "200.00");
    assert!(d.get("product_setting").is_none());

    // Product-level markup 10% + preview of a SKU override that exceeds the cap.
    let body = json!({"settings": [
        {"sku_id": 0, "is_listed": true, "pricing_mode": "markup_percent", "markup_percent": "10"},
        {"sku_id": skus[1], "is_listed": true, "pricing_mode": "fixed_price", "fixed_price_amount": 500},
    ]});
    let preview = app
        .call("POST", &format!("{url}/preview"), Some(body), Some(&token))
        .await;
    let items = data(&preview)["items"].as_array().unwrap().clone();
    assert_eq!(
        items[0],
        json!({"sku_id": 0, "is_listed": true, "base_price_amount": "100.00", "effective_price_amount": "110.00", "valid": true})
    );
    assert_eq!(items[2]["valid"], false);
    assert_eq!(items[2]["error_code"], "markup_exceeded");

    let list = app
        .call(
            "GET",
            "/api/v1/reseller/product-settings?configured=configured",
            None,
            Some(&token),
        )
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    let list = app
        .call(
            "GET",
            "/api/v1/reseller/product-settings?configured=unconfigured",
            None,
            Some(&token),
        )
        .await;
    assert_eq!(list["pagination"]["total"], 0);

    // Hide the product, then reset the SKU rule.
    let hidden = json!({"settings": [{"sku_id": 0, "is_listed": false}]});
    data(&app.call("PUT", &url, Some(hidden), Some(&token)).await);
    let list = app
        .call(
            "GET",
            "/api/v1/reseller/product-settings?listed=hidden",
            None,
            Some(&token),
        )
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    let del = app
        .call(
            "DELETE",
            &format!("{url}?sku_id={}", skus[0]),
            None,
            Some(&token),
        )
        .await;
    assert_eq!(data(&del), &json!({"deleted": true}));
    let detail = app.call("GET", &url, None, Some(&token)).await;
    assert!(data(&detail)["skus"][0].get("setting").is_none());
    assert_eq!(data(&detail)["product_setting"]["is_listed"], false);
}

// RSL-07 / RSL-10: tenant-scoped order views with masked buyers and snapshot pricing.
#[tokio::test]
async fn rsl07_rsl10_orders_are_scoped_and_masked() {
    let app = App::new().await;
    let (ruid, token, pid) = app.reseller("seller@example.test", "0").await;
    let (_, other_token, other_pid) = app.reseller("other@example.test", "0").await;
    let (buyer, _) = app.user("buyer-label@example.test").await;
    let (product, skus) = app.product("p2", "100.00", &[("100.00", "0")]).await;
    let (_, no) = app
        .order(
            pid, ruid, buyer, product, skus[0], "100.00", "130.00", "paid",
        )
        .await;
    let (_, guest_no) = app
        .order(
            pid,
            ruid,
            0,
            product,
            skus[0],
            "100.00",
            "110.00",
            "pending_payment",
        )
        .await;
    let (_, foreign_no) = app
        .order(
            other_pid, 0, buyer, product, skus[0], "100.00", "120.00", "paid",
        )
        .await;

    let list = app
        .call("GET", "/api/v1/reseller/orders", None, Some(&token))
        .await;
    assert_eq!(list["pagination"]["total"], 2);
    let rows = data(&list).as_array().unwrap();
    let paid = rows.iter().find(|r| r["order_no"] == no.as_str()).unwrap();
    assert_eq!(paid["buyer_label"], "b***@example.test");
    assert_eq!(paid["profit_amount"], "30.00");
    assert_eq!(paid["base_amount"], "100.00");
    assert_eq!(paid["total_amount"], "130.00");
    assert_eq!(paid["profit_status"], "pending");
    assert_eq!(paid["items_count"], 1);
    let guest = rows
        .iter()
        .find(|r| r["order_no"] == guest_no.as_str())
        .unwrap();
    assert_eq!(guest["buyer_label"], "guest");

    let detail = app
        .call(
            "GET",
            &format!("/api/v1/reseller/orders/{no}"),
            None,
            Some(&token),
        )
        .await;
    let d = data(&detail);
    assert_eq!(d["buyer_label"], "b***@example.test");
    assert_eq!(d["items"][0]["profit_amount"], "30.00");
    assert_eq!(d["items"][0]["base_unit_amount"], "100.00");
    assert_eq!(d["items"][0]["reseller_unit_amount"], "130.00");

    err(
        &app.call(
            "GET",
            &format!("/api/v1/reseller/orders/{foreign_no}"),
            None,
            Some(&token),
        )
        .await,
        404,
        "订单不存在",
    );
    let other = app
        .call("GET", "/api/v1/reseller/orders", None, Some(&other_token))
        .await;
    assert_eq!(other["pagination"]["total"], 1);

    let stats = app
        .call("GET", "/api/v1/reseller/orders/stats", None, Some(&token))
        .await;
    assert_eq!(
        data(&stats),
        &json!({"total": 2, "by_status": {"paid": 1, "pending_payment": 1}, "by_currency": {"CNY": 2}})
    );
    let filtered = app
        .call(
            "GET",
            "/api/v1/reseller/orders?status=paid&created_from=2026-07-01",
            None,
            Some(&token),
        )
        .await;
    assert_eq!(filtered["pagination"]["total"], 1);
    err(
        &app.call(
            "GET",
            "/api/v1/reseller/orders?created_from=bad",
            None,
            Some(&token),
        )
        .await,
        400,
        "请求参数错误",
    );
}

/// DB-04 ①: a first-time SKU setting saved with `is_listed = false` is stored as false
/// (no column default of `true` swallowing the Go/Rust zero value) and reads back hidden.
#[tokio::test]
async fn db_04_first_sku_setting_keeps_is_listed_false() {
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    use zs_infra::db::entity::reseller_product_settings as settings;
    let app = App::new().await;
    let (_, token, _) = app.reseller("db04@example.test", "50").await;
    let (pid, skus) = app
        .product(
            "db04",
            "100.00",
            &[("100.00", "90.00"), ("120.00", "90.00")],
        )
        .await;
    let saved = app
        .call(
            "PUT",
            &format!("/api/v1/reseller/product-settings/{pid}"),
            Some(json!({"settings": [{"sku_id": skus[1], "is_listed": false, "pricing_mode": "inherit"}]})),
            Some(&token),
        )
        .await;
    data(&saved);
    let rows = settings::Entity::find()
        .filter(settings::Column::ProductId.eq(pid))
        .filter(settings::Column::SkuId.eq(skus[1]))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert!(!rows[0].is_listed, "stored is_listed must stay false");
    let detail = app
        .call(
            "GET",
            &format!("/api/v1/reseller/product-settings/{pid}"),
            None,
            Some(&token),
        )
        .await;
    let text = data(&detail).to_string();
    assert!(text.contains("\"is_listed\":false"), "{text}");
}
