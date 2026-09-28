//! Payment callbacks and webhooks through the real router (signatures computed with the same
//! algorithms the gateways use; vectors of those algorithms are pinned against Go in `zs-infra`).

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod payment_common;

use std::collections::BTreeMap;

use axum::http::StatusCode;
use chrono::Utc;
use payment_common::{PUB_PEM, PayApp, rsa_sha256};
use serde_json::{Value, json};
use zs_domain::payment::form::encode_pairs;
use zs_domain::settings::SettingsStore;
use zs_infra::payment::epusdt::SignValue;
use zs_infra::payment::http::HttpResponse;
use zs_infra::payment::{alipay, bepusdt, epay, epusdt, okpay, stripe, tokenpay};

const EPAY_KEY: &str = "epay-secret-key";

async fn epay_setup(app: &PayApp) -> (i64, i64, i64) {
    let channel = app
        .seed_channel(
            "epay",
            "alipay",
            "qr",
            json!({"gateway_url": "https://pay.example.com", "merchant_id": "1001", "merchant_key": EPAY_KEY,
                "notify_url": "https://shop/api/v1/payments/callback", "return_url": "https://shop/pay"}),
        )
        .await;
    let order = app.seed_order("DJ1001", "9.90").await;
    let payment = app
        .seed_payment(order, channel, "DJP20260924000001", "9.90", "CNY")
        .await;
    (channel, order, payment)
}

fn epay_query(pid: &str, money: &str, key: &str) -> String {
    epay_query_for("DJP20260924000001", pid, money, key)
}

fn epay_query_for(out_trade_no: &str, pid: &str, money: &str, key: &str) -> String {
    let mut p: BTreeMap<String, String> = [
        ("pid", pid),
        ("trade_no", "EP-T1"),
        ("out_trade_no", out_trade_no),
        ("type", "alipay"),
        ("name", "DJ1001"),
        ("money", money),
        ("trade_status", "TRADE_SUCCESS"),
        ("sign_type", "MD5"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let sign = epay::sign_md5(&epay::sign_content(&p), key);
    p.insert("sign".into(), sign);
    encode_pairs(&p.into_iter().collect::<Vec<_>>())
}

/// PAY-14: an unsigned generic payload is not a callback; nothing changes.
#[tokio::test]
async fn pay_14_unsigned_callback_rejected() {
    let app = PayApp::offline().await;
    let (_, _, payment) = epay_setup(&app).await;
    let (status, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            r#"{"payment_id":1,"status":"success","amount":"9.90","currency":"CNY"}"#,
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, "");
    assert_eq!(app.payment(payment).await.status, "pending");
}

/// PAY-04 / PAY-35 / PAY-01: signed epay callback settles once; replays are idempotent;
/// other pending payments of the order expire.
#[tokio::test]
async fn pay_04_epay_callback_settles_once() {
    let app = PayApp::offline().await;
    let (channel, order, payment) = epay_setup(&app).await;
    let older = app
        .seed_payment(order, channel, "DJP20260924000000", "9.90", "CNY")
        .await;
    let uri = format!(
        "/api/v1/payments/callback?{}",
        epay_query("1001", "9.90", EPAY_KEY)
    );
    let (status, ct, body) = app.callback("GET", &uri, "", &[], Vec::new()).await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "success"));
    assert!(ct.starts_with("text/plain"));
    let row = app.payment(payment).await;
    assert_eq!(row.status, "success");
    assert!(row.paid_at.is_some() && row.callback_at.is_some());
    assert_eq!(row.provider_ref, "EP-T1");
    // PAY-44: payload merged, creation snapshot kept.
    let payload = row.provider_payload.unwrap();
    assert_eq!(payload["display_channel_type"], "demo.type");
    assert_eq!(payload["trade_status"], "TRADE_SUCCESS");
    let other = app.payment(older).await;
    assert_eq!(other.status, "expired");
    let paid_at = row.paid_at;
    let (_, _, again) = app.callback("GET", &uri, "", &[], Vec::new()).await;
    assert_eq!(again, "success");
    assert_eq!(app.payment(payment).await.paid_at, paid_at);
}

/// PAY-04 / PAY-35 / PAY-01: amount mismatch, foreign pid and forged signatures are refused.
#[tokio::test]
async fn pay_35_epay_rejections() {
    let app = PayApp::offline().await;
    let (_, _, payment) = epay_setup(&app).await;
    for query in [
        epay_query("1001", "9.00", EPAY_KEY),
        epay_query("1002", "9.90", EPAY_KEY),
        epay_query("1001", "9.90", "wrong-key"),
    ] {
        let uri = format!("/api/v1/payments/callback?{query}");
        let (status, _, body) = app.callback("GET", &uri, "", &[], Vec::new()).await;
        assert_eq!((status, body.as_str()), (StatusCode::OK, "fail"), "{query}");
    }
    assert_eq!(app.payment(payment).await.status, "pending");
}

/// PAY-18: `;`-separated and `&amp;`-escaped queries.
#[tokio::test]
async fn pay_18_semicolon_query() {
    let app = PayApp::offline().await;
    let (_, _, payment) = epay_setup(&app).await;
    let query = epay_query("1001", "9.90", EPAY_KEY).replace('&', ";");
    let uri = format!("/api/v1/payments/callback?{query}");
    let (_, _, body) = app.callback("GET", &uri, "", &[], Vec::new()).await;
    assert_eq!(body, "success");
    assert_eq!(app.payment(payment).await.status, "success");
}

fn alipay_form(app_id: &str, amount: &str) -> String {
    let mut pairs: Vec<(String, String)> = [
        ("app_id", app_id),
        ("out_trade_no", "DJP-ALI-1"),
        ("trade_no", "2026092422001"),
        ("trade_status", "TRADE_SUCCESS"),
        ("total_amount", amount),
        ("gmt_payment", "2026-09-24 18:01:02"),
        ("notify_id", "n1"),
        ("subject", "DJ2"),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    let sign = alipay::sign(
        &alipay::sign_content(&pairs),
        payment_common::PRIV_PEM,
        "RSA2",
    )
    .unwrap();
    pairs.push(("sign_type".into(), "RSA2".into()));
    pairs.push(("sign".into(), sign));
    encode_pairs(&pairs)
}

/// PAY-34: Alipay form notifications; foreign app_id refused.
#[tokio::test]
async fn pay_34_alipay_notification() {
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel(
            "official",
            "alipay",
            "qr",
            json!({"app_id": "2021000000000001", "private_key": payment_common::PRIV_PEM, "alipay_public_key": PUB_PEM,
                "notify_url": "https://shop/api/v1/payments/callback"}),
        )
        .await;
    let order = app.seed_order("DJ2", "72.43").await;
    let payment = app
        .seed_payment(order, channel, "DJP-ALI-1", "72.43", "CNY")
        .await;
    let form_ct = "application/x-www-form-urlencoded";
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            form_ct,
            &[],
            alipay_form("2026999999999999", "72.43"),
        )
        .await;
    assert_eq!(body, "fail");
    assert_eq!(app.payment(payment).await.status, "pending");
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            form_ct,
            &[],
            alipay_form("2021000000000001", "72.43"),
        )
        .await;
    assert_eq!(body, "success");
    let row = app.payment(payment).await;
    assert_eq!(row.status, "success");
    assert_eq!(
        row.paid_at.map(|t| t.to_rfc3339()),
        Some("2026-09-24T10:01:02+00:00".into())
    );
}

/// PAY-41 / PAY-42 / PAY-43: epusdt JSON callbacks (`ok`), wrong provider channel refused.
#[tokio::test]
async fn pay_42_epusdt_and_bepusdt_callbacks() {
    let app = PayApp::offline().await;
    let ep_channel = app
        .seed_channel("epusdt", "usdt", "qr", json!({"gateway_url": "https://gm", "pid": "1000", "secret_key": "sk",
            "token": "usdt", "network": "tron", "notify_url": "https://n", "return_url": "https://r"}))
        .await;
    let bep_channel = app
        .seed_channel(
            "bepusdt",
            "bepusdt",
            "redirect",
            json!({"gateway_url": "https://bep", "auth_token": "tok",
            "notify_url": "https://n", "return_url": "https://r"}),
        )
        .await;
    let order = app.seed_order("DJ3", "12.50").await;
    let ep_payment = app
        .seed_payment(order, ep_channel, "DJP-EP-1", "12.50", "CNY")
        .await;
    let order2 = app.seed_order("DJ4", "10.50").await;
    let bep_payment = app
        .seed_payment(order2, bep_channel, "DJP-BEP-1", "10.50", "CNY")
        .await;

    let ep_params = |status: i64| {
        vec![
            ("pid", SignValue::Str("1000".into())),
            ("trade_id", SignValue::Str("T100".into())),
            ("order_id", SignValue::Str("DJP-EP-1".into())),
            ("amount", SignValue::Float(12.5)),
            ("actual_amount", SignValue::Float(1.7361)),
            ("receive_address", SignValue::Str("TAddr".into())),
            ("token", SignValue::Str("usdt".into())),
            ("block_transaction_id", SignValue::Str("0xabc".into())),
            ("status", SignValue::Int(status)),
        ]
    };
    let ep_body = |status: i64| {
        json!({"pid": "1000", "trade_id": "T100", "order_id": "DJP-EP-1", "amount": 12.5, "actual_amount": 1.7361,
            "receive_address": "TAddr", "token": "usdt", "block_transaction_id": "0xabc", "status": status,
            "signature": epusdt::sign(&ep_params(status), "sk")})
        .to_string()
    };
    // PAY-43: a signed "expired" notification never pays.
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            ep_body(3),
        )
        .await;
    assert_eq!(body, "fail");
    assert_eq!(app.payment(ep_payment).await.status, "pending");
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            ep_body(2),
        )
        .await;
    assert_eq!(body, "ok");
    assert_eq!(app.payment(ep_payment).await.status, "success");

    let bep_params = vec![
        ("trade_id", SignValue::Str("B1".into())),
        ("order_id", SignValue::Str("DJP-BEP-1".into())),
        ("amount", SignValue::Float(10.5)),
        ("actual_amount", SignValue::Float(1.46)),
        ("token", SignValue::Str("TAddr".into())),
        ("block_transaction_id", SignValue::Str("0xb".into())),
        ("status", SignValue::Int(2)),
    ];
    let bep_body = json!({"trade_id": "B1", "order_id": "DJP-BEP-1", "amount": 10.5, "actual_amount": "1.46",
        "token": "TAddr", "block_transaction_id": "0xb", "status": 2, "signature": bepusdt::sign(&bep_params, "tok")})
    .to_string();
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            bep_body,
        )
        .await;
    assert_eq!(body, "success");
    assert_eq!(app.payment(bep_payment).await.status, "success");
}

/// PAY-46: TokenPay compares the fiat ActualAmount/BaseCurrency.
#[tokio::test]
async fn pay_46_tokenpay_callback() {
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel("tokenpay", "usdt", "qr", json!({"gateway_url": "https://tp", "notify_secret": "secret", "currency": "USDT_TRC20"}))
        .await;
    let order = app.seed_order("DJ5", "10.00").await;
    let payment = app
        .seed_payment(order, channel, "DJP-TP-1", "10.00", "CNY")
        .await;
    let mut v = json!({"Id": "TP1", "OutOrderId": "DJP-TP-1", "OrderUserKey": "g@example.com", "Status": 1,
        "ActualAmount": 10.00, "Amount": "1.38", "BaseCurrency": "CNY", "Currency": "USDT_TRC20"});
    let raw = zs_infra::payment::raw_json::RawJson::parse(v.to_string().as_bytes()).unwrap();
    v["Signature"] = json!(tokenpay::sign_members(raw.members().unwrap(), "secret"));
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            v.to_string(),
        )
        .await;
    assert_eq!(body, "ok");
    assert_eq!(app.payment(payment).await.status, "success");
}

/// PAY-39: OKPay JSON callbacks answer `{"status":"success"}` as JSON; coin must match.
#[tokio::test]
async fn pay_39_okpay_callback() {
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel(
            "okpay",
            "usdt",
            "qr",
            json!({"merchant_id": "10001", "merchant_token": "TOKEN", "return_url": "https://r",
            "callback_url": "https://shop/api/v1/payments/callback", "coin": "USDT"}),
        )
        .await;
    let order = app.seed_order("DJ6", "616").await;
    let payment = app
        .seed_payment(order, channel, "DJP-OK-1", "616.00", "USDT")
        .await;
    let make = |coin: &str| {
        let unsigned = format!(
            r#"{{"code":200,"data":{{"order_id":"OK1","unique_id":"DJP-OK-1","amount":"616.00000000","coin":"{coin}","status":1}},"id":10001,"status":"success"}}"#
        );
        let raw = zs_infra::payment::raw_json::RawJson::parse(unsigned.as_bytes()).unwrap();
        let mut pairs = okpay::flatten(&raw).unwrap();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        let sign = okpay::build_signature(&pairs, "TOKEN");
        format!("{},\"sign\":\"{sign}\"}}", unsigned.trim_end_matches('}'))
    };
    let (_, ct, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            make("TRX"),
        )
        .await;
    assert_eq!(body, r#"{"status":"fail"}"#);
    assert_eq!(ct, "application/json");
    assert_eq!(app.payment(payment).await.status, "pending");
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            make("USDT"),
        )
        .await;
    assert_eq!(body, r#"{"status":"success"}"#);
    assert_eq!(app.payment(payment).await.status, "success");
}

fn stripe_event(order_no: &str, payment_status: &str) -> String {
    json!({"id": "evt_1", "type": "checkout.session.completed", "data": {"object": {"object": "checkout.session",
        "id": "cs_1", "payment_status": payment_status, "status": "complete", "currency": "usd", "amount_total": 1050,
        "created": Utc::now().timestamp(), "metadata": {"order_no": order_no}}}})
    .to_string()
}

fn stripe_header(secret: &str, body: &str) -> String {
    let ts = Utc::now().timestamp();
    format!(
        "t={ts},v1={}",
        stripe::compute_signature(secret, ts, body.as_bytes())
    )
}

/// PAY-25 / PAY-07: Stripe webhooks (explicit and blind channel matching).
#[tokio::test]
async fn pay_25_stripe_webhook() {
    let app = PayApp::offline().await;
    let cfg = |secret: &str| {
        json!({"secret_key": "sk", "webhook_secret": secret, "success_url": "https://r", "cancel_url": "https://c",
            "api_base_url": "https://stripe.example.com"})
    };
    let _other = app
        .seed_channel("official", "stripe", "redirect", cfg("whsec_other"))
        .await;
    let channel = app
        .seed_channel("official", "stripe", "redirect", cfg("whsec_right"))
        .await;
    let order = app.seed_order("DJ7", "10.50").await;
    let payment = app
        .seed_payment(order, channel, "DJP-ST-1", "10.50", "USD")
        .await;

    let unpaid = stripe_event("DJP-ST-1", "unpaid");
    let res = app
        .callback(
            "POST",
            &format!("/api/v1/payments/webhook/stripe?channel_id={channel}"),
            "application/json",
            &[("Stripe-Signature", &stripe_header("whsec_right", &unpaid))],
            unpaid,
        )
        .await;
    let v: Value = serde_json::from_str(&res.2).unwrap();
    assert_eq!(v["data"]["status"], "pending");
    assert_eq!(app.payment(payment).await.status, "pending");

    let paid = stripe_event("DJP-ST-1", "paid");
    let bad = app
        .callback(
            "POST",
            "/api/v1/payments/webhook/stripe",
            "application/json",
            &[("Stripe-Signature", &stripe_header("whsec_forged", &paid))],
            paid.clone(),
        )
        .await;
    let v: Value = serde_json::from_str(&bad.2).unwrap();
    assert_eq!(v["status_code"], 400);
    // Blind matching tries every active Stripe channel (PAY-07).
    let ok = app
        .callback(
            "POST",
            "/api/v1/payments/webhook/stripe",
            "application/json",
            &[("Stripe-Signature", &stripe_header("whsec_right", &paid))],
            paid,
        )
        .await;
    let v: Value = serde_json::from_str(&ok.2).unwrap();
    assert_eq!(v["status_code"], 0, "{v}");
    assert_eq!(
        v["data"],
        json!({"accepted": true, "event_type": "success", "updated": true, "payment_id": payment, "status": "success"})
    );
    assert_eq!(app.payment(payment).await.status, "success");
}

/// PAY-07 / PAY-28 / PAY-30: PayPal needs channel_id; verification goes to PayPal with raw bytes.
#[tokio::test]
async fn pay_30_paypal_webhook() {
    let app = PayApp::new(|req| {
        if req.url.ends_with("/v1/oauth2/token") {
            Ok(HttpResponse::new(200, r#"{"access_token":"tok"}"#))
        } else if String::from_utf8_lossy(&req.body).contains(r#""transmission_sig":"good""#) {
            Ok(HttpResponse::new(
                200,
                r#"{"verification_status":"SUCCESS"}"#,
            ))
        } else {
            Ok(HttpResponse::new(
                200,
                r#"{"verification_status":"FAILURE"}"#,
            ))
        }
    })
    .await;
    let channel = app
        .seed_channel("official", "paypal", "redirect", json!({"client_id": "c", "client_secret": "s",
            "base_url": "https://pp.example.com", "return_url": "https://r", "cancel_url": "https://c", "webhook_id": "WH1"}))
        .await;
    let order = app.seed_order("DJ8", "10.50").await;
    let payment = app
        .seed_payment(order, channel, "DJP-PP-1", "10.50", "USD")
        .await;
    let event = r#"{"id":"WH-1","event_type":"PAYMENT.CAPTURE.COMPLETED","resource":{"status":"COMPLETED","amount":{"value":"10.50","currency_code":"USD"},"supplementary_data":{"related_ids":{"order_id":"REF-DJP-PP-1"}}}}"#;
    let headers = |sig: &'static str| {
        vec![
            ("Paypal-Transmission-Id", "tid"),
            ("Paypal-Transmission-Time", "2026-09-24T10:00:00Z"),
            ("Paypal-Cert-Url", "https://api.paypal.com/cert"),
            ("Paypal-Auth-Algo", "SHA256withRSA"),
            ("Paypal-Transmission-Sig", sig),
        ]
    };
    let no_channel = app
        .callback(
            "POST",
            "/api/v1/payments/webhook/paypal",
            "application/json",
            &headers("good"),
            event,
        )
        .await;
    let v: Value = serde_json::from_str(&no_channel.2).unwrap();
    assert_eq!(
        (v["status_code"].clone(), v["msg"].is_string()),
        (json!(400), true)
    );
    let uri = format!("/api/v1/payments/webhook/paypal?channel_id={channel}");
    let forged = app
        .callback("POST", &uri, "application/json", &headers("bad"), event)
        .await;
    let v: Value = serde_json::from_str(&forged.2).unwrap();
    assert_eq!(v["status_code"], 400);
    assert_eq!(app.payment(payment).await.status, "pending");
    let ok = app
        .callback("POST", &uri, "application/json", &headers("good"), event)
        .await;
    let v: Value = serde_json::from_str(&ok.2).unwrap();
    assert_eq!(v["data"]["updated"], true, "{v}");
    assert_eq!(app.payment(payment).await.status, "success");
}

fn djp_headers(secret: &str, body: &str) -> Vec<(String, String)> {
    let ts = Utc::now().timestamp().to_string();
    let mut msg = format!("{ts}.").into_bytes();
    msg.extend_from_slice(body.as_bytes());
    let sig = zs_shared::sign::hmac_sha256(secret.as_bytes(), &msg);
    let sig: String = sig.iter().map(|b| format!("{b:02x}")).collect();
    vec![
        ("DJP-Webhook-Timestamp".into(), ts),
        ("DJP-Webhook-Signature".into(), format!("sha256={sig}")),
    ]
}

/// PAY-48 / PAY-07: DujiaoPay webhooks match the right channel without channel_id.
#[tokio::test]
async fn pay_48_dujiaopay_blind_matching() {
    let app = PayApp::offline().await;
    let cfg = |secret: &str| json!({"api_base_url": "https://djp", "api_key_id": "k", "api_secret": "s", "webhook_secret": secret, "token_id": "tron-usdt"});
    app.seed_channel("dujiaopay", "tron-usdt", "redirect", cfg("wh-a"))
        .await;
    let channel = app
        .seed_channel("dujiaopay", "tron-usdt", "redirect", cfg("wh-b"))
        .await;
    let order = app.seed_order("DJ9", "9.90").await;
    let payment = app
        .seed_payment(order, channel, "DJP-DJ-1", "9.90", "CNY")
        .await;
    let body = r#"{"event_id":"e1","event_type":"order.paid","created_at":"2026-09-24T10:00:00Z","data":{"order_id":"djp_1","merchant_order_id":"DJP-DJ-1","fiat_currency":"CNY","fiat_amount":"9.90","tx_id":"0xabc"}}"#;
    let headers = djp_headers("wh-b", body);
    let h: Vec<(&str, &str)> = headers
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    let res = app
        .callback(
            "POST",
            "/api/v1/payments/webhook/dujiaopay",
            "application/json",
            &h,
            body,
        )
        .await;
    let v: Value = serde_json::from_str(&res.2).unwrap();
    assert_eq!(v["data"]["payment_id"], payment, "{v}");
    let row = app.payment(payment).await;
    assert_eq!(row.status, "success");
    assert_eq!(row.provider_payload.unwrap()["tx_hash"], "0xabc");
}

/// Go vector: AES-256-GCM resource (key 0123…cdef, nonce abcdefghijkl, aad "transaction").
const WX_CIPHERTEXT: &str = "Eu6myr+6GaECeJdOZPZ6BX518U+IokN5asxf8qE9rZ4Fp86jRzDQmWhOopA+o7fsvHmewMk/Ep6BjDss2Miqf+6g3I65tTUeJ3UhG1wLh1iJhrQAwfJCK+tzQuITiVcFiKLBjq2ZcjSQ/lKvSJtQlzGKzHwHcBAXkFH5J5p2Niild0JsZd1Ug/xWtCbcZq2a5pFJn7PiZzSHuYHz18s4G4yLXOVundf1yDZ7rHi3G6emLZ4X+oMBBfpr";

fn wechat_config(api_v3_key: &str) -> Value {
    json!({"appid": "wx1", "mchid": "m1", "merchant_serial_no": "MSN1", "merchant_private_key": payment_common::PRIV_PEM,
        "api_v3_key": api_v3_key, "verification_mode": "wechatpay_public_key", "wechatpay_public_key_id": "PUB_KEY_ID_01",
        "wechatpay_public_key": PUB_PEM, "notify_url": "https://shop/api/v1/payments/callback"})
}

/// PAY-07 / PAY-32: WeChat notifications on the shared endpoint without channel_id; the
/// channel whose API v3 key decrypts the resource wins.
#[tokio::test]
async fn pay_32_wechat_notification_blind_matching() {
    let app = PayApp::offline().await;
    app.seed_channel(
        "official",
        "wechat",
        "qr",
        wechat_config("ffffffffffffffffffffffffffffffff"),
    )
    .await;
    let channel = app
        .seed_channel(
            "official",
            "wechat",
            "qr",
            wechat_config("0123456789abcdef0123456789abcdef"),
        )
        .await;
    let order = app.seed_order("DJ10", "72.43").await;
    let payment = app
        .seed_payment(order, channel, "DJP1", "72.43", "CNY")
        .await;
    let body = json!({"id": "EV-1", "event_type": "TRANSACTION.SUCCESS", "resource_type": "encrypt-resource",
        "resource": {"algorithm": "AEAD_AES_256_GCM", "ciphertext": WX_CIPHERTEXT, "associated_data": "transaction",
        "original_type": "transaction", "nonce": "abcdefghijkl"}})
    .to_string();
    let ts = Utc::now().timestamp().to_string();
    let good = rsa_sha256(&format!("{ts}\nN1\n{body}\n"));
    let send = |sig: String| {
        let headers = [
            ("Wechatpay-Serial".to_owned(), "PUB_KEY_ID_01".to_owned()),
            ("Wechatpay-Signature".to_owned(), sig),
            ("Wechatpay-Timestamp".to_owned(), ts.clone()),
            ("Wechatpay-Nonce".to_owned(), "N1".to_owned()),
        ];
        let body = body.clone();
        let app = &app;
        async move {
            let h: Vec<(&str, &str)> = headers
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            app.callback(
                "POST",
                "/api/v1/payments/callback",
                "application/json",
                &h,
                body,
            )
            .await
        }
    };
    let (status, _, reply) = send(rsa_sha256("tampered")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(reply, r#"{"code":"FAIL","message":"失败"}"#);
    let (status, _, reply) = send(good).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(reply, r#"{"code":"SUCCESS","message":"成功"}"#);
    let row = app.payment(payment).await;
    assert_eq!(row.status, "success");
    assert_eq!(row.provider_ref, "4200001");
}

/// Wallet recharges (`order_id = 0`) match their recharge number.
#[tokio::test]
async fn recharge_payment_callback() {
    let app = PayApp::offline().await;
    let (channel, _, _) = epay_setup(&app).await;
    let payment = app
        .seed_payment(0, channel, "DJP-WR-1", "9.90", "CNY")
        .await;
    app.seed_recharge(payment, "WR1001", 7).await;
    let query = epay_query_for("DJP-WR-1", "1001", "9.90", EPAY_KEY);
    let uri = format!("/api/v1/payments/callback?{query}");
    let (_, _, body) = app.callback("GET", &uri, "", &[], Vec::new()).await;
    assert_eq!(body, "success");
    assert_eq!(app.payment(payment).await.status, "success");
}

/// Custom callback paths replace the default ones (`callback_routes_config`).
#[tokio::test]
async fn custom_callback_route() {
    let app = PayApp::offline().await;
    let (_, _, payment) = epay_setup(&app).await;
    let settings = zs_infra::db::repo::settings::SeaSettingsStore::new(app.db.clone());
    settings
        .set(
            "callback_routes_config",
            &json!({"payment_callback": "/api/pay/notify"}),
        )
        .await
        .unwrap();
    let query = epay_query("1001", "9.90", EPAY_KEY);
    let (status, _, _) = app
        .callback(
            "GET",
            &format!("/api/v1/payments/callback?{query}"),
            "",
            &[],
            Vec::new(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(app.payment(payment).await.status, "pending");
    let (status, _, body) = app
        .callback(
            "GET",
            &format!("/api/pay/notify/?{query}"),
            "",
            &[],
            Vec::new(),
        )
        .await;
    assert_eq!((status, body.as_str()), (StatusCode::OK, "success"));
    assert_eq!(app.payment(payment).await.status, "success");
    let (status, _, _) = app
        .callback("POST", "/api/unknown/path", "", &[], Vec::new())
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

/// PAY-04: callback bodies are capped at 1 MiB.
#[tokio::test]
async fn pay_04_body_limit() {
    let app = PayApp::offline().await;
    let big = format!(
        "{{\"pid\":\"1\",\"trade_id\":\"T\",\"order_id\":\"O\",\"pad\":\"{}\"}}",
        "x".repeat(1 << 20)
    );
    let (status, _, _) = app
        .callback(
            "POST",
            "/api/v1/payments/callback",
            "application/json",
            &[],
            big.clone(),
        )
        .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (_, _, body) = app
        .callback(
            "POST",
            "/api/v1/payments/webhook/stripe",
            "application/json",
            &[],
            big,
        )
        .await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["status_code"], 400);
}
