//! Merchant collection QR: shared web/API authentication and late force settlement.

mod payment_common;

use axum::body::Body;
use axum::http::{Request, header};
use base64::Engine as _;
use chrono::Utc;
use payment_common::PayApp;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, Set};
use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use zs_infra::db::entity::orders;

fn api(id: i64) -> String {
    format!("/api/v1/manual-payments/{id}")
}

#[tokio::test]
async fn manual_review_key_rotation_and_exact_confirmation() {
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel(
            "manual_qr",
            "wechat",
            "qr",
            json!({"qr_image_url": "/uploads/merchant.png"}),
        )
        .await;
    let order = app.seed_order("MQR-001", "10.00").await;
    let payment = app
        .seed_payment(order, channel, "MQR-GW-001", "10.11", "CNY")
        .await;
    let mut row = app.payment(payment).await.into_active_model();
    row.provider_payload = Set(Some(
        json!({"manual_link_token": format!("mql_{}", "a".repeat(64))}),
    ));
    row.update(&app.db).await.unwrap();

    let first = app
        .call(
            "POST",
            &format!("/api/v1/admin/payment-channels/{channel}/manual-key"),
            Some(json!({})),
        )
        .await;
    assert_eq!(first["status_code"], 0, "{first}");
    let old_key = first["data"]["key"].as_str().unwrap();
    let uri = api(payment);
    let denied = app.call_as("GET", &uri, None, Some("wrong")).await;
    assert_ne!(denied["status_code"], 0);
    let review = app.call_as("GET", &uri, None, Some(old_key)).await;
    assert_eq!(review["data"]["amount"], "10.11", "{review}");
    assert_eq!(review["data"]["can_confirm"], true);
    let link_token = format!("mql_{}", "a".repeat(64));
    let link_review = app.call_as("GET", &uri, None, Some(&link_token)).await;
    assert_eq!(link_review["data"]["order_no"], "MQR-001");

    let second = app
        .call(
            "POST",
            &format!("/api/v1/admin/payment-channels/{channel}/manual-key"),
            Some(json!({"key": "replacement_key_012345678901234567890"})),
        )
        .await;
    assert_eq!(second["status_code"], 0, "{second}");
    assert_ne!(
        app.call_as("GET", &uri, None, Some(old_key)).await["status_code"],
        0
    );
    // PAY-45: a review link stays valid after rotating the channel API key.
    assert_eq!(
        app.call_as("GET", &uri, None, Some(&link_token)).await["status_code"],
        0
    );
    let key = second["data"]["key"].as_str().unwrap();
    assert_eq!(
        app.call_as("GET", &uri, None, Some(key)).await["status_code"],
        0
    );
    let deleted = app
        .call(
            "DELETE",
            &format!("/api/v1/admin/payment-channels/{channel}"),
            None,
        )
        .await;
    assert_eq!(deleted["status_code"], 0, "{deleted}");
    // A deleted channel is no longer selectable; its unresolved order link remains usable.
    assert_eq!(
        app.call_as("GET", &uri, None, Some(&link_token)).await["status_code"],
        0
    );
    let confirm_uri = format!("{uri}/confirm");
    let mut expired_payment = app.payment(payment).await.into_active_model();
    expired_payment.status = Set("expired".to_owned());
    expired_payment.update(&app.db).await.unwrap();
    let mut overdue_order = orders::Entity::find_by_id(order)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    overdue_order.expires_at = Set(Some(Utc::now() - chrono::Duration::seconds(1)));
    overdue_order.update(&app.db).await.unwrap();
    // PAY-04: verified late receipts can settle before the order is canceled.
    assert_eq!(
        app.call_as("GET", &uri, None, Some(&link_token)).await["data"]["can_confirm"],
        true
    );
    let wrong_amount = app
        .call_as(
            "POST",
            &confirm_uri,
            Some(json!({"received_amount": "10.10"})),
            Some(&link_token),
        )
        .await;
    assert_ne!(wrong_amount["status_code"], 0);
    let confirmed = app
        .call_as(
            "POST",
            &confirm_uri,
            Some(json!({"received_amount": "10.11"})),
            Some(&link_token),
        )
        .await;
    assert_eq!(confirmed["status_code"], 0, "{confirmed}");
    assert_eq!(confirmed["data"]["can_confirm"], false);
    assert_eq!(app.payment(payment).await.status, "success");
    // PAY-04: duplicate calls cannot settle the order twice.
    assert_ne!(
        app.call_as(
            "POST",
            &confirm_uri,
            Some(json!({"received_amount": "10.11"})),
            Some(&link_token)
        )
        .await["status_code"],
        0
    );
}

/// ORD-04: a closed order can be marked received only through the force path.
#[tokio::test]
async fn closed_order_link_remains_usable_until_force_confirmation() {
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel(
            "manual_qr",
            "alipay",
            "qr",
            json!({"qr_image_url": "/qr.png"}),
        )
        .await;
    let order = app.seed_order("MQR-CLOSED", "10.00").await;
    let payment = app
        .seed_payment(order, channel, "MQR-GW-002", "10.01", "CNY")
        .await;
    let token = format!("mql_{}", "b".repeat(64));
    let mut payment_row = app.payment(payment).await.into_active_model();
    payment_row.provider_payload = Set(Some(json!({"manual_link_token": token})));
    payment_row.status = Set("expired".to_owned());
    payment_row.update(&app.db).await.unwrap();
    let mut order_row = orders::Entity::find_by_id(order)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    order_row.status = Set("canceled".to_owned());
    order_row.canceled_at = Set(Some(Utc::now()));
    order_row.update(&app.db).await.unwrap();
    let uri = api(payment);
    let review = app.call_as("GET", &uri, None, Some(&token)).await;
    assert_eq!(review["data"]["requires_force"], true, "{review}");
    let confirm_uri = format!("{uri}/confirm");
    assert_ne!(
        app.call_as(
            "POST",
            &confirm_uri,
            Some(json!({"received_amount": "10.01"})),
            Some(&token)
        )
        .await["status_code"],
        0
    );
    let forced = app
        .call_as(
            "POST",
            &confirm_uri,
            Some(json!({"received_amount": "10.01", "force": true, "note": "账单已核实"})),
            Some(&token),
        )
        .await;
    assert_eq!(forced["status_code"], 0, "{forced}");
    assert_eq!(forced["data"]["order_status"], "fulfilling");
    assert_eq!(forced["data"]["can_confirm"], false);
    assert_eq!(
        app.payment(payment).await.exception_code,
        "manual_force_confirmed_closed_order"
    );
}

#[tokio::test]
async fn buyer_report_is_owned_saved_and_sent_without_confirming_payment() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let mail = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        stream
            .get_mut()
            .write_all(b"220 localhost ready\r\n")
            .await
            .unwrap();
        let mut body = String::new();
        let mut in_data = false;
        loop {
            let mut line = String::new();
            if stream.read_line(&mut line).await.unwrap() == 0 {
                break;
            }
            if in_data {
                if line == ".\r\n" {
                    stream
                        .get_mut()
                        .write_all(b"250 accepted\r\n")
                        .await
                        .unwrap();
                    in_data = false;
                } else {
                    body.push_str(&line);
                }
            } else if line.starts_with("EHLO") || line.starts_with("HELO") {
                stream
                    .get_mut()
                    .write_all(b"250 localhost\r\n")
                    .await
                    .unwrap();
            } else if line.starts_with("DATA") {
                stream
                    .get_mut()
                    .write_all(b"354 send data\r\n")
                    .await
                    .unwrap();
                in_data = true;
            } else if line.starts_with("QUIT") {
                stream.get_mut().write_all(b"221 bye\r\n").await.unwrap();
                break;
            } else {
                stream.get_mut().write_all(b"250 ok\r\n").await.unwrap();
            }
        }
        body
    });
    let app = PayApp::offline().await;
    let channel = app
        .seed_channel(
            "manual_qr",
            "wechat",
            "qr",
            json!({
                "qr_image_url": "/qr.png", "smtp_host": "127.0.0.1", "smtp_port": port.to_string(),
                "smtp_from": "merchant@example.test", "smtp_to": "reviewer@example.test"
            }),
        )
        .await;
    let order = app.seed_order("MQR-REPORT", "10.00").await;
    let mut row = orders::Entity::find_by_id(order)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    row.guest_password = Set(zs_domain::order::guest::hash_credential(
        "test-app-secret-0123456789abcdef",
        "g@example.com",
        "guest-password",
    ));
    row.update(&app.db).await.unwrap();
    let payment = app
        .seed_payment(order, channel, "MQR-REPORT-GW", "10.12", "CNY")
        .await;
    let token = format!("mql_{}", "c".repeat(64));
    let mut row = app.payment(payment).await.into_active_model();
    row.provider_payload = Set(Some(json!({"manual_link_token": token})));
    row.update(&app.db).await.unwrap();
    let uri = format!("/api/v1/guest/payments/{payment}/manual-issue");
    let payload =
        json!({"payer_name": "Alice Zhang", "issue": "not_received", "note": "Paid at 15:30"});
    let credentials =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode("g@example.com\nguest-password");
    let request = |auth: String, data: serde_json::Value| {
        Request::builder()
            .method("POST")
            .uri(&uri)
            .header(header::AUTHORIZATION, auth)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(data.to_string()))
            .unwrap()
    };
    let (_, _, bytes) = app
        .send(request("Guest invalid".to_owned(), payload.clone()))
        .await;
    let denied: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_ne!(denied["status_code"], 0);
    let (_, _, bytes) = app
        .send(request(format!("Guest {credentials}"), payload))
        .await;
    let sent: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sent["status_code"], 0, "{sent}");
    assert_eq!(sent["data"]["submitted"], true);
    let mail_body = mail.await.unwrap();
    let encoded = mail_body
        .split("\r\n\r\n")
        .nth(1)
        .unwrap()
        .replace(['\r', '\n'], "");
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let notice = String::from_utf8(decoded).unwrap();
    assert!(notice.contains("MQR-REPORT"), "{notice}");
    assert!(notice.contains("Alice Zhang"), "{notice}");
    assert!(notice.contains("/admin/manual-confirm/"), "{notice}");
    assert_eq!(app.payment(payment).await.status, "pending");
    let review = app.call_as("GET", &api(payment), None, Some(&token)).await;
    assert_eq!(review["data"]["buyer_report"]["payer_name"], "Alice Zhang");
    assert_eq!(review["data"]["buyer_report"]["issue"], "not_received");
    let mut closed = orders::Entity::find_by_id(order)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    closed.status = Set("canceled".to_owned());
    closed.canceled_at = Set(Some(Utc::now()));
    closed.update(&app.db).await.unwrap();
    let latest = Request::builder()
        .method("GET")
        .uri("/api/v1/guest/payments/latest?order_no=MQR-REPORT")
        .header(header::AUTHORIZATION, format!("Guest {credentials}"))
        .body(Body::empty())
        .unwrap();
    let (_, _, bytes) = app.send(latest).await;
    let closed_payment: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        closed_payment["data"]["payment_id"], payment,
        "{closed_payment}"
    );
}
