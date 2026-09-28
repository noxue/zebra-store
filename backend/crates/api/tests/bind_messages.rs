//! `binding:"..."` validation messages of every module (original `ginutil.RespondBindError`:
//! `"Field: rule; Field: rule"` with the Go field names in struct order), plus the
//! endpoints whose original handler answers with the plain `error.bad_request`.

mod integration_common;

use integration_common::{IntApp, err};
use serde_json::{Value, json};

const BAD: &str = "请求参数错误";

async fn admin(app: &IntApp, method: &str, path: &str, body: Value) -> Value {
    app.admin(method, &format!("/api/v1/admin{path}"), Some(body))
        .await
}

async fn user(app: &IntApp, token: &str, method: &str, path: &str, body: Value) -> Value {
    app.call(method, &format!("/api/v1{path}"), Some(body), Some(token))
        .await
}

async fn anon(app: &IntApp, method: &str, path: &str, body: Value) -> Value {
    app.call(method, &format!("/api/v1{path}"), Some(body), None)
        .await
}

#[tokio::test]
async fn admin_modules_name_failed_fields() {
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;
    let cases: &[(&str, &str, Value, &str)] = &[
        // settings
        (
            "PUT",
            "/settings",
            json!({}),
            "Key: 不能为空; Value: 不能为空",
        ),
        (
            "PUT",
            "/settings",
            json!({"key": "site_config", "value": null}),
            "Value: 不能为空",
        ),
        (
            "POST",
            "/settings/smtp/test",
            json!({"to_email": ""}),
            "ToEmail: 不能为空",
        ),
        // authz
        ("POST", "/authz/roles", json!({}), "Role: 不能为空"),
        (
            "POST",
            "/authz/policies",
            json!({"role": "r"}),
            "Object: 不能为空; Action: 不能为空",
        ),
        (
            "POST",
            "/authz/admins",
            json!({}),
            "Username: 不能为空; Password: 不能为空",
        ),
        // users
        (
            "PUT",
            "/users/batch-status",
            json!({"status": "active"}),
            "UserIDs: 不能为空",
        ),
        // orders / refunds / fulfillment
        ("PATCH", "/orders/1", json!({}), "Status: 不能为空"),
        (
            "POST",
            "/orders/1/refund-to-wallet",
            json!({"amount": ""}),
            "Amount: 不能为空",
        ),
        (
            "POST",
            "/orders/1/manual-refund",
            json!({}),
            "Amount: 不能为空",
        ),
        (
            "PATCH",
            "/order-refunds/1/payment-fee",
            json!({"payment_fee_refunded": null}),
            "PaymentFeeRefunded: 不能为空",
        ),
        (
            "POST",
            "/fulfillments",
            json!({"payload": "x"}),
            "OrderID: 不能为空",
        ),
        // catalog
        (
            "PATCH",
            "/products/1/wholesale-prices",
            json!({}),
            "WholesalePrices: 不能为空",
        ),
        ("POST", "/products/batch-delete", json!({}), "IDs: 不能为空"),
        (
            "POST",
            "/products/batch-delete",
            json!({"ids": []}),
            "IDs: 最小值为 1",
        ),
        (
            "POST",
            "/products/batch-status",
            json!({"ids": [], "is_active": true}),
            "IDs: 最小值为 1",
        ),
        (
            "POST",
            "/card-secrets/export-available",
            json!({}),
            "ProductID: 不能为空; Limit: 不能为空; Format: 不能为空",
        ),
        (
            "PATCH",
            "/card-secrets/batch-status",
            json!({"ids": [1]}),
            "Status: 不能为空",
        ),
        (
            "POST",
            "/card-secrets/export",
            json!({"ids": [1]}),
            "Format: 不能为空",
        ),
        // content
        (
            "POST",
            "/post-categories",
            json!({}),
            "NameJSON: 不能为空; Slug: 不能为空",
        ),
        (
            "PATCH",
            "/post-categories/1/status",
            json!({}),
            "IsActive: 不能为空",
        ),
        (
            "POST",
            "/media/batch-delete",
            json!({"ids": []}),
            "IDs: 最小值为 1",
        ),
        // marketing
        (
            "POST",
            "/member-level-prices/batch",
            json!({}),
            "Prices: 不能为空",
        ),
        (
            "PATCH",
            "/gift-cards/batch-status",
            json!({"ids": []}),
            "Status: 不能为空",
        ),
        (
            "POST",
            "/gift-cards/export",
            json!({}),
            "IDs: 不能为空; Format: 不能为空",
        ),
        // wallet / affiliate
        (
            "POST",
            "/users/1/wallet/adjust",
            json!({"remark": "x"}),
            "Amount: 不能为空",
        ),
        (
            "PATCH",
            "/affiliates/users/batch-status",
            json!({}),
            "ProfileIDs: 不能为空; Status: 不能为空",
        ),
        (
            "PATCH",
            "/affiliates/users/1/status",
            json!({"status": ""}),
            "Status: 不能为空",
        ),
        // integration
        (
            "POST",
            "/product-mappings/import",
            json!({}),
            "ConnectionID: 不能为空; UpstreamProductID: 不能为空",
        ),
        (
            "POST",
            "/product-mappings/batch-import",
            json!({"connection_id": 1, "upstream_product_ids": []}),
            "UpstreamProductIDs: 最小值为 1",
        ),
        (
            "POST",
            "/product-mappings/batch-sync",
            json!({"ids": []}),
            "IDs: 最小值为 1",
        ),
        (
            "POST",
            "/api-credentials/1/reject",
            json!({}),
            "Reason: 不能为空",
        ),
        (
            "PUT",
            "/site-connections/1/status",
            json!({}),
            "Status: 不能为空",
        ),
        // notify
        (
            "POST",
            "/settings/notification-center/test",
            json!({}),
            "Channel: 不能为空; Target: 不能为空",
        ),
        (
            "POST",
            "/channel-clients",
            json!({"name": "Bot"}),
            "ChannelType: 不能为空",
        ),
        (
            "PUT",
            "/channel-clients/1/status",
            json!({"status": 2}),
            "Status: 必须是以下值之一: 0 1",
        ),
        (
            "POST",
            "/telegram-bot/broadcasts",
            json!({}),
            "Title: 不能为空; RecipientType: 不能为空; MessageHTML: 不能为空",
        ),
    ];
    for (method, path, body, msg) in cases {
        let res = admin(&app, method, path, body.clone()).await;
        assert_eq!(
            (res["status_code"].as_i64(), res["msg"].as_str()),
            (Some(400), Some(*msg)),
            "{method} {path} {body}: {res}"
        );
    }

    // Rule parameters are substituted per locale.
    let en = admin(
        &app,
        "POST",
        "/products/batch-delete?lang=en-US",
        json!({"ids": []}),
    )
    .await;
    err(&en, 400, "IDs: minimum is 1");
    let en = admin(
        &app,
        "PUT",
        "/channel-clients/1/status?lang=en-US",
        json!({"status": 5}),
    )
    .await;
    err(&en, 400, "Status: must be one of: 0 1");

    // Passing rules reach the handler: an absent `status` is the Go zero value 0 (allowed
    // by `oneof=0 1`), `false` satisfies a `*bool` `required`, and the update of a post
    // category has no rules at all.
    for (method, path, body) in [
        ("PUT", "/channel-clients/999/status", json!({})),
        (
            "PATCH",
            "/order-refunds/999/payment-fee",
            json!({"payment_fee_refunded": false}),
        ),
        ("PUT", "/post-categories/999", json!({})),
    ] {
        let res = admin(&app, method, path, body).await;
        assert!(
            !res["msg"].as_str().unwrap_or_default().contains("不能为空"),
            "{method} {path}: {res}"
        );
    }

    // Type errors stay the generic message, even with missing fields.
    let typed = admin(&app, "PUT", "/settings", json!({"key": 5})).await;
    err(&typed, 400, BAD);
}

#[tokio::test]
async fn storefront_modules_name_failed_fields() {
    let app = IntApp::new().await;
    let (_, token) = app.user("binder@example.com").await;
    let cases: &[(&str, &str, Value, &str)] = &[
        (
            "POST",
            "/cart/items",
            json!({}),
            "ProductID: 不能为空; Quantity: 不能为空",
        ),
        (
            "POST",
            "/orders",
            json!({"coupon_code": "x"}),
            "Items: 不能为空",
        ),
        ("POST", "/orders/preview", json!({}), "Items: 不能为空"),
        (
            "POST",
            "/orders/create-and-pay",
            json!({}),
            "Items: 不能为空",
        ),
        (
            "POST",
            "/order/payment-channels",
            json!({}),
            "Amount: 不能为空",
        ),
        (
            "POST",
            "/payments",
            json!({"channel_id": 1}),
            "OrderNo: 不能为空",
        ),
        (
            "POST",
            "/wallet/recharge",
            json!({}),
            "Amount: 不能为空; ChannelID: 不能为空",
        ),
        (
            "POST",
            "/wallet/payment-channels",
            json!({}),
            "Amount: 不能为空",
        ),
        ("POST", "/gift-cards/redeem", json!({}), "Code: 不能为空"),
        (
            "PUT",
            "/me/password",
            json!({"old_password": "x"}),
            "NewPassword: 不能为空",
        ),
        (
            "POST",
            "/me/email/change",
            json!({}),
            "NewEmail: 不能为空; NewCode: 不能为空",
        ),
        (
            "POST",
            "/me/email/send-verify-code",
            json!({}),
            "Kind: 不能为空",
        ),
        ("POST", "/me/2fa/enable", json!({}), "Code: 不能为空"),
        (
            "POST",
            "/me/telegram/bind",
            json!({"id": 1}),
            "AuthDate: 不能为空; Hash: 不能为空",
        ),
        (
            "POST",
            "/me/telegram/oidc/callback",
            json!({"code": "c"}),
            "State: 不能为空",
        ),
        ("POST", "/me/google/bind", json!({}), "Credential: 不能为空"),
    ];
    for (method, path, body, msg) in cases {
        let res = user(&app, &token, method, path, body.clone()).await;
        assert_eq!(
            (res["status_code"].as_i64(), res["msg"].as_str()),
            (Some(400), Some(*msg)),
            "{method} {path} {body}: {res}"
        );
    }

    let guest: &[(&str, &str, Value, &str)] = &[
        (
            "POST",
            "/guest/orders",
            json!({"items": []}),
            "Email: 不能为空; OrderPassword: 不能为空",
        ),
        (
            "POST",
            "/guest/orders/preview",
            json!({}),
            "Email: 不能为空; OrderPassword: 不能为空; Items: 不能为空",
        ),
        (
            "POST",
            "/guest/payments",
            json!({"order_no": "DJ1"}),
            "ChannelID: 不能为空",
        ),
        (
            "POST",
            "/auth/register",
            json!({"agreement_accepted": true}),
            "Email: 不能为空; Password: 不能为空",
        ),
        (
            "POST",
            "/auth/send-verify-code",
            json!({"email": "a@b.co"}),
            "Purpose: 不能为空",
        ),
        (
            "POST",
            "/auth/login/verify-2fa",
            json!({"code": "1"}),
            "ChallengeToken: 不能为空",
        ),
        (
            "POST",
            "/auth/google/login",
            json!({}),
            "Credential: 不能为空",
        ),
    ];
    for (method, path, body, msg) in guest {
        let res = anon(&app, method, path, body.clone()).await;
        assert_eq!(
            (res["status_code"].as_i64(), res["msg"].as_str()),
            (Some(400), Some(*msg)),
            "{method} {path} {body}: {res}"
        );
    }

    // The original login endpoints answer with the plain bad-request message.
    for path in [
        "/auth/login",
        "/auth/telegram/login",
        "/auth/telegram/oidc/callback",
    ] {
        let res = anon(&app, "POST", path, json!({})).await;
        err(&res, 400, BAD);
    }
}

/// The upstream API echoes the validator error text after `invalid request body: `.
#[tokio::test]
async fn upstream_create_order_reports_validator_text() {
    let app = IntApp::new().await;
    let (_, _, _, key, secret) = app.buyer("up@example.com").await;
    let (status, body) = app
        .upstream(
            "POST",
            "/api/v1/upstream/orders",
            Some(json!({"quantity": -1})),
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(status, 400, "{body}");
    assert_eq!(body["error_code"], "bad_request");
    assert_eq!(
        body["error_message"],
        "invalid request body: Key: 'createOrderRequest.SKUID' Error:Field validation for 'SKUID' failed on the 'required' tag\n\
         Key: 'createOrderRequest.Quantity' Error:Field validation for 'Quantity' failed on the 'min' tag"
    );
}
