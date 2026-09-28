//! Regression tests for checklist items of `bugfix-lessons.md` §21 on the channel API that
//! had no coverage: RISK-05; plus the channel API bind-validation messages.

mod common;
mod notify_common;

use common::TestApp;
use notify_common::{channel, channel_client, channel_raw, signed_headers};
use serde_json::json;

/// RISK-05 ①: the signed channel API reads at most 10 MiB of body; an 11 MB body that is
/// even correctly signed is refused as a validation error before any processing.
#[tokio::test]
async fn risk_05_channel_api_body_is_capped() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "", "").await;
    let path = "/api/v1/channel/telegram/heartbeat";
    let big = json!({"pad": "x".repeat(11 << 20)});
    let raw = big.to_string();
    let headers = signed_headers(&creds.1, &creds.2, "POST", path, raw.as_bytes(), 0);
    let res = channel_raw(&app, "POST", "/telegram/heartbeat", Some(big), &headers).await;
    assert_ne!(res.status, 200, "{:?}", res.status);
    assert_ne!(res.body["status_code"], 0);
    assert_ne!(
        res.body["error_code"], "channel_client_unauthorized",
        "rejected for its size, not its signature"
    );
    // (②) the refusal carries no internal details
    let text = res.body.to_string().to_lowercase();
    assert!(!text.contains("sql") && !text.contains("panick"), "{text}");
}

/// Channel API bodies bound with `binding:"required"` answer with the Go field list
/// (`respondChannelBindError` / `channelresponse.BindError`), HTTP 400 `validation_error`.
#[tokio::test]
async fn channel_bind_errors_name_go_fields() {
    let app = TestApp::new().await;
    let creds = channel_client(&app, "", "").await;
    let cases = [
        (
            "POST",
            "/payments",
            json!({"channel_user_id": "1"}),
            "OrderID: 不能为空",
        ),
        (
            "POST",
            "/wallet/recharge",
            json!({}),
            "Amount: 不能为空; ChannelID: 不能为空",
        ),
        (
            "POST",
            "/wallet/gift-card/redeem",
            json!({"code": ""}),
            "Code: 不能为空",
        ),
        (
            "POST",
            "/affiliate/click",
            json!({}),
            "AffiliateCode: 不能为空",
        ),
        (
            "POST",
            "/affiliate/withdraws",
            json!({"amount": "1"}),
            "Channel: 不能为空; Account: 不能为空",
        ),
    ];
    for (method, path, body, msg) in cases {
        let res = channel(&app, &creds, method, path, Some(body.clone())).await;
        assert_eq!(res.status, 400, "{path} {res:?}");
        assert_eq!(res.body["msg"], msg, "{path} {res:?}");
        assert_eq!(res.body["error_code"], "validation_error", "{path} {res:?}");
    }
    // query binding of `GET /payments/latest`: absent / zero → required, garbage → bad request
    for (query, msg) in [
        ("", "OrderID: 不能为空"),
        ("?order_id=0", "OrderID: 不能为空"),
        ("?order_id=abc", "请求参数错误"),
    ] {
        let res = channel(
            &app,
            &creds,
            "GET",
            &format!("/payments/latest{query}"),
            None,
        )
        .await;
        assert_eq!(
            (res.status, res.body["msg"].as_str()),
            (400, Some(msg)),
            "{res:?}"
        );
    }
}
