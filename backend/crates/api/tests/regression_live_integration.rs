//! Live QA 2026-09-26 regressions of the integrations (对接, LQA-I*): the pre-order stock
//! guard, the rejected-purchase lifecycle with a real refund, and friendly errors.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use chrono::Utc;
use integration_common::{IntApp, data, remote_product, start_supplier};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{
    orders, procurement_orders, product_skus, products, sku_mappings, wallet_accounts,
};

/// App with the real order-group ports behind the integration services.
async fn app() -> IntApp {
    let mut cfg = integration_common::config();
    // Loopback suppliers for every group (the order group's stock guard included).
    cfg.integration.allow_private_addresses = true;
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let services = zs_infra::wire::services(&ctx);
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let router = zs_api::build(zs_api::AppState::new(services.clone(), cfg)).router;
    let app = IntApp {
        router,
        db: db.clone(),
        services,
        admin_token: String::new(),
        lifecycle: std::sync::Arc::default(),
        ordering: std::sync::Arc::new(integration_common::FakeOrdering {
            db,
            fail_payment: std::sync::Mutex::new(false),
            placed: std::sync::Mutex::new(0),
        }),
    };
    let login = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": "Admin12345"})),
            None,
        )
        .await;
    let app = IntApp {
        admin_token: login["data"]["token"].as_str().unwrap().to_owned(),
        ..app
    };
    app.acknowledge_compliance().await;
    app
}

async fn fund(app: &IntApp, user_id: i64, amount: &str) {
    let now = Utc::now();
    wallet_accounts::ActiveModel {
        user_id: Set(user_id),
        balance: Set(amount.parse().unwrap()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
}

async fn balance(app: &IntApp, user_id: i64) -> String {
    let row = wallet_accounts::Entity::find()
        .filter(wallet_accounts::Column::UserId.eq(user_id))
        .one(&app.db)
        .await
        .unwrap();
    format!("{:.2}", row.map(|r| r.balance).unwrap_or_default())
}

/// An active imported upstream product (supplier product 9, SKU 101, stock `stock`):
/// `(connection, product_id, sku_id)`.
async fn imported(
    app: &IntApp,
    base: &str,
    supplier: &integration_common::Supplier,
    stock: i64,
) -> (i64, i64, i64) {
    let conn = app
        .connection(base, json!({"retry_max": 0, "retry_intervals": "[30]"}))
        .await;
    data(
        &app.admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}/status"),
            Some(json!({"status": "active"})),
        )
        .await,
    );
    supplier.set_products(vec![remote_product(
        9,
        "5.00",
        json!([{"id": 101, "sku_code": "A", "price_amount": "5.00", "stock_quantity": stock, "is_active": true}]),
    )]);
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 9})),
        )
        .await;
    let product_id = data(&res)["local_product_id"].as_i64().unwrap();
    let cat = app
        .admin(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"name": {"zh-CN": "up"}, "slug": "up", "parent_id": 0})),
        )
        .await;
    let mut row: products::ActiveModel = products::Entity::find_by_id(product_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    row.is_active = Set(true);
    row.category_id = Set(data(&cat)["id"].as_i64().unwrap());
    row.update(&app.db).await.unwrap();
    let sku_id = product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    (conn, product_id, sku_id)
}

async fn place(app: &IntApp, token: &str, product: i64, sku: i64) -> Value {
    app.call(
        "POST",
        "/api/v1/orders",
        Some(json!({"items": [{"product_id": product, "sku_id": sku, "quantity": 1}]})),
        Some(token),
    )
    .await
}

/// Makes the cached supplier stock look `minutes` old.
async fn age_stock_cache(app: &IntApp, minutes: i64) {
    for m in sku_mappings::Entity::find().all(&app.db).await.unwrap() {
        let mut row: sku_mappings::ActiveModel = m.into();
        row.stock_synced_at = Set(Some(Utc::now() - chrono::Duration::minutes(minutes)));
        row.update(&app.db).await.unwrap();
    }
}

// LQA-I3 (I-007): the pre-order guard does not trust a 5-minute-old "in stock": the
// supplier sold out since the last sync, so the checkout asks it again and refuses with
// a localized message before the buyer pays. A fresh cache is trusted as before.
#[tokio::test]
async fn lqa_i3_stale_stock_cache_is_rechecked_before_payment() {
    let (base, supplier) = start_supplier().await;
    let app = app().await;
    let (_, product, sku) = imported(&app, &base, &supplier, 5).await;
    let (_, token) = app.user("shopper@example.com").await;
    // Supplier sells out; our cache (just synced) still says 5.
    supplier.set_products(vec![remote_product(
        9,
        "5.00",
        json!([{"id": 101, "sku_code": "A", "price_amount": "5.00", "stock_quantity": 0, "is_active": true}]),
    )]);
    let fetches = || {
        supplier
            .paths()
            .iter()
            .filter(|p| p.starts_with("/api/v1/upstream/products"))
            .count()
    };
    let before = fetches();
    data(&place(&app, &token, product, sku).await);
    assert_eq!(fetches(), before, "a fresh cache is trusted");

    age_stock_cache(&app, 5).await;
    let refused = place(&app, &token, product, sku).await;
    assert_eq!(
        refused["status_code"],
        400,
        "{refused} {:?}",
        supplier.paths()
    );
    assert_eq!(refused["msg"], "上游库存不足");
    assert!(fetches() > before, "stale cache re-checked at the supplier");
}

// LQA-I3 (I-007 / I-076): a purchase the supplier rejects leaves the paid order flagged in
// the admin order list (`procurement_issue`), and the admin cancel refunds the buyer in
// full exactly once: wallet 100.00 → 94.00 → 100.00, order refunded, flag gone.
#[tokio::test]
async fn lqa_i3_rejected_purchase_is_flagged_and_cancel_refunds() {
    let (base, supplier) = start_supplier().await;
    let app = app().await;
    let (_, product, sku) = imported(&app, &base, &supplier, 5).await;
    let (uid, token) = app.user("shopper@example.com").await;
    fund(&app, uid, "100.00").await;
    let created = place(&app, &token, product, sku).await;
    let order_no = data(&created)["order_no"].as_str().unwrap().to_owned();
    let total = data(&created)["total_amount"].as_str().unwrap().to_owned();
    let paid = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0, "use_balance": true})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&paid)["order_paid"], true, "{paid}");
    let after_pay = balance(&app, uid).await;
    assert_eq!(
        after_pay,
        format!("{:.2}", 100.0 - total.parse::<f64>().unwrap())
    );

    // The supplier refuses (sold out) — permanent error, no retry.
    *supplier.order_reply.lock().unwrap() = Some((
        200,
        json!({"ok": false, "error_code": "product_out_of_stock", "error_message": "库存不足"}),
    ));
    let proc = procurement_orders::Entity::find()
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    app.services
        .integration
        .procurement
        .submit(proc.id)
        .await
        .unwrap();
    let proc = procurement_orders::Entity::find_by_id(proc.id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proc.status, "rejected");

    // Flagged in the admin list, and filterable.
    let list = app
        .admin("GET", "/api/v1/admin/orders?procurement_issue=1", None)
        .await;
    let rows = data(&list).as_array().unwrap();
    assert_eq!(rows.len(), 1, "{list}");
    assert_eq!(rows[0]["order_no"], order_no.as_str());
    assert_eq!(rows[0]["procurement_issue"], "rejected");
    assert_eq!(rows[0]["status"], "paid");

    // Cancel = give up: full refund to the wallet.
    let canceled = app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{}/cancel", proc.id),
            None,
        )
        .await;
    assert_eq!(data(&canceled)["refund_type"], "wallet", "{canceled}");
    assert_eq!(data(&canceled)["refund_amount"], total.as_str());
    assert_eq!(balance(&app, uid).await, "100.00");
    let parent = orders::Entity::find()
        .filter(orders::Column::OrderNo.eq(order_no.as_str()))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(parent.status, "refunded");
    let list = app
        .admin("GET", "/api/v1/admin/orders?procurement_issue=1", None)
        .await;
    assert!(data(&list).as_array().unwrap().is_empty(), "{list}");

    // A second cancel (or a retry after the refund) never pays twice.
    let again = app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{}/cancel", proc.id),
            None,
        )
        .await;
    assert_ne!(again["status_code"], 0);
    assert_eq!(balance(&app, uid).await, "100.00");
}

/// A host answering every request with an HTML page (QA pointed connections at a public
/// static docs site to simulate an outage): `http://127.0.0.1:port`.
async fn html_host() -> String {
    use axum::response::Html;
    let app = axum::Router::new()
        .fallback(|| async { Html("<!doctype html><html><body><h1>Docs</h1></body></html>") });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    format!("http://{addr}")
}

fn endpoint(base: &str, protocol: &str) -> zs_domain::integration::connection::Endpoint {
    zs_domain::integration::connection::Endpoint {
        connection_id: 1,
        base_url: base.into(),
        api_key: "15".into(),
        api_secret: "k".repeat(32),
        protocol: protocol.into(),
        features: None,
        extra: serde_json::Map::new(),
    }
}

fn order_of(line_sku: i64) -> zs_domain::integration::adapter::PlaceOrder {
    zs_domain::integration::adapter::PlaceOrder {
        lines: vec![zs_domain::integration::adapter::OrderLine {
            sku_id: line_sku,
            quantity: 1,
            manual_form_data: None,
        }],
        downstream_order_no: "DJ1-01".into(),
        trace_id: "t".into(),
        callback_url: String::new(),
        idempotency_key: "procurement:1".into(),
        quote_id: None,
    }
}

// LQA-I4 (I-008 / I-075): the exact outage QA staged — the supplier URL answers HTML.
// The read-only step before ordering (zebra-store quote, acg-faka commodity lookup)
// fails as "nothing placed" (so the purchase is rejected, rolled back and flagged — never
// "may have been executed"); the order call itself answering HTML is "maybe executed"
// and goes to manual review.
#[tokio::test]
async fn lqa_i4_html_answering_host_is_classified_by_step() {
    use std::sync::Arc;
    use zs_domain::integration::adapter::SupplierAdapter;
    use zs_infra::integration::client::HttpConnector;
    use zs_infra::integration::http::AddressPolicy;
    let base = html_host().await;
    let http = HttpConnector::new(
        AddressPolicy::AllowPrivate,
        Arc::new(zs_shared::clock::SystemClock),
        0,
    );

    let zs = zs_infra::integration::zebra_store::ZebraStoreAdapter::new(http.clone())
        .open(&endpoint(&base, "zebra-store"))
        .unwrap();
    let quote = zs.quote(&order_of(7).lines).await.unwrap_err();
    let settled = quote.not_executed();
    assert!(!settled.may_have_executed());
    let text = settled.to_string();
    assert!(!text.contains("may have been executed"), "{text}");
    assert!(text.contains("nothing placed"), "{text}");
    let placed = zs.place_order(&order_of(7)).await.unwrap_err();
    assert!(placed.may_have_executed(), "{placed}");

    let acg = zs_infra::integration::acg_faka::AcgFakaAdapter::new(http)
        .open(&endpoint(&base, "acg-faka"))
        .unwrap();
    match acg.place_order(&order_of(7)).await {
        Err(e) => {
            assert!(!e.may_have_executed(), "{e}");
            assert!(!e.to_string().contains("may have been executed"), "{e}");
        }
        Ok(r) => assert!(!r.ok, "an HTML page never confirms a purchase: {r:?}"),
    }
}

async fn import_local_cards(app: &IntApp, product: i64, sku: i64, cards: &[&str]) {
    let created = app
        .admin(
            "POST",
            "/api/v1/admin/card-secrets/batch",
            Some(json!({"product_id":product,"sku_id":sku,"secrets":cards})),
        )
        .await;
    assert_eq!(data(&created)["created"], cards.len());
}
async fn local_first_order(
    app: &IntApp,
    token: &str,
    product: i64,
    sku: i64,
    quantity: i32,
) -> Value {
    app.call(
        "POST",
        "/api/v1/orders",
        Some(json!({"items":[{"product_id":product,"sku_id":sku,"quantity":quantity}]})),
        Some(token),
    )
    .await
}

// DLV-03 / UPS-09: local imports need no converter or special source marker.
#[tokio::test]
async fn local_first_splits_shortfall_and_procures_only_missing_quantity() {
    for defer_stock in [false, true] {
        use zs_infra::db::entity::{card_secrets, fulfillments, order_items};
        let (base, supplier) = start_supplier().await;
        let app = app().await;
        let (_, product, sku) = imported(&app, &base, &supplier, 1).await;
        data(&app.admin("PATCH", &format!("/api/v1/admin/products/{product}/wholesale-prices"), Some(json!({"wholesale_prices":[{"sku_id":sku,"min_quantity":3,"unit_price":"4.00"}]}))).await);
        data(
            &app.admin(
                "PUT",
                "/api/v1/admin/settings",
                Some(
                    json!({"key":"order_config","value":{"reserve_stock_on_payment":defer_stock}}),
                ),
            )
            .await,
        );
        import_local_cards(&app, product, sku, &["PREBOUGHT-A", "PREBOUGHT-B"]).await;
        let (uid, token) = app.user("local-first@example.com").await;
        fund(&app, uid, "100.00").await;
        let created = local_first_order(&app, &token, product, sku, 3).await;
        assert_eq!(data(&created)["total_amount"], "12.00");
        let no = data(&created)["order_no"].as_str().unwrap();
        let parent = orders::Entity::find()
            .filter(orders::Column::OrderNo.eq(no))
            .one(&app.db)
            .await
            .unwrap()
            .unwrap();
        let children = orders::Entity::find()
            .filter(orders::Column::ParentId.eq(parent.id))
            .all(&app.db)
            .await
            .unwrap();
        assert_eq!(children.len(), 2);
        let items = order_items::Entity::find()
            .filter(order_items::Column::OrderId.is_in(children.iter().map(|c| c.id)))
            .all(&app.db)
            .await
            .unwrap();
        let local = items.iter().find(|i| i.fulfillment_type == "auto").unwrap();
        let remote = items
            .iter()
            .find(|i| i.fulfillment_type == "upstream")
            .unwrap();
        assert_eq!(local.quantity, 2);
        assert_eq!(remote.quantity, 1);
        assert_eq!(
            children
                .iter()
                .find(|c| c.id == local.order_id)
                .unwrap()
                .total_amount,
            sea_orm::prelude::Decimal::from(8)
        );
        assert_eq!(
            children
                .iter()
                .find(|c| c.id == remote.order_id)
                .unwrap()
                .total_amount,
            sea_orm::prelude::Decimal::from(4)
        );
        let cards = card_secrets::Entity::find()
            .filter(card_secrets::Column::ProductId.eq(product))
            .all(&app.db)
            .await
            .unwrap();
        if defer_stock {
            assert!(
                cards
                    .iter()
                    .all(|c| c.status == "available" && c.order_id.is_none())
            );
        } else {
            assert!(
                cards
                    .iter()
                    .all(|c| c.status == "reserved" && c.order_id == Some(local.order_id))
            );
        }
        let paid = app
            .call(
                "POST",
                "/api/v1/payments",
                Some(json!({"order_no":no,"channel_id":0,"use_balance":true})),
                Some(&token),
            )
            .await;
        assert_eq!(data(&paid)["order_paid"], true);
        app.services
            .order
            .service
            .auto_fulfill(local.order_id)
            .await
            .unwrap();
        let delivered = fulfillments::Entity::find()
            .filter(fulfillments::Column::OrderId.eq(local.order_id))
            .one(&app.db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(delivered.payload, "PREBOUGHT-A\nPREBOUGHT-B");
        let procurement = procurement_orders::Entity::find()
            .all(&app.db)
            .await
            .unwrap();
        assert_eq!(procurement.len(), 1);
        assert_eq!(procurement[0].local_order_id, remote.order_id);
        app.services
            .integration
            .procurement
            .submit(procurement[0].id)
            .await
            .unwrap();
        let requests = supplier.hits();
        let purchase = requests
            .iter()
            .find(|h| h.method == "POST" && h.path.ends_with("/orders"))
            .unwrap();
        let body: Value = serde_json::from_str(&purchase.body).unwrap();
        assert_eq!(body["quantity"], 1);
        let cards = card_secrets::Entity::find()
            .filter(card_secrets::Column::ProductId.eq(product))
            .all(&app.db)
            .await
            .unwrap();
        assert!(cards.iter().all(|c| c.status == "used"));
    }
}

#[tokio::test]
async fn local_first_sells_with_empty_supplier_and_cancel_releases_local_reservations() {
    use zs_infra::db::entity::{card_secrets, order_items};
    let (base, supplier) = start_supplier().await;
    let app = app().await;
    let (_, product, sku) = imported(&app, &base, &supplier, 0).await;
    import_local_cards(&app, product, sku, &["LOCAL-ONLY-A", "LOCAL-ONLY-B"]).await;
    let slug = products::Entity::find_by_id(product)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .slug;
    let public = app
        .call(
            "GET",
            &format!("/api/v1/public/products/{slug}"),
            None,
            None,
        )
        .await;
    assert_eq!(data(&public)["auto_stock_available"], 2);
    assert_eq!(data(&public)["is_sold_out"], false);
    let (_, token) = app.user("local-only@example.com").await;
    let created = local_first_order(&app, &token, product, sku, 2).await;
    let no = data(&created)["order_no"].as_str().unwrap();
    let parent = orders::Entity::find()
        .filter(orders::Column::OrderNo.eq(no))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let child = orders::Entity::find()
        .filter(orders::Column::ParentId.eq(parent.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let item = order_items::Entity::find()
        .filter(order_items::Column::OrderId.eq(child.id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(item.fulfillment_type, "auto");
    assert_eq!(item.quantity, 2);
    data(
        &app.call(
            "POST",
            &format!("/api/v1/orders/{no}/cancel"),
            None,
            Some(&token),
        )
        .await,
    );
    let cards = card_secrets::Entity::find()
        .filter(card_secrets::Column::ProductId.eq(product))
        .all(&app.db)
        .await
        .unwrap();
    assert!(
        cards
            .iter()
            .all(|c| c.status == "available" && c.order_id.is_none())
    );
    assert!(
        !supplier
            .hits()
            .iter()
            .any(|h| h.method == "POST" && h.path.ends_with("/orders"))
    );
}

// DLV-03: concurrent mapped-product orders cannot reserve the same local cards.
#[tokio::test]
async fn local_first_concurrent_checkout_never_double_reserves() {
    use zs_infra::db::entity::card_secrets;
    let (base, supplier) = start_supplier().await;
    let app = app().await;
    let (_, product, sku) = imported(&app, &base, &supplier, 0).await;
    import_local_cards(&app, product, sku, &["CONCURRENT-ONLY-CARD"]).await;
    let (_, a) = app.user("local-concurrent-a@example.com").await;
    let (_, b) = app.user("local-concurrent-b@example.com").await;
    let (first, second) = tokio::join!(
        local_first_order(&app, &a, product, sku, 1),
        local_first_order(&app, &b, product, sku, 1)
    );
    assert_eq!(
        [&first, &second]
            .iter()
            .filter(|r| r["status_code"] == 0)
            .count(),
        1
    );
    let rows = card_secrets::Entity::find()
        .filter(card_secrets::Column::ProductId.eq(product))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "reserved");
    assert!(rows[0].order_id.is_some());
}
