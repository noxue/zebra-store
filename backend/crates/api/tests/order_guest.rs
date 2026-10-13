//! Guest checkout end to end: create-and-pay → signed gateway callback → auto delivery job
//! → guest detail / download; duplicate and late callbacks; underpaid callbacks.

#![expect(
    clippy::unwrap_used,
    reason = "integration test fixtures use unwrap for concise assertions"
)]

mod integration_common;
mod order_common;
mod payment_common;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use integration_common::{SUP_KEY, SUP_SECRET, remote_product, start_supplier};
use order_common::{Auth, OrderApp};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, PaginatorTrait, QueryFilter, Set,
};
use serde_json::{Value, json};
use std::sync::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use zs_domain::queue::kinds;
use zs_infra::db::entity::{
    gateway_refund_attempts, order_items, orders, payments, procurement_orders, products,
};
use zs_infra::payment::http::{HttpResponse, MockTransport};

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
    let detail = app
        .call(
            "GET",
            &format!("/api/v1/guest/orders/{order_no}"),
            None,
            &guest(),
        )
        .await;
    assert_eq!(detail["data"]["refund_mode"], "contact_support");
    let unsupported = app
        .call(
            "POST",
            &format!("/api/v1/guest/orders/{order_no}/refund"),
            None,
            &guest(),
        )
        .await;
    assert_ne!(
        unsupported["status_code"], 0,
        "unsupported channel must not submit a refund"
    );
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

/// FE payment-method selection: one Epay connection publishes all explicitly enabled choices.
#[tokio::test]
async fn epay_multiple_methods_are_published_for_the_storefront() {
    let app = OrderApp::new().await;
    let created = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name": "Epay",
                "provider_type": "epay",
                "channel_type": "wechat",
                "interaction_mode": "qr",
                "payment_roles": ["guest", "member"],
                "payment_types": ["order"],
                "config_json": {
                    "epay_version": "v1",
                    "gateway_url": "https://pay.example.com",
                    "merchant_id": "M-1",
                    "merchant_key": "test-key",
                    "notify_url": "https://shop.example.com/api/v1/payments/callback",
                    "return_url": "https://shop.example.com/pay",
                    "supported_channel_types": ["wechat", "alipay", "qqpay", "card"]
                }
            })),
        )
        .await;
    assert_eq!(created["status_code"], 0, "{created}");
    let channel_id = created["data"]["id"].as_i64().unwrap();
    let public = app
        .call("GET", "/api/v1/public/config", None, &Auth::None)
        .await;
    let channel = public["data"]["payment_channels"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == channel_id))
        .expect("Epay channel should be public");
    assert_eq!(
        channel["supported_channel_types"],
        json!(["wechat", "alipay", "qqpay"])
    );
}

/// PAY-52: one Huifu connection accepts both configured methods and rejects unconfigured types.
#[tokio::test]
async fn huifu_multi_method_selection_is_sent_and_persisted_per_payment() {
    let methods = Arc::new(Mutex::new(Vec::new()));
    let observed = methods.clone();
    let transport = MockTransport::new(move |request| {
        assert_eq!(
            request.header_value("jpt-x-skill-source"),
            Some("hfps/1.3.1;sandbox/1.0.0"),
            "Loopback Huifu sandbox calls must use the sandbox-specific source"
        );
        let body: Value =
            serde_json::from_slice(&request.body).map_err(|error| error.to_string())?;
        assert_eq!(
            body["data"]["trans_amt"], "0.01",
            "the minimum supported amount must be sent without rounding up"
        );
        assert!(
            body["data"].get("appid").is_none() && body["data"].get("app_id").is_none(),
            "WeChat AppID is bound in Huifu merchant configuration, not supplied by Zebra Store"
        );
        observed.lock().map_err(|error| error.to_string())?.push(
            body["data"]["trans_type"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        );
        let data = json!({
            "resp_code": "00000000",
            "resp_desc": "success",
            "jump_url": "https://pay.example.com/huifu"
        });
        let sign = huifu_pay::sign_value(payment_common::PRIV_PEM, &data)
            .map_err(|error| error.to_string())?;
        Ok(HttpResponse::new(
            200,
            json!({"data": data, "sign": sign}).to_string(),
        ))
    });
    let app = OrderApp::with_transport(Arc::new(transport)).await;
    let (product, sku) = app
        .product("huifu-methods-card", json!({"price_amount": "0.01"}))
        .await;
    app.secrets(product, sku, 2).await;
    let created = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name": "Huifu",
                "provider_type": "huifu",
                "channel_type": "wechat",
                "interaction_mode": "qr",
                "payment_roles": ["guest", "member"],
                "payment_types": ["order"],
                "config_json": {
                    "api_base_url": "http://127.0.0.1:18766",
                    "sys_id": "SYS-1",
                    "product_id": "PROD-1",
                    "huifu_id": "HU-1",
                    "merchant_private_key": payment_common::PRIV_PEM,
                    "huifu_public_key": payment_common::PUB_PEM,
                    "project_id": "PROJECT-1",
                    "skill_source": "caller-controlled/9",
                    "notify_url": "https://shop.example.com/api/v1/payments/callback",
                    "return_url": "https://shop.example.com/pay",
                    "supported_channel_types": ["wechat", "alipay"]
                }
            })),
        )
        .await;
    assert_eq!(created["status_code"], 0, "{created}");
    let channel = created["data"]["id"].as_i64().unwrap();
    let public = app
        .call("GET", "/api/v1/public/config", None, &Auth::None)
        .await;
    let public_channel = public["data"]["payment_channels"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == channel))
        .expect("Huifu methods must be available to the storefront");
    assert_eq!(
        public_channel["supported_channel_types"],
        json!(["wechat", "alipay"])
    );
    assert!(public_channel.get("merchant_private_key").is_none());

    for (method, expected_type) in [("alipay", "A_NATIVE"), ("wechat", "T_JSAPI")] {
        let payment = app
            .call(
                "POST",
                "/api/v1/guest/orders/create-and-pay",
                Some(json!({
                    "email": format!("{method}@example.com"),
                    "order_password": PASSWORD,
                    "items": [{"product_id": product, "sku_id": sku, "quantity": 1}],
                    "channel_id": channel,
                    "channel_type": method
                })),
                &Auth::None,
            )
            .await;
        assert_eq!(payment["status_code"], 0, "{payment}");
        assert_eq!(payment["data"]["channel_type"], method);
        assert_eq!(
            methods.lock().unwrap().last().map(String::as_str),
            Some(expected_type)
        );
    }

    let rejected = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({
                "email": "unsupported@example.com",
                "order_password": PASSWORD,
                "items": [{"product_id": product, "sku_id": sku, "quantity": 1}],
                "channel_id": channel,
                "channel_type": "qqpay"
            })),
            &Auth::None,
        )
        .await;
    assert_ne!(
        rejected["status_code"], 0,
        "unconfigured type must be rejected: {rejected}"
    );
    assert_eq!(
        methods.lock().unwrap().len(),
        2,
        "invalid method must not contact Huifu"
    );
}

/// PAY-04/PAY-14: a signed direct Alipay QR query settles the real guest order once.
#[tokio::test]
async fn huifu_direct_alipay_qr_capture_pays_guest_order() {
    let app = OrderApp::with_transport(Arc::new(MockTransport::new(|request| {
        let body: Value = serde_json::from_slice(&request.body).map_err(|error| error.to_string())?;
        let data = &body["data"];
        let response = if request.url.ends_with("/v2/trade/payment/jspay") {
            assert_eq!(data["trade_type"], "A_NATIVE");
            assert_eq!(data["trans_amt"], "0.01");
            json!({"resp_code":"00000100","resp_desc":"下单成功","qr_code":"https://qr.alipay.com/guest-test","trans_stat":"P"})
        } else if request.url.ends_with("/v2/trade/payment/scanpay/query") {
            assert!(data["org_req_seq_id"].as_str().is_some_and(|value| value.starts_with("DJP")));
            json!({"resp_code":"00000000","trans_stat":"S","trans_amt":"0.01"})
        } else {
            return Err(format!("unexpected request: {}", request.url));
        };
        let sign = huifu_pay::sign_value(payment_common::PRIV_PEM, &response)
            .map_err(|error| error.to_string())?;
        Ok(HttpResponse::new(200, json!({"data":response,"sign":sign}).to_string()))
    }))).await;
    let (product, sku) = app
        .product("huifu-direct-card", json!({"price_amount":"0.01"}))
        .await;
    app.secrets(product, sku, 1).await;
    let channel = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name": "Huifu direct Alipay",
                "provider_type": "huifu",
                "channel_type": "alipay",
                "interaction_mode": "qr",
                "payment_roles": ["guest", "member"],
                "payment_types": ["order"],
                "config_json": {
                    "collection_mode": "direct_qr",
                    "api_base_url": "http://127.0.0.1:18766",
                    "sys_id": "SYS-1",
                    "product_id": "PROD-1",
                    "huifu_id": "HU-1",
                    "merchant_private_key": payment_common::PRIV_PEM,
                    "huifu_public_key": payment_common::PUB_PEM,
                    "notify_url": "https://shop.example.com/api/v1/payments/callback"
                }
            })),
        )
        .await;
    assert_eq!(channel["status_code"], 0, "{channel}");
    let paid = create_and_pay(
        &app,
        product,
        sku,
        1,
        channel["data"]["id"].as_i64().unwrap(),
    )
    .await;
    assert_eq!(paid["qr_code"], "https://qr.alipay.com/guest-test");
    let payment_id = paid["payment_id"].as_i64().unwrap();
    let captured = app
        .call(
            "POST",
            &format!("/api/v1/guest/payments/{payment_id}/capture"),
            None,
            &guest(),
        )
        .await;
    assert_eq!(captured["status_code"], 0, "{captured}");
    assert_eq!(captured["data"]["status"], "success");
    let order = app.order_by_no(paid["order_no"].as_str().unwrap()).await;
    assert_eq!(order.status, "paid");
}

/// Original-route refunds persist the submitted refund locator before the gateway call. A retry
/// queries that locator instead of issuing a second refund, then commits one local refund record.
#[tokio::test]
async fn huifu_pending_original_refund_is_queried_and_applied_once() {
    let refund_calls = Arc::new(AtomicUsize::new(0));
    let query_calls = Arc::new(AtomicUsize::new(0));
    let refunds = refund_calls.clone();
    let queries = query_calls.clone();
    let transport = MockTransport::new(move |request| {
        let path = request.url.as_str();
        let status = if path.ends_with(huifu_pay::REFUND_PATH) {
            refunds.fetch_add(1, Ordering::SeqCst);
            "P"
        } else if path.ends_with(huifu_pay::REFUND_QUERY_PATH) {
            queries.fetch_add(1, Ordering::SeqCst);
            "S"
        } else if path.ends_with(huifu_pay::PREORDER_PATH) {
            "S"
        } else {
            return Err(format!("unexpected Huifu endpoint: {path}"));
        };
        let data = if path.ends_with(huifu_pay::PREORDER_PATH) {
            json!({
                "resp_code": "00000000",
                "resp_desc": "success",
                "jump_url": "https://pay.example.com/huifu"
            })
        } else {
            json!({
                "resp_code": "00000000",
                "resp_desc": "success",
                "trans_stat": status
            })
        };
        let sign = huifu_pay::sign_value(payment_common::PRIV_PEM, &data)
            .map_err(|error| error.to_string())?;
        Ok(HttpResponse::new(
            200,
            json!({"data": data, "sign": sign}).to_string(),
        ))
    });
    let app = OrderApp::with_transport(Arc::new(transport)).await;
    let (product, sku) = app
        .product("huifu-refund-card", json!({"price_amount": "0.01"}))
        .await;
    app.secrets(product, sku, 1).await;
    let channel = app
        .admin_call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(json!({
                "name": "Huifu",
                "provider_type": "huifu",
                "channel_type": "alipay",
                "interaction_mode": "redirect",
                "payment_roles": ["guest"],
                "payment_types": ["order"],
                "config_json": {
                    "sys_id": "SYS-1",
                    "product_id": "PROD-1",
                    "huifu_id": "HU-1",
                    "merchant_private_key": payment_common::PRIV_PEM,
                    "huifu_public_key": payment_common::PUB_PEM,
                    "project_id": "PROJECT-1",
                    "notify_url": "https://shop.example.com/api/v1/payments/callback",
                    "return_url": "https://shop.example.com/pay",
                    "supported_channel_types": ["alipay"]
                }
            })),
        )
        .await["data"]["id"]
        .as_i64()
        .unwrap();
    let created = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({
                "email": EMAIL,
                "order_password": PASSWORD,
                "items": [{"product_id": product, "sku_id": sku, "quantity": 1}],
                "channel_id": channel,
                "channel_type": "alipay"
            })),
            &Auth::None,
        )
        .await;
    assert_eq!(created["status_code"], 0, "{created}");
    let payment_id = created["data"]["payment_id"].as_i64().unwrap();
    let payment = payments::Entity::find_by_id(payment_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let order_id = payment.order_id;
    let paid_at = chrono::Utc::now();
    let mut payment = payment.into_active_model();
    payment.status = Set("success".into());
    payment.paid_at = Set(Some(paid_at));
    payment.update(&app.db).await.unwrap();
    let order = orders::Entity::find_by_id(order_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let mut order = order.into_active_model();
    order.status = Set("paid".into());
    order.paid_at = Set(Some(paid_at));
    order.online_paid_amount = Set(order_common::dec("0.01"));
    order.update(&app.db).await.unwrap();

    let detail = app
        .call(
            "GET",
            &format!(
                "/api/v1/guest/orders/{}",
                created["data"]["order_no"].as_str().unwrap()
            ),
            None,
            &guest(),
        )
        .await;
    assert_eq!(detail["data"]["refund_mode"], "automatic");
    // A final provider failure must stop buyer retries until staff reviews it.
    let failed = gateway_refund_attempts::ActiveModel {
        order_id: Set(order_id),
        payment_id: Set(payment_id),
        channel_id: Set(channel),
        request_date: Set("20261014".into()),
        refund_no: Set("FAILED-BUYER-REFUND".into()),
        provider_ref: Set("20261014:FAILED-BUYER-REFUND".into()),
        amount: Set(order_common::dec("0.01")),
        status: Set("failed".into()),
        remark: Set("buyer_requested_refund".into()),
        payment_fee_refunded: Set(false),
        card_confirmed: Set(false),
        payload: Set(None),
        error: Set("provider rejected".into()),
        refund_record_id: Set(None),
        created_at: Set(paid_at),
        updated_at: Set(paid_at),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let failed_detail = app
        .call(
            "GET",
            &format!(
                "/api/v1/guest/orders/{}",
                created["data"]["order_no"].as_str().unwrap()
            ),
            None,
            &guest(),
        )
        .await;
    assert_eq!(failed_detail["data"]["refund_mode"], "contact_support");
    assert_eq!(failed_detail["data"]["refund_state"], "needs_review");
    let buyer_retry = app
        .call(
            "POST",
            &format!(
                "/api/v1/guest/orders/{}/refund",
                created["data"]["order_no"].as_str().unwrap()
            ),
            None,
            &guest(),
        )
        .await;
    assert_ne!(buyer_retry["status_code"], 0);
    assert_eq!(refund_calls.load(Ordering::SeqCst), 0);
    gateway_refund_attempts::Entity::delete_by_id(failed.id)
        .exec(&app.db)
        .await
        .unwrap();
    let denied = app
        .call(
            "POST",
            &format!(
                "/api/v1/guest/orders/{}/refund",
                created["data"]["order_no"].as_str().unwrap()
            ),
            None,
            &Auth::Guest(EMAIL.into(), "wrong-password".into()),
        )
        .await;
    assert_ne!(denied["status_code"], 0);
    let request = json!({"amount": "0.01", "remark": "buyer_requested_refund"});
    let first = app
        .call(
            "POST",
            &format!(
                "/api/v1/guest/orders/{}/refund",
                created["data"]["order_no"].as_str().unwrap()
            ),
            None,
            &guest(),
        )
        .await;
    assert_eq!(first["status_code"], 0, "{first}");
    assert_eq!(first["data"]["status"], "pending");
    let guest_path = format!(
        "/api/v1/guest/orders/{}/refund",
        created["data"]["order_no"].as_str().unwrap()
    );
    let admin_path = format!("/api/v1/admin/orders/{order_id}/original-refund");
    let buyer_auth = guest();
    let (buyer_retry, admin_retry) = tokio::join!(
        app.call("POST", &guest_path, None, &buyer_auth),
        app.admin_call("POST", &admin_path, Some(request.clone()))
    );
    assert_eq!(buyer_retry["status_code"], 0, "{buyer_retry}");
    assert_eq!(admin_retry["status_code"], 0, "{admin_retry}");
    assert_eq!(refund_calls.load(Ordering::SeqCst), 1);
    assert_eq!(query_calls.load(Ordering::SeqCst), 0);
    let attempt_id = gateway_refund_attempts::Entity::find()
        .filter(gateway_refund_attempts::Column::OrderId.eq(order_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    gateway_refund_attempts::Entity::update_many()
        .col_expr(
            gateway_refund_attempts::Column::CreatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now() - chrono::Duration::seconds(31)),
        )
        .col_expr(
            gateway_refund_attempts::Column::UpdatedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now() - chrono::Duration::seconds(31)),
        )
        .filter(gateway_refund_attempts::Column::Id.eq(attempt_id))
        .exec(&app.db)
        .await
        .unwrap();
    app.services
        .order
        .service
        .reconcile_gateway_refunds()
        .await
        .unwrap();

    let second = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{order_id}/original-refund"),
            Some(request.clone()),
        )
        .await;
    assert_eq!(second["status_code"], 0, "{second}");
    assert_eq!(second["data"]["attempt"]["status"], "succeeded");
    assert_eq!(second["data"]["order"]["refunded_amount"], "0.01");
    assert!(
        gateway_refund_attempts::Entity::find_by_id(attempt_id)
            .one(&app.db)
            .await
            .unwrap()
            .unwrap()
            .card_confirmed
    );

    let replay = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{order_id}/original-refund"),
            Some(request),
        )
        .await;
    assert_eq!(
        replay["status_code"], 0,
        "successful refund replay: {replay}"
    );
    assert_eq!(replay["data"]["attempt"]["status"], "succeeded");
    assert_eq!(replay["data"]["order"]["refunded_amount"], "0.01");
    assert_eq!(
        replay["data"]["refund_record"]["id"], second["data"]["refund_record"]["id"],
        "idempotent replay must return the original refund record"
    );
    assert_eq!(refund_calls.load(Ordering::SeqCst), 1);
    assert_eq!(query_calls.load(Ordering::SeqCst), 1);
}

#[derive(Default)]
struct ConverterMock {
    fail: AtomicBool,
    exchanges: AtomicUsize,
    keys: std::sync::Mutex<Vec<String>>,
}

async fn converter_health() -> Json<Value> {
    Json(json!({"ok": true}))
}

async fn converter_exchange(
    State(mock): State<Arc<ConverterMock>>,
    Json(request): Json<Value>,
) -> axum::response::Response {
    assert_eq!(request.as_object().map(|body| body.len()), Some(3));
    assert!(request.get("order_no").and_then(Value::as_str).is_some());
    assert!(request.get("items").is_some());
    assert!(request.get("extra").is_some());
    assert!(request.get("protocol_version").is_none());
    assert!(request.get("idempotency_key").is_none());
    for field in ["order", "site", "product", "sku"] {
        assert!(request.get(field).is_none());
    }
    assert_eq!(
        request["extra"].as_object().map(|extra| extra.len()),
        Some(2)
    );
    let first_card = request["items"][0]["upstream_card"]
        .as_str()
        .unwrap_or_default();
    assert_eq!(
        request["extra"]["provider"],
        if first_card.starts_with("CARD-") {
            "p1"
        } else {
            "manual_review"
        }
    );
    assert!(
        request["extra"]["chosen_order"]
            .as_str()
            .is_some_and(|value| !value.is_empty() && !value.contains("{{"))
    );
    mock.exchanges.fetch_add(1, Ordering::SeqCst);
    mock.keys
        .lock()
        .unwrap()
        .push(request["order_no"].as_str().unwrap().to_owned());
    if mock.fail.load(Ordering::SeqCst) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"error":{"code":"temporarily_unavailable"}})),
        )
            .into_response();
    }
    let items = request["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            json!({
                "index": item["index"],
                "card": format!("SYSTEM-{}", item["upstream_card"].as_str().unwrap()),
            })
        })
        .collect::<Vec<_>>();
    (StatusCode::OK, Json(json!({"items":items}))).into_response()
}

async fn mixed_system_and_raw_exchange(
    State(mock): State<Arc<ConverterMock>>,
    Json(request): Json<Value>,
) -> Json<Value> {
    let items = request["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["index"], 0);
    assert_eq!(items[0]["upstream_card"], "RAW-MIXED");
    assert_eq!(request["extra"]["provider"], "p3");
    mock.exchanges.fetch_add(1, Ordering::SeqCst);
    Json(json!({"items":[{"index":0,"card":"SYSTEM-RAW-MIXED"}]}))
}

/// A failed conversion holds the exact stock cards and retry resumes only conversion.
#[tokio::test]
async fn local_card_converter_retry_does_not_consume_stock_twice() {
    use sea_orm::{ActiveModelTrait, Set};
    use zs_domain::integration::card_converter::Binding;

    let mock = Arc::new(ConverterMock {
        fail: AtomicBool::new(true),
        ..Default::default()
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/health", get(converter_health))
        .route("/integration/v1/exchanges", post(converter_exchange))
        .with_state(mock.clone());
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });

    let app = OrderApp::with_local_converter().await;
    let (product, sku) = app.product("converted", json!({})).await;
    app.services
        .catalog
        .card_secret
        .create_batch(
            zs_app::catalog::card_secret::CreateSecretsInput {
                product_id: product,
                sku_id: sku,
                secrets: (0..3).map(|i| format!("CARD-{product}-{i}")).collect(),
                note: "渠道 A".into(),
                ..Default::default()
            },
            zs_domain::catalog::card_secret::BatchSource::Manual,
        )
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let token_enc = app
        .ctx
        .cipher
        .encrypt("test-converter-token-with-sufficient-length")
        .unwrap();
    let converter = zs_infra::db::entity::extra::card_converters::ActiveModel {
        name: Set("test converter".into()),
        base_url: Set(format!("http://{address}")),
        token_enc: Set(token_enc),
        enabled: Set(true),
        health: Set("unknown".into()),
        consecutive_failures: Set(0),
        last_checked_at: Set(None),
        last_error: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let converter_id = converter.id;
    app.services
        .integration
        .card_converters
        .save_binding(Binding {
            id: 0,
            product_id: product,
            sku_id: sku,
            converter_id,
            type_id: "account".into(),
            extra_template: json!({
                "chosen_order":"{{order.order_no}}",
                "provider":{"$zs_rule":{"cases":[
                    {"when":{"variable":"card.notes","operator":"eq","value":"渠道 A"},"then":{"literal":"p1"}}
                ],"else":{"literal":"manual_review"}}}
            }),
            issues: Vec::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .await
        .unwrap();

    let channel = app.epay_channel().await;
    let order = create_and_pay(&app, product, sku, 2, channel).await;
    let order_no = order["order_no"].as_str().unwrap().to_owned();
    let payment_id = order["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment_id, "20.00").await, "success");
    let child = app
        .children(app.order_by_no(&order_no).await.id)
        .await
        .remove(0);

    assert!(
        app.services
            .order
            .service
            .auto_fulfill(child.id)
            .await
            .is_err()
    );
    let held = app.order(child.id).await;
    assert_eq!(held.status, "fulfilling");
    assert_eq!(app.secrets_with(product, "used").await, 2);
    assert_eq!(app.secrets_with(product, "available").await, 1);
    assert_eq!(mock.exchanges.load(Ordering::SeqCst), 1);

    mock.fail.store(false, Ordering::SeqCst);
    app.services
        .order
        .service
        .retry_pending_conversions()
        .await
        .unwrap();
    let final_order = app.order(child.id).await;
    assert_eq!(final_order.status, "completed");
    let fulfillment = zs_infra::db::entity::fulfillments::Entity::find()
        .filter(zs_infra::db::entity::fulfillments::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        fulfillment.payload,
        format!("SYSTEM-CARD-{product}-0\nSYSTEM-CARD-{product}-1")
    );
    assert_eq!(mock.exchanges.load(Ordering::SeqCst), 2);
    assert_eq!(
        *mock.keys.lock().unwrap(),
        vec![final_order.order_no.clone(); 2]
    );
    assert_eq!(app.secrets_with(product, "used").await, 2);

    let manual_order = create_and_pay(&app, product, sku, 1, channel).await;
    let manual_no = manual_order["order_no"].as_str().unwrap().to_owned();
    let manual_payment_id = manual_order["payment_id"].as_i64().unwrap();
    assert_eq!(
        app.epay_callback(manual_payment_id, "10.00").await,
        "success"
    );
    let manual_child = app
        .children(app.order_by_no(&manual_no).await.id)
        .await
        .remove(0);
    let manual = app
        .services
        .order
        .service
        .manual_fulfill(manual_child.id, 1, "MANUAL-RAW-CARD", &Default::default())
        .await
        .unwrap();
    assert_eq!(manual.payload, "SYSTEM-MANUAL-RAW-CARD");
    assert_eq!(mock.exchanges.load(Ordering::SeqCst), 3);
}

/// DLV-03: a preconverted system card is delivered as-is even when the SKU has a converter.
#[tokio::test]
async fn preconverted_local_card_skips_bound_converter() {
    use sea_orm::{ActiveModelTrait, Set};
    use zs_domain::integration::card_converter::Binding;

    let mock = Arc::new(ConverterMock::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Router::new()
        .route("/health", get(converter_health))
        .route(
            "/integration/v1/exchanges",
            post(mixed_system_and_raw_exchange),
        )
        .with_state(mock.clone());
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let app = OrderApp::with_local_converter().await;
    let (product, sku) = app.product("ready-system-card", json!({})).await;
    app.services
        .catalog
        .card_secret
        .create_batch(
            zs_app::catalog::card_secret::CreateSecretsInput {
                product_id: product,
                sku_id: sku,
                secrets: vec!["SYSTEM-READY-CARD".into()],
                note: "p3".into(),
                preconverted: true,
                ..Default::default()
            },
            zs_domain::catalog::card_secret::BatchSource::Manual,
        )
        .await
        .unwrap();
    let now = chrono::Utc::now();
    let converter = zs_infra::db::entity::extra::card_converters::ActiveModel {
        name: Set("test converter".into()),
        base_url: Set(format!("http://{address}")),
        token_enc: Set(app
            .ctx
            .cipher
            .encrypt("test-converter-token-with-sufficient-length")
            .unwrap()),
        enabled: Set(true),
        health: Set("unknown".into()),
        consecutive_failures: Set(0),
        last_checked_at: Set(None),
        last_error: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    app.services
        .integration
        .card_converters
        .save_binding(Binding {
            id: 0,
            product_id: product,
            sku_id: sku,
            converter_id: converter.id,
            type_id: "account".into(),
            extra_template: json!({"provider":"p3"}),
            issues: Vec::new(),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let channel = app.epay_channel().await;
    let order = create_and_pay(&app, product, sku, 1, channel).await;
    let order_no = order["order_no"].as_str().unwrap();
    let payment_id = order["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment_id, "10.00").await, "success");
    let child = app
        .children(app.order_by_no(order_no).await.id)
        .await
        .remove(0);
    app.services
        .order
        .service
        .auto_fulfill(child.id)
        .await
        .unwrap();
    assert_eq!(app.order(child.id).await.status, "completed");
    let fulfillment = zs_infra::db::entity::fulfillments::Entity::find()
        .filter(zs_infra::db::entity::fulfillments::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fulfillment.payload, "SYSTEM-READY-CARD");
    assert_eq!(mock.exchanges.load(Ordering::SeqCst), 0);
    assert_eq!(app.secrets_with(product, "used").await, 1);

    // A second order may contain both types: only the raw card goes to the
    // converter, and the buyer receives both cards in their original order.
    for (secret, preconverted) in [("SYSTEM-READY-MIXED", true), ("RAW-MIXED", false)] {
        app.services
            .catalog
            .card_secret
            .create_batch(
                zs_app::catalog::card_secret::CreateSecretsInput {
                    product_id: product,
                    sku_id: sku,
                    secrets: vec![secret.into()],
                    note: "p3".into(),
                    preconverted,
                    ..Default::default()
                },
                zs_domain::catalog::card_secret::BatchSource::Manual,
            )
            .await
            .unwrap();
    }
    let order = create_and_pay(&app, product, sku, 2, channel).await;
    let order_no = order["order_no"].as_str().unwrap();
    let payment_id = order["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment_id, "20.00").await, "success");
    let child = app
        .children(app.order_by_no(order_no).await.id)
        .await
        .remove(0);
    app.services
        .order
        .service
        .auto_fulfill(child.id)
        .await
        .unwrap();
    assert_eq!(app.order(child.id).await.status, "completed");
    let fulfillment = zs_infra::db::entity::fulfillments::Entity::find()
        .filter(zs_infra::db::entity::fulfillments::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fulfillment.payload, "SYSTEM-READY-MIXED\nSYSTEM-RAW-MIXED");
    assert_eq!(mock.exchanges.load(Ordering::SeqCst), 1);
    assert_eq!(app.secrets_with(product, "used").await, 3);
}

async fn hybrid_exchange(Json(request): Json<Value>) -> Json<Value> {
    assert_eq!(request["extra"]["provider"], "p1");
    assert_eq!(request["extra"]["category"], "new_activation_go");
    let item = &request["items"][0];
    assert_eq!(item["upstream_card"], "P1-RAW-UNIQUE-CARD");
    Json(json!({"items":[{"index":0,"card":"P1-SYSTEM-CARD"}]}))
}

// DLV-03: a P1 import on an upstream mapped product takes the local route;
// unmarked inventory is not selected and no supplier order is created.
#[tokio::test]
async fn mapped_product_prefers_marked_p1_import_and_delivers_converted_card() {
    use zs_domain::integration::card_converter::Binding;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/health", get(converter_health))
                .route("/integration/v1/exchanges", post(hybrid_exchange)),
        )
        .await
        .unwrap();
    });
    let real_ai = local_ai_topup();
    let app = OrderApp::with_local_converter().await;
    let (supplier_url, supplier) = start_supplier().await;
    supplier.set_products(vec![remote_product(
        9,
        "10.00",
        json!([{
            "id":101,"sku_code":"DEFAULT","price_amount":"10.00",
            "stock_quantity":5,"is_active":true
        }]),
    )]);
    let connected = app.admin_call("POST", "/api/v1/admin/site-connections", Some(json!({
        "name":"Fake P2","base_url":supplier_url,"api_key":SUP_KEY,"api_secret":SUP_SECRET,
        "protocol":"dujiao-next","callback_url":"https://buyer.example.com/api/v1/upstream/callback"
    }))).await;
    assert_eq!(connected["status_code"], 0, "{connected}");
    let conn = connected["data"]["id"].as_i64().unwrap();
    let active = app
        .admin_call(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}/status"),
            Some(json!({"status":"active"})),
        )
        .await;
    assert_eq!(active["status_code"], 0, "{active}");
    let imported = app
        .admin_call(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id":conn,"upstream_product_id":9})),
        )
        .await;
    assert_eq!(imported["status_code"], 0, "{imported}");
    let product = imported["data"]["local_product_id"].as_i64().unwrap();
    products::ActiveModel {
        id: Set(product),
        category_id: Set(app.category),
        is_active: Set(true),
        purchase_type: Set("guest".into()),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let sku = zs_infra::db::entity::product_skus::Entity::find()
        .filter(zs_infra::db::entity::product_skus::Column::ProductId.eq(product))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    for (note, secret) in [("p1", "P1-RAW-UNIQUE-CARD"), ("", "UNKNOWN-RAW-CARD")] {
        app.services
            .catalog
            .card_secret
            .create_batch(
                zs_app::catalog::card_secret::CreateSecretsInput {
                    product_id: product,
                    sku_id: sku,
                    secrets: vec![secret.into()],
                    note: note.into(),
                    ..Default::default()
                },
                zs_domain::catalog::card_secret::BatchSource::Manual,
            )
            .await
            .unwrap();
    }
    let now = chrono::Utc::now();
    let converter = zs_infra::db::entity::extra::card_converters::ActiveModel {
        name: Set("AI Top-up mock".into()),
        base_url: Set(real_ai
            .as_ref()
            .map_or_else(|| format!("http://{address}"), |(url, _)| url.clone())),
        token_enc: Set(app
            .ctx
            .cipher
            .encrypt(real_ai.as_ref().map_or(
                "test-converter-token-with-sufficient-length",
                |(_, token)| token,
            ))
            .unwrap()),
        enabled: Set(true),
        health: Set("unknown".into()),
        consecutive_failures: Set(0),
        last_checked_at: Set(None),
        last_error: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    app.services
        .integration
        .card_converters
        .save_binding(Binding {
            id: 0,
            product_id: product,
            sku_id: 0,
            converter_id: converter.id,
            type_id: String::new(),
            extra_template: json!({"category":"new_activation_go","provider":{"$zs_rule":{"cases":[
            {"when":{"variable":"card.notes","operator":"eq","value":"p1"},"then":{"literal":"p1"}},
            {"when":{"variable":"source.id","operator":"eq","value":conn.to_string()},"then":{"literal":"p2"}}
        ],"else":{"literal":""}}}}),
            issues: Vec::new(),
            created_at: now,
            updated_at: now,
        })
        .await
        .unwrap();

    let channel = app.epay_channel().await;
    let created = create_and_pay(&app, product, sku, 1, channel).await;
    let parent = app.order_by_no(created["order_no"].as_str().unwrap()).await;
    let child = app.children(parent.id).await.remove(0);
    let item = order_items::Entity::find()
        .filter(order_items::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.fulfillment_type, "auto");
    assert_eq!(app.secrets_with(product, "reserved").await, 1);
    assert_eq!(
        app.epay_callback(created["payment_id"].as_i64().unwrap(), "10.00")
            .await,
        "success"
    );
    app.services
        .order
        .service
        .auto_fulfill(child.id)
        .await
        .unwrap();
    let delivered = app.order(child.id).await;
    assert_eq!(delivered.status, "completed");
    let fulfillment = zs_infra::db::entity::fulfillments::Entity::find()
        .filter(zs_infra::db::entity::fulfillments::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    if let Some((base, _)) = &real_ai {
        verify_real_ai_topup(base, &fulfillment.payload, "p1").await;
    } else {
        assert_eq!(fulfillment.payload, "P1-SYSTEM-CARD");
    }
    assert_eq!(app.secrets_with(product, "available").await, 1);
    assert_eq!(
        procurement_orders::Entity::find()
            .count(&app.db)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        supplier
            .hits()
            .iter()
            .filter(|hit| hit.path == "/api/v1/upstream/orders")
            .count(),
        0
    );
}

async fn p2_hybrid_exchange(Json(request): Json<Value>) -> Json<Value> {
    assert_eq!(request["extra"]["provider"], "p2");
    assert_eq!(request["extra"]["category"], "new_activation_go");
    assert_eq!(request["items"][0]["upstream_card"], "P2-RAW-CARD");
    Json(json!({"items":[{"index":0,"card":"P2-SYSTEM-CARD"}]}))
}

fn local_ai_topup() -> Option<(String, String)> {
    let url = std::env::var("ZS_TEST_AI_TOPUP_URL").ok()?;
    let token = std::env::var("ZS_TEST_AI_TOPUP_TOKEN").ok()?;
    Some((url.trim_end_matches('/').to_owned(), token))
}

async fn verify_real_ai_topup(base: &str, system_card: &str, provider: &str) {
    assert!(system_card.starts_with(&format!("{}-", provider.to_ascii_uppercase())));
    let client = reqwest::Client::new();
    let validated: Value = client
        .post(format!("{base}/api/topups/validate"))
        .json(&json!({"user_key":system_card}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(validated["valid"], true, "{validated}");
    assert_eq!(validated["category"], "new_activation_go");
    let submitted: Value = client.post(format!("{base}/api/topups"))
        .json(&json!({"user_key":system_card,"session":r#"{"accessToken":"fake","sessionToken":"fake","user":{"email":"flow@example.invalid"},"expires":"2099-01-01T00:00:00Z"}"#}))
        .send().await.unwrap().json().await.unwrap();
    assert!(submitted["trackingId"].as_str().is_some(), "{submitted}");
    for _ in 0..100 {
        let history: Value = client
            .post(format!("{base}/api/topups/query"))
            .json(&json!({"user_key":system_card}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if history["items"][0]["status"] == "success" {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("AI Top-up did not complete mock {provider} redemption");
}

// UPS-07 / DLV-03: with no P1 stock the same mapped product buys from P2,
// converts the supplier's original card, and exposes only the system card.
#[tokio::test]
async fn mapped_product_falls_back_to_fake_p2_supplier_and_delivers_converted_card() {
    use zs_domain::integration::card_converter::Binding;

    let (supplier_url, supplier) = start_supplier().await;
    supplier.set_products(vec![remote_product(
        9,
        "10.00",
        json!([{
            "id":101,"sku_code":"DEFAULT","price_amount":"10.00",
            "stock_quantity":5,"is_active":true
        }]),
    )]);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let converter_address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new()
                .route("/health", get(converter_health))
                .route("/integration/v1/exchanges", post(p2_hybrid_exchange)),
        )
        .await
        .unwrap();
    });
    let real_ai = local_ai_topup();
    let app = OrderApp::with_local_converter().await;
    let connected = app.admin_call("POST", "/api/v1/admin/site-connections", Some(json!({
        "name":"Fake P2","base_url":supplier_url,"api_key":SUP_KEY,"api_secret":SUP_SECRET,
        "protocol":"dujiao-next","callback_url":"https://buyer.example.com/api/v1/upstream/callback"
    }))).await;
    assert_eq!(connected["status_code"], 0, "{connected}");
    let conn = connected["data"]["id"].as_i64().unwrap();
    let active = app
        .admin_call(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}/status"),
            Some(json!({"status":"active"})),
        )
        .await;
    assert_eq!(active["status_code"], 0, "{active}");
    let imported = app
        .admin_call(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({
                "connection_id":conn,"upstream_product_id":9
            })),
        )
        .await;
    assert_eq!(imported["status_code"], 0, "{imported}");
    let product = imported["data"]["local_product_id"].as_i64().unwrap();
    products::ActiveModel {
        id: Set(product),
        category_id: Set(app.category),
        is_active: Set(true),
        purchase_type: Set("guest".into()),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let sku = zs_infra::db::entity::product_skus::Entity::find()
        .filter(zs_infra::db::entity::product_skus::Column::ProductId.eq(product))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    let now = chrono::Utc::now();
    let converter = zs_infra::db::entity::extra::card_converters::ActiveModel {
        name: Set("AI Top-up mock".into()),
        base_url: Set(real_ai.as_ref().map_or_else(
            || format!("http://{converter_address}"),
            |(url, _)| url.clone(),
        )),
        token_enc: Set(app
            .ctx
            .cipher
            .encrypt(real_ai.as_ref().map_or(
                "test-converter-token-with-sufficient-length",
                |(_, token)| token,
            ))
            .unwrap()),
        enabled: Set(true),
        health: Set("unknown".into()),
        consecutive_failures: Set(0),
        last_checked_at: Set(None),
        last_error: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    app.services.integration.card_converters.save_binding(Binding {
        id: 0, product_id: product, sku_id: 0, converter_id: converter.id,
        type_id: String::new(),
        extra_template: json!({"category":"new_activation_go","provider":{"$zs_rule":{"cases":[
            {"when":{"variable":"card.notes","operator":"eq","value":"p1"},"then":{"literal":"p1"}},
            {"when":{"variable":"source.id","operator":"eq","value":conn.to_string()},"then":{"literal":"p2"}}
        ],"else":{"literal":""}}}}),
        issues: Vec::new(), created_at: now, updated_at: now,
    }).await.unwrap();

    let channel = app.epay_channel().await;
    let created = create_and_pay(&app, product, sku, 1, channel).await;
    let parent = app.order_by_no(created["order_no"].as_str().unwrap()).await;
    let child = app.children(parent.id).await.remove(0);
    let item = order_items::Entity::find()
        .filter(order_items::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.fulfillment_type, "upstream");
    assert_eq!(
        app.epay_callback(created["payment_id"].as_i64().unwrap(), "10.00")
            .await,
        "success"
    );
    let procurement = procurement_orders::Entity::find()
        .filter(procurement_orders::Column::LocalOrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    app.services
        .integration
        .procurement
        .submit(procurement.id)
        .await
        .unwrap();
    supplier.orders.lock().unwrap().insert(
        88,
        json!({
            "order_id":88,"status":"completed","amount":"7.00",
            "fulfillment":{"type":"auto","status":"delivered","payload":"P2-RAW-CARD"}
        }),
    );
    app.services
        .integration
        .procurement
        .poll(procurement.id)
        .await
        .unwrap();
    let fulfillment = zs_infra::db::entity::fulfillments::Entity::find()
        .filter(zs_infra::db::entity::fulfillments::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    if let Some((base, _)) = &real_ai {
        verify_real_ai_topup(base, &fulfillment.payload, "p2").await;
    } else {
        assert_eq!(fulfillment.payload, "P2-SYSTEM-CARD");
    }
    assert_eq!(app.order(child.id).await.status, "delivered");
    assert_eq!(
        supplier
            .hits()
            .iter()
            .filter(|hit| hit.path == "/api/v1/upstream/orders")
            .count(),
        1
    );
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
