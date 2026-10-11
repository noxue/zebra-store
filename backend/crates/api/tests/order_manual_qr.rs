//! End-to-end manual QR payment creation and shared unique amount allocation.

mod order_common;

use order_common::{Auth, OrderApp};
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet, IntoActiveModel, Set};
use serde_json::json;

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
