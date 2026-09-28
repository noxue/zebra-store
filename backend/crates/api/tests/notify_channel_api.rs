//! Channel API (`/api/v1/channel/*`) contract: HMAC auth, response envelope,
//! Telegram bot config/heartbeat, identities, catalog, member levels and
//! payment channels.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures: failures should abort the test"
)]

mod common;
mod notify_common;

use chrono::Utc;
use common::{TestApp, data};
use notify_common::{channel, channel_client, channel_raw, ok_data, set_setting, signed_headers};
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::payment_channels;

fn without_request_id(mut v: Value) -> Value {
    if let Some(m) = v.as_object_mut() {
        m.remove("request_id");
    }
    v
}

// NTF-06 (2): wrong key, wrong signature and stale timestamps are indistinguishable.
#[tokio::test]
async fn ntf06_channel_auth_failures_are_uniform() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "", "").await;

    let missing = channel_raw(&app, "GET", "/me?channel_user_id=1", None, &[]).await;
    assert_eq!(missing.status, 401);
    assert_eq!(missing.body["status_code"], 401);
    assert_eq!(missing.body["error_code"], "channel_client_unauthorized");
    assert!(missing.body["request_id"].is_string());
    assert!(missing.body.get("data").is_none());

    let path = "/api/v1/channel/me";
    let wrong_key = signed_headers("f".repeat(64).as_str(), &creds.2, "GET", path, b"", 0);
    let wrong_sig = signed_headers(&creds.1, "not-the-secret", "GET", path, b"", 0);
    let stale = signed_headers(&creds.1, &creds.2, "GET", path, b"", -61);
    let short_path = signed_headers(&creds.1, &creds.2, "GET", "/me", b"", 0);
    let a = channel_raw(&app, "GET", "/me?channel_user_id=1", None, &wrong_key).await;
    let b = channel_raw(&app, "GET", "/me?channel_user_id=1", None, &wrong_sig).await;
    let c = channel_raw(&app, "GET", "/me?channel_user_id=1", None, &stale).await;
    let d = channel_raw(&app, "GET", "/me?channel_user_id=1", None, &short_path).await;
    for r in [&a, &b, &c, &d] {
        assert_eq!(r.status, 401);
    }
    assert_eq!(
        without_request_id(a.body.clone()),
        without_request_id(b.body.clone())
    );
    assert_eq!(without_request_id(a.body), without_request_id(c.body));

    // Signed correctly: accepted; the query string is not part of the signature.
    let ok = channel(&app, &creds, "GET", "/me?channel_user_id=1", None).await;
    assert_eq!(ok_data(&ok), json!({"bound": false}));

    // Tampered body → 401.
    let headers = signed_headers(
        &creds.1,
        &creds.2,
        "POST",
        "/api/v1/channel/identities/telegram/resolve",
        b"{}",
        0,
    );
    let tampered = channel_raw(
        &app,
        "POST",
        "/identities/telegram/resolve",
        Some(json!({"channel_user_id": "1"})),
        &headers,
    )
    .await;
    assert_eq!(tampered.status, 401);

    // Disabled client → 403 channel_client_disabled.
    app.put(
        &format!("/api/v1/admin/channel-clients/{}/status", creds.0),
        json!({"status": 0}),
    )
    .await;
    let disabled = channel(&app, &creds, "GET", "/me?channel_user_id=1", None).await;
    assert_eq!(disabled.status, 403);
    assert_eq!(disabled.body["status_code"], 403);
    assert_eq!(disabled.body["error_code"], "channel_client_disabled");
}

#[tokio::test]
async fn telegram_config_and_heartbeat() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "777:SECRET-TOKEN", "").await;

    let cfg = ok_data(&channel(&app, &creds, "GET", "/telegram/config", None).await);
    assert_eq!(cfg["config"]["bot_token"], "777:SECRET-TOKEN");
    assert_eq!(cfg["config"]["menu"]["items"].as_array().unwrap().len(), 7);
    assert_eq!(cfg["config_version"], 0);

    let hb = channel(
        &app,
        &creds,
        "POST",
        "/telegram/heartbeat",
        Some(json!({"bot_version": "2.1.0", "webhook_status": "ok", "warnings": ["license soon"]})),
    )
    .await;
    assert_eq!(ok_data(&hb), json!({"config_version": 0}));
    let status = app
        .get("/api/v1/admin/settings/telegram-bot/runtime-status")
        .await;
    let s = data(&status);
    assert_eq!(s["connected"], true);
    assert_eq!(s["bot_version"], "2.1.0");
    assert_eq!(s["warnings"], json!(["license soon"]));
    assert!(s["last_seen_at"].as_str().unwrap().ends_with('Z'));

    let bad = channel(
        &app,
        &creds,
        "POST",
        "/telegram/heartbeat",
        Some(json!("nope")),
    )
    .await;
    assert_eq!(bad.status, 400);
    assert_eq!(bad.body["error_code"], "validation_error");
}

#[tokio::test]
async fn telegram_identity_flows() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "", "").await;

    let unbound = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/resolve",
        Some(json!({"channel_user_id": "10001"})),
    )
    .await;
    assert_eq!(ok_data(&unbound), json!({"bound": false}));
    let invalid = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/resolve",
        Some(json!({})),
    )
    .await;
    assert_eq!(
        (invalid.status, invalid.body["error_code"].clone()),
        (400, json!("validation_error"))
    );

    let created = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/provision",
        Some(json!({"telegram_user_id": "10001", "telegram_username": "neo", "first_name": "Thomas"})),
    )
    .await;
    let d = ok_data(&created);
    assert_eq!(d["bound"], true);
    assert_eq!(d["created"], true);
    assert_eq!(d["identity"]["provider"], "telegram");
    assert_eq!(d["identity"]["provider_user_id"], "10001");
    assert_eq!(d["identity"]["username"], "neo");
    assert_eq!(d["user"]["email"], "telegram_10001@login.local");
    assert_eq!(d["user"]["display_name"], "Thomas");
    assert_eq!(d["user"]["status"], "active");
    assert_eq!(d["user"]["password_setup_required"], true);
    assert_eq!(d["user"]["email_verified"], false);
    let user_id = d["user"]["id"].as_i64().unwrap();

    let again = ok_data(
        &channel(
            &app,
            &creds,
            "POST",
            "/identities/telegram/provision",
            Some(json!({"channel_user_id": "10001"})),
        )
        .await,
    );
    assert_eq!(again["created"], false);
    assert_eq!(again["user"]["id"], user_id);

    let me = ok_data(&channel(&app, &creds, "GET", "/me?telegram_user_id=10001", None).await);
    assert_eq!(me["bound"], true);
    assert_eq!(me["user"]["id"], user_id);
    let no_id = channel(&app, &creds, "GET", "/me", None).await;
    assert_eq!(no_id.status, 400);

    // Bind needs a valid email code; unsupported bind modes are rejected.
    let bad_mode = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/bind",
        Some(json!({"channel_user_id": "10001", "bind_mode": "sms"})),
    )
    .await;
    assert_eq!(bad_mode.body["error_code"], "validation_error");
    let bad_code = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/bind",
        Some(json!({"channel_user_id": "10001", "email": "buyer@example.com", "code": "000000"})),
    )
    .await;
    assert_eq!(bad_code.status, 400);
    assert_eq!(bad_code.body["error_code"], "verify_code_invalid");
    let bad_email = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/bind",
        Some(json!({"channel_user_id": "10001", "email": "nope", "code": "1"})),
    )
    .await;
    assert_eq!(bad_email.body["error_code"], "validation_error");

    // 803f11a1: no Telegram sign-up while registration is closed.
    set_setting(
        &app,
        "registration_config",
        json!({"registration_enabled": false}),
    )
    .await;
    let closed = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/provision",
        Some(json!({"channel_user_id": "20002"})),
    )
    .await;
    assert_eq!(closed.status, 500);
    assert_eq!(closed.body["error_code"], "internal_error");
    // Existing bindings keep working.
    let existing = channel(
        &app,
        &creds,
        "POST",
        "/identities/telegram/provision",
        Some(json!({"channel_user_id": "10001"})),
    )
    .await;
    assert_eq!(ok_data(&existing)["user"]["id"], user_id);
}

async fn category(app: &TestApp, slug: &str, parent: i64) -> i64 {
    let res = app
        .post(
            "/api/v1/admin/categories",
            json!({"name": {"zh-CN": format!("分类{slug}"), "en-US": format!("Cat {slug}")}, "slug": slug, "parent_id": parent}),
        )
        .await;
    data(&res)["id"].as_i64().unwrap()
}

async fn product(app: &TestApp, category_id: i64, slug: &str) -> i64 {
    let res = app
        .post(
            "/api/v1/admin/products",
            json!({
                "category_id": category_id,
                "slug": slug,
                "title": {"zh-CN": format!("商品 {slug}"), "en-US": format!("Product {slug}")},
                "description": {"zh-CN": "<p>第一行</p>\n<p>第二行</p>"},
                "content": {"en-US": "<h1>Detail</h1>"},
                "price_amount": 10,
                "fulfillment_type": "manual",
                "manual_stock_total": 50,
                "is_active": true,
            }),
        )
        .await;
    data(&res)["id"].as_i64().unwrap()
}

#[tokio::test]
async fn catalog_endpoints() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "", "").await;
    let games = category(&app, "games", 0).await;
    let empty = category(&app, "empty", 0).await;
    let lonely = category(&app, "lonely", 0).await;
    let sub = category(&app, "sub", empty).await;
    let pid = product(&app, games, "steam").await;

    let cats = ok_data(
        &channel(
            &app,
            &creds,
            "GET",
            "/catalog/categories?locale=en-US",
            None,
        )
        .await,
    );
    let items = cats["items"].as_array().unwrap();
    let ids: Vec<i64> = items.iter().map(|c| c["id"].as_i64().unwrap()).collect();
    assert!(ids.contains(&games) && ids.contains(&empty) && ids.contains(&sub));
    assert!(
        !ids.contains(&lonely),
        "empty root without children is hidden"
    );
    let g = items.iter().find(|c| c["id"] == games).unwrap();
    assert_eq!(g["name"], "Cat games");
    assert_eq!(g["product_count"], 1);
    assert_eq!(g["slug"], "games");

    let list = ok_data(
        &channel(
            &app,
            &creds,
            "GET",
            &format!("/catalog/products?category_id={games}&page_size=50"),
            None,
        )
        .await,
    );
    assert_eq!(list["total"], 1);
    assert_eq!(list["page"], 1);
    assert_eq!(list["page_size"], 20, "page size is capped at 20");
    assert_eq!(list["total_page"], 1);
    let p = &list["items"][0];
    assert_eq!(p["id"], pid);
    assert_eq!(p["title"], "商品 steam");
    assert_eq!(p["summary"], "第一行\n第二行");
    assert_eq!(p["price_from"], "10.00");
    assert_eq!(p["currency"], "CNY");
    assert_eq!(p["stock_status"], "in_stock");
    assert_eq!(p["stock_count"], 50);
    assert_eq!(p["category_name"], "分类games");
    assert!(p.get("member_price_from").is_none());
    assert!(p.get("wholesale_prices").is_none());

    let detail = ok_data(
        &channel(
            &app,
            &creds,
            "GET",
            &format!("/catalog/products/{pid}?locale=en-US"),
            None,
        )
        .await,
    );
    assert_eq!(detail["title"], "Product steam");
    assert_eq!(detail["description"], "Detail");
    assert_eq!(detail["fulfillment_type"], "manual");
    assert_eq!(detail["member_price_from"], "");
    assert!(detail["wholesale_prices"].is_null());
    assert_eq!(detail["purchase_note"], "");
    assert_eq!(detail["manual_form_schema"], json!({"fields": []}));
    assert_eq!(detail["skus"][0]["sku_code"], "DEFAULT");
    assert_eq!(detail["skus"][0]["price"], "10.00");
    assert_eq!(detail["skus"][0]["stock_count"], 50);

    for bad in ["/catalog/products/99999", "/catalog/products/abc"] {
        let r = channel(&app, &creds, "GET", bad, None).await;
        assert_eq!(r.status, 404);
        assert_eq!(r.body["error_code"], "product_not_found");
    }

    let levels = ok_data(&channel(&app, &creds, "GET", "/member-levels?locale=en-US", None).await);
    assert!(levels["items"].is_array());
}

async fn payment_channel(app: &TestApp, provider: &str, fee: &str) -> i64 {
    let now = Utc::now();
    payment_channels::ActiveModel {
        name: Set(format!("{provider} channel")),
        icon: Set(String::new()),
        provider_type: Set(provider.into()),
        channel_type: Set("alipay".into()),
        interaction_mode: Set("redirect".into()),
        fee_rate: Set(fee.parse::<Decimal>().unwrap()),
        fixed_fee: Set(Decimal::ZERO),
        min_amount: Set(Decimal::ZERO),
        max_amount: Set(Decimal::ZERO),
        hide_amount_out_range: Set(false),
        is_active: Set(true),
        sort_order: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id
}

#[tokio::test]
async fn payment_channels_endpoint() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "", "").await;
    let epay = payment_channel(&app, "epay", "1.5").await;
    let _wallet = payment_channel(&app, "wallet", "0").await;

    for path in ["/payment-channels", "/payment-methods"] {
        let d = ok_data(&channel(&app, &creds, "GET", path, None).await);
        let items = d["items"].as_array().unwrap();
        assert_eq!(items.len(), 1, "wallet provider is never offered");
        assert_eq!(items[0]["id"], epay);
        assert_eq!(items[0]["provider_type"], "epay");
        assert_eq!(items[0]["fee_rate"], "1.50");
        assert_eq!(items[0]["fixed_fee"], "0.00");
        assert!(d.get("wallet_only_payment").is_none());
    }

    set_setting(
        &app,
        "wallet_config",
        json!({"wallet_only_payment": true, "recharge_channel_ids": [999]}),
    )
    .await;
    let recharge = ok_data(
        &channel(
            &app,
            &creds,
            "GET",
            "/payment-channels?context=recharge",
            None,
        )
        .await,
    );
    assert_eq!(recharge["items"], json!([]));
    assert_eq!(recharge["wallet_only_payment"], true);
}
