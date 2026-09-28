//! Regression tests for high-severity checklist items of `bugfix-lessons.md` §21 that had
//! no (or only partial) coverage: PAY-02, PAY-03, PAY-04, PAY-09, PAY-13, PAY-23, ORD-01,
//! ORD-04, RISK-01, NTF-01, NTF-05, MISC-02, DB-06.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests: failures should abort the test"
)]

mod order_common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, header};

use order_common::{Auth, OrderApp, dec};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use zs_domain::queue::kinds;
use zs_infra::db::entity::{coupons, payment_channels, payments, wallet_accounts};
use zs_infra::payment::http::{HttpRequest, HttpResponse, HttpTransport};

/// A member with an access token.
async fn member(app: &OrderApp, email: &str) -> (i64, Auth) {
    let (uid, token) = app.user(email, 0).await;
    (uid, Auth::Bearer(token))
}

/// Creates an unpaid member order of one piece; returns its order number.
async fn member_order(app: &OrderApp, auth: &Auth, product: (i64, i64), extra: Value) -> String {
    let mut body =
        json!({"items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}]});
    if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            b.insert(k.clone(), v.clone());
        }
    }
    let res = app.call("POST", "/api/v1/orders", Some(body), auth).await;
    assert_eq!(res["status_code"], 0, "order create failed: {res}");
    res["data"]["order_no"].as_str().unwrap().to_owned()
}

/// Creates an online payment for `order_no`; returns the payment id.
async fn pay(app: &OrderApp, auth: &Auth, order_no: &str, channel: i64) -> i64 {
    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": channel})),
            auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "payment create failed: {res}");
    res["data"]["payment_id"].as_i64().unwrap()
}

/// A JSON request as sent over a real connection from `peer` (so the client IP is known).
fn peer_request(
    method: &str,
    uri: &str,
    body: &Value,
    bearer: Option<&str>,
    peer: &str,
    forwarded_for: Option<&str>,
) -> Request<Body> {
    let mut req = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::ACCEPT_LANGUAGE, "en-US")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(token) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {token}"));
    }
    if let Some(xff) = forwarded_for {
        req = req.header("x-forwarded-for", xff);
    }
    let mut req = req.body(Body::from(body.to_string())).unwrap();
    let addr: SocketAddr = peer.parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    req
}

async fn send_json(app: &OrderApp, req: Request<Body>) -> Value {
    let (_, _, bytes) = app.send(req).await;
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

async fn set_balance(app: &OrderApp, user_id: i64, amount: &str) {
    wallet_accounts::Entity::update_many()
        .col_expr(wallet_accounts::Column::Balance, Expr::value(dec(amount)))
        .filter(wallet_accounts::Column::UserId.eq(user_id))
        .exec(&app.db)
        .await
        .unwrap();
}

/// PAY-02: an underpaid success is credited to the wallet (once); the buyer then completes
/// the order with the balance.
#[tokio::test]
async fn pay_02_underpaid_credit_then_balance_completes_the_order() {
    let app = OrderApp::new().await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 1).await;
    let channel = app.epay_channel().await;
    let (uid, auth) = member(&app, "pay02@example.com").await;
    app.fund(uid, "5.00").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;
    let stale = pay(&app, &auth, &order_no, channel).await;
    // the link only covered 5.00 of the 10.00 order (e.g. created before a switch)
    app.set_payment_amount(stale, "5.00").await;
    assert_eq!(app.epay_callback(stale, "5.00").await, "success");
    assert_eq!(app.epay_callback(stale, "5.00").await, "success");
    let order = app.order_by_no(&order_no).await;
    assert_eq!(order.status, "pending_payment");
    assert_eq!(
        app.payment(stale).await.exception_code,
        "underpaid_payment_succeeded"
    );
    assert_eq!(app.balance(uid).await, "10.00", "credited once");

    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0, "use_balance": true})),
            &auth,
        )
        .await;
    assert_eq!(res["data"]["order_paid"], true, "{res}");
    assert_eq!(app.order_by_no(&order_no).await.status, "paid");
    assert_eq!(app.balance(uid).await, "0.00");
}

/// PAY-03: a new payment link supersedes the open one (expired + superseded_by); a late
/// success of the superseded link is flagged and the order is fulfilled exactly once.
#[tokio::test]
async fn pay_03_new_link_supersedes_old_and_late_success_is_flagged() {
    let app = OrderApp::new().await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 2).await;
    let channel_a = app.epay_channel().await;
    let channel_b = app.epay_channel().await;
    let (_, auth) = member(&app, "pay03@example.com").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;

    let a = pay(&app, &auth, &order_no, channel_a).await;
    let b = pay(&app, &auth, &order_no, channel_b).await;
    assert_ne!(a, b);
    let old = app.payment(a).await;
    assert_eq!(old.status, "expired");
    assert_eq!(old.superseded_by_payment_id, Some(b));
    assert!(old.superseded_at.is_some());
    assert_eq!(app.payment(b).await.status, "pending");

    // the superseded link is paid late: flagged, the order is paid once
    assert_eq!(app.epay_callback(a, "10.00").await, "success");
    let late = app.payment(a).await;
    assert_eq!(late.status, "success");
    assert_eq!(late.exception_code, "superseded_payment_succeeded");
    assert_eq!(app.order_by_no(&order_no).await.status, "paid");
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 1);

    // B is paid too: recorded as a duplicate, nothing is fulfilled again
    assert_eq!(app.epay_callback(b, "10.00").await, "success");
    assert_eq!(
        app.payment(b).await.exception_code,
        "duplicate_payment_succeeded"
    );
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 1);
}

/// PAY-04: the same success notification delivered 10 times concurrently pays the order
/// once, queues one delivery and adds the member's spending once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pay_04_concurrent_duplicate_callbacks_settle_once() {
    let app = Arc::new(OrderApp::new().await);
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 1).await;
    let channel = app.epay_channel().await;
    let (uid, auth) = member(&app, "pay04@example.com").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;
    let payment = pay(&app, &auth, &order_no, channel).await;

    let mut tasks = Vec::new();
    for _ in 0..10 {
        let app = Arc::clone(&app);
        tasks.push(tokio::spawn(async move {
            app.epay_callback(payment, "10.00").await
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), "success");
    }
    let order = app.order_by_no(&order_no).await;
    assert_eq!(order.status, "paid");
    assert_eq!(app.payment(payment).await.status, "success");
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 1);
    let user = zs_infra::db::entity::users::Entity::find_by_id(uid)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(format!("{:.2}", user.total_spent), "10.00", "spent once");
}

/// PAY-09: every payment attempt has its own gateway order number (`DJP…`, never derived
/// from the internal payment id) and the callback locates it by that number.
#[tokio::test]
async fn pay_09_each_payment_gets_its_own_gateway_order_no() {
    let app = OrderApp::new().await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 1).await;
    let channel_a = app.epay_channel().await;
    let channel_b = app.epay_channel().await;
    let (_, auth) = member(&app, "pay09@example.com").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;

    let a = app
        .payment(pay(&app, &auth, &order_no, channel_a).await)
        .await;
    let b = app
        .payment(pay(&app, &auth, &order_no, channel_b).await)
        .await;
    for p in [&a, &b] {
        assert!(
            p.gateway_order_no.starts_with("DJP"),
            "{}",
            p.gateway_order_no
        );
        assert_ne!(p.gateway_order_no, format!("DJP{}", p.id));
        assert_ne!(p.gateway_order_no, order_no);
    }
    assert_ne!(a.gateway_order_no, b.gateway_order_no);
    assert_eq!(app.epay_callback(b.id, "10.00").await, "success");
    assert_eq!(app.order_by_no(&order_no).await.status, "paid");
}

/// Gateway that answers after a delay (PAY-13).
#[derive(Debug)]
struct SlowGateway(Duration);

#[async_trait::async_trait]
impl HttpTransport for SlowGateway {
    async fn send(&self, _request: HttpRequest) -> Result<HttpResponse, String> {
        tokio::time::sleep(self.0).await;
        Ok(HttpResponse::new(
            200,
            r#"{"code":1,"trade_no":"SLOW-1","payurl":"https://pay.example.com/pay/SLOW-1"}"#,
        ))
    }
}

/// PAY-13: a client that disconnects while the gateway call is in flight does not abort
/// the creation: the payment still records the gateway result and stays pending.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pay_13_client_disconnect_does_not_abort_gateway_call() {
    let app = OrderApp::with_transport(Arc::new(SlowGateway(Duration::from_millis(800)))).await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 1).await;
    let channel = app.epay_channel().await;
    // an API-mode channel calls the gateway on creation
    payment_channels::Entity::update_many()
        .col_expr(payment_channels::Column::InteractionMode, Expr::value("qr"))
        .filter(payment_channels::Column::Id.eq(channel))
        .exec(&app.db)
        .await
        .unwrap();
    let (_, token) = app.user("pay13@example.com", 0).await;
    let auth = Auth::Bearer(token.clone());
    let order_no = member_order(&app, &auth, product, json!({})).await;

    let request = send_json(
        &app,
        peer_request(
            "POST",
            "/api/v1/payments",
            &json!({"order_no": order_no, "channel_id": channel}),
            Some(&token),
            "198.51.100.20:40000",
            None,
        ),
    );
    // the client gives up (connection dropped) long before the gateway answers
    if let Ok(answer) = tokio::time::timeout(Duration::from_millis(150), request).await {
        panic!("the request should still be waiting for the gateway: {answer}");
    }

    let order = app.order_by_no(&order_no).await;
    let mut row = None;
    // Generous window: slow backends (MySQL in Docker under load) need several seconds.
    for _ in 0..300 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let found = payments::Entity::find()
            .filter(payments::Column::OrderId.eq(order.id))
            .one(&app.db)
            .await
            .unwrap();
        if found.as_ref().is_some_and(|p| !p.provider_ref.is_empty()) {
            row = found;
            break;
        }
    }
    let row = row.expect("gateway result persisted after the disconnect");
    assert_eq!(row.status, "pending");
    assert_eq!(row.provider_ref, "SLOW-1");
    assert_eq!(row.pay_url, "https://pay.example.com/pay/SLOW-1");
    assert_eq!(app.order_by_no(&order_no).await.status, "pending_payment");
}

/// PAY-23: "wallet only" mode is enforced on the server: online channels are refused,
/// guests cannot order, an insufficient balance is refused up front without side
/// effects, a sufficient balance pays the order with a wallet payment record.
#[tokio::test]
async fn pay_23_wallet_only_mode_is_enforced() {
    let app = OrderApp::new().await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 5).await;
    let channel = app.epay_channel().await;
    app.set_setting("wallet_config", json!({"wallet_only_payment": true}))
        .await;
    let only_wallet = "Only wallet balance payment is accepted, please recharge first";

    // guests are refused
    let guest = app
        .call(
            "POST",
            "/api/v1/guest/orders",
            Some(
                json!({"email": "g@example.com", "order_password": "secret-pass",
                "items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}]}),
            ),
            &Auth::None,
        )
        .await;
    assert_eq!(guest["msg"], only_wallet, "{guest}");

    // balance 5 < 10: refused before the order exists, stock untouched
    let (uid, auth) = member(&app, "pay23@example.com").await;
    app.fund(uid, "5.00").await;
    let res = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}]})),
            &auth,
        )
        .await;
    assert_ne!(res["status_code"], 0, "{res}");
    assert_eq!(app.secrets_with(product.0, "available").await, 5);
    assert_eq!(app.balance(uid).await, "5.00");

    // with 20 the order is created; an online channel is refused
    set_balance(&app, uid, "20.00").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;
    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": channel})),
            &auth,
        )
        .await;
    assert_eq!(res["msg"], only_wallet, "{res}");
    assert_eq!(app.balance(uid).await, "20.00");

    // balance dropped to 5 meanwhile: refused, nothing debited
    set_balance(&app, uid, "5.00").await;
    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0})),
            &auth,
        )
        .await;
    assert_ne!(res["status_code"], 0, "{res}");
    assert_eq!(app.balance(uid).await, "5.00");
    let order = app.order_by_no(&order_no).await;
    assert_eq!(order.status, "pending_payment");
    assert_eq!(format!("{:.2}", order.wallet_paid_amount), "0.00");

    // enough balance: paid by wallet with a wallet payment record
    set_balance(&app, uid, "20.00").await;
    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0})),
            &auth,
        )
        .await;
    assert_eq!(res["data"]["order_paid"], true, "{res}");
    assert_eq!(app.balance(uid).await, "10.00");
    let order = app.order_by_no(&order_no).await;
    assert_eq!(order.status, "paid");
    let rows = payments::Entity::find()
        .filter(payments::Column::OrderId.eq(order.id))
        .all(&app.db)
        .await
        .unwrap();
    assert!(
        rows.iter()
            .any(|p| p.provider_type == "wallet" && p.status == "success"),
        "{rows:?}"
    );
}

/// ORD-01: canceling an order returns the coupon usage (used_count − 1), so a coupon
/// limited to one use can be used again.
#[tokio::test]
async fn ord_01_user_cancel_returns_coupon_usage() {
    let app = OrderApp::new().await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 3).await;
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/coupons",
            Some(
                json!({"code": "ONCE", "type": "fixed", "value": 2, "usage_limit": 1,
                "scope_ref_ids": [product.0]}),
            ),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let used = |app: &OrderApp| {
        let db = app.db.clone();
        async move {
            coupons::Entity::find()
                .filter(coupons::Column::Code.eq("ONCE"))
                .one(&db)
                .await
                .unwrap()
                .unwrap()
                .used_count
        }
    };
    let (_, auth) = member(&app, "ord01@example.com").await;
    let order_no = member_order(&app, &auth, product, json!({"coupon_code": "ONCE"})).await;
    assert_eq!(used(&app).await, 1);
    // the only use is taken
    let res = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}], "coupon_code": "ONCE"})),
            &auth,
        )
        .await;
    assert_ne!(res["status_code"], 0, "{res}");

    let res = app
        .call(
            "POST",
            &format!("/api/v1/orders/{order_no}/cancel"),
            None,
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(used(&app).await, 0, "usage returned on cancel");
    member_order(&app, &auth, product, json!({"coupon_code": "ONCE"})).await;
    assert_eq!(used(&app).await, 1);
}

/// ORD-04: a user cancel expires the order's open payments; a succeeded one is kept.
#[tokio::test]
async fn ord_04_user_cancel_expires_open_payments_only() {
    let app = OrderApp::new().await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 1).await;
    let channel = app.epay_channel().await;
    let (_, auth) = member(&app, "ord04@example.com").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;

    // an underpaid success (PAY-02) leaves the order open
    let first = pay(&app, &auth, &order_no, channel).await;
    app.set_payment_amount(first, "3.00").await;
    assert_eq!(app.epay_callback(first, "3.00").await, "success");
    let open = pay(&app, &auth, &order_no, channel).await;
    assert_eq!(app.payment(open).await.status, "pending");

    let res = app
        .call(
            "POST",
            &format!("/api/v1/orders/{order_no}/cancel"),
            None,
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.order_by_no(&order_no).await.status, "canceled");
    assert_eq!(app.payment(open).await.status, "expired");
    assert_eq!(app.payment(first).await.status, "success");
    // the expired link can no longer be resumed
    let latest = app
        .call(
            "GET",
            &format!("/api/v1/payments/latest?order_no={order_no}"),
            None,
            &auth,
        )
        .await;
    assert_ne!(latest["status_code"], 0, "{latest}");
}

/// RISK-01: the guest pending-order limit per risk IP holds under concurrency (checks are
/// serialized by the risk lock keys); `X-Forwarded-For` from an untrusted peer is ignored;
/// IPv6 clients of one /64 share the quota; a trusted proxy's XFF is honoured.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn risk_01_guest_pending_limit_holds_under_concurrency() {
    let app = Arc::new(OrderApp::new().await);
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 30).await;
    app.set_setting(
        "order_risk_control_config",
        json!({"enabled": true, "guest": {"enabled": true, "max_pending_orders_per_ip": 2,
            "max_quantity_per_product_per_order": 10, "max_pending_quantity_per_ip_product": 100,
            "rate_limit": {"enabled": false}}}),
    )
    .await;
    let order = move |i: usize| {
        json!({"email": format!("g{i}@example.com"), "order_password": "secret-pass",
            "items": [{"product_id": product.0, "sku_id": product.1, "quantity": 1}]})
    };
    let mut tasks = Vec::new();
    for i in 0..10 {
        let app = Arc::clone(&app);
        tasks.push(tokio::spawn(async move {
            // every request forges a different XFF; the untrusted peer is what counts
            let xff = format!("203.0.113.{i}");
            send_json(
                &app,
                peer_request(
                    "POST",
                    "/api/v1/guest/orders",
                    &order(i),
                    None,
                    "198.51.100.7:5000",
                    Some(&xff),
                ),
            )
            .await
        }));
    }
    let mut ok = 0;
    for t in tasks {
        let res = t.await.unwrap();
        if res["status_code"] == 0 {
            ok += 1;
        } else {
            assert_eq!(res["status_code"], 429, "{res}");
        }
    }
    assert_eq!(ok, 2, "only the configured number of pending guest orders");
    assert_eq!(app.secrets_with(product.0, "reserved").await, 2);

    // two addresses of one IPv6 /64 share the quota
    let v6 = |i: usize, peer: &str| {
        peer_request(
            "POST",
            "/api/v1/guest/orders",
            &order(100 + i),
            None,
            peer,
            None,
        )
    };
    for (i, peer) in ["[2001:db8:1:2::1]:1", "[2001:db8:1:2::ffff]:1"]
        .iter()
        .enumerate()
    {
        let res = send_json(&app, v6(i, peer)).await;
        assert_eq!(res["status_code"], 0, "{res}");
    }
    let res = send_json(&app, v6(2, "[2001:db8:1:2::abcd]:1")).await;
    assert_eq!(res["status_code"], 429, "{res}");
    let res = send_json(&app, v6(3, "[2001:db8:1:3::1]:1")).await;
    assert_eq!(res["status_code"], 0, "another /64: {res}");

    // behind the trusted local proxy the forwarded client address is used
    let res = send_json(
        &app,
        peer_request(
            "POST",
            "/api/v1/guest/orders",
            &order(200),
            None,
            "127.0.0.1:9000",
            Some("192.0.2.55"),
        ),
    )
    .await;
    assert_eq!(res["status_code"], 0, "{res}");
}

/// NTF-01: status mails are not a spam vector: expired guest orders queue no mail, fully
/// auto-delivered orders skip the "paid" mail, and a queued mail is dropped (successfully)
/// once SMTP or order notifications are switched off.
#[tokio::test]
async fn ntf_01_status_mail_rules() {
    let app = OrderApp::new().await;
    app.set_setting(
        "smtp_config",
        json!({"enabled": true, "host": "smtp.example.com", "port": 587, "from": "shop@example.com",
            "order_notification_enabled": true}),
    )
    .await;
    let product = app.product("card", json!({"price_amount": "10"})).await;
    app.secrets(product.0, product.1, 2).await;
    let manual = app
        .product(
            "manual",
            json!({"price_amount": "10", "fulfillment_type": "manual", "manual_stock_total": 5}),
        )
        .await;
    let channel = app.epay_channel().await;
    let status_mails = |app: &OrderApp| {
        let db = app.db.clone();
        async move {
            zs_infra::db::entity::extra::jobs::Entity::find()
                .filter(
                    zs_infra::db::entity::extra::jobs::Column::Kind.eq(kinds::ORDER_STATUS_EMAIL),
                )
                .all(&db)
                .await
                .unwrap()
                .into_iter()
                .map(|j| j.payload)
                .collect::<Vec<_>>()
        }
    };
    let guest_order = |p: (i64, i64)| {
        json!({"email": "buyer@example.com", "order_password": "secret-pass",
            "items": [{"product_id": p.0, "sku_id": p.1, "quantity": 1}], "channel_id": channel})
    };

    // guest order that expires: canceled without any mail
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(guest_order(product)),
            &Auth::None,
        )
        .await;
    let expired = app
        .order_by_no(res["data"]["order_no"].as_str().unwrap())
        .await;
    app.expire(expired.id).await;
    app.services
        .order
        .service
        .cancel_expired(expired.id)
        .await
        .unwrap();
    assert_eq!(app.order(expired.id).await.status, "canceled");
    assert!(
        status_mails(&app).await.is_empty(),
        "no mail for a canceled guest order"
    );

    // fully automatic order: no "paid" mail (the "completed" one follows the delivery)
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(guest_order(product)),
            &Auth::None,
        )
        .await;
    let payment = res["data"]["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment, "10.00").await, "success");
    assert!(
        status_mails(&app).await.is_empty(),
        "fully automatic orders skip the paid mail"
    );

    // manual order: the paid mail is queued
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(guest_order(manual)),
            &Auth::None,
        )
        .await;
    let payment = res["data"]["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment, "10.00").await, "success");
    let mails = status_mails(&app).await;
    assert_eq!(mails.len(), 1, "{mails:?}");
    assert!(mails[0].contains("\"paid\""), "{mails:?}");

    // switched off before the worker runs: the job ends successfully without sending
    app.set_setting(
        "smtp_config",
        json!({"enabled": false, "host": "smtp.example.com", "port": 587, "from": "shop@example.com",
            "order_notification_enabled": true}),
    )
    .await;
    let order = app
        .order_by_no(res["data"]["order_no"].as_str().unwrap())
        .await;
    let payload = zs_app::order::email::StatusEmailPayload {
        order_id: order.id,
        refund_record_id: None,
        status: "paid".into(),
    };
    app.services
        .order
        .service
        .send_status_email(&payload)
        .await
        .unwrap();
    // a canceled status is never mailed, even with SMTP on
    app.set_setting(
        "smtp_config",
        json!({"enabled": true, "host": "smtp.example.com", "port": 587, "from": "shop@example.com",
            "order_notification_enabled": true}),
    )
    .await;
    let canceled = zs_app::order::email::StatusEmailPayload {
        order_id: expired.id,
        refund_record_id: None,
        status: "canceled".into(),
    };
    app.services
        .order
        .service
        .send_status_email(&canceled)
        .await
        .unwrap();
}

/// NTF-05: with SMTP disabled (the default) a paid order queues no status mail at all.
#[tokio::test]
async fn ntf_05_no_mail_jobs_while_smtp_is_disabled() {
    let app = OrderApp::new().await;
    let manual = app
        .product(
            "manual",
            json!({"price_amount": "10", "fulfillment_type": "manual", "manual_stock_total": 5}),
        )
        .await;
    let channel = app.epay_channel().await;
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({"email": "buyer@example.com", "order_password": "secret-pass",
                "items": [{"product_id": manual.0, "sku_id": manual.1, "quantity": 1}], "channel_id": channel})),
            &Auth::None,
        )
        .await;
    let payment = res["data"]["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment, "10.00").await, "success");
    // manual orders move on to "fulfilling" once paid
    assert_eq!(
        app.order_by_no(res["data"]["order_no"].as_str().unwrap())
            .await
            .status,
        "fulfilling"
    );
    assert!(app.jobs(kinds::ORDER_STATUS_EMAIL).await.is_empty());
}

/// MISC-02: storefront DTOs are allow-lists: no cost price in order items, no internal
/// ids as order handles, no provider payload in the latest payment.
#[tokio::test]
async fn misc_02_storefront_dtos_hide_internal_fields() {
    let app = OrderApp::new().await;
    let product = app
        .product(
            "card",
            json!({"price_amount": "10", "cost_price_amount": "4.50"}),
        )
        .await;
    app.secrets(product.0, product.1, 1).await;
    let channel = app.epay_channel().await;
    let (_, auth) = member(&app, "misc02@example.com").await;
    let order_no = member_order(&app, &auth, product, json!({})).await;
    pay(&app, &auth, &order_no, channel).await;

    let detail = app
        .call("GET", &format!("/api/v1/orders/{order_no}"), None, &auth)
        .await;
    assert_eq!(detail["status_code"], 0, "{detail}");
    let text = detail["data"].to_string();
    assert!(!text.contains("cost_price"), "{text}");
    assert!(detail["data"].get("id").is_none(), "{text}");
    let list = app.call("GET", "/api/v1/orders", None, &auth).await;
    assert!(!list["data"].to_string().contains("cost_price"), "{list}");

    // numeric ids are not order handles
    let order = app.order_by_no(&order_no).await;
    let by_id = app
        .call("GET", &format!("/api/v1/orders/{}", order.id), None, &auth)
        .await;
    assert_eq!(by_id["status_code"], 404, "{by_id}");

    let latest = app
        .call(
            "GET",
            &format!("/api/v1/payments/latest?order_no={order_no}"),
            None,
            &auth,
        )
        .await;
    assert_eq!(latest["status_code"], 0, "{latest}");
    assert!(latest["data"].get("provider_payload").is_none(), "{latest}");
    assert!(latest["data"].get("gateway_order_no").is_none(), "{latest}");
}

/// DB-06: the admin order list filters by product keyword (CJK title, inside the
/// parentheses of the other filters), paginates with a correct total and falls back to
/// `id desc` for an unknown/injected sort column.
#[tokio::test]
async fn db_06_admin_order_list_keyword_status_paging_and_sort() {
    let app = OrderApp::new().await;
    let apple = app
        .product(
            "apple",
            json!({"price_amount": "10", "title": {"zh-CN": "苹果礼品卡", "en-US": "Apple"}}),
        )
        .await;
    let pear = app
        .product(
            "pear",
            json!({"price_amount": "10", "title": {"zh-CN": "梨子会员", "en-US": "Pear"}}),
        )
        .await;
    app.secrets(apple.0, apple.1, 5).await;
    app.secrets(pear.0, pear.1, 5).await;
    let channel = app.epay_channel().await;
    let (_, auth) = member(&app, "db06@example.com").await;
    let a1 = member_order(&app, &auth, apple, json!({})).await;
    let a2 = member_order(&app, &auth, apple, json!({})).await;
    let p1 = member_order(&app, &auth, pear, json!({})).await;
    let paid = pay(&app, &auth, &a2, channel).await;
    assert_eq!(app.epay_callback(paid, "10.00").await, "success");

    let app = &app;
    let list = |query: &str| {
        let uri = format!("/api/v1/admin/orders?{query}");
        async move { app.admin_call("GET", &uri, None).await }
    };
    let nos = |v: &Value| -> Vec<String> {
        v["data"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["order_no"].as_str().map(str::to_owned))
            .collect()
    };
    let kw = list("product_keyword=%E8%8B%B9%E6%9E%9C").await; // 苹果
    assert_eq!(kw["status_code"], 0, "{kw}");
    assert_eq!(nos(&kw), [a2.clone(), a1.clone()], "{kw}");
    let kw_status = list("product_keyword=%E8%8B%B9%E6%9E%9C&status=pending_payment").await;
    assert_eq!(nos(&kw_status), std::slice::from_ref(&a1), "{kw_status}");
    let page2 = list("page=2&page_size=2").await;
    assert_eq!(page2["pagination"]["total"], 3, "{page2}");
    assert_eq!(nos(&page2), std::slice::from_ref(&a1));
    let injected = list("sort_by=id%3Bdrop%20table%20orders&sort_order=asc").await;
    assert_eq!(injected["status_code"], 0, "{injected}");
    let default = list("").await;
    assert_eq!(nos(&default), [p1.clone(), a2.clone(), a1.clone()]);
    let mut asc = nos(&default);
    asc.reverse();
    assert_eq!(nos(&injected), asc, "unknown column falls back to id");
}

/// NTF-05 (remaining paths): the status-mail worker only fails — and so is retried — on
/// transient SMTP errors: an unreachable server is `Err`, a malformed recipient is `Ok`
/// (dropped), and a working server receives exactly one mail.
#[tokio::test]
async fn ntf_05_worker_retries_only_transient_failures() {
    use zs_infra::db::entity::orders;
    let app = OrderApp::new().await;
    let smtp = zs_infra::testkit::MockSmtp::start("").await;
    let smtp_at = |port: u16| {
        json!({"enabled": true, "host": "127.0.0.1", "port": port, "from": "shop@example.com",
            "use_tls": false, "use_ssl": false, "order_notification_enabled": true})
    };
    app.set_setting("smtp_config", smtp_at(smtp.port)).await;
    let manual = app
        .product(
            "manual",
            json!({"price_amount": "10", "fulfillment_type": "manual", "manual_stock_total": 5}),
        )
        .await;
    let channel = app.epay_channel().await;
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({"email": "buyer@example.com", "order_password": "secret-pass",
                "items": [{"product_id": manual.0, "sku_id": manual.1, "quantity": 1}], "channel_id": channel})),
            &Auth::None,
        )
        .await;
    let payment = res["data"]["payment_id"].as_i64().unwrap();
    assert_eq!(app.epay_callback(payment, "10.00").await, "success");
    let order = app
        .order_by_no(res["data"]["order_no"].as_str().unwrap())
        .await;
    let payload = zs_app::order::email::StatusEmailPayload {
        order_id: order.id,
        refund_record_id: None,
        status: "paid".into(),
    };
    let orders = &app.services.order.service;

    orders.send_status_email(&payload).await.unwrap();
    let sent = smtp.messages();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].rcpt_to, vec!["<buyer@example.com>".to_owned()]);

    // unreachable server: an error, so the queue retries the job
    let closed = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    app.set_setting("smtp_config", smtp_at(closed)).await;
    assert!(orders.send_status_email(&payload).await.is_err());

    // malformed recipient: dropped without error (no retry storm)
    orders::Entity::update_many()
        .col_expr(orders::Column::GuestEmail, Expr::value("not-an-email"))
        .filter(orders::Column::Id.eq(order.id))
        .exec(&app.db)
        .await
        .unwrap();
    app.set_setting("smtp_config", smtp_at(smtp.port)).await;
    orders.send_status_email(&payload).await.unwrap();
    assert_eq!(smtp.messages().len(), 1, "nothing sent to the bad address");
}

/// Captures formatted `tracing` output of the current thread.
#[derive(Clone, Default)]
struct LogSink(Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for LogSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogSink {
    type Writer = Self;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}

/// PAY-21: a callback whose amount differs from the stored one is refused and the
/// rejection is logged with both amounts; the payment stays pending.
#[tokio::test]
async fn pay_21_amount_mismatch_is_logged_with_both_amounts() {
    let app = OrderApp::new().await;
    let manual = app
        .product(
            "manual",
            json!({"price_amount": "10", "fulfillment_type": "manual", "manual_stock_total": 5}),
        )
        .await;
    let channel = app.epay_channel().await;
    let res = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({"email": "buyer@example.com", "order_password": "secret-pass",
                "items": [{"product_id": manual.0, "sku_id": manual.1, "quantity": 1}], "channel_id": channel})),
            &Auth::None,
        )
        .await;
    let payment = res["data"]["payment_id"].as_i64().unwrap();

    let sink = LogSink::default();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(sink.clone())
        .with_ansi(false)
        .with_max_level(tracing::Level::WARN)
        .finish();
    let reply = {
        let _guard = tracing::subscriber::set_default(subscriber);
        app.epay_callback(payment, "9.00").await
    };
    assert_ne!(reply, "success");
    let logs = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("payment_callback_facts_rejected"), "{logs}");
    assert!(logs.contains("stored_amount=10.00"), "{logs}");
    assert!(logs.contains("callback_amount=9.00"), "{logs}");
    assert_eq!(app.payment(payment).await.status, "pending");
}
