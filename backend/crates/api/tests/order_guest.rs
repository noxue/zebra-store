//! Guest checkout end to end: create-and-pay → signed gateway callback → auto delivery job
//! → guest detail / download; duplicate and late callbacks; underpaid callbacks.

mod order_common;

use axum::http::StatusCode;
use order_common::{Auth, OrderApp};
use serde_json::{Value, json};
use zs_domain::queue::kinds;

const EMAIL: &str = "Buyer@Example.com";
const PASSWORD: &str = "secret-pass";

fn guest() -> Auth {
    Auth::Guest(EMAIL.into(), PASSWORD.into())
}

async fn create_and_pay(app: &OrderApp, product: i64, sku: i64, qty: i32, channel: i64) -> Value {
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({
                "email": EMAIL,
                "order_password": PASSWORD,
                "items": [{"product_id": product, "sku_id": sku, "quantity": qty}],
                "channel_id": channel,
            })),
            &Auth::None,
        )
        .await;
    assert_eq!(res["status_code"], 0, "create-and-pay failed: {res}");
    res["data"].clone()
}

/// ORD-02 / DLV-05 / DLV-09 / PAY-04: the full guest flow.
#[tokio::test]
async fn guest_checkout_callback_auto_delivery_and_download() {
    let app = OrderApp::new().await;
    let (product, sku) = app.product("card", json!({"price_amount": "12.50"})).await;
    app.secrets(product, sku, 3).await;
    let channel = app.epay_channel().await;

    let data = create_and_pay(&app, product, sku, 2, channel).await;
    let order_no = data["order_no"].as_str().unwrap().to_owned();
    assert_eq!(data["order"]["total_amount"], "25.00");
    assert_eq!(data["order_paid"], false);
    assert!(
        data["pay_url"]
            .as_str()
            .unwrap()
            .starts_with("https://pay.example.com"),
        "{data}"
    );
    let payment_id = data["payment_id"].as_i64().unwrap();
    // reserved for the child order
    assert_eq!(app.secrets_with(product, "reserved").await, 2);
    let parent = app.order_by_no(&order_no).await;
    assert!(
        parent.guest_password.starts_with("hmac-sha256:"),
        "ORD-02: keyed digest stored"
    );
    assert_eq!(app.jobs(kinds::ORDER_TIMEOUT_CANCEL).await.len(), 1);

    // ORD-02: credentials only from the Guest header
    let res = app
        .call(
            "GET",
            &format!("/api/v1/guest/orders/{order_no}?email={EMAIL}&order_password={PASSWORD}"),
            None,
            &Auth::None,
        )
        .await;
    assert_ne!(res["status_code"], 0);
    let wrong = Auth::Guest(EMAIL.into(), "wrong-pass".into());
    let res = app
        .call(
            "GET",
            &format!("/api/v1/guest/orders/{order_no}"),
            None,
            &wrong,
        )
        .await;
    assert_eq!(res["status_code"], 404, "{res}");

    // latest payment for the guest
    let res = app
        .call(
            "GET",
            &format!("/api/v1/guest/payments/latest?order_no={order_no}"),
            None,
            &guest(),
        )
        .await;
    assert_eq!(res["data"]["payment_id"], payment_id, "{res}");

    // signed callback pays the order
    assert_eq!(app.epay_callback(payment_id, "25.00").await, "success");
    let parent = app.order_by_no(&order_no).await;
    assert_eq!(parent.status, "paid");
    assert_eq!(app.payment(payment_id).await.status, "success");
    let children = app.children(parent.id).await;
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].status, "paid");
    let jobs = app.jobs(kinds::ORDER_AUTO_FULFILL).await;
    assert_eq!(jobs.len(), 1, "one auto delivery job per paid child");

    // PAY-04: a duplicate callback changes nothing and queues nothing new
    assert_eq!(app.epay_callback(payment_id, "25.00").await, "success");
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 1);

    // DLV-09: the job delivers the reserved secrets
    app.services
        .order
        .service
        .auto_fulfill(children[0].id)
        .await
        .unwrap();
    // running it again is a no-op (fulfillment exists)
    app.services
        .order
        .service
        .auto_fulfill(children[0].id)
        .await
        .unwrap();
    assert_eq!(app.secrets_with(product, "used").await, 2);
    assert_eq!(app.secrets_with(product, "available").await, 1);
    assert_eq!(app.order(children[0].id).await.status, "completed");
    assert_eq!(app.order_by_no(&order_no).await.status, "completed");

    let res = app
        .call(
            "GET",
            &format!("/api/v1/guest/orders/{order_no}"),
            None,
            &guest(),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(res["data"]["status"], "completed");
    assert!(res["data"].get("id").is_none(), "internal ids are hidden");
    let child = &res["data"]["children"][0];
    assert!(
        child["fulfillment"]["payload"]
            .as_str()
            .unwrap()
            .contains("CARD-"),
        "{res}"
    );

    // DLV-05: download of the child's delivery
    let child_no = children[0].order_no.clone();
    let (status, headers, body) = app
        .raw(
            "GET",
            &format!("/api/v1/guest/orders/{child_no}/fulfillment/download"),
            None,
            &guest(),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let disposition = headers
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        disposition.contains(&format!("fulfillment-{child_no}.txt")),
        "{disposition}"
    );
    assert_eq!(String::from_utf8(body).unwrap().lines().count(), 2);

    // guest list by credentials
    let res = app
        .call("GET", "/api/v1/guest/orders", None, &guest())
        .await;
    assert_eq!(res["data"].as_array().unwrap().len(), 1, "{res}");
}

/// PAY-02: a success notification that does not cover the order leaves it unpaid
/// (guests have no wallet to credit).
#[tokio::test]
async fn pay02_underpaid_callback_does_not_pay() {
    let app = OrderApp::new().await;
    let (product, sku) = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product, sku, 1).await;
    let channel = app.epay_channel().await;
    let data = create_and_pay(&app, product, sku, 1, channel).await;
    let order_no = data["order_no"].as_str().unwrap().to_owned();
    let payment_id = data["payment_id"].as_i64().unwrap();

    assert_eq!(
        app.epay_callback(payment_id, "5.00").await,
        "fail",
        "amount mismatch"
    );
    app.set_payment_amount(payment_id, "5.00").await;
    assert_eq!(app.epay_callback(payment_id, "5.00").await, "success");
    assert_eq!(app.payment(payment_id).await.status, "success");
    let order = app.order_by_no(&order_no).await;
    assert_eq!(order.status, "pending_payment");
    assert!(order.paid_at.is_none());
    assert!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.is_empty());
    assert_eq!(app.secrets_with(product, "reserved").await, 1);
}

/// ORD-01 / ORD-04 / PAY-04: the timeout job cancels an unpaid order, releases the
/// reservation, expires the open payment; a late callback never revives the order.
#[tokio::test]
async fn timeout_cancel_then_late_callback() {
    let app = OrderApp::new().await;
    let (product, sku) = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product, sku, 1).await;
    let channel = app.epay_channel().await;
    let data = create_and_pay(&app, product, sku, 1, channel).await;
    let order_no = data["order_no"].as_str().unwrap().to_owned();
    let payment_id = data["payment_id"].as_i64().unwrap();
    let parent = app.order_by_no(&order_no).await;

    // not yet expired: the job leaves it alone
    app.services
        .order
        .service
        .cancel_expired(parent.id)
        .await
        .unwrap();
    assert_eq!(app.order(parent.id).await.status, "pending_payment");

    app.expire(parent.id).await;
    app.services
        .order
        .service
        .cancel_expired(parent.id)
        .await
        .unwrap();
    assert_eq!(app.order(parent.id).await.status, "canceled");
    assert_eq!(app.children(parent.id).await[0].status, "canceled");
    assert_eq!(
        app.secrets_with(product, "available").await,
        1,
        "reservation released"
    );
    assert_eq!(app.payment(payment_id).await.status, "expired");

    app.epay_callback(payment_id, "10.00").await;
    let order = app.order(parent.id).await;
    assert_eq!(
        order.status, "canceled",
        "a late payment never revives a canceled order"
    );
    assert!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.is_empty());
}

/// ORD-01: once paid, the timeout job is a no-op (paid vs. timeout race).
#[tokio::test]
async fn ord01_paid_order_survives_timeout_job() {
    let app = OrderApp::new().await;
    let (product, sku) = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product, sku, 1).await;
    let channel = app.epay_channel().await;
    let data = create_and_pay(&app, product, sku, 1, channel).await;
    let order_no = data["order_no"].as_str().unwrap().to_owned();
    let payment_id = data["payment_id"].as_i64().unwrap();
    let parent = app.order_by_no(&order_no).await;
    app.expire(parent.id).await;

    // callback and timeout race; whichever wins, the states stay consistent
    let svc = app.services.order.service.clone();
    let id = parent.id;
    let (answer, cancel) = tokio::join!(
        app.epay_callback(payment_id, "10.00"),
        svc.cancel_expired(id)
    );
    cancel.unwrap();
    let order = app.order(parent.id).await;
    match order.status.as_str() {
        "paid" => {
            assert_eq!(answer, "success");
            assert_eq!(app.secrets_with(product, "reserved").await, 1);
        }
        "canceled" => assert_eq!(app.secrets_with(product, "available").await, 1),
        other => panic!("unexpected status {other}"),
    }
    // after the race a second timeout run changes nothing
    app.services
        .order
        .service
        .cancel_expired(parent.id)
        .await
        .unwrap();
    assert_eq!(app.order(parent.id).await.status, order.status);
}
