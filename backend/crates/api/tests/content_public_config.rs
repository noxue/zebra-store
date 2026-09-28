//! `GET /api/v1/public/config` shape, caching and invalidation.

#![expect(clippy::unwrap_used, reason = "test helpers abort on failure")]

mod common;

use chrono::{Duration, Utc};
use common::{TestApp, data};
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use zs_infra::db::entity::payment_channels;

async fn insert_channel(
    app: &TestApp,
    name: &str,
    roles: serde_json::Value,
    active: bool,
    sort: i32,
) {
    payment_channels::ActiveModel {
        name: Set(name.into()),
        icon: Set(String::new()),
        provider_type: Set("epay".into()),
        channel_type: Set("alipay".into()),
        interaction_mode: Set("redirect".into()),
        fee_rate: Set(Decimal::new(150, 2)),
        fixed_fee: Set(Decimal::ZERO),
        min_amount: Set(Decimal::ZERO),
        max_amount: Set(Decimal::new(100_000, 2)),
        hide_amount_out_range: Set(false),
        payment_roles: Set(Some(roles)),
        member_levels: Set(None),
        payment_types: Set(Some(json!(["order"]))),
        config_json: Set(Some(json!({"secret": "never-public"}))),
        is_active: Set(active),
        sort_order: Set(sort),
        created_at: Set(Utc::now()),
        updated_at: Set(Utc::now()),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn public_config_shape() {
    let app = TestApp::new().await;
    insert_channel(&app, "Alipay", json!([]), true, 1).await;
    insert_channel(&app, "Members", json!(["member"]), true, 5).await;
    insert_channel(&app, "Disabled", json!([]), false, 9).await;

    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    let d = data(&v);
    assert!(d["server_time"].as_i64().unwrap() > 0);
    assert!(d["app_version"].is_string());
    assert_eq!(d["tenant"]["mode"], "main");
    assert_eq!(d["languages"], json!(["zh-CN", "zh-TW", "en-US"]));
    assert_eq!(d["currency"], "CNY");
    assert_eq!(d["storefront_template"], "classic");
    assert_eq!(d["contact"]["telegram"], "https://telegram.me/dujiaoka");
    assert_eq!(d["scripts"], json!([]));
    assert_eq!(
        d["captcha"],
        json!({"provider": "none", "scenes": {
        "login": false, "register_send_code": false, "reset_send_code": false,
        "guest_create_order": false, "gift_card_redeem": false}})
    );
    assert_eq!(
        d["telegram_auth"],
        json!({"enabled": false, "bot_username": "", "mini_app_url": "", "mode": ""})
    );
    assert_eq!(d["google_auth"], json!({"enabled": false, "client_id": ""}));
    assert_eq!(d["affiliate"]["enabled"], false);
    assert_eq!(d["smtp_enabled"], false);
    assert_eq!(d["registration_enabled"], true);
    assert_eq!(d["email_verification_enabled"], true);
    assert_eq!(d["email_domain_allowlist_enabled"], false);
    assert_eq!(d["allowed_email_domains"], json!([]));
    assert_eq!(
        d["nav_config"],
        json!({"builtin": {"blog": true, "notice": true, "about": true}, "custom_items": []})
    );
    assert!(d.get("announcement").is_none());
    assert!(d.get("wallet_only_payment").is_none());
    // theme defaults (docs/DESIGN.md)
    assert_eq!(d["theme"]["primary_color"], "#ff5fa2");
    assert_eq!(d["theme"]["secondary_color"], "#8b5cf6");
    assert_eq!(d["theme"]["accent_color"], "#38bdf8");
    assert_eq!(
        d["theme"]["effects"],
        json!({"sakura": true, "sparkle": true})
    );
    assert_eq!(d["theme"]["default_mode"], "system");
    // only active, guest-visible channels; no secrets
    let channels = d["payment_channels"].as_array().unwrap();
    assert_eq!(channels.len(), 1);
    assert_eq!(channels[0]["name"], "Alipay");
    assert_eq!(channels[0]["max_amount"], "1000.00");
    assert!(channels[0].get("config_json").is_none());
    assert!(channels[0].get("fee_rate").is_none());
}

// CNT-07: saving a setting that feeds the public config invalidates the 60 s cache.
#[tokio::test]
async fn cache_is_invalidated_on_save() {
    let app = TestApp::new().await;
    let first = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&first)["currency"], "CNY");

    app.put(
        "/api/v1/admin/settings",
        json!({"key": "site_config", "value": {"currency": "USD", "brand": {"site_name": "Zebra"}}}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&v)["currency"], "USD");
    assert_eq!(data(&v)["brand"]["site_name"], "Zebra");

    app.put(
        "/api/v1/admin/settings",
        json!({"key": "payment_config", "value": {"customer_fee_enabled": true}}),
    )
    .await;
    app.put(
        "/api/v1/admin/settings",
        json!({"key": "wallet_config", "value": {"wallet_only_payment": true, "recharge_channel_ids": [3, 0]}}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&v)["wallet_only_payment"], true);
    assert_eq!(data(&v)["wallet_recharge_channel_ids"], json!([3]));

    // captcha / google updates go through dedicated endpoints and also invalidate
    app.put(
        "/api/v1/admin/settings/google-auth",
        json!({"enabled": true, "client_id": "cid"}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(
        data(&v)["google_auth"],
        json!({"enabled": true, "client_id": "cid"})
    );

    app.put(
        "/api/v1/admin/settings",
        json!({"key": "registration_config", "value": {"registration_enabled": false,
            "email_domain_allowlist_enabled": true, "allowed_email_domains": "gmail.com, bad_x"}}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&v)["registration_enabled"], false);
    assert_eq!(data(&v)["allowed_email_domains"], json!(["gmail.com"]));
}

// SET-02: the announcement is evaluated per request (schedule) and never exposes `enabled`.
#[tokio::test]
async fn set_02_announcement_schedule() {
    let app = TestApp::new().await;
    let past = (Utc::now() - Duration::days(1)).to_rfc3339();
    app.put(
        "/api/v1/admin/settings",
        json!({"key": "home_announcement", "value": {"enabled": true, "type": "warning",
            "title": {"zh-CN": "维护"}, "content": {"zh-CN": "<p>今晚维护</p>"}, "end_at": past}}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    assert!(
        data(&v).get("announcement").is_none(),
        "expired announcement hidden"
    );

    app.put(
        "/api/v1/admin/settings",
        json!({"key": "home_announcement", "value": {"enabled": true, "type": "warning",
            "title": {"zh-CN": "维护"}, "content": {"zh-CN": "<p>今晚维护</p>"}}}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    let a = &data(&v)["announcement"];
    assert_eq!(a["type"], "warning");
    assert_eq!(a["content"]["zh-CN"], "<p>今晚维护</p>");
    assert_eq!(a["version"].as_str().unwrap().len(), 8);
    assert!(a.get("enabled").is_none());

    // nav_config is stored normalized and served as-is
    app.put(
        "/api/v1/admin/settings",
        json!({"key": "nav_config", "value": {"builtin": {"about": false}}}),
    )
    .await;
    let v = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&v)["nav_config"]["builtin"]["about"], false);
    assert_eq!(data(&v)["nav_config"]["builtin"]["blog"], true);
}
