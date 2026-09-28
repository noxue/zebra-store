//! Channel (Telegram bot) order and payment API: preview, create (legacy single line),
//! list / detail / by number, wallet payment, latest payment and cancel.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod order_common;

use axum::body::Body;
use axum::http::{Request, header};
use order_common::OrderApp;
use serde_json::{Value, json};

struct Bot {
    key: String,
    secret: String,
}

async fn bot(app: &OrderApp) -> Bot {
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/channel-clients",
            Some(json!({"name": "Bot", "channel_type": "telegram_bot", "bot_token": "", "callback_url": ""})),
        )
        .await;
    let d = &res["data"];
    Bot {
        key: d["channel_key"]
            .as_str()
            .unwrap_or_else(|| panic!("{res}"))
            .to_owned(),
        secret: d["channel_secret"].as_str().unwrap().to_owned(),
    }
}

/// Signed channel request; returns `(http status, body)`.
async fn call(
    app: &OrderApp,
    bot: &Bot,
    method: &str,
    path_and_query: &str,
    body: Option<Value>,
) -> (u16, Value) {
    let uri = format!("/api/v1/channel{path_and_query}");
    let path = uri.split('?').next().unwrap().to_owned();
    let raw = body.as_ref().map(ToString::to_string).unwrap_or_default();
    let ts = chrono::Utc::now().timestamp();
    let signature = zs_shared::sign::sign(&bot.secret, method, &path, ts, raw.as_bytes());
    let req = Request::builder()
        .method(method)
        .uri(&uri)
        .header("Dujiao-Next-Channel-Key", &bot.key)
        .header("Dujiao-Next-Channel-Timestamp", ts.to_string())
        .header("Dujiao-Next-Channel-Signature", signature)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(raw))
        .unwrap();
    let (status, _, bytes) = app.send(req).await;
    (
        status.as_u16(),
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn channel_order_flow() {
    let app = OrderApp::new().await;
    let bot = bot(&app).await;
    let (product, sku) = app.product("tg", json!({"price_amount": "6.00"})).await;
    app.secrets(product, sku, 3).await;

    // preview
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/orders/preview",
        Some(json!({"channel_user_id": "777", "items": [{"product_id": product, "sku_id": sku, "quantity": 2}]})),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["total_amount"], "12.00");
    assert_eq!(b["data"]["items"][0]["subtotal"], "12.00");
    assert_eq!(b["data"]["valid"], true);

    // create with the legacy single-line fields
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/orders",
        Some(
            json!({"channel_user_id": "777", "product_id": product, "sku_id": sku, "quantity": 1}),
        ),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let order_id = b["data"]["order_id"].as_i64().unwrap();
    let order_no = b["data"]["order_no"].as_str().unwrap().to_owned();
    assert_eq!(b["data"]["status"], "pending_payment");
    assert_eq!(b["data"]["fulfillment_type"], "auto");

    // validation / stock errors carry channel error codes
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/orders",
        Some(json!({"channel_user_id": "777"})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("validation_error")),
        "{b}"
    );
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/orders",
        Some(json!({"channel_user_id": "777", "items": [{"product_id": product, "sku_id": sku, "quantity": 9}]})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("sku_out_of_stock")),
        "{b}"
    );

    // list / detail / by number
    let (s, b) = call(&app, &bot, "GET", "/orders?channel_user_id=777", None).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["total"], 1);
    assert_eq!(b["data"]["page_size"], 5);
    let (s, b) = call(
        &app,
        &bot,
        "GET",
        &format!("/orders/{order_id}?channel_user_id=777"),
        None,
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["order_no"], order_no.as_str());
    assert_eq!(b["data"]["children"].as_array().unwrap().len(), 1);
    let (s, _) = call(
        &app,
        &bot,
        "GET",
        &format!("/orders/by-order-no/{order_no}?channel_user_id=777"),
        None,
    )
    .await;
    assert_eq!(s, 200);
    // another channel user cannot see it
    let (s, b) = call(
        &app,
        &bot,
        "GET",
        &format!("/orders/{order_id}?channel_user_id=888"),
        None,
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (404, Some("order_not_found")),
        "{b}"
    );

    // wallet payment of the provisioned user
    let user_id = app.order(order_id).await.user_id;
    app.fund(user_id, "10.00").await;
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/payments",
        Some(json!({"channel_user_id": "777", "order_id": order_id, "channel_id": 0, "use_balance": true})),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["order_paid"], true);
    assert_eq!(app.balance(user_id).await, "4.00");
    // a paid order has no latest payment and cannot be canceled
    let (s, b) = call(
        &app,
        &bot,
        "GET",
        &format!("/payments/latest?channel_user_id=777&order_id={order_id}"),
        None,
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("order_status_invalid")),
        "{b}"
    );
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        &format!("/orders/{order_id}/cancel"),
        Some(json!({"channel_user_id": "777"})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("order_status_invalid")),
        "{b}"
    );

    // a second order is canceled through the channel
    let (_, b) = call(
        &app,
        &bot,
        "POST",
        "/orders",
        Some(json!({"channel_user_id": "777", "items": [{"product_id": product, "sku_id": sku, "quantity": 1}]})),
    )
    .await;
    let second = b["data"]["order_id"].as_i64().unwrap();
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        &format!("/orders/{second}/cancel"),
        Some(json!({"channel_user_id": "777"})),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["status"], "canceled");
}

/// Channel wallet, gift card and affiliate endpoints.
#[tokio::test]
async fn channel_wallet_and_affiliate() {
    let app = OrderApp::new().await;
    let bot = bot(&app).await;

    // wallet of a freshly provisioned user
    let (s, b) = call(&app, &bot, "GET", "/wallet?channel_user_id=555", None).await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["balance"], "0.00");
    let (s, b) = call(&app, &bot, "GET", "/wallet", None).await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("validation_error")),
        "{b}"
    );

    // gift card redemption credits the wallet once
    let now = chrono::Utc::now();
    use sea_orm::{ActiveModelTrait, Set};
    zs_infra::db::entity::gift_cards::ActiveModel {
        batch_id: Set(None),
        name: Set("card".into()),
        code: Set("GIFT-1".into()),
        amount: Set("8.00".parse().unwrap()),
        currency: Set("CNY".into()),
        status: Set("active".into()),
        expires_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let body = json!({"channel_user_id": "555", "code": "GIFT-1"});
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/wallet/gift-card/redeem",
        Some(body.clone()),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["wallet"]["balance"], "8.00");
    let (s, b) = call(&app, &bot, "POST", "/wallet/gift-card/redeem", Some(body)).await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("gift_card_redeemed")),
        "{b}"
    );
    let (s, b) = call(
        &app,
        &bot,
        "GET",
        "/wallet/transactions?channel_user_id=555",
        None,
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["total"], 1);
    assert_eq!(b["data"]["items"][0]["amount"], "8.00");

    // recharge through an epay channel
    let channel = app.epay_channel().await;
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/wallet/recharge",
        Some(json!({"channel_user_id": "555", "amount": "5", "channel_id": channel})),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert!(b["data"]["recharge_no"].as_str().unwrap().starts_with("WR"));
    assert!(
        b["data"]["payment"]["pay_url"]
            .as_str()
            .unwrap()
            .starts_with("https://pay.example.com")
    );

    // affiliate: disabled program, then enabled
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/affiliate/open",
        Some(json!({"channel_user_id": "555"})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("affiliate_disabled")),
        "{b}"
    );
    app.set_setting(
        "affiliate_config",
        json!({"enabled": true, "commission_rate": 10, "confirm_days": 7, "min_withdraw_amount": 1, "withdraw_channels": ["alipay"]}),
    )
    .await;
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/affiliate/withdraws",
        Some(json!({"channel_user_id": "555", "amount": "5", "channel": "alipay", "account": "a@b.c"})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("affiliate_not_opened")),
        "{b}"
    );
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/affiliate/open",
        Some(json!({"channel_user_id": "555"})),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let code = b["data"]["code"].as_str().unwrap().to_owned();
    let (s, b) = call(
        &app,
        &bot,
        "GET",
        "/affiliate/dashboard?channel_user_id=555",
        None,
    )
    .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["data"]["opened"], true);
    assert_eq!(b["data"]["withdraw_channels"], json!(["alipay"]));
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/affiliate/click",
        Some(json!({"channel_user_id": "556", "affiliate_code": code})),
    )
    .await;
    assert_eq!(s, 200, "{b}");
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/affiliate/withdraws",
        Some(json!({"channel_user_id": "555", "amount": "5", "channel": "bank", "account": "x"})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("affiliate_withdraw_channel_invalid")),
        "{b}"
    );
    let (s, b) = call(
        &app,
        &bot,
        "POST",
        "/affiliate/withdraws",
        Some(json!({"channel_user_id": "555", "amount": "5", "channel": "alipay", "account": "x"})),
    )
    .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("affiliate_withdraw_insufficient")),
        "{b}"
    );
    for path in ["/affiliate/commissions", "/affiliate/withdraws"] {
        let (s, b) = call(
            &app,
            &bot,
            "GET",
            &format!("{path}?channel_user_id=555"),
            None,
        )
        .await;
        assert_eq!(s, 200, "{b}");
        assert_eq!(b["data"]["total"], 0);
    }
}
