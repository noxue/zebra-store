//! Admin payment channels and payment records (compliance-gated).

mod payment_common;

use axum::body::Body;
use axum::http::{Request, header};
use chrono::Utc;
use payment_common::{PUB_PEM, PayApp, rsa_sha256};
use serde_json::{Value, json};
use zs_infra::payment::http::HttpResponse;

fn epay_body() -> Value {
    json!({
        "name": "易支付-支付宝", "provider_type": "epay", "channel_type": "alipay", "interaction_mode": "qr",
        "fee_rate": "1.50", "fixed_fee": 0.3, "payment_roles": ["member"], "payment_types": ["order"],
        "config_json": {"gateway_url": "https://pay.example.com", "merchant_id": "1001", "merchant_key": "secret-key",
            "notify_url": "https://shop/api/v1/payments/callback", "return_url": "https://shop/pay", "exchange_rate": "7.2"}
    })
}

fn huifu_body() -> Value {
    json!({
        "name": "汇付-支付宝",
        "provider_type": "huifu",
        "channel_type": "alipay",
        "interaction_mode": "redirect",
        "payment_roles": ["guest", "member"],
        "payment_types": ["order"],
        "config_json": {
            "api_base_url": "http://127.0.0.1:18766",
            "sys_id": "6666000108840829",
            "product_id": "YYZY",
            "huifu_id": "6666000100000001",
            "merchant_private_key": payment_common::PRIV_PEM,
            "huifu_public_key": PUB_PEM,
            "skill_source": "hfps/1.3.1;sandbox/1.0.0",
            "project_id": "ZEBRA-SANDBOX",
            "project_title": "Zebra Store",
            "notify_url": "http://127.0.0.1:8080/api/v1/payments/callback",
            "return_url": "http://127.0.0.1:8080/pay"
        }
    })
}

fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    &v["data"]
}

/// Channel CRUD: shapes, secret redaction and merge semantics (PAY-19).
#[tokio::test]
async fn channel_crud() {
    let app = PayApp::offline().await;
    let created = app
        .call("POST", "/api/v1/admin/payment-channels", Some(epay_body()))
        .await;
    let ch = data(&created).clone();
    let id = ch["id"].as_i64().unwrap();
    assert_eq!(ch["fee_rate"], "1.50");
    assert_eq!(ch["fixed_fee"], "0.30");
    assert_eq!(ch["min_amount"], "0.00");
    assert_eq!(ch["is_active"], true);
    assert_eq!(ch["payment_roles"], json!(["member"]));
    assert_eq!(ch["member_levels"], json!([]));
    assert_eq!(ch["config_json"]["merchant_key"], "••••••••");
    assert_eq!(ch["config_json"]["gateway_url"], "https://pay.example.com");
    assert!(ch["created_at"].is_string());

    let list = app
        .call(
            "GET",
            "/api/v1/admin/payment-channels?page=1&page_size=10&provider_type=epay",
            None,
        )
        .await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
    assert_eq!(list["pagination"]["total"], 1);
    let none = app
        .call(
            "GET",
            "/api/v1/admin/payment-channels?provider_type=stripe",
            None,
        )
        .await;
    assert_eq!(data(&none), &json!([]));

    // Update: redacted secret kept, cleared exchange rate removed, name kept when omitted.
    let upd = app
        .call(
            "PUT",
            &format!("/api/v1/admin/payment-channels/{id}"),
            Some(json!({"sort_order": 9, "config_json": {"gateway_url": "https://pay2.example.com", "merchant_id": "1001",
                "merchant_key": "••••••••", "notify_url": "https://shop/cb", "return_url": "https://shop/pay"}})),
        )
        .await;
    let upd = data(&upd).clone();
    assert_eq!(upd["sort_order"], 9);
    assert_eq!(upd["name"], "易支付-支付宝");
    assert!(upd["config_json"].get("exchange_rate").is_none());
    let row = app.services.payment.channels.get(id).await.unwrap();
    assert_eq!(row.config_json["merchant_key"], "••••••••");

    let got = app
        .call("GET", &format!("/api/v1/admin/payment-channels/{id}"), None)
        .await;
    assert_eq!(
        data(&got)["config_json"]["gateway_url"],
        "https://pay2.example.com"
    );

    let del = app
        .call(
            "DELETE",
            &format!("/api/v1/admin/payment-channels/{id}"),
            None,
        )
        .await;
    assert_eq!(data(&del), &json!({"deleted": true}));
    let gone = app
        .call("GET", &format!("/api/v1/admin/payment-channels/{id}"), None)
        .await;
    assert_eq!(gone["status_code"], 404);
    assert!(gone["msg"].is_string());
}

async fn expect_error(
    app: &PayApp,
    method: &str,
    uri: &str,
    body: Option<Value>,
    code: u16,
    key_msg: &str,
) {
    let res = app.call(method, uri, body).await;
    assert_eq!(res["status_code"], code, "{uri}: {res}");
    let expected = zs_api::i18n::translate("zh-CN", key_msg);
    assert_eq!(res["msg"], expected, "{uri}: {res}");
}

/// Validation errors keep the original keys (PAY-12, PAY-37).
#[tokio::test]
async fn channel_validation_errors() {
    let app = PayApp::offline().await;
    let uri = "/api/v1/admin/payment-channels";
    let mut missing_key = epay_body();
    missing_key["config_json"]["merchant_key"] = json!("");
    expect_error(
        &app,
        "POST",
        uri,
        Some(missing_key),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;
    let mut unsupported = epay_body();
    unsupported["channel_type"] = json!("paypal");
    expect_error(
        &app,
        "POST",
        uri,
        Some(unsupported),
        400,
        "error.payment_provider_not_supported",
    )
    .await;
    let mut unknown = epay_body();
    unknown["provider_type"] = json!("nope");
    expect_error(
        &app,
        "POST",
        uri,
        Some(unknown),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;
    let mut negative_fee = epay_body();
    negative_fee["fixed_fee"] = json!("-1");
    expect_error(
        &app,
        "POST",
        uri,
        Some(negative_fee),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;
    let mut big_fee = epay_body();
    big_fee["fixed_fee"] = json!("10000");
    expect_error(
        &app,
        "POST",
        uri,
        Some(big_fee),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;
    let mut bad_mode = epay_body();
    bad_mode["interaction_mode"] = json!("foo");
    expect_error(
        &app,
        "POST",
        uri,
        Some(bad_mode),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;
    let mut no_name = epay_body();
    no_name["name"] = json!("");
    // Original `RespondBindError` names the Go field.
    expect_error(&app, "POST", uri, Some(no_name), 400, "Name: 不能为空").await;
    expect_error(
        &app,
        "GET",
        "/api/v1/admin/payment-channels/abc",
        None,
        400,
        "error.payment_channel_invalid",
    )
    .await;
    expect_error(
        &app,
        "PUT",
        "/api/v1/admin/payment-channels/999",
        Some(json!({})),
        404,
        "error.payment_channel_not_found",
    )
    .await;
    // PAY-31: PayPal channels may be saved without webhook_id.
    let paypal = json!({"name": "PayPal", "provider_type": "official", "channel_type": "paypal", "interaction_mode": "redirect",
        "config_json": {"client_id": "c", "client_secret": "s", "base_url": "https://api-m.paypal.com",
        "return_url": "https://shop/pay", "cancel_url": "https://shop/cancel"}});
    assert_eq!(app.call("POST", uri, Some(paypal)).await["status_code"], 0);
}

/// Huifu is a first-class payment provider: valid hosted Alipay config can be stored, private
/// keys are never echoed, and unsupported channels or interaction modes are rejected.
#[tokio::test]
async fn huifu_channel_validation_and_secret_redaction() {
    let app = PayApp::offline().await;
    let uri = "/api/v1/admin/payment-channels";
    let created = app.call("POST", uri, Some(huifu_body())).await;
    let channel = data(&created);
    assert_eq!(channel["provider_type"], "huifu");
    assert_eq!(channel["channel_type"], "alipay");
    assert_eq!(channel["interaction_mode"], "redirect");
    assert_eq!(channel["config_json"]["merchant_private_key"], "••••••••");
    assert_eq!(channel["config_json"]["huifu_public_key"], PUB_PEM);

    let mut unsupported_channel = huifu_body();
    unsupported_channel["channel_type"] = json!("paypal");
    expect_error(
        &app,
        "POST",
        uri,
        Some(unsupported_channel),
        400,
        "error.payment_provider_not_supported",
    )
    .await;

    let mut unsupported_mode = huifu_body();
    unsupported_mode["interaction_mode"] = json!("qr");
    expect_error(
        &app,
        "POST",
        uri,
        Some(unsupported_mode),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;

    let mut missing_project = huifu_body();
    missing_project["config_json"]["project_id"] = json!("");
    expect_error(
        &app,
        "POST",
        uri,
        Some(missing_project),
        400,
        "error.payment_channel_config_invalid",
    )
    .await;
}

/// Huifu trade bills are queried through a signed API response and downloaded through the
/// server-side proxy, so the short-lived provider URL is never exposed to the browser.
#[tokio::test]
async fn huifu_trade_bill_query_and_download() {
    let app = PayApp::new(|request| {
        if request.method == "GET" {
            assert_eq!(request.url, "http://127.0.0.1:18766/bills/trade.csv");
            return Ok(HttpResponse::new(
                200,
                b"huifu_id,file_date,bill_type\n6666000100000001,20260929,TRADE_BILL\n".to_vec(),
            ));
        }
        assert!(request.url.ends_with("/v2/trade/check/filequery"));
        let data = json!({
            "resp_code": "00000000",
            "resp_desc": "success",
            "file_details": [{
                "huifu_id": "6666000100000001",
                "file_date": "20260929",
                "file_id": "FILE-1",
                "file_name": "trade-20260929.csv",
                "bill_type": "TRADE_BILL",
                "download_url": "http://127.0.0.1:18766/bills/trade.csv"
            }],
            "task_details": [{
                "huifu_id": "6666000100000001",
                "data_date": "20260929",
                "file_id": "FILE-1",
                "file_name": "trade-20260929.csv",
                "bill_type": "TRADE_BILL",
                "task_stat": "S",
                "task_start_time": "2026-09-30 12:00:00",
                "task_end_time": "2026-09-30 12:00:01"
            }]
        });
        let sign = huifu_pay::sign_value(payment_common::PRIV_PEM, &data).unwrap();
        Ok(HttpResponse::new(
            200,
            json!({"data": data, "sign": sign}).to_string(),
        ))
    })
    .await;
    let channel = app
        .seed_channel(
            "huifu",
            "alipay",
            "redirect",
            huifu_body()["config_json"].clone(),
        )
        .await;

    let queried = app
        .call(
            "GET",
            &format!("/api/v1/admin/payment-channels/{channel}/trade-bill?file_date=20260929"),
            None,
        )
        .await;
    let result = data(&queried);
    assert_eq!(result["files"][0]["file_id"], "FILE-1");
    assert_eq!(result["files"][0]["file_name"], "trade-20260929.csv");
    assert_eq!(result["tasks"][0]["task_stat"], "S");
    assert!(
        result["files"][0].get("download_url").is_none(),
        "the provider URL must stay server-side"
    );

    let request = Request::builder()
        .uri(format!(
            "/api/v1/admin/payment-channels/{channel}/trade-bill/download?file_date=20260929&file_id=FILE-1"
        ))
        .header(header::AUTHORIZATION, format!("Bearer {}", app.token))
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = app.send(request).await;
    assert_eq!(status, 200);
    assert_eq!(headers[header::CONTENT_TYPE], "application/octet-stream");
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .contains("trade-20260929.csv")
    );
    assert!(body.starts_with(b"huifu_id,file_date,bill_type"));
}

/// Compliance gate and authentication on the finance routes.
#[tokio::test]
async fn compliance_and_auth_gates() {
    let app = PayApp::without_ack(|_| Err("offline".into())).await;
    let res = app
        .call("GET", "/api/v1/admin/payment-channels", None)
        .await;
    assert_eq!(res["status_code"], 403, "{res}");
    let res = app.call("GET", "/api/v1/admin/payments", None).await;
    assert_eq!(res["status_code"], 403, "{res}");
    let anon = app
        .call_as("GET", "/api/v1/admin/payments", None, None)
        .await;
    assert_eq!(anon["status_code"], 401, "{anon}");
}

/// WeChat Pay public-key test: unsupported providers and a successful signed echo.
#[tokio::test]
async fn wechatpay_public_key_test() {
    let app = PayApp::new(|req| {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let resp_body = json!({"echo_message": body["echo_message"]}).to_string();
        let ts = Utc::now().timestamp().to_string();
        let sig = rsa_sha256(&format!("{ts}\nNONCE\n{resp_body}\n"));
        Ok(HttpResponse::new(200, resp_body)
            .header("Wechatpay-Serial", "PUB_KEY_ID_0114")
            .header("Wechatpay-Signature", sig)
            .header("Wechatpay-Timestamp", ts)
            .header("Wechatpay-Nonce", "NONCE"))
    })
    .await;
    let epay = app
        .call("POST", "/api/v1/admin/payment-channels", Some(epay_body()))
        .await;
    let epay_id = data(&epay)["id"].as_i64().unwrap();
    expect_error(
        &app,
        "POST",
        &format!("/api/v1/admin/payment-channels/{epay_id}/wechatpay-public-key-test"),
        None,
        400,
        "error.wechatpay_key_test_unsupported",
    )
    .await;
    let wechat = app
        .seed_channel("official", "wechat", "qr", json!({"appid": "wx1", "mchid": "m1", "merchant_serial_no": "MSN1",
            "merchant_private_key": payment_common::PRIV_PEM, "api_v3_key": "0123456789abcdef0123456789abcdef",
            "verification_mode": "wechatpay_public_key", "wechatpay_public_key_id": "PUB_KEY_ID_0114",
            "wechatpay_public_key": PUB_PEM, "notify_url": "https://shop/api/v1/payments/callback"}))
        .await;
    let res = app
        .call(
            "POST",
            &format!("/api/v1/admin/payment-channels/{wechat}/wechatpay-public-key-test"),
            None,
        )
        .await;
    assert_eq!(
        data(&res),
        &json!({"verification_mode": "wechatpay_public_key", "response_serial": "PUB_KEY_ID_0114",
            "request_signature_accepted": true, "response_signature_valid": true, "echo_message_matched": true})
    );
}

/// PAY-45 / PAY-44: admin payment list/detail/export shapes.
#[tokio::test]
async fn payment_records() {
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel("epay", "alipay", "qr", json!({"merchant_key": "k"}))
        .await;
    let order = app.seed_order("DJ2001", "9.90").await;
    let p1 = app
        .seed_payment(order, channel, "DJP-A-1", "9.90", "CNY")
        .await;
    let p2 = app.seed_payment(0, channel, "DJP-A-2", "5.00", "CNY").await;
    app.seed_recharge(p2, "WR2001", 42).await;

    let list = app
        .call("GET", "/api/v1/admin/payments?page=1&page_size=20", None)
        .await;
    let items = data(&list).as_array().unwrap().clone();
    assert_eq!(list["pagination"]["total"], 2);
    assert_eq!(items[0]["id"], p2);
    assert_eq!(items[0]["recharge_no"], "WR2001");
    assert_eq!(items[0]["recharge_user_id"], 42);
    let first = items.iter().find(|i| i["id"] == p1).unwrap();
    assert_eq!(first["order_no"], "DJ2001");
    assert_eq!(first["channel_name"], "epay-alipay");
    assert_eq!(first["display_channel_type"], "demo.type");
    assert_eq!(first["provider_payload"], json!({}));
    assert_eq!(first["pay_url"], "");
    assert_eq!(first["amount"], "9.90");
    assert_eq!(first["status"], "pending");
    assert!(first.get("recharge_no").is_none());

    let by_user = app
        .call("GET", "/api/v1/admin/payments?user_id=42", None)
        .await;
    assert_eq!(data(&by_user).as_array().unwrap().len(), 1);
    let filtered = app
        .call(
            "GET",
            &format!("/api/v1/admin/payments?order_id={order}&status=pending"),
            None,
        )
        .await;
    assert_eq!(data(&filtered).as_array().unwrap().len(), 1);
    let bad = app
        .call("GET", "/api/v1/admin/payments?order_id=0", None)
        .await;
    assert_eq!(bad["status_code"], 400);

    let one = app
        .call("GET", &format!("/api/v1/admin/payments/{p1}"), None)
        .await;
    assert_eq!(data(&one)["gateway_order_no"], "DJP-A-1");
    let missing = app.call("GET", "/api/v1/admin/payments/99999", None).await;
    assert_eq!(missing["status_code"], 404);
    let invalid = app.call("GET", "/api/v1/admin/payments/x", None).await;
    assert_eq!(invalid["status_code"], 400);

    let req = Request::builder()
        .uri("/api/v1/admin/payments/export?status=pending")
        .header(header::AUTHORIZATION, format!("Bearer {}", app.token))
        .body(Body::empty())
        .unwrap();
    let (status, headers, body) = app.send(req).await;
    assert_eq!(status, 200);
    assert_eq!(headers[header::CONTENT_TYPE], "text/csv; charset=utf-8");
    assert!(
        headers[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"payments_")
    );
    let csv = String::from_utf8(body).unwrap();
    let mut lines = csv.lines();
    assert_eq!(
        lines.next(),
        Some(
            "id,order_id,recharge_no,recharge_status,recharge_user_id,channel_id,provider_type,channel_type,display_channel_type,status,amount,currency,created_at,paid_at,expired_at,provider_ref"
        )
    );
    let row = lines.find(|l| l.starts_with(&format!("{p1},"))).unwrap();
    assert!(
        row.contains(",epay,alipay,demo.type,pending,9.90,CNY,"),
        "{row}"
    );
}

/// E2E finding: `/public/config` embeds the storefront payment channels and is cached for
/// 60 s; creating, editing or deleting a channel must refresh it immediately, otherwise a
/// freshly configured channel is missing from guest checkout.
#[tokio::test]
async fn channel_changes_refresh_public_config() {
    let app = PayApp::offline().await;
    let names = |v: &Value| -> Vec<String> {
        data(v)["payment_channels"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["name"].as_str().unwrap_or_default().to_owned())
            .collect()
    };
    // warm the cache while no channel exists
    let before = app
        .call_as("GET", "/api/v1/public/config", None, None)
        .await;
    assert!(names(&before).is_empty());

    let mut body = epay_body();
    body["payment_roles"] = json!([]);
    let created = app
        .call("POST", "/api/v1/admin/payment-channels", Some(body))
        .await;
    let id = data(&created)["id"].as_i64().unwrap();
    let after = app
        .call_as("GET", "/api/v1/public/config", None, None)
        .await;
    assert_eq!(names(&after), vec!["易支付-支付宝".to_owned()]);

    let renamed = app
        .call(
            "PUT",
            &format!("/api/v1/admin/payment-channels/{id}"),
            Some(json!({"name": "E2E epay"})),
        )
        .await;
    data(&renamed);
    let after = app
        .call_as("GET", "/api/v1/public/config", None, None)
        .await;
    assert_eq!(names(&after), vec!["E2E epay".to_owned()]);

    let deleted = app
        .call(
            "DELETE",
            &format!("/api/v1/admin/payment-channels/{id}"),
            None,
        )
        .await;
    assert_eq!(deleted["status_code"], 0);
    let after = app
        .call_as("GET", "/api/v1/public/config", None, None)
        .await;
    assert!(names(&after).is_empty());
}
