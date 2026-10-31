//! Reseller console `/api/v1/reseller/*`: onboarding, domains, site config, product
//! pricing rules and sales orders (RSL-03, RSL-07, RSL-10).

mod reseller_common;

use axum::body::Body;
use axum::http::{Request, header};
use http_body_util::BodyExt;
use reseller_common::{App, data, err};
use serde_json::json;
use tower::ServiceExt;

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
                "storefront_template": "aurora",
                "template_mode": "list",
                "theme": {"primary_color": "#123abc", "background_color": "#eefbf7"},
                "support": {"wechat_id": "shop_wechat", "wechat_qr_code": "/uploads/reseller/wechat.png", "telegram_qr_code": "/uploads/reseller/telegram.png", "qq_number": "123456", "qq_group": "654321", "wechat_group_qr_code": "/uploads/reseller/group.png"},
                "footer_links": [{"name": {"zh-CN": "帮助"}, "url": "https://help.test"}],
                "nav_config": {"builtin": {"blog": false}},
                "scripts": [{"name": "客服", "enabled": true, "position": "body_end", "code": "window.widget=1", "scope": "main"}],
            })),
            Some(&token),
        )
        .await;
    let c = data(&saved);
    assert_eq!(c["footer_links"][0]["url"], "https://help.test");
    assert_eq!(c["scripts"][0]["scope"], "reseller");
    assert_eq!(c["storefront_template"], "aurora");
    assert_eq!(c["template_mode"], "list");
    assert_eq!(c["theme"]["primary_color"], "#123abc");
    assert_eq!(
        c["nav_config"]["builtin"],
        json!({"blog": false, "notice": true, "about": true})
    );
    assert_eq!(
        c["support"],
        json!({"telegram": "", "whatsapp": "", "email": "", "support_url": "", "wechat_id": "shop_wechat", "wechat_qr_code": "/uploads/reseller/wechat.png", "telegram_qr_code": "/uploads/reseller/telegram.png", "qq_number": "123456", "qq_group": "654321", "wechat_group_qr_code": "/uploads/reseller/group.png"})
    );
    let snap = app
        .call("GET", "/api/v1/reseller/site-config", None, Some(&token))
        .await;
    assert_eq!(data(&snap)["config"]["site_name"], "Shop");
    assert_eq!(
        data(&snap)["config"]["support"]["wechat_group_qr_code"],
        "/uploads/reseller/group.png"
    );
    assert_eq!(
        data(&snap)["config"]["theme"]["background_color"],
        "#eefbf7"
    );

    for (body, msg) in [
        (
            json!({"storefront_template": "unknown"}),
            "站点配置不合法，请检查后重试",
        ),
        (
            json!({"theme": {"primary_color": "url(evil)"}}),
            "站点配置不合法，请检查后重试",
        ),
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
            json!({"support": {"telegram_qr_code": "javascript:alert(1)"}}),
            "图片地址无效，请重新上传或填写以 https:// 开头的完整链接",
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

// UPL-03 / UPL-05: the reseller upload endpoint returns a usable image path.
#[tokio::test]
async fn reseller_customer_qr_can_be_uploaded_and_saved() {
    let mut cfg = reseller_common::config();
    let root = std::env::temp_dir().join(format!("zs-reseller-qr-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    cfg.upload.dir = root.to_string_lossy().into_owned();
    let app = App::with(cfg).await;
    let (_, token, _) = app.reseller("qr-support@example.test", "0").await;
    let mut png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR".to_vec();
    png.extend_from_slice(&32_u32.to_be_bytes());
    png.extend_from_slice(&32_u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]);
    let boundary = "zebraQrBoundary";
    let mut body = format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"support.png\"\r\nContent-Type: image/png\r\n\r\n").into_bytes();
    body.extend_from_slice(&png);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/reseller/upload")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let uploaded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let url = data(&uploaded)["url"].as_str().unwrap();
    assert!(url.starts_with("/uploads/reseller/"));
    assert!(root.join(url.trim_start_matches("/uploads/")).exists());

    let saved = app
        .call(
            "PUT",
            "/api/v1/reseller/site-config",
            Some(json!({
                "support": {"wechat_qr_code": url, "telegram_qr_code": url}
            })),
            Some(&token),
        )
        .await;
    assert_eq!(data(&saved)["support"]["wechat_qr_code"], url);
    assert_eq!(data(&saved)["support"]["telegram_qr_code"], url);
    std::fs::remove_dir_all(&root).unwrap();
}

// RSL-11: the reseller list, detail and preview must expose this site's cost,
// while another reseller continues to receive the shared agent price.
// RSL-12: recommended prices remain the main-site retail for both resellers.
#[tokio::test]
async fn rsl11_site_agent_price_in_console_responses() {
    let app = App::new().await;
    let (_, token, reseller_id) = app.reseller("site-price@example.test", "0").await;
    let (_, other_token, _) = app.reseller("shared-price@example.test", "0").await;
    let (product_id, skus) = app
        .product("site-price", "60.00", &[("60.00", "33.00")])
        .await;
    use sea_orm::{ActiveModelTrait, Set};
    use zs_infra::db::entity::product_skus;
    product_skus::ActiveModel {
        id: Set(skus[0]),
        agent_price_amount: Set(Some("45.00".parse().unwrap())),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let url = format!("/api/v1/reseller/product-settings/{product_id}");
    let admin_url = format!("/api/v1/admin/resellers/product-settings/{reseller_id}/{product_id}");
    data(&app.admin_call(
        "PUT",
        &admin_url,
        Some(json!({"settings": [{"sku_id": skus[0], "is_listed": true, "pricing_mode": "inherit", "agent_price_amount": "38.67"}]})),
    ).await);

    let list = app
        .call(
            "GET",
            "/api/v1/reseller/product-settings",
            None,
            Some(&token),
        )
        .await;
    let row = &data(&list)[0];
    assert_eq!(row["skus"][0]["base_price_amount"], "38.67");
    assert_eq!(row["skus"][0]["shared_agent_price_amount"], "45.00");
    assert_eq!(row["product"]["price_amount"], "45.00");
    assert_eq!(row["product"]["retail_price_amount"], "60.00");
    assert_eq!(row["skus"][0]["retail_price_amount"], "60.00");
    let detail = app.call("GET", &url, None, Some(&token)).await;
    assert_eq!(data(&detail)["skus"][0]["base_price_amount"], "38.67");
    assert_eq!(data(&detail)["product"]["retail_price_amount"], "60.00");
    assert_eq!(data(&detail)["skus"][0]["retail_price_amount"], "60.00");
    let preview = app.call("POST", &format!("{url}/preview"), Some(json!({"settings": [{"sku_id": skus[0], "is_listed": true, "pricing_mode": "inherit"}]})), Some(&token)).await;
    assert_eq!(data(&preview)["items"][1]["base_price_amount"], "38.67");

    let other = app
        .call(
            "GET",
            "/api/v1/reseller/product-settings",
            None,
            Some(&other_token),
        )
        .await;
    assert_eq!(data(&other)[0]["skus"][0]["base_price_amount"], "45.00");
    assert_eq!(data(&other)[0]["skus"][0]["retail_price_amount"], "60.00");
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

#[tokio::test]
async fn reseller_product_content_isolated_and_inherited() {
    use sea_orm::{ActiveModelTrait, EntityTrait, Set};
    use zs_infra::db::entity::{order_items, products};

    let app = App::new().await;
    let (_, first_token, first_id) = app.reseller("content-first@example.test", "0").await;
    let (_, _, second_id) = app.reseller("content-second@example.test", "0").await;
    let first_host = app.system_domain(first_id, "content-first").await;
    let second_host = app.system_domain(second_id, "content-second").await;
    let (product_id, skus) = app
        .product("content-product", "10.00", &[("10.00", "0")])
        .await;
    let baseline = products::Entity::find_by_id(product_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let mut model: products::ActiveModel = baseline.into();
    model.content_json = Set(Some(json!({"zh-CN": "主站详情"})));
    model.instructions_json = Set(Some(json!({"zh-CN": "主站说明"})));
    model.update(&app.db).await.unwrap();

    let path = "/api/v1/public/products/content-product";
    assert_eq!(
        data(
            &app.raw("GET", path, None, None, &[("host", &first_host)])
                .await
                .1
        )["content"]["zh-CN"],
        "主站详情"
    );
    let setting_url = format!("/api/v1/reseller/product-settings/{product_id}");
    let saved = app
        .call(
            "PUT",
            &setting_url,
            Some(json!({"settings": [{
                "sku_id": 0, "is_listed": true, "content": {"zh-CN": "分站详情"},
                "instructions": {"zh-CN": "分站说明"}
            }]})),
            Some(&first_token),
        )
        .await;
    assert_eq!(
        data(&saved)["product_setting"]["instructions"]["zh-CN"],
        "分站说明"
    );
    assert_eq!(
        data(
            &app.raw("GET", path, None, None, &[("host", &first_host)])
                .await
                .1
        )["content"]["zh-CN"],
        "分站详情"
    );
    assert_eq!(
        data(
            &app.raw("GET", path, None, None, &[("host", &second_host)])
                .await
                .1
        )["content"]["zh-CN"],
        "主站详情"
    );
    assert_eq!(
        data(&app.call("GET", path, None, None).await)["content"]["zh-CN"],
        "主站详情"
    );

    let (_, buyer_token) = app.user("content-buyer@example.test").await;
    let order = app
        .raw(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product_id, "sku_id": skus[0], "quantity": 1}]})),
            Some(&buyer_token),
            &[("host", &first_host)],
        )
        .await
        .1;
    assert_eq!(order["status_code"], 0, "{order}");
    let items = order_items::Entity::find().all(&app.db).await.unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(
        items[0].instructions_json.as_ref().unwrap()["zh-CN"],
        "分站说明"
    );

    data(
        &app.call(
            "PUT",
            &setting_url,
            Some(json!({"settings": [{
                "sku_id": 0, "is_listed": true, "content": null, "instructions": null
            }]})),
            Some(&first_token),
        )
        .await,
    );
    assert_eq!(
        data(
            &app.raw("GET", path, None, None, &[("host", &first_host)])
                .await
                .1
        )["content"]["zh-CN"],
        "主站详情"
    );
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
