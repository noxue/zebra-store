//! `/admin/dashboard/*`: overview KPIs, funnel and alerts, trends bucketed by
//! the requested zone, rankings, inventory alerts, cache and errors.

mod common;
mod dashboard_common;

use common::{TestApp, data};
use dashboard_common::{
    channel, item, order, payment, product, refund, secrets, setting, sku, upstream_mapping, user,
    wallet,
};
use serde_json::{Value, json};

/// Window 2026-09-01 .. 2026-09-02 in Asia/Shanghai.
const WINDOW: &str =
    "range=custom&tz=Asia/Shanghai&from=2026-08-31T16:00:00Z&to=2026-09-02T15:59:59Z";

/// Seeds the scenario described in each test's comments.
async fn seed(app: &TestApp) {
    let db = &app.db;
    let c1 = channel(db, "Alipay").await;
    let c2 = channel(db, " USDT ").await;

    let p1 = product(db, "P1", "auto", 0).await;
    let s1 = sku(db, p1, "S1", 0, true).await;
    secrets(db, p1, s1, 3, "available").await;
    secrets(db, p1, s1, 2, "used").await;
    let p2 = product(db, "P2", "manual", 3).await;
    let p3 = product(db, "P3", "upstream", 0).await;
    let s3 = sku(db, p3, "S3", 0, true).await;
    upstream_mapping(db, p3, &[(s3, 0, true)]).await;

    // 09-01 00:30 in Shanghai (DB-05: UTC day would be 08-31).
    let o1 = order(db, "O1", None, "paid", "100", "2026-08-31T16:30:00Z").await;
    item(db, o1, p1, s1, "P1", 2, "100", "10", "30").await;
    payment(
        db,
        o1,
        c1,
        "epay",
        "success",
        "100",
        "3",
        "merchant_absorbed",
        "2026-08-31T16:40:00Z",
    )
    .await;
    refund(db, o1, "50", "1", "2026-09-02T01:00:00Z").await;

    let o2 = order(db, "O2", None, "completed", "50", "2026-09-02T10:00:00Z").await;
    item(db, o2, p2, 0, "P2", 1, "50", "0", "0").await;
    payment(
        db,
        o2,
        c1,
        "epay",
        "success",
        "50",
        "2",
        "customer_surcharge",
        "2026-09-02T10:01:00Z",
    )
    .await;

    let o3 = order(
        db,
        "O3",
        None,
        "pending_payment",
        "20",
        "2026-09-01T02:00:00Z",
    )
    .await;
    item(db, o3, p2, 0, "P2", 1, "20", "0", "0").await;
    payment(
        db,
        o3,
        c1,
        "epay",
        "failed",
        "20",
        "0",
        "",
        "2026-09-01T02:05:00Z",
    )
    .await;

    // Parent order whose items live on its child (MISC-03 ③).
    let o4 = order(db, "O4", None, "paid", "80", "2026-09-01T03:00:00Z").await;
    let child = order(db, "O4-1", Some(o4), "paid", "80", "2026-09-01T03:00:00Z").await;
    item(db, child, p1, s1, "P1", 1, "80", "0", "30").await;
    payment(
        db,
        o4,
        c2,
        "epusdt",
        "success",
        "80",
        "0",
        "",
        "2026-09-01T03:10:00Z",
    )
    .await;
    // PAY-24: wallet payments never count as online payments.
    payment(
        db,
        o4,
        0,
        "wallet",
        "success",
        "80",
        "0",
        "",
        "2026-09-01T03:10:00Z",
    )
    .await;

    // DB-02 boundaries: 08-31 23:59:59 (+08) is outside, 09-01 00:00:00 inside.
    order(db, "O5", None, "paid", "999", "2026-08-31T15:59:59Z").await;
    order(db, "O6", None, "canceled", "1", "2026-08-31T16:00:00Z").await;

    let u1 = user(db, "u1@example.com", "Passw0rd!", "2026-09-01T05:00:00Z").await;
    let u2 = user(db, "u2@example.com", "Passw0rd!", "2026-08-01T05:00:00Z").await;
    wallet(db, u1, "12.3").await;
    wallet(db, u2, "7.7").await;
}

#[tokio::test]
async fn overview_kpis_funnel_and_alerts() {
    let app = TestApp::new().await;
    seed(&app).await;
    let v = app
        .get(&format!("/api/v1/admin/dashboard/overview?{WINDOW}"))
        .await;
    let d = data(&v);
    assert_eq!(d["range"], "custom");
    assert_eq!(d["timezone"], "Asia/Shanghai");
    assert_eq!(d["from"], "2026-09-01T00:00:00+08:00");
    assert_eq!(d["to"], "2026-09-02T23:59:59+08:00");
    assert_eq!(d["currency"], "CNY");
    let k = &d["kpi"];
    assert_eq!(k["orders_total"], 5, "O1..O4 and O6; child and O5 excluded");
    assert_eq!(k["paid_orders"], 3);
    assert_eq!(k["completed_orders"], 1);
    assert_eq!(k["pending_payment_orders"], 1);
    assert_eq!(k["processing_orders"], 2);
    assert_eq!(k["gmv_paid"], "230.00");
    // Revenue 90 + 50 + 80 - 50 refund = 170; cost 60 + 30 = 90; fee 3 - 1 = 2.
    assert_eq!(k["payment_fee"], "2.00");
    assert_eq!(k["total_cost"], "92.00");
    assert_eq!(k["total_profit"], "78.00");
    assert_eq!(k["profit_margin"], "45.88");
    assert_eq!(k["payments_total"], 4);
    assert_eq!(k["payments_success"], 3);
    assert_eq!(k["payments_failed"], 1);
    assert_eq!(k["payment_success_rate"], "75.00");
    assert_eq!(k["new_users"], 1);
    assert_eq!(k["active_products"], 3);
    assert_eq!(k["out_of_stock_products"], 0);
    assert_eq!(k["low_stock_products"], 2);
    assert_eq!(k["out_of_stock_skus"], 0);
    assert_eq!(k["low_stock_skus"], 1);
    assert_eq!(k["auto_available_secrets"], 3);
    assert_eq!(k["manual_available_units"], 3);
    assert_eq!(k["total_user_balance"], "20.00");
    assert_eq!(
        d["funnel"],
        json!({
            "orders_created": 5,
            "payments_created": 4,
            "payments_success": 3,
            "orders_paid": 3,
            "orders_completed": 1,
            "payment_conversion_rate": "60.00",
            "completion_rate": "33.33"
        })
    );
    assert_eq!(
        d["alerts"],
        json!([{"type": "low_stock_products", "level": "warning", "value": 2}])
    );
}

// MISC-03 (2): with refund_reverses_cost the refunded share of cost (60 × 50/100) is reversed.
#[tokio::test]
async fn refund_reverses_cost_setting() {
    let app = TestApp::new().await;
    seed(&app).await;
    setting(
        &app.db,
        "dashboard_config",
        json!({"accounting": {"refund_reverses_cost": true}, "alert": {"payments_failed_threshold": 1}}),
    )
    .await;
    let v = app
        .get(&format!("/api/v1/admin/dashboard/overview?{WINDOW}"))
        .await;
    let k = &data(&v)["kpi"];
    assert_eq!(k["total_cost"], "62.00");
    assert_eq!(k["total_profit"], "108.00");
    assert_eq!(k["profit_margin"], "63.53");
    let alerts = &data(&v)["alerts"];
    assert_eq!(
        alerts[1],
        json!({"type": "payments_failed", "level": "warning", "value": 1})
    );
}

#[tokio::test]
async fn trends_bucket_by_requested_zone() {
    let app = TestApp::new().await;
    seed(&app).await;
    let v = app
        .get(&format!("/api/v1/admin/dashboard/trends?{WINDOW}"))
        .await;
    let d = data(&v);
    assert_eq!(d["timezone"], "Asia/Shanghai");
    assert_eq!(
        d["points"],
        json!([
            {"date": "2026-09-01", "orders_total": 4, "orders_paid": 2, "payments_success": 2,
             "payments_failed": 1, "gmv_paid": "180.00", "profit": "77.00"},
            {"date": "2026-09-02", "orders_total": 1, "orders_paid": 1, "payments_success": 1,
             "payments_failed": 0, "gmv_paid": "50.00", "profit": "1.00"}
        ])
    );
    // The same data bucketed in UTC moves O1 (08-31 16:30Z) out of the window.
    let v = app
        .get("/api/v1/admin/dashboard/trends?range=custom&tz=UTC&from=2026-09-01T00:00:00Z&to=2026-09-01T23:59:59Z")
        .await;
    let points = &data(&v)["points"];
    assert_eq!(points.as_array().unwrap().len(), 1);
    assert_eq!(points[0]["orders_total"], 2, "O3 and O4 only");
}

#[tokio::test]
async fn rankings_products_and_channels() {
    let app = TestApp::new().await;
    seed(&app).await;
    let v = app
        .get(&format!("/api/v1/admin/dashboard/rankings?{WINDOW}"))
        .await;
    let d = data(&v);
    let products = d["top_products"].as_array().unwrap();
    assert_eq!(products.len(), 2);
    let p1 = &products[0];
    assert_eq!(p1["title"], "P1");
    assert_eq!(p1["sku_code"], "S1");
    assert_eq!(p1["sku_spec_values"], json!({"size": "S1"}));
    assert_eq!(p1["paid_orders"], 2);
    assert_eq!(p1["quantity"], 3);
    assert_eq!(p1["paid_amount"], "170.00");
    assert_eq!(p1["total_cost"], "90.00");
    assert_eq!(p1["profit"], "80.00");
    assert!(products[1].get("sku_id").is_none(), "sku_id 0 is omitted");
    assert_eq!(products[1]["paid_amount"], "50.00");
    let channels = d["top_channels"].as_array().unwrap();
    assert_eq!(channels.len(), 2, "wallet payments excluded");
    assert_eq!(channels[0]["channel_name"], "Alipay");
    assert_eq!(channels[0]["success_count"], 2);
    assert_eq!(channels[0]["failed_count"], 1);
    assert_eq!(channels[0]["success_amount"], "150.00");
    assert_eq!(channels[0]["success_rate"], "66.67");
    assert_eq!(channels[1]["channel_name"], "USDT");
    assert_eq!(channels[1]["provider_type"], "epusdt");
    assert_eq!(channels[1]["success_rate"], "100.00");

    setting(
        &app.db,
        "dashboard_config",
        json!({"ranking": {"top_products_limit": 1, "top_channels_limit": 1}}),
    )
    .await;
    let v = app
        .get(&format!("/api/v1/admin/dashboard/rankings?{WINDOW}"))
        .await;
    assert_eq!(data(&v)["top_products"].as_array().unwrap().len(), 1);
    assert_eq!(data(&v)["top_channels"].as_array().unwrap().len(), 1);
}

// DB-07 / MISC-05: SKU-level rows keep their sku_id; upstream stock comes from the mappings.
#[tokio::test]
async fn inventory_alerts_rows() {
    let app = TestApp::new().await;
    seed(&app).await;
    let v = app.get("/api/v1/admin/dashboard/inventory-alerts").await;
    let rows = data(&v).as_array().unwrap().clone();
    let summary: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!([
                r["product_title"]["zh-CN"],
                r["sku_code"],
                r["fulfillment_type"],
                r["alert_type"],
                r["available_stock"]
            ])
        })
        .collect();
    assert_eq!(
        summary,
        vec![
            json!(["P3", "S3", "upstream", "out_of_stock_products", 0]),
            json!(["P2", null, "manual", "low_stock_products", 3]),
            json!(["P1", "S1", "auto", "low_stock_products", 3]),
        ]
    );
    assert!(rows[0]["sku_id"].as_i64().unwrap() > 0);
    assert!(rows[1].get("sku_id").is_none());
    assert_eq!(rows[2]["sku_spec_values"], json!({"size": "S1"}));

    setting(
        &app.db,
        "dashboard_config",
        json!({"alert": {"low_stock_threshold": 2}}),
    )
    .await;
    let v = app.get("/api/v1/admin/dashboard/inventory-alerts").await;
    assert_eq!(
        data(&v).as_array().unwrap().len(),
        1,
        "only the sold-out upstream SKU"
    );
}

// MISC-03 ④: an empty window still reports the latest order currency.
#[tokio::test]
async fn empty_window_and_cache() {
    let app = TestApp::new().await;
    seed(&app).await;
    let empty = "range=custom&tz=UTC&from=2026-07-01T00:00:00Z&to=2026-07-01T23:59:59Z";
    let v = app
        .get(&format!("/api/v1/admin/dashboard/overview?{empty}"))
        .await;
    let d = data(&v);
    assert_eq!(d["currency"], "CNY");
    assert_eq!(d["kpi"]["orders_total"], 0);
    assert_eq!(d["kpi"]["profit_margin"], "0.00");
    assert_eq!(d["funnel"]["completion_rate"], "0.00");

    // Cached for 45 s unless force_refresh.
    let uri = format!("/api/v1/admin/dashboard/overview?{WINDOW}");
    assert_eq!(data(&app.get(&uri).await)["kpi"]["orders_total"], 5);
    order(&app.db, "O7", None, "paid", "1", "2026-09-01T06:00:00Z").await;
    assert_eq!(data(&app.get(&uri).await)["kpi"]["orders_total"], 5);
    let fresh = app.get(&format!("{uri}&force_refresh=true")).await;
    assert_eq!(data(&fresh)["kpi"]["orders_total"], 6);
}

#[tokio::test]
async fn default_range_is_seven_days() {
    let app = TestApp::new().await;
    let v = app
        .get("/api/v1/admin/dashboard/trends?tz=Asia/Shanghai")
        .await;
    let d = data(&v);
    assert_eq!(d["range"], "7d");
    assert_eq!(d["points"].as_array().unwrap().len(), 7);
    let v = app
        .get("/api/v1/admin/dashboard/overview?range=today&tz=Not/AZone")
        .await;
    assert_eq!(data(&v)["timezone"], "UTC");
}

#[tokio::test]
async fn invalid_queries_and_permissions() {
    let app = TestApp::new().await;
    for q in [
        "range=forever",
        "range=custom",
        "range=custom&from=2026-01-01T00:00:00Z&to=2026-06-01T00:00:00Z",
        "range=custom&from=2026-02-01T00:00:00Z&to=2026-01-01T00:00:00Z",
        "range=custom&from=yesterday&to=2026-01-01T00:00:00Z",
        "force_refresh=maybe",
    ] {
        let v = app
            .get(&format!("/api/v1/admin/dashboard/overview?{q}"))
            .await;
        assert_eq!(v["status_code"], 400, "{q}: {v}");
        assert_eq!(v["msg"], "请求参数错误", "{q}");
    }
    let anon = app
        .call("GET", "/api/v1/admin/dashboard/overview", None, None)
        .await;
    assert_eq!(anon["status_code"], 401);
}
