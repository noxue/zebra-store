//! Purchase orders: creation for paid upstream items (UPS-07), submission and error
//! classes (UPS-11), supplier callbacks with signature / ownership / state machine /
//! body limit (UPS-02, UPS-03, UPS-16), polling and periodic sync (UPS-11), admin
//! queries (UPS-22).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

mod integration_common;

use integration_common::{
    IntApp, SUP_KEY, SUP_SECRET, Supplier, callback_request, data, err, jobs_of, remote_product,
    seed_item, seed_order_row, start_supplier,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use zs_domain::queue::kinds;
use zs_infra::db::entity::procurement_orders;

struct Fixture {
    app: IntApp,
    supplier: Supplier,
    conn: i64,
    product_id: i64,
    sku_id: i64,
}

async fn fixture() -> Fixture {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let conn = app
        .connection(&base, json!({"retry_max": 2, "retry_intervals": "[30,60]"}))
        .await;
    data(
        &app.admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}/status"),
            Some(json!({"status": "active"})),
        )
        .await,
    );
    supplier.set_products(vec![remote_product(9, "1.00", json!([{"id": 101, "sku_code": "A", "price_amount": "1.00", "stock_quantity": 5, "is_active": true}]))]);
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 9})),
        )
        .await;
    let product_id = data(&res)["local_product_id"].as_i64().unwrap();
    let sku_id = zs_infra::db::entity::product_skus::Entity::find()
        .filter(zs_infra::db::entity::product_skus::Column::ProductId.eq(product_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    Fixture {
        app,
        supplier,
        conn,
        product_id,
        sku_id,
    }
}

impl Fixture {
    /// A paid order with one upstream item; returns `(order_id, order_no)`.
    async fn order(&self, no: &str, parent: Option<i64>) -> (i64, String) {
        let id = seed_order_row(&self.app.db, no, 1, "paid", "12.00", parent).await;
        seed_item(
            &self.app.db,
            id,
            self.product_id,
            self.sku_id,
            2,
            "upstream",
        )
        .await;
        (id, no.to_owned())
    }

    async fn procurement(&self, local_order_id: i64) -> procurement_orders::Model {
        procurement_orders::Entity::find()
            .filter(procurement_orders::Column::LocalOrderId.eq(local_order_id))
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
    }

    /// Paid order → purchase order submitted and accepted (supplier order 88).
    async fn accepted(&self, no: &str) -> (i64, procurement_orders::Model) {
        let (oid, _) = self.order(no, None).await;
        self.app
            .services
            .integration
            .order_events
            .order_paid(oid)
            .await
            .unwrap();
        let p = self.procurement(oid).await;
        self.app
            .services
            .integration
            .procurement
            .submit(p.id)
            .await
            .unwrap();
        (oid, self.procurement(oid).await)
    }

    async fn callback(&self, key: &str, secret: &str, body: Value) -> Value {
        let (_, _, bytes) = self
            .app
            .send(callback_request(key, secret, &body.to_string(), 0))
            .await;
        serde_json::from_slice(&bytes).unwrap()
    }
}

fn delivered(no: &str, order_id: i64, payload: &str) -> Value {
    json!({
        "event": "order.fulfilled",
        "order_id": order_id,
        "order_no": "UP88",
        "downstream_order_no": no,
        "status": "delivered",
        "fulfillment": {"type": "auto", "status": "delivered", "payload": payload, "delivery_data": {"k": "v"}},
        "timestamp": 1,
    })
}

// UPS-07: creation is idempotent; submission sends the mapped SKU and marks fulfilling.
#[tokio::test]
async fn create_and_submit() {
    let f = fixture().await;
    let (oid, no) = f.order("L-1", None).await;
    let events = f.app.services.integration.order_events.clone();
    events.order_paid(oid).await.unwrap();
    events.order_paid(oid).await.unwrap();
    let all = procurement_orders::Entity::find()
        .all(&f.app.db)
        .await
        .unwrap();
    assert_eq!(all.len(), 1, "UPS-07 ⑦: one purchase order per local order");
    let p = &all[0];
    assert_eq!(p.status, "pending");
    assert_eq!(p.connection_id, f.conn);
    assert_eq!(p.local_order_no, no);
    assert_eq!(p.local_sell_amount.normalize().to_string(), "12");
    assert!(!p.trace_id.is_empty());
    assert_eq!(
        jobs_of(&f.app.db, kinds::PROCUREMENT_SUBMIT).await,
        vec![json!({"procurement_order_id": p.id})]
    );

    f.app
        .services
        .integration
        .procurement
        .submit(p.id)
        .await
        .unwrap();
    let p = f.procurement(oid).await;
    assert_eq!(p.status, "accepted");
    assert_eq!(p.upstream_order_id, 88);
    assert_eq!(p.upstream_order_no, "UP88");
    assert_eq!(p.upstream_amount.normalize().to_string(), "7");
    assert_eq!(p.upstream_currency, "USD");
    assert_eq!(p.retry_count, 0);
    assert_eq!(f.app.lifecycle.calls(), vec![format!("fulfilling:{oid}")]);
    assert_eq!(jobs_of(&f.app.db, kinds::PROCUREMENT_POLL).await.len(), 1);
    let hit = f
        .supplier
        .hits()
        .into_iter()
        .find(|h| h.path == "/api/v1/upstream/orders")
        .unwrap();
    let body: Value = serde_json::from_str(&hit.body).unwrap();
    assert_eq!(body["sku_id"], 101);
    assert_eq!(body["quantity"], 2);
    assert_eq!(body["downstream_order_no"], no);
    assert_eq!(body["trace_id"], p.trace_id);
    assert_eq!(
        body["callback_url"],
        "https://buyer.example.com/api/v1/upstream/callback"
    );
    assert_eq!(body["manual_form_data"]["account"], "a@b.c");

    // A second submit of an accepted order is a no-op.
    f.app
        .services
        .integration
        .procurement
        .submit(p.id)
        .await
        .unwrap();
    assert_eq!(
        f.supplier
            .paths()
            .iter()
            .filter(|p| *p == "/api/v1/upstream/orders")
            .count(),
        1
    );
}

// Parent orders create one purchase order per child with upstream items.
#[tokio::test]
async fn parent_with_children() {
    let f = fixture().await;
    let parent = seed_order_row(&f.app.db, "P-1", 1, "paid", "24.00", None).await;
    let (c1, _) = f.order("P-1-1", Some(parent)).await;
    let c2 = seed_order_row(&f.app.db, "P-1-2", 1, "paid", "5.00", Some(parent)).await;
    seed_item(&f.app.db, c2, f.product_id, f.sku_id, 1, "manual").await;
    let out = f
        .app
        .services
        .integration
        .procurement
        .create_for_order(parent)
        .await
        .unwrap();
    assert_eq!(out.created.len(), 1);
    assert_eq!(f.procurement(c1).await.local_order_no, "P-1-1");
}

// UPS-11 / UPS-07: permanent errors reject and roll back; transient ones retry.
#[tokio::test]
async fn submit_error_classes() {
    let f = fixture().await;
    let procurement = f.app.services.integration.procurement.clone();

    // Non-retryable → rejected, local order rolled back, admin alerted.
    *f.supplier.order_reply.lock().unwrap() = Some((
        200,
        json!({"ok": false, "error_code": "insufficient_balance", "error_message": "no money"}),
    ));
    let (o1, _) = f.order("E-1", None).await;
    f.app
        .services
        .integration
        .order_events
        .order_paid(o1)
        .await
        .unwrap();
    let p1 = f.procurement(o1).await;
    procurement.submit(p1.id).await.unwrap();
    let p1 = f.procurement(o1).await;
    assert_eq!(p1.status, "rejected");
    assert_eq!(p1.error_message, "no money");
    assert!(f.app.lifecycle.calls().contains(&format!("rollback:{o1}")));
    let alerts = jobs_of(&f.app.db, kinds::NOTIFICATION_DISPATCH).await;
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0]["event_type"], "exception_alert");
    assert_eq!(alerts[0]["biz_type"], "procurement");
    assert_eq!(alerts[0]["data"]["local_order_no"], "E-1");

    // Retryable (supplier 5xx) → failed, retry scheduled, order untouched.
    *f.supplier.order_reply.lock().unwrap() = Some((502, json!({"ok": false})));
    let (o2, _) = f.order("E-2", None).await;
    f.app
        .services
        .integration
        .order_events
        .order_paid(o2)
        .await
        .unwrap();
    let p2 = f.procurement(o2).await;
    procurement.submit(p2.id).await.unwrap();
    let p2 = f.procurement(o2).await;
    assert_eq!(p2.status, "failed");
    assert_eq!(p2.retry_count, 1);
    assert!(p2.next_retry_at.is_some());
    assert!(p2.error_message.contains("502"), "{}", p2.error_message);
    assert!(!f.app.lifecycle.calls().contains(&format!("rollback:{o2}")));
    // A 502 gateway answer may hide a committed order: retries exhausted (retry_max 2)
    // → held for manual review, never rolled back automatically (money safety).
    procurement.submit(p2.id).await.unwrap();
    assert_eq!(f.procurement(o2).await.status, "failed");
    procurement.submit(p2.id).await.unwrap();
    let p2 = f.procurement(o2).await;
    assert_eq!(p2.status, "manual_review", "{}", p2.error_message);
    assert!(p2.error_message.starts_with("[result unknown]"));
    assert!(!f.app.lifecycle.calls().contains(&format!("rollback:{o2}")));

    // Missing SKU mapping → rejected without retry.
    let (o3, _) = f.order("E-3", None).await;
    f.app
        .services
        .integration
        .order_events
        .order_paid(o3)
        .await
        .unwrap();
    zs_infra::db::entity::sku_mappings::Entity::update_many()
        .col_expr(
            zs_infra::db::entity::sku_mappings::Column::DeletedAt,
            sea_orm::sea_query::Expr::value(chrono::Utc::now()),
        )
        .exec(&f.app.db)
        .await
        .unwrap();
    let p3 = f.procurement(o3).await;
    procurement.submit(p3.id).await.unwrap();
    let p3 = f.procurement(o3).await;
    assert_eq!(p3.status, "rejected");
    assert!(
        p3.error_message.contains("no sku mapping"),
        "{}",
        p3.error_message
    );
}

// UPS-02 / UPS-03 / UPS-16: supplier callback.
#[tokio::test]
async fn supplier_callback_rules() {
    let f = fixture().await;
    let (oid, p) = f.accepted("C-1").await;
    let no = p.local_order_no.clone();

    // Authentication failures.
    let r = f
        .callback("unknown", SUP_SECRET, delivered(&no, 88, "X"))
        .await;
    assert_eq!(r, json!({"ok": false, "message": "invalid api key"}));
    let r = f.callback(SUP_KEY, "wrong", delivered(&no, 88, "X")).await;
    assert_eq!(r["message"], "signature verification failed");
    let (_, _, b) = f
        .app
        .send(callback_request(SUP_KEY, SUP_SECRET, "{}", -61))
        .await;
    assert_eq!(
        serde_json::from_slice::<Value>(&b).unwrap()["message"],
        "timestamp expired"
    );
    let r = f
        .callback(SUP_KEY, SUP_SECRET, json!({"status": "delivered"}))
        .await;
    assert_eq!(r["message"], "missing required fields");

    // UPS-02: body over 1 MB is refused before verification.
    let big = format!("{{\"pad\":\"{}\"}}", "x".repeat(1 << 20));
    let (status, _, b) = f
        .app
        .send(callback_request(SUP_KEY, SUP_SECRET, &big, 0))
        .await;
    assert_eq!(status, 200);
    assert_eq!(
        serde_json::from_slice::<Value>(&b).unwrap()["message"],
        "failed to read request body"
    );

    // UPS-03: another authenticated connection cannot touch this purchase order.
    let other = f
        .app
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({"name": "Other", "base_url": "https://o.example.com", "api_key": "k2", "api_secret": "s2"})),
        )
        .await;
    let other_id = data(&other)["id"].as_i64().unwrap();
    data(
        &f.app
            .admin(
                "PUT",
                &format!("/api/v1/admin/site-connections/{other_id}/status"),
                Some(json!({"status": "active"})),
            )
            .await,
    );
    let r = f.callback("k2", "s2", delivered(&no, 88, "X")).await;
    assert_eq!(
        r,
        json!({"ok": false, "message": "procurement order not found"})
    );
    // UPS-03: same connection, wrong supplier order id.
    let r = f
        .callback(SUP_KEY, SUP_SECRET, delivered(&no, 99, "X"))
        .await;
    assert_eq!(r["message"], "procurement order not found");
    assert_eq!(f.procurement(oid).await.status, "accepted");

    // UPS-02: a failed fulfillment write keeps the order open for a retry.
    *f.app.lifecycle.fail_delivery.lock().unwrap() = true;
    let r = f
        .callback(SUP_KEY, SUP_SECRET, delivered(&no, 88, "CARD-1"))
        .await;
    assert_eq!(
        r,
        json!({"ok": false, "message": "callback processing failed"})
    );
    assert_eq!(f.procurement(oid).await.status, "accepted");
    *f.app.lifecycle.fail_delivery.lock().unwrap() = false;

    // Delivered: delivery first, then fulfilled with the payload.
    let r = f
        .callback(SUP_KEY, SUP_SECRET, delivered(&no, 88, "CARD-1"))
        .await;
    assert_eq!(r, json!({"ok": true, "message": "received"}));
    let p = f.procurement(oid).await;
    assert_eq!(p.status, "fulfilled");
    assert_eq!(p.upstream_payload, "CARD-1");
    let deliveries = f.app.lifecycle.deliveries.lock().unwrap().clone();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(deliveries[0].0, oid);
    assert_eq!(deliveries[0].1.payload, "CARD-1");
    assert_eq!(deliveries[0].1.delivery_data["k"], "v");

    // UPS-02: fulfilled → delivered / canceled are ignored (no second delivery).
    let r = f
        .callback(SUP_KEY, SUP_SECRET, delivered(&no, 88, "CARD-2"))
        .await;
    assert_eq!(r["ok"], true);
    let mut cancel = delivered(&no, 88, "");
    cancel["status"] = "Cancelled ".into();
    f.callback(SUP_KEY, SUP_SECRET, cancel).await;
    assert_eq!(f.procurement(oid).await.status, "fulfilled");
    assert_eq!(f.app.lifecycle.deliveries.lock().unwrap().len(), 1);
    assert!(!f.app.lifecycle.calls().contains(&format!("rollback:{oid}")));

    // UPS-16: refunds only move the purchase order.
    let mut refund = delivered(&no, 88, "");
    refund["status"] = "refunded".into();
    let calls_before = f.app.lifecycle.calls().len();
    assert_eq!(f.callback(SUP_KEY, SUP_SECRET, refund).await["ok"], true);
    assert_eq!(f.procurement(oid).await.status, "refunded");
    assert_eq!(f.app.lifecycle.calls().len(), calls_before);
}

// UPS-07 ④ / UPS-16: an upstream cancel rolls the local order back; a secret that
// cannot be decrypted is refused (UPS-02).
#[tokio::test]
async fn cancel_callback_and_bad_secret() {
    let f = fixture().await;
    let (oid, p) = f.accepted("K-1").await;
    let mut cancel = delivered(&p.local_order_no, 88, "");
    cancel["status"] = "cancelled".into();
    assert_eq!(f.callback(SUP_KEY, SUP_SECRET, cancel).await["ok"], true);
    assert_eq!(f.procurement(oid).await.status, "canceled");
    assert!(f.app.lifecycle.calls().contains(&format!("rollback:{oid}")));

    zs_infra::db::entity::site_connections::ActiveModel {
        id: Set(f.conn),
        api_secret: Set("not-hex".into()),
        ..Default::default()
    }
    .update(&f.app.db)
    .await
    .unwrap();
    let r = f
        .callback(SUP_KEY, SUP_SECRET, delivered(&p.local_order_no, 88, "X"))
        .await;
    assert_eq!(r, json!({"ok": false, "message": "internal error"}));
}

// UPS-11: polling, hand-off to the periodic sync and idempotent delivery.
#[tokio::test]
async fn poll_and_periodic_sync() {
    let f = fixture().await;
    let (oid, p) = f.accepted("Q-1").await;
    let procurement = f.app.services.integration.procurement.clone();
    f.supplier.orders.lock().unwrap().insert(
        88,
        json!({"order_id": 88, "status": "processing", "amount": "7.00"}),
    );
    procurement.poll(p.id).await.unwrap();
    let p = f.procurement(oid).await;
    assert_eq!((p.status.as_str(), p.retry_count), ("accepted", 1));
    assert_eq!(jobs_of(&f.app.db, kinds::PROCUREMENT_POLL).await.len(), 2);

    // Short schedule exhausted: stays accepted, no new poll.
    procurement_orders::ActiveModel {
        id: Set(p.id),
        retry_count: Set(9),
        ..Default::default()
    }
    .update(&f.app.db)
    .await
    .unwrap();
    procurement.poll(p.id).await.unwrap();
    assert_eq!(f.procurement(oid).await.status, "accepted");
    assert_eq!(jobs_of(&f.app.db, kinds::PROCUREMENT_POLL).await.len(), 2);

    // Periodic sync sees the delivery once.
    f.supplier.orders.lock().unwrap().insert(
        88,
        json!({"order_id": 88, "status": "completed", "amount": "7.00", "fulfillment": {"type": "auto", "status": "delivered", "payload": "SYNCED"}}),
    );
    procurement.sync_accepted().await.unwrap();
    procurement.sync_accepted().await.unwrap();
    assert_eq!(f.procurement(oid).await.status, "fulfilled");
    assert_eq!(f.app.lifecycle.deliveries.lock().unwrap().len(), 1);
    assert_eq!(
        f.app.lifecycle.deliveries.lock().unwrap()[0].1.payload,
        "SYNCED"
    );
}

// UPS-22 and admin actions.
#[tokio::test]
async fn admin_queries_and_actions() {
    let f = fixture().await;
    let (_, accepted) = f.accepted("A-1").await;
    *f.supplier.order_reply.lock().unwrap() =
        Some((200, json!({"ok": false, "error_code": "sku_unavailable"})));
    let (o2, _) = f.order("A-2", None).await;
    f.app
        .services
        .integration
        .order_events
        .order_paid(o2)
        .await
        .unwrap();
    let rejected = f.procurement(o2).await;
    f.app
        .services
        .integration
        .procurement
        .submit(rejected.id)
        .await
        .unwrap();

    let list = f
        .app
        .admin("GET", "/api/v1/admin/procurement-orders", None)
        .await;
    let items = data(&list).as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(list["pagination"]["total"], 2);
    let first = items.iter().find(|i| i["id"] == accepted.id).unwrap();
    assert_eq!(first["upstream_amount"], "7.00");
    assert_eq!(first["local_sell_amount"], "12.00");
    assert_eq!(first["connection"]["id"], f.conn);
    assert_eq!(first["local_order"]["order_no"], "A-1");
    assert_eq!(
        first["local_order"]["items"][0]["fulfillment_type"],
        "upstream"
    );
    assert!(first.get("upstream_order_id").is_none());

    // UPS-22: stats ignore the status filter; times are parsed (invalid → 400).
    let stats = f
        .app
        .admin(
            "GET",
            "/api/v1/admin/procurement-orders/stats?status=failed",
            None,
        )
        .await;
    assert_eq!(data(&stats)["total"], 2);
    assert_eq!(data(&stats)["by_status"]["accepted"], 1);
    assert_eq!(data(&stats)["by_status"]["rejected"], 1);
    let bad = f
        .app
        .admin(
            "GET",
            "/api/v1/admin/procurement-orders?created_from=2026-04-27",
            None,
        )
        .await;
    err(&bad, 400, "请求参数错误");
    let tomorrow = (chrono::Utc::now() + chrono::Duration::days(1))
        .format("%Y-%m-%dT23:59:59%%2B08:00")
        .to_string();
    let ok = f
        .app
        .admin(
            "GET",
            &format!("/api/v1/admin/procurement-orders?status=rejected&created_to={tomorrow}"),
            None,
        )
        .await;
    assert_eq!(data(&ok).as_array().unwrap().len(), 1);
    let past = f
        .app
        .admin(
            "GET",
            "/api/v1/admin/procurement-orders?created_to=2000-01-01T00:00:00Z",
            None,
        )
        .await;
    assert_eq!(data(&past).as_array().unwrap().len(), 0);

    // Retry only failed / rejected.
    let r = f
        .app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{}/retry", accepted.id),
            None,
        )
        .await;
    err(&r, 400, "procurement order status invalid");
    let r = f
        .app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{}/retry", rejected.id),
            None,
        )
        .await;
    assert_eq!(data(&r)["ok"], true);
    assert_eq!(f.procurement(o2).await.status, "pending");

    // Cancel an accepted order also cancels at the supplier.
    let r = f
        .app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{}/cancel", accepted.id),
            None,
        )
        .await;
    assert_eq!(data(&r)["ok"], true);
    assert!(
        f.supplier
            .paths()
            .contains(&"/api/v1/upstream/orders/88/cancel".to_owned())
    );
    let r = f
        .app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{}/cancel", accepted.id),
            None,
        )
        .await;
    err(&r, 400, "procurement order status invalid");

    // Detail + payload preview / download.
    let payload: String = (0..150).map(|i| format!("line{i}\n")).collect();
    procurement_orders::ActiveModel {
        id: Set(accepted.id),
        upstream_payload: Set(payload.trim_end().to_owned()),
        ..Default::default()
    }
    .update(&f.app.db)
    .await
    .unwrap();
    let d = f
        .app
        .admin(
            "GET",
            &format!("/api/v1/admin/procurement-orders/{}", accepted.id),
            None,
        )
        .await;
    assert_eq!(data(&d)["upstream_payload_line_count"], 150);
    assert_eq!(
        data(&d)["upstream_payload"]
            .as_str()
            .unwrap()
            .lines()
            .count(),
        100
    );
    let req = axum::http::Request::builder()
        .uri(format!(
            "/api/v1/admin/procurement-orders/{}/upstream-payload/download",
            accepted.id
        ))
        .header("authorization", format!("Bearer {}", f.app.admin_token))
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, headers, body) = f.app.send(req).await;
    assert_eq!(status, 200);
    assert!(
        headers["content-disposition"]
            .to_str()
            .unwrap()
            .contains(&format!("upstream-payload-{}.txt", accepted.id))
    );
    assert_eq!(String::from_utf8(body).unwrap().lines().count(), 150);
    let none = f
        .app
        .admin(
            "GET",
            &format!(
                "/api/v1/admin/procurement-orders/{}/upstream-payload/download",
                rejected.id
            ),
            None,
        )
        .await;
    err(&none, 404, "交付记录不存在");
    let missing = f
        .app
        .admin("GET", "/api/v1/admin/procurement-orders/9999", None)
        .await;
    err(&missing, 404, "采购单不存在");
}
