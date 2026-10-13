//! End-to-end manual QR payment creation and shared unique amount allocation.

mod order_common;

use order_common::{Auth, OrderApp};
use sea_orm::EntityTrait;
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet, IntoActiveModel, Set};
use serde_json::json;
use zs_infra::db::entity::{orders, payments};

/// A manual channel must lock the order before support transfers funds, and recording
/// the external transfer twice may create only one local refund.
#[tokio::test]
async fn manual_refund_requires_transfer_confirmation_and_is_idempotent() {
    let app = OrderApp::new().await;
    let (product, sku) = app
        .product("manual-refund", json!({"price_amount": "10.00"}))
        .await;
    app.secrets(product, sku, 1).await;
    let (_, token) = app.user("refund-manual@example.com", 0).await;
    let auth = Auth::Bearer(token);
    let channel = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name":"Manual", "provider_type":"manual_qr", "channel_type":"alipay",
                "interaction_mode":"qr", "payment_types":["order"],
                "config_json":{"qr_image_url":"/uploads/manual.png"}
            })),
        )
        .await;
    assert_eq!(channel["status_code"], 0, "{channel}");
    let created = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({
                "items":[{"product_id":product,"sku_id":sku,"quantity":1}]
            })),
            &auth,
        )
        .await;
    assert_eq!(created["status_code"], 0, "{created}");
    let payment = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({
                "order_no": created["data"]["order_no"], "channel_id": channel["data"]["id"]
            })),
            &auth,
        )
        .await;
    assert_eq!(payment["status_code"], 0, "{payment}");
    let payment_id = payment["data"]["payment_id"].as_i64().unwrap();
    let payment_row = payments::Entity::find_by_id(payment_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let order_id = payment_row.order_id;
    let mut payment_row = payment_row.into_active_model();
    payment_row.status = Set("success".into());
    payment_row.paid_at = Set(Some(chrono::Utc::now()));
    payment_row.update(&app.db).await.unwrap();
    let order_row = orders::Entity::find_by_id(order_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let mut order_row = order_row.into_active_model();
    order_row.status = Set("paid".into());
    order_row.paid_at = Set(Some(chrono::Utc::now()));
    order_row.online_paid_amount = Set("10.00".parse().unwrap());
    order_row.update(&app.db).await.unwrap();

    let detail = app
        .call(
            "GET",
            &format!(
                "/api/v1/orders/{}",
                created["data"]["order_no"].as_str().unwrap()
            ),
            None,
            &auth,
        )
        .await;
    assert_eq!(detail["data"]["refund_mode"], "contact_support");
    let automatic = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{order_id}/original-refund"),
            Some(json!({"amount":"10.00"})),
        )
        .await;
    assert_ne!(automatic["status_code"], 0, "{automatic}");

    let uri = format!("/api/v1/admin/orders/{order_id}/manual-refund/prepare");
    let mut prepared = app
        .admin_call(
            "POST",
            &uri,
            Some(json!({"amount":"10.00","remark":"support"})),
        )
        .await;
    assert_eq!(prepared["status_code"], 0, "{prepared}");
    assert_eq!(prepared["data"]["attempt"]["status"], "manual_awaiting");
    assert_eq!(
        format!("{:.2}", app.order(order_id).await.refunded_amount),
        "0.00"
    );
    let duplicate = app
        .admin_call(
            "POST",
            &uri,
            Some(json!({"amount":"10.00","remark":"support"})),
        )
        .await;
    assert_eq!(
        duplicate["data"]["attempt"]["id"],
        prepared["data"]["attempt"]["id"]
    );
    // No external transfer occurred: cancel the lock, then a fresh attempt is allowed.
    let cancel_uri = format!("/api/v1/admin/orders/{order_id}/manual-refund/cancel");
    let cancel_body = json!({"attempt_id": prepared["data"]["attempt"]["id"]});
    let canceled = app
        .admin_call("POST", &cancel_uri, Some(cancel_body.clone()))
        .await;
    assert_eq!(canceled["status_code"], 0, "{canceled}");
    assert_eq!(canceled["data"]["attempt"]["status"], "manual_canceled");
    let canceled_again = app.admin_call("POST", &cancel_uri, Some(cancel_body)).await;
    assert_eq!(canceled_again["status_code"], 0, "{canceled_again}");
    let cannot_confirm_canceled = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{order_id}/manual-refund/confirm"),
            Some(json!({
                "attempt_id": canceled["data"]["attempt"]["id"],
                "transfer_ref": "NO-TRANSFER"
            })),
        )
        .await;
    assert_ne!(cannot_confirm_canceled["status_code"], 0);
    assert_eq!(
        format!("{:.2}", app.order(order_id).await.refunded_amount),
        "0.00"
    );
    prepared = app
        .admin_call(
            "POST",
            &uri,
            Some(json!({"amount":"10.00","remark":"support"})),
        )
        .await;
    assert_eq!(prepared["status_code"], 0, "{prepared}");
    assert_ne!(
        prepared["data"]["attempt"]["id"],
        canceled["data"]["attempt"]["id"]
    );
    let blocked = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{order_id}/manual-refund"),
            Some(json!({"amount":"10.00"})),
        )
        .await;
    assert_ne!(blocked["status_code"], 0, "{blocked}");
    let confirm_uri = format!("/api/v1/admin/orders/{order_id}/manual-refund/confirm");
    let invalid = app
        .admin_call(
            "POST",
            &confirm_uri,
            Some(json!({
                "attempt_id": prepared["data"]["attempt"]["id"], "transfer_ref":""
            })),
        )
        .await;
    assert_ne!(invalid["status_code"], 0);
    let body =
        json!({"attempt_id": prepared["data"]["attempt"]["id"], "transfer_ref":"BANK-REF-1"});
    let confirmed = app
        .admin_call("POST", &confirm_uri, Some(body.clone()))
        .await;
    assert_eq!(confirmed["status_code"], 0, "{confirmed}");
    let replay = app.admin_call("POST", &confirm_uri, Some(body)).await;
    assert_eq!(replay["status_code"], 0, "{replay}");
    let too_late = app
        .admin_call(
            "POST",
            &cancel_uri,
            Some(json!({"attempt_id": prepared["data"]["attempt"]["id"]})),
        )
        .await;
    assert_ne!(too_late["status_code"], 0, "{too_late}");
    assert_eq!(
        replay["data"]["refund_record"]["id"],
        confirmed["data"]["refund_record"]["id"]
    );
    assert_eq!(
        format!("{:.2}", app.order(order_id).await.refunded_amount),
        "10.00"
    );
    let records = app
        .admin_call("GET", "/api/v1/admin/order-refunds", None)
        .await;
    assert_eq!(records["data"].as_array().unwrap().len(), 1);
}

/// PAY-03: one manual channel exposes two named methods beside another gateway,
/// and each selected method reaches its own collection image.
#[tokio::test]
async fn manual_methods_and_epay_coexist_on_checkout_and_payment() {
    let app = OrderApp::new().await;
    let product = app
        .product("multi-method", json!({"price_amount": "10.00"}))
        .await;
    app.secrets(product.0, product.1, 3).await;
    let (_, token) = app.user("multi-method@example.com", 0).await;
    let auth = Auth::Bearer(token);
    let manual = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name":"人工收款", "provider_type":"manual_qr", "channel_type":"alipay",
                "interaction_mode":"qr", "payment_types":["order"],
                "config_json": {
                    "supported_channel_types":["alipay","wechat"],
                    "alipay_qr_image_url":"/uploads/alipay.png",
                    "wechat_qr_image_url":"/uploads/wechat.png",
                    "notification_type":"telegram", "telegram_token":"12345:bot_token"
                }
            })),
        )
        .await;
    assert_eq!(manual["status_code"], 0, "{manual}");
    let manual_id = manual["data"]["id"].as_i64().unwrap();
    let epay = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name":"自动支付", "provider_type":"epay", "channel_type":"wechat",
                "interaction_mode":"qr", "payment_types":["order"],
                "config_json": {
                    "epay_version":"v1", "gateway_url":"https://pay.example.com",
                    "merchant_id":"M-1", "merchant_key":"test-key",
                    "notify_url":"https://shop.example.com/api/v1/payments/callback",
                    "return_url":"https://shop.example.com/pay"
                }
            })),
        )
        .await;
    assert_eq!(epay["status_code"], 0, "{epay}");
    let listed = app
        .call("GET", "/api/v1/public/config", None, &Auth::None)
        .await;
    let channels = listed["data"]["payment_channels"].as_array().unwrap();
    assert!(
        channels
            .iter()
            .any(|c| c["id"] == manual_id
                && c["supported_channel_types"] == json!(["alipay", "wechat"])),
        "{listed}"
    );
    assert!(channels.iter().any(|c| c["provider_type"] == "epay"));
    let mut amounts = Vec::new();
    for (method, image) in [
        ("alipay", "/uploads/alipay.png"),
        ("wechat", "/uploads/wechat.png"),
    ] {
        let order = app
            .call(
                "POST",
                "/api/v1/orders",
                Some(json!({
                    "items":[{"product_id":product.0,"sku_id":product.1,"quantity":1}]
                })),
                &auth,
            )
            .await;
        assert_eq!(order["status_code"], 0, "{order}");
        let payment = app.call("POST", "/api/v1/payments", Some(json!({
            "order_no":order["data"]["order_no"], "channel_id":manual_id, "channel_type":method
        })), &auth).await;
        assert_eq!(payment["status_code"], 0, "{payment}");
        assert_eq!(payment["data"]["channel_type"], method);
        let row = app
            .payment(payment["data"]["payment_id"].as_i64().unwrap())
            .await;
        assert_eq!(row.qr_code, image);
        amounts.push(row.amount);
    }
    assert_ne!(amounts[0], amounts[1]);
}

/// PAY-03: simultaneous live manual payments must stay distinguishable across channels.
#[tokio::test]
async fn different_manual_channels_allocate_distinct_live_amounts() {
    let app = OrderApp::new().await;
    let product = app
        .product("manual-qr", json!({"price_amount": "10.00"}))
        .await;
    app.secrets(product.0, product.1, 3).await;
    let (_, token) = app.user("manual-qr@example.com", 0).await;
    let auth = Auth::Bearer(token);
    let mut channels = Vec::new();
    for channel_type in ["wechat", "alipay"] {
        let result = app.admin_call("POST", "/api/v1/admin/payment-channels", Some(json!({
            "name": format!("QR {channel_type}"), "provider_type": "manual_qr",
            "channel_type": channel_type, "interaction_mode": "qr",
            "payment_types": ["order"], "config_json": {"qr_image_url": "/uploads/merchant.png"}
        }))).await;
        assert_eq!(result["status_code"], 0, "{result}");
        channels.push(result["data"]["id"].as_i64().unwrap());
    }
    let mut amounts = Vec::new();
    let mut first_payment = None;
    for &channel in &channels {
        let order = app
            .call(
                "POST",
                "/api/v1/orders",
                Some(json!({
                    "items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}]
                })),
                &auth,
            )
            .await;
        assert_eq!(order["status_code"], 0, "{order}");
        let order_no = order["data"]["order_no"].as_str().unwrap();
        let response = app
            .call(
                "POST",
                "/api/v1/payments",
                Some(json!({
                    "order_no": order_no, "channel_id": channel
                })),
                &auth,
            )
            .await;
        assert_eq!(response["status_code"], 0, "{response}");
        let payment_id = response["data"]["payment_id"].as_i64().unwrap();
        let row = app.payment(payment_id).await;
        assert_eq!(row.provider_type, "manual_qr");
        assert!(row.provider_payload.as_ref().unwrap()["manual_link_token"].is_string());
        assert!(!response.to_string().contains("manual_link_token"));
        assert!(!row.qr_code.is_empty());
        assert!(row.amount >= "10.00".parse().unwrap());
        assert!(row.amount <= "10.99".parse().unwrap());
        if first_payment.is_none() {
            first_payment = Some((
                payment_id,
                row.provider_payload.as_ref().unwrap()["manual_link_token"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
                row.amount,
            ));
        }
        amounts.push(row.amount);
    }
    assert_ne!(amounts[0], amounts[1]);
    let (payment_id, link_key, amount) = first_payment.unwrap();
    let seed = app.payment(payment_id).await;
    for cents in 0..100_i64 {
        let candidate = Decimal::new(1000 + cents, 2);
        if amounts.contains(&candidate) {
            continue;
        }
        let mut occupied = seed.clone().into_active_model().reset_all();
        occupied.id = NotSet;
        occupied.amount = Set(candidate);
        occupied.status = Set("expired".to_owned());
        occupied.gateway_order_no = Set(format!("MQR-RESERVED-{cents}"));
        occupied.insert(&app.db).await.unwrap();
    }
    let third_order = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({
                "items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}]
            })),
            &auth,
        )
        .await;
    assert_eq!(third_order["status_code"], 0, "{third_order}");
    let third = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({
                "order_no": third_order["data"]["order_no"], "channel_id": channels[0]
            })),
            &auth,
        )
        .await;
    assert_ne!(third["status_code"], 0, "{third}");
    let confirmed = app
        .call(
            "POST",
            &format!("/api/v1/manual-payments/{payment_id}/confirm"),
            Some(json!({"received_amount": amount.to_string()})),
            &Auth::Bearer(link_key),
        )
        .await;
    assert_eq!(confirmed["status_code"], 0, "{confirmed}");
    assert_eq!(app.payment(payment_id).await.status, "success");
}
