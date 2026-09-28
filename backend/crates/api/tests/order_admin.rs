//! Admin side: orders, manual delivery, refunds (wallet / manual, records, fee flag),
//! status changes; concurrency on the last stock; wallet recharges settled through the
//! order-aware settlement.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod order_common;

use order_common::{Auth, OrderApp};
use serde_json::json;

/// A member order of a manual product paid from the wallet; returns
/// `(user_id, token, parent_id, child_id)`.
async fn paid_manual_order(app: &OrderApp, balance: &str, qty: i32) -> (i64, Auth, i64, i64) {
    let (product, sku) = app
        .product(
            "manual",
            json!({"price_amount": 10, "fulfillment_type": "manual", "manual_stock_total": 5}),
        )
        .await;
    let (uid, token) = app.user("m@example.com", 0).await;
    app.fund(uid, balance).await;
    let auth = Auth::Bearer(token);
    let created = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product, "sku_id": sku, "quantity": qty}]})),
            &auth,
        )
        .await;
    assert_eq!(created["status_code"], 0, "{created}");
    let order_no = created["data"]["order_no"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0, "use_balance": true})),
            &auth,
        )
        .await;
    assert_eq!(res["data"]["order_paid"], true, "{res}");
    let parent = app.order_by_no(&order_no).await;
    let child = app.children(parent.id).await[0].id;
    (uid, auth, parent.id, child)
}

/// DLV-02 / DLV-03 / RFD-01 … RFD-04: manual delivery, partial wallet refund, manual
/// refund of the rest with the fee flag, over-refund refused, records listed.
#[tokio::test]
async fn manual_delivery_and_refunds() {
    let app = OrderApp::new().await;
    let (uid, auth, parent, child) = paid_manual_order(&app, "30.00", 2).await;
    assert_eq!(app.balance(uid).await, "10.00");

    // admin list / detail
    // manual items go straight to fulfilling after payment
    assert_eq!(app.order(child).await.status, "fulfilling");
    let list = app
        .admin_call("GET", "/api/v1/admin/orders?status=fulfilling", None)
        .await;
    assert_eq!(list["data"].as_array().unwrap().len(), 1, "{list}");
    let detail = app
        .admin_call("GET", &format!("/api/v1/admin/orders/{parent}"), None)
        .await;
    assert_eq!(detail["status_code"], 0, "{detail}");

    // manual delivery of the child
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/fulfillments",
            Some(json!({"order_id": child, "payload": "", "delivery_data": {"tracking_no": "SF123"}})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.order(child).await.status, "delivered");
    assert_eq!(app.order(parent).await.status, "delivered");
    // DLV-03: a second delivery is refused
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/fulfillments",
            Some(json!({"order_id": child, "payload": "again"})),
        )
        .await;
    assert_ne!(res["status_code"], 0);
    // DLV-05: the admin download of the delivery
    let (status, _, body) = app
        .raw(
            "GET",
            &format!("/api/v1/admin/orders/{child}/fulfillment/download"),
            None,
            &Auth::Bearer(app.admin.clone()),
        )
        .await;
    assert_eq!(status, 200);
    assert!(String::from_utf8(body).unwrap().contains("SF123"));

    // partial refund to the wallet
    let res = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{parent}/refund-to-wallet"),
            Some(json!({"amount": "5.00", "remark": "partial"})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.balance(uid).await, "15.00");
    assert_eq!(app.order(parent).await.status, "partially_refunded");
    assert_eq!(
        format!("{:.2}", app.order(parent).await.refunded_amount),
        "5.00"
    );

    // over-refund refused, nothing moves
    let res = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{parent}/refund-to-wallet"),
            Some(json!({"amount": "15.01"})),
        )
        .await;
    assert_ne!(res["status_code"], 0, "{res}");
    assert_eq!(app.balance(uid).await, "15.00");

    // manual refund of the rest (no wallet movement)
    let res = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{parent}/manual-refund"),
            Some(json!({"amount": 15, "remark": "bank", "payment_fee_refunded": true})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.balance(uid).await, "15.00");
    assert_eq!(app.order(parent).await.status, "refunded");
    assert_eq!(app.order(child).await.status, "refunded");

    let records = app
        .admin_call("GET", "/api/v1/admin/order-refunds", None)
        .await;
    let rows = records["data"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "{records}");
    let manual = rows.iter().find(|r| r["type"] == "manual").unwrap();
    let id = manual["id"].as_i64().unwrap();
    let res = app
        .admin_call(
            "PATCH",
            &format!("/api/v1/admin/order-refunds/{id}/payment-fee"),
            Some(json!({"payment_fee_refunded": false})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(res["data"]["payment_fee_refunded"], false);

    // the buyer sees the refund records
    let order_no = app.order(parent).await.order_no;
    let detail = app
        .call("GET", &format!("/api/v1/orders/{order_no}"), None, &auth)
        .await;
    assert_eq!(detail["data"]["status"], "refunded", "{detail}");
    assert_eq!(
        detail["data"]["refund_records"].as_array().unwrap().len(),
        2,
        "{detail}"
    );
}

/// Admin status change: delivered → completed on a parent cascades to the children;
/// invalid transitions are refused.
#[tokio::test]
async fn admin_status_changes() {
    let app = OrderApp::new().await;
    let (_, _, parent, child) = paid_manual_order(&app, "10.00", 1).await;
    let res = app
        .admin_call(
            "PATCH",
            &format!("/api/v1/admin/orders/{parent}"),
            Some(json!({"status": "completed"})),
        )
        .await;
    assert_ne!(res["status_code"], 0, "paid parents cannot complete: {res}");
    app.admin_call(
        "POST",
        "/api/v1/admin/fulfillments",
        Some(json!({"order_id": child, "payload": "x"})),
    )
    .await;
    let res = app
        .admin_call(
            "PATCH",
            &format!("/api/v1/admin/orders/{parent}"),
            Some(json!({"status": "completed"})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.order(parent).await.status, "completed");
    assert_eq!(app.order(child).await.status, "completed");
}

/// QA-A02 (live QA I-2): the generic status change cannot mark an order refunded
/// (neither parent nor child); the real refund still works exactly once afterwards.
#[tokio::test]
async fn qa_a02_status_change_cannot_fake_refund() {
    let app = OrderApp::new().await;
    let (uid, _, parent, child) = paid_manual_order(&app, "10.00", 1).await;
    assert_eq!(app.balance(uid).await, "0.00");
    for id in [parent, child] {
        for status in ["refunded", "partially_refunded"] {
            let res = app
                .admin_call(
                    "PATCH",
                    &format!("/api/v1/admin/orders/{id}"),
                    Some(json!({"status": status})),
                )
                .await;
            assert_eq!(res["status_code"], 400, "{res}");
            assert_eq!(
                res["msg"], "Refunds must be made through the refund action",
                "{res}"
            );
        }
    }
    assert_eq!(app.order(parent).await.status, "fulfilling");
    assert_eq!(app.order(child).await.status, "fulfilling");
    assert_eq!(
        format!("{:.2}", app.order(parent).await.refunded_amount),
        "0.00"
    );
    // the full amount is still refundable once, through the refund flow
    let refund_uri = format!("/api/v1/admin/orders/{parent}/refund-to-wallet");
    let refund = || app.admin_call("POST", &refund_uri, Some(json!({"amount": "10.00"})));
    let res = refund().await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.balance(uid).await, "10.00");
    assert_eq!(app.order(parent).await.status, "refunded");
    let res = refund().await;
    assert_ne!(res["status_code"], 0, "no double refund: {res}");
    assert_eq!(app.balance(uid).await, "10.00");
}

/// DB-01 / ORD-03 / DLV-09: two buyers race for the last card secret on a single
/// connection; exactly one order is created and the secret is reserved once.
#[tokio::test]
async fn concurrent_orders_on_last_card_secret() {
    let app = OrderApp::new().await;
    let (product, sku) = app.product("last", json!({"price_amount": 10})).await;
    app.secrets(product, sku, 1).await;
    let body = |email: &str| {
        json!({
            "email": email,
            "order_password": "secret-pass",
            "items": [{"product_id": product, "sku_id": sku, "quantity": 1}],
        })
    };
    let (a, b) = tokio::join!(
        app.call(
            "POST",
            "/api/v1/guest/orders",
            Some(body("a@example.com")),
            &Auth::None
        ),
        app.call(
            "POST",
            "/api/v1/guest/orders",
            Some(body("b@example.com")),
            &Auth::None
        ),
    );
    let ok = [&a, &b].iter().filter(|r| r["status_code"] == 0).count();
    assert_eq!(ok, 1, "exactly one wins: {a} / {b}");
    assert_eq!(app.secrets_with(product, "reserved").await, 1);
    assert_eq!(app.secrets_with(product, "available").await, 0);
}

/// ORD-03: the same for the last manual stock unit.
#[tokio::test]
async fn concurrent_orders_on_last_manual_stock() {
    let app = OrderApp::new().await;
    let (product, sku) = app
        .product(
            "stock",
            json!({"price_amount": 10, "fulfillment_type": "manual", "manual_stock_total": 1}),
        )
        .await;
    let body = |email: &str| {
        json!({
            "email": email,
            "order_password": "secret-pass",
            "items": [{"product_id": product, "sku_id": sku, "quantity": 1}],
        })
    };
    let (a, b) = tokio::join!(
        app.call(
            "POST",
            "/api/v1/guest/orders",
            Some(body("a@example.com")),
            &Auth::None
        ),
        app.call(
            "POST",
            "/api/v1/guest/orders",
            Some(body("b@example.com")),
            &Auth::None
        ),
    );
    let ok = [&a, &b].iter().filter(|r| r["status_code"] == 0).count();
    assert_eq!(ok, 1, "exactly one wins: {a} / {b}");
}

/// RFD-02: two concurrent refunds of the whole amount never refund twice.
#[tokio::test]
async fn concurrent_refunds_do_not_double_credit() {
    let app = OrderApp::new().await;
    let (uid, _, parent, _) = paid_manual_order(&app, "10.00", 1).await;
    let uri = format!("/api/v1/admin/orders/{parent}/refund-to-wallet");
    let (a, b) = tokio::join!(
        app.admin_call("POST", &uri, Some(json!({"amount": "10.00"}))),
        app.admin_call("POST", &uri, Some(json!({"amount": "10.00"}))),
    );
    let ok = [&a, &b].iter().filter(|r| r["status_code"] == 0).count();
    assert_eq!(ok, 1, "{a} / {b}");
    assert_eq!(app.balance(uid).await, "10.00");
}

/// Wallet recharges paid through the gateway callback are credited exactly once by the
/// wallet group's settlement behind the order-aware settlement.
#[tokio::test]
async fn recharge_callback_credits_once() {
    let app = OrderApp::new().await;
    let channel = app.epay_channel().await;
    let (uid, token) = app.user("r@example.com", 0).await;
    let res = app
        .call(
            "POST",
            "/api/v1/wallet/recharge",
            Some(json!({"amount": "20", "channel_id": channel})),
            &Auth::Bearer(token),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let payment_id = res["data"]["payment_id"]
        .as_i64()
        .unwrap_or_else(|| panic!("{res}"));
    assert_eq!(app.epay_callback(payment_id, "20.00").await, "success");
    assert_eq!(app.epay_callback(payment_id, "20.00").await, "success");
    assert_eq!(app.balance(uid).await, "20.00");
}
