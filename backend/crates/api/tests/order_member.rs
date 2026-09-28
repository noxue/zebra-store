//! Member checkout: pricing (promotion, wholesale, member level / member price, coupon),
//! wallet payments (partial, full, returned on cancel), underpaid credit and cart.

mod order_common;

use order_common::{Auth, OrderApp};
use serde_json::{Value, json};
use zs_domain::queue::kinds;

struct Shop {
    a: (i64, i64),
    b: (i64, i64),
    c: (i64, i64),
    vip: i64,
}

/// A: promotion 20 % off; B: wholesale 7.00 from 3 pieces; C: member price 6.00 and a
/// 3.00 coupon `SAVE3`; members of `vip` get 90 %.
async fn shop(app: &OrderApp) -> Shop {
    let a = app.product("a", json!({"price_amount": 10})).await;
    let b = app
        .product(
            "b",
            json!({"price_amount": 10, "wholesale_prices": [{"min_quantity": 3, "unit_price": 7}]}),
        )
        .await;
    let c = app.product("c", json!({"price_amount": 10})).await;
    for (p, s) in [a, b, c] {
        app.secrets(p, s, 5).await;
    }
    let level = app
        .admin_call(
            "POST",
            "/api/v1/admin/member-levels",
            Some(json!({"name": {"zh-CN": "VIP"}, "slug": "vip", "discount_rate": 90, "sort_order": 10})),
        )
        .await;
    let vip = level["data"]["id"]
        .as_i64()
        .unwrap_or_else(|| panic!("{level}"));
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/member-level-prices/batch",
            Some(json!({"prices": [{"member_level_id": vip, "product_id": c.0, "sku_id": c.1, "price_amount": 6}]})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/promotions",
            Some(json!({"name": "A sale", "type": "percent", "scope_ref_id": a.0, "value": 20})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/coupons",
            Some(json!({"code": "SAVE3", "type": "fixed", "value": 3, "scope_ref_ids": [c.0]})),
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    Shop { a, b, c, vip }
}

fn items(s: &Shop) -> Value {
    json!([
        {"product_id": s.a.0, "sku_id": s.a.1, "quantity": 1},
        {"product_id": s.b.0, "sku_id": s.b.1, "quantity": 3},
        {"product_id": s.c.0, "sku_id": s.c.1, "quantity": 1},
    ])
}

/// PRC-01 … PRC-08: promotion, wholesale, member price and coupon combine as in the
/// original (promotion vs. wholesale: the cheaper wins; member price on top; coupon last).
#[tokio::test]
async fn prc_member_pricing_with_coupon_promotion_and_wholesale() {
    let app = OrderApp::new().await;
    let s = shop(&app).await;
    let (_, token) = app.user("vip@example.com", s.vip).await;
    let auth = Auth::Bearer(token);

    let body = json!({"items": items(&s), "coupon_code": "SAVE3"});
    let preview = app
        .call("POST", "/api/v1/orders/preview", Some(body.clone()), &auth)
        .await;
    assert_eq!(preview["status_code"], 0, "{preview}");
    let p = &preview["data"];
    assert_eq!(p["original_amount"], "50.00", "{p}");
    assert_eq!(p["promotion_discount_amount"], "2.00");
    assert_eq!(p["wholesale_discount_amount"], "9.00");
    assert_eq!(p["member_discount_amount"], "6.90");
    assert_eq!(p["discount_amount"], "3.00");
    assert_eq!(p["total_amount"], "29.10");

    let created = app.call("POST", "/api/v1/orders", Some(body), &auth).await;
    assert_eq!(created["status_code"], 0, "{created}");
    let d = &created["data"];
    assert_eq!(d["total_amount"], "29.10");
    let order = app.order_by_no(d["order_no"].as_str().unwrap()).await;
    let children = app.children(order.id).await;
    assert_eq!(children.len(), 3, "one child per line");
    let totals: Vec<String> = children
        .iter()
        .map(|c| format!("{:.2}", c.total_amount))
        .collect();
    assert_eq!(totals, ["7.20", "18.90", "3.00"]);
    assert!(order.coupon_id.is_some());

    // PRC-01: the coupon usage is claimed once; a cancel gives it back
    let res = app
        .call(
            "POST",
            &format!("/api/v1/orders/{}/cancel", order.order_no),
            None,
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.order(order.id).await.status, "canceled");
    assert_eq!(
        app.secrets_with(s.b.0, "available").await,
        5,
        "reservations released"
    );
    let again = app
        .call(
            "POST",
            "/api/v1/orders/preview",
            Some(json!({"items": items(&s), "coupon_code": "SAVE3"})),
            &auth,
        )
        .await;
    assert_eq!(again["data"]["discount_amount"], "3.00");
}

/// PAY-05 / PAY-12: wallet first, the rest online; the callback of the online part pays
/// the order and the wallet movement is recorded once.
#[tokio::test]
async fn wallet_partial_payment_then_callback() {
    let app = OrderApp::new().await;
    let s = shop(&app).await;
    let channel = app.epay_channel().await;
    let (uid, token) = app.user("vip@example.com", s.vip).await;
    app.fund(uid, "20.00").await;
    let auth = Auth::Bearer(token);

    let res = app
        .call(
            "POST",
            "/api/v1/orders/create-and-pay",
            Some(json!({"items": items(&s), "coupon_code": "SAVE3", "channel_id": channel, "use_balance": true})),
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let d = &res["data"];
    assert_eq!(d["order_paid"], false, "{d}");
    assert_eq!(d["wallet_paid_amount"], "20.00");
    assert_eq!(d["online_pay_amount"], "9.10");
    let payment_id = d["payment_id"].as_i64().unwrap();
    assert_eq!(
        format!("{:.2}", app.payment(payment_id).await.amount),
        "9.10"
    );
    assert_eq!(app.balance(uid).await, "0.00");

    assert_eq!(app.epay_callback(payment_id, "9.10").await, "success");
    let order = app.order_by_no(d["order_no"].as_str().unwrap()).await;
    assert_eq!(order.status, "paid");
    assert_eq!(format!("{:.2}", order.wallet_paid_amount), "20.00");
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 3);
    let txns = app.wallet_txns(uid).await;
    assert_eq!(txns.len(), 1, "one order_pay movement");
    assert_eq!(txns[0].reference, format!("order:{}:order_pay", order.id));

    // user views: detail, list, stats
    let detail = app
        .call(
            "GET",
            &format!("/api/v1/orders/{}", order.order_no),
            None,
            &auth,
        )
        .await;
    assert_eq!(detail["data"]["status"], "paid", "{detail}");
    let list = app.call("GET", "/api/v1/orders", None, &auth).await;
    assert_eq!(list["data"].as_array().unwrap().len(), 1);
    let latest = app
        .call(
            "GET",
            &format!("/api/v1/payments/latest?order_no={}", order.order_no),
            None,
            &auth,
        )
        .await;
    assert_ne!(
        latest["status_code"], 0,
        "no open payment after success: {latest}"
    );
}

/// PAY-24 / ORD-04: a balance covering everything pays at once; a cancel of a partly
/// wallet-paid order returns the money.
#[tokio::test]
async fn wallet_full_payment_and_refund_on_cancel() {
    let app = OrderApp::new().await;
    let s = shop(&app).await;
    let channel = app.epay_channel().await;
    let (uid, token) = app.user("vip@example.com", s.vip).await;
    app.fund(uid, "35.00").await;
    let auth = Auth::Bearer(token);

    let one = json!({"items": [{"product_id": s.a.0, "sku_id": s.a.1, "quantity": 1}]});
    let created = app
        .call("POST", "/api/v1/orders", Some(one.clone()), &auth)
        .await;
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
    assert_eq!(app.order_by_no(&order_no).await.status, "paid");
    assert_eq!(app.balance(uid).await, "27.80");

    // second order (32.10): wallet part applied, then canceled → balance back
    let created = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": items(&s)})),
            &auth,
        )
        .await;
    let order_no = created["data"]["order_no"].as_str().unwrap().to_owned();
    let res = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": channel, "use_balance": true})),
            &auth,
        )
        .await;
    assert_eq!(res["data"]["wallet_paid_amount"], "27.80", "{res}");
    assert_eq!(res["data"]["online_pay_amount"], "4.30");
    assert_eq!(app.balance(uid).await, "0.00");
    let res = app
        .call(
            "POST",
            &format!("/api/v1/orders/{order_no}/cancel"),
            None,
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    assert_eq!(app.balance(uid).await, "27.80");
    let order = app.order_by_no(&order_no).await;
    assert_eq!(format!("{:.2}", order.wallet_paid_amount), "0.00");
    // a second cancel is refused and does not credit twice
    let res = app
        .call(
            "POST",
            &format!("/api/v1/orders/{order_no}/cancel"),
            None,
            &auth,
        )
        .await;
    assert_ne!(res["status_code"], 0);
    assert_eq!(app.balance(uid).await, "27.80");
}

/// PAY-02: an underpaid success notification of a member credits the covered amount to
/// the wallet (once) and leaves the order unpaid.
#[tokio::test]
async fn pay02_member_underpaid_credit() {
    let app = OrderApp::new().await;
    let s = shop(&app).await;
    let channel = app.epay_channel().await;
    let (uid, token) = app.user("m@example.com", 0).await;
    let auth = Auth::Bearer(token);
    let res = app
        .call(
            "POST",
            "/api/v1/orders/create-and-pay",
            Some(json!({"items": [{"product_id": s.a.0, "sku_id": s.a.1, "quantity": 1}], "channel_id": channel})),
            &auth,
        )
        .await;
    let payment_id = res["data"]["payment_id"]
        .as_i64()
        .unwrap_or_else(|| panic!("{res}"));
    let order_no = res["data"]["order_no"].as_str().unwrap().to_owned();

    // the signed amount must match the payment (PAY-02 facts); a stale 3.00 payment
    // no longer covers the 10.00 order
    assert_eq!(
        app.epay_callback(payment_id, "3.00").await,
        "fail",
        "amount mismatch"
    );
    app.set_payment_amount(payment_id, "3.00").await;
    assert_eq!(app.epay_callback(payment_id, "3.00").await, "success");
    assert_eq!(app.epay_callback(payment_id, "3.00").await, "success");
    assert_eq!(app.payment(payment_id).await.status, "success");
    assert_eq!(app.order_by_no(&order_no).await.status, "pending_payment");
    assert_eq!(app.balance(uid).await, "3.00", "credited once");
}

/// ORD-11: cart rows are upserted and hard-deleted.
#[tokio::test]
async fn cart_round_trip() {
    let app = OrderApp::new().await;
    let (product, sku) = app.product("a", json!({})).await;
    app.secrets(product, sku, 2).await;
    let (_, token) = app.user("c@example.com", 0).await;
    let auth = Auth::Bearer(token);
    let res = app
        .call(
            "POST",
            "/api/v1/cart/items",
            Some(json!({"product_id": product, "sku_id": sku, "quantity": 2})),
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let cart = app.call("GET", "/api/v1/cart", None, &auth).await;
    assert_eq!(cart["data"]["items"].as_array().unwrap().len(), 1, "{cart}");
    let res = app
        .call(
            "DELETE",
            &format!("/api/v1/cart/items/{product}?sku_id={sku}"),
            None,
            &auth,
        )
        .await;
    assert_eq!(res["status_code"], 0, "{res}");
    let cart = app.call("GET", "/api/v1/cart", None, &auth).await;
    assert!(cart["data"]["items"].as_array().unwrap().is_empty());
}
