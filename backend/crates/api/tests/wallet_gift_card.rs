//! `POST /gift-cards/redeem`: one transaction (lock card → credit wallet → mark redeemed),
//! captcha scene `gift_card_redeem` and the per user+IP rate limit (RISK-02).

mod wallet_common;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use serde_json::json;
use wallet_common::{App, data, err, keys};

const REDEEM: &str = "/api/v1/gift-cards/redeem";

#[tokio::test]
async fn redeem_credits_wallet_once() {
    let app = App::new().await;
    let (uid, token) = app.user("gift@example.com").await;
    let card = app.gift_card("GCABC", "25.50", "active", None).await;

    let res = app
        .call(
            "POST",
            REDEEM,
            Some(json!({"code": " gcabc "})),
            Some(&token),
        )
        .await;
    let d = data(&res);
    assert_eq!(
        keys(d),
        ["gift_card", "transaction", "wallet", "wallet_delta"]
    );
    assert_eq!(
        keys(&d["gift_card"]),
        [
            "amount",
            "code",
            "currency",
            "id",
            "name",
            "redeemed_at",
            "status"
        ]
    );
    assert_eq!(d["gift_card"]["status"], "redeemed");
    assert_eq!(d["gift_card"]["amount"], "25.50");
    assert_eq!(d["wallet"], json!({"balance": "25.50"}));
    assert_eq!(d["wallet_delta"], "25.50");
    assert_eq!(d["transaction"]["type"], "gift_card_redeem");
    assert_eq!(d["transaction"]["remark"], "礼品卡兑换：GCABC");
    let row = app.gift_card_row(card).await;
    assert_eq!(row.status, "redeemed");
    assert_eq!(row.redeemed_user_id, Some(uid));
    assert_eq!(row.wallet_txn_id, d["transaction"]["id"].as_i64());

    // The same card cannot be redeemed again.
    let res = app
        .call("POST", REDEEM, Some(json!({"code": "GCABC"})), Some(&token))
        .await;
    err(&res, 400, "Gift card has been redeemed");
    assert_eq!(app.balance(uid).await, "25.50");
}

#[tokio::test]
async fn redeem_errors() {
    let app = App::new().await;
    let (uid, token) = app.user("gift2@example.com").await;
    app.gift_card("GCOFF", "5", "disabled", None).await;
    app.gift_card("GCOLD", "5", "active", Some(Utc::now() - Duration::days(1)))
        .await;
    err(
        &app.call("POST", REDEEM, Some(json!({"code": "NOPE"})), Some(&token))
            .await,
        404,
        "Gift card not found",
    );
    err(
        &app.call("POST", REDEEM, Some(json!({"code": "GCOFF"})), Some(&token))
            .await,
        400,
        "Gift card is disabled",
    );
    err(
        &app.call("POST", REDEEM, Some(json!({"code": "GCOLD"})), Some(&token))
            .await,
        400,
        "Gift card expired",
    );
    err(
        &app.call("POST", REDEEM, Some(json!({"code": ""})), Some(&token))
            .await,
        400,
        "Code: is required",
    );
    assert_eq!(
        app.call("POST", REDEEM, Some(json!({"code": "GCOFF"})), None)
            .await["status_code"],
        401
    );
    assert_eq!(app.balance(uid).await, "0.00");
}

/// Concurrent redemptions of one card by two users: exactly one wins (conditional update).
#[tokio::test]
async fn double_redeem_is_impossible_under_concurrency() {
    let app = App::new().await;
    let (a, token_a) = app.user("a@example.com").await;
    let (b, token_b) = app.user("b@example.com").await;
    app.gift_card("GCRACE", "40", "active", None).await;
    let (ra, rb) = tokio::join!(
        app.call(
            "POST",
            REDEEM,
            Some(json!({"code": "GCRACE"})),
            Some(&token_a)
        ),
        app.call(
            "POST",
            REDEEM,
            Some(json!({"code": "GCRACE"})),
            Some(&token_b)
        ),
    );
    let wins = [&ra, &rb].iter().filter(|r| r["status_code"] == 0).count();
    assert_eq!(wins, 1, "{ra} / {rb}");
    let total = format!("{}+{}", app.balance(a).await, app.balance(b).await);
    assert!(total == "40.00+0.00" || total == "0.00+40.00", "{total}");
}

/// RISK-02: 10 redemptions per minute per user+IP, then 429 for 300 s.
#[tokio::test]
async fn risk_02_redeem_rate_limit() {
    let app = App::new().await;
    let (_, token) = app.user("spam@example.com").await;
    for _ in 0..10 {
        let (status, res) = app
            .raw("POST", REDEEM, Some(json!({"code": "GUESS"})), Some(&token))
            .await;
        assert_eq!(status, StatusCode::OK);
        err(&res, 404, "Gift card not found");
    }
    let (status, res) = app
        .raw("POST", REDEEM, Some(json!({"code": "GUESS"})), Some(&token))
        .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(res["status_code"], 429);
    assert!(
        res["msg"]
            .as_str()
            .unwrap()
            .starts_with("Too many requests, retry in")
    );
    // Another user is not affected.
    let (_, other) = app.user("calm@example.com").await;
    let (status, _) = app
        .raw("POST", REDEEM, Some(json!({"code": "GUESS"})), Some(&other))
        .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn captcha_scene_is_enforced() {
    let app = App::new().await;
    app.set_setting(
        "captcha_config",
        json!({"provider": "image", "scenes": {"gift_card_redeem": true}}),
    )
    .await;
    let (uid, token) = app.user("cap@example.com").await;
    app.gift_card("GCCAP", "5", "active", None).await;
    let res = app
        .call("POST", REDEEM, Some(json!({"code": "GCCAP"})), Some(&token))
        .await;
    err(&res, 400, "Please complete captcha verification");
    assert_eq!(app.balance(uid).await, "0.00");
}
