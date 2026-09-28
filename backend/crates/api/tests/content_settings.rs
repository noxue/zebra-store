//! `/admin/settings/*` endpoints follow the original contract.

mod common;

use common::{TestApp, data};
use serde_json::json;

#[tokio::test]
async fn generic_settings_get_put_normalize() {
    let app = TestApp::new().await;

    // never saved → {}
    let v = app.get("/api/v1/admin/settings?key=site_config").await;
    assert_eq!(data(&v), &json!({}));

    let saved = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "site_config", "value": {
                "brand": {"site_name": " Zebra ", "site_logo": "/uploads/common/l.png"},
                "currency": "usd",
                "storefront_template": "evil",
                "scripts": [{"name": "ga", "code": "<script>1</script>", "position": "footer", "enabled": true}],
                "theme": {"primary_color": "#FF0000", "background_image": "javascript:alert(1)"},
            }}),
        )
        .await;
    let d = data(&saved);
    assert_eq!(d["brand"]["site_name"], "Zebra");
    assert_eq!(d["brand"]["site_logo"], "/uploads/common/l.png");
    assert_eq!(d["currency"], "USD");
    // SET-04: enum settings are whitelisted
    assert_eq!(d["storefront_template"], "classic");
    assert_eq!(d["scripts"][0]["position"], "head");
    assert_eq!(d["theme"]["primary_color"], "#ff0000");
    assert_eq!(d["theme"]["background_image"], "");

    // default key is site_config
    let v = app.get("/api/v1/admin/settings").await;
    assert_eq!(data(&v)["currency"], "USD");

    // QA-A01: unknown keys are refused on write and read (whitelist)
    let v = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "custom_x", "value": {"a": [1]}}),
        )
        .await;
    assert_eq!(v["status_code"], 400, "{v}");
    assert_eq!(v["msg"], "未知的设置项");
    let v = app.get("/api/v1/admin/settings?key=custom_x").await;
    assert_eq!(v["status_code"], 400, "{v}");

    // SET-04: upstream sync clamps
    let v = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "upstream_sync_config", "value": {"sync_page_size": 100000, "sync_max_pages": 0}}),
        )
        .await;
    assert_eq!(data(&v)["sync_page_size"], 200);
    assert_eq!(data(&v)["sync_max_pages"], 200);

    // value must be an object
    let bad = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "site_config", "value": [1]}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);

    // google auth is only writable through its dedicated endpoint
    let bad = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "google_auth_config", "value": {"enabled": true}}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    assert_eq!(
        bad["msg"],
        "google_auth_config must be updated through /admin/settings/google-auth"
    );
}

// SET-01 ①②③: callback routes are normalized on save; QA-A09 (live QA I-9):
// reserved, conflicting, traversal and duplicate paths are refused, not dropped.
#[tokio::test]
async fn set_01_callback_routes_normalized_via_api() {
    let app = TestApp::new().await;
    let v = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "callback_routes_config", "value": {
                "payment_callback": "",
                "stripe_webhook": "/api/pay/cb/?a=1#x",
                "paypal_webhook": "/api/cb",
            }}),
        )
        .await;
    let d = data(&v);
    assert_eq!(d["payment_callback"], "");
    assert_eq!(d["stripe_webhook"], "/api/pay/cb");
    assert_eq!(d["paypal_webhook"], "/api/cb");
    assert_eq!(d["upstream_callback"], "");
    let routes = app
        .services
        .content
        .settings
        .callback_routes()
        .await
        .unwrap()
        .unwrap();
    assert_eq!(routes.stripe_webhook, "/api/pay/cb");

    for bad in [
        json!({"payment_callback": "/api/v1/admin/x"}),
        json!({"payment_callback": "/api/v1/payments/webhook/paypal"}),
        json!({"payment_callback": "/api/v1/../admin/x"}),
        json!({"paypal_webhook": "/api/cb", "upstream_callback": "/api/cb"}),
    ] {
        let v = app
            .put(
                "/api/v1/admin/settings",
                json!({"key": "callback_routes_config", "value": bad}),
            )
            .await;
        assert_eq!(v["status_code"], 400, "{bad}: {v}");
        assert_eq!(v["msg"], "回调路由无效或与已有路由冲突", "{v}");
    }
    // the refused writes left the stored routes untouched
    let v = app
        .get("/api/v1/admin/settings?key=callback_routes_config")
        .await;
    assert_eq!(data(&v)["stripe_webhook"], "/api/pay/cb");
}

#[tokio::test]
async fn settings_require_admin() {
    let app = TestApp::anonymous().await;
    let v = app
        .call("GET", "/api/v1/admin/settings/smtp", None, None)
        .await;
    assert_eq!(v["status_code"], 401);
}

// CNT-09: secrets are masked and an empty secret in a patch keeps the stored one.
#[tokio::test]
async fn smtp_mask_patch_and_test() {
    let app = TestApp::new().await;
    let v = app.get("/api/v1/admin/settings/smtp").await;
    let d = data(&v);
    assert_eq!(d["password"], "");
    assert_eq!(d["has_password"], false);
    assert_eq!(d["order_notification_enabled"], true);
    assert_eq!(d["verify_code"]["length"], 6);

    let v = app
        .put(
            "/api/v1/admin/settings/smtp",
            json!({"host": " smtp.example.com ", "port": 587, "password": "pw", "from": "shop@example.com"}),
        )
        .await;
    assert_eq!(data(&v)["host"], "smtp.example.com");
    assert_eq!(data(&v)["has_password"], true);
    let v = app
        .put("/api/v1/admin/settings/smtp", json!({"password": ""}))
        .await;
    assert_eq!(data(&v)["has_password"], true);
    assert_eq!(
        app.services.content.settings.smtp().await.unwrap().password,
        "pw"
    );

    let bad = app
        .put(
            "/api/v1/admin/settings/smtp",
            json!({"use_tls": true, "use_ssl": true}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    assert_eq!(bad["msg"], "smtp config invalid: TLS 与 SSL 不能同时开启");

    let bad = app
        .post(
            "/api/v1/admin/settings/smtp/test",
            json!({"to_email": "not-mail"}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    assert_eq!(bad["msg"], "邮箱格式不正确");
}

#[tokio::test]
async fn smtp_test_requires_configuration() {
    let app = TestApp::new().await;
    let bad = app
        .post(
            "/api/v1/admin/settings/smtp/test",
            json!({"to_email": "a@b.com"}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    assert_eq!(bad["msg"], "邮箱服务未配置");
}

#[tokio::test]
async fn captcha_telegram_google_settings() {
    let app = TestApp::new().await;
    let v = app.get("/api/v1/admin/settings/captcha").await;
    assert_eq!(data(&v)["provider"], "none");
    assert_eq!(data(&v)["turnstile"]["has_secret"], false);

    let bad = app
        .put(
            "/api/v1/admin/settings/captcha",
            json!({"scenes": {"login": true}}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);

    let v = app
        .put(
            "/api/v1/admin/settings/captcha",
            json!({"provider": "turnstile", "scenes": {"login": true},
                   "turnstile": {"site_key": "sk", "secret_key": "sec"}}),
        )
        .await;
    assert_eq!(data(&v)["turnstile"]["secret_key"], "");
    assert_eq!(data(&v)["turnstile"]["has_secret"], true);

    // CNT-10: telegram auth masks token/secret and reports the mode
    let v = app
        .put(
            "/api/v1/admin/settings/telegram-auth",
            json!({"enabled": true, "bot_username": "@shop_bot", "bot_token": "123:abc"}),
        )
        .await;
    let d = data(&v);
    assert_eq!(d["bot_username"], "shop_bot");
    assert_eq!(d["bot_token"], "");
    assert_eq!(d["has_bot_token"], true);
    assert_eq!(d["mode"], "widget");
    let bad = app
        .put(
            "/api/v1/admin/settings/telegram-auth",
            json!({"client_secret": "x"}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);

    let bad = app
        .put(
            "/api/v1/admin/settings/google-auth",
            json!({"enabled": true}),
        )
        .await;
    assert_eq!(bad["msg"], "google auth config invalid: Client ID 不能为空");
    let v = app
        .put(
            "/api/v1/admin/settings/google-auth",
            json!({"enabled": true, "client_id": " cid "}),
        )
        .await;
    assert_eq!(data(&v), &json!({"enabled": true, "client_id": "cid"}));
    let v = app.get("/api/v1/admin/settings/google-auth").await;
    assert_eq!(data(&v)["client_id"], "cid");
}

#[tokio::test]
async fn affiliate_templates_notifications_bot() {
    let app = TestApp::new().await;
    let v = app
        .put(
            "/api/v1/admin/settings/affiliate",
            json!({"enabled": true, "commission_rate": 12.345, "confirm_days": 7,
                   "min_withdraw_amount": 10, "withdraw_channels": ["USDT", "usdt", " Alipay "]}),
        )
        .await;
    let d = data(&v);
    assert_eq!(d["commission_rate"], 12.35);
    assert_eq!(d["withdraw_channels"], json!(["USDT", "Alipay"]));
    assert_eq!(
        data(&app.get("/api/v1/admin/settings/affiliate").await)["confirm_days"],
        7
    );

    // order e-mail templates: patch, invalid, reset
    let v = app
        .put(
            "/api/v1/admin/settings/order-email-template",
            json!({"guest_tip": {"en-US": "Hi"}}),
        )
        .await;
    assert_eq!(data(&v)["guest_tip"]["en-US"], "Hi");
    let bad = app
        .put(
            "/api/v1/admin/settings/order-email-template",
            json!({"templates": {"paid": {"zh-CN": {"subject": ""}}}}),
        )
        .await;
    assert_eq!(bad["status_code"], 400);
    let v = app
        .post(
            "/api/v1/admin/settings/order-email-template/reset",
            json!({}),
        )
        .await;
    assert_eq!(
        data(&v)["guest_tip"]["en-US"],
        "Guest orders can be queried on the site using the checkout email and order password."
    );

    // notification center + legacy alias
    let v = app.get("/api/v1/admin/settings/notifications").await;
    assert_eq!(data(&v)["default_locale"], "zh-CN");
    assert_eq!(data(&v)["channels"]["feishu"]["has_app_secret"], false);
    let bad = app
        .put(
            "/api/v1/admin/settings/notification-center",
            json!({"channels": {"email": {"enabled": true, "recipients": []}}}),
        )
        .await;
    assert_eq!(
        bad["msg"],
        "notification config invalid: 邮件渠道已启用但未配置收件邮箱"
    );
    let v = app
        .put(
            "/api/v1/admin/settings/notification-center",
            json!({"dedupe_ttl_seconds": 5, "channels": {"telegram": {"enabled": true, "recipients": ["123456789"]}}}),
        )
        .await;
    assert_eq!(data(&v)["dedupe_ttl_seconds"], 300);
    assert_eq!(data(&v)["channels"]["telegram"]["enabled"], true);

    // telegram bot: whole-object update bumps config_version, runtime status follows
    let v = app.get("/api/v1/admin/settings/telegram-bot").await;
    let mut cfg = data(&v).clone();
    assert_eq!(cfg["menu"]["items"].as_array().unwrap().len(), 7);
    cfg["enabled"] = json!(true);
    let v = app.put("/api/v1/admin/settings/telegram-bot", cfg).await;
    assert_eq!(data(&v)["config_version"], 1);
    assert_eq!(data(&v)["enabled"], true);
    let v = app
        .get("/api/v1/admin/settings/telegram-bot/runtime-status")
        .await;
    assert_eq!(data(&v)["config_version"], 1);
    assert_eq!(data(&v)["connected"], false);
    assert_eq!(data(&v)["warnings"], json!([]));
}

/// QA-A01 (live QA I-1): the generic endpoint never returns stored secrets, a
/// generic write of a secret key goes through the validated patch (an empty
/// secret keeps the stored one), and server-owned keys are read-only.
#[tokio::test]
async fn qa_a01_generic_settings_mask_secrets() {
    let app = TestApp::new().await;
    let v = app
        .put(
            "/api/v1/admin/settings/smtp",
            json!({"host": "smtp.example.com", "port": 465, "username": "u", "password": "s3cret", "from": "a@example.com"}),
        )
        .await;
    assert_eq!(v["status_code"], 0, "{v}");
    let v = app
        .put(
            "/api/v1/admin/settings/captcha",
            json!({"provider": "turnstile", "turnstile": {"site_key": "sk", "secret_key": "ts-secret"}}),
        )
        .await;
    assert_eq!(v["status_code"], 0, "{v}");
    let v = app
        .put(
            "/api/v1/admin/settings/notification-center",
            json!({"channels": {"feishu": {"app_id": "cli", "app_secret": "fs-secret"}}}),
        )
        .await;
    assert_eq!(v["status_code"], 0, "{v}");

    for key in [
        "smtp_config",
        "captcha_config",
        "telegram_auth_config",
        "notification_center_config",
    ] {
        let v = app.get(&format!("/api/v1/admin/settings?key={key}")).await;
        assert_eq!(v["status_code"], 0, "{key}: {v}");
        let text = v.to_string();
        for secret in ["s3cret", "ts-secret", "fs-secret"] {
            assert!(!text.contains(secret), "{key} leaks {secret}: {text}");
        }
    }
    let v = app.get("/api/v1/admin/settings?key=smtp_config").await;
    assert_eq!(data(&v)["password"], "");
    assert_eq!(data(&v)["has_password"], true);
    assert_eq!(data(&v)["host"], "smtp.example.com");

    // generic write: empty password keeps the stored one, other fields change
    let v = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "smtp_config", "value": {"host": "mail.example.com", "password": ""}}),
        )
        .await;
    assert_eq!(v["status_code"], 0, "{v}");
    assert_eq!(data(&v)["password"], "");
    assert_eq!(data(&v)["has_password"], true);
    let smtp = app.services.content.settings.smtp().await.unwrap();
    assert_eq!(smtp.password, "s3cret");
    assert_eq!(smtp.host, "mail.example.com");

    // generic write is validated like the dedicated endpoint
    let v = app
        .put(
            "/api/v1/admin/settings",
            json!({"key": "captcha_config", "value": {"provider": "turnstile", "turnstile": {"site_key": ""}}}),
        )
        .await;
    assert_eq!(v["status_code"], 400, "{v}");

    // server-owned keys are readable but not writable
    for key in [
        "telegram_bot_runtime_status",
        "compliance.acknowledgement.v1",
    ] {
        let v = app
            .put(
                "/api/v1/admin/settings",
                json!({"key": key, "value": {"connected": true}}),
            )
            .await;
        assert_eq!(v["status_code"], 400, "{key}: {v}");
    }
}

/// QA-A04 (live QA I-4): saving affiliate (and every other key feeding
/// `/public/config`) invalidates the cached payload immediately.
#[tokio::test]
async fn qa_a04_affiliate_save_invalidates_public_config() {
    let app = TestApp::new().await;
    let before = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&before)["affiliate"]["enabled"], false, "{before}");
    let v = app
        .put(
            "/api/v1/admin/settings/affiliate",
            json!({"enabled": true, "commission_rate": 10, "confirm_days": 7, "min_withdraw_amount": 10, "withdraw_channels": ["alipay"]}),
        )
        .await;
    assert_eq!(v["status_code"], 0, "{v}");
    let after = app.call("GET", "/api/v1/public/config", None, None).await;
    assert_eq!(data(&after)["affiliate"]["enabled"], true, "{after}");

    // smtp_enabled is part of the cached payload too
    let smtp_before =
        data(&app.call("GET", "/api/v1/public/config", None, None).await)["smtp_enabled"].clone();
    app.put(
        "/api/v1/admin/settings/smtp",
        json!({"enabled": smtp_before != json!(true), "host": "h", "port": 25, "from": "a@example.com"}),
    )
    .await;
    let smtp_after =
        data(&app.call("GET", "/api/v1/public/config", None, None).await)["smtp_enabled"].clone();
    assert_ne!(smtp_before, smtp_after, "smtp_enabled must not be stale");
}
