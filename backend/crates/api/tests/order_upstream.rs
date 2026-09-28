//! The order group's integration ports end to end: API buyers ordering through
//! `/api/v1/upstream/orders` (`UpstreamOrdering`, UPS-10/19), and a storefront order of a
//! mapped product going through procurement to a local delivery (`ProcurementLifecycle`,
//! `IntegrationOrderEvents`, UPS-02/07).

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use chrono::Utc;
use integration_common::{
    IntApp, SUP_KEY, SUP_SECRET, callback_request, data, remote_product, seed_product,
    start_supplier,
};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::json;
use zs_infra::db::entity::{
    downstream_order_refs, fulfillments, orders, procurement_orders, product_skus, products,
    wallet_accounts,
};
use zs_infra::integration::http::AddressPolicy;
use zs_infra::wire::integration::Adapters;

/// App with the real order-group ports behind the integration services (loopback
/// suppliers allowed).
async fn app() -> IntApp {
    let cfg = integration_common::config();
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
    let mut services = zs_infra::wire::services(&ctx);
    services.integration = zs_infra::wire::integration::build_with(
        &ctx,
        &Adapters {
            address_policy: AddressPolicy::AllowPrivate,
            ..Adapters::default()
        },
    );
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

async fn manual_stock(app: &IntApp, sku: i64, total: i32) {
    let mut row: product_skus::ActiveModel = product_skus::Entity::find_by_id(sku)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    row.manual_stock_total = Set(total);
    row.update(&app.db).await.unwrap();
}

/// UPS-10 / UPS-19: an API order is created and paid from the buyer's wallet with its
/// downstream reference in the same transaction; a retry answers the same order; a
/// buyer without balance gets `payment_failed` and a canceled order.
#[tokio::test]
async fn ups10_api_buyer_places_wallet_paid_order() {
    let app = app().await;
    let (uid, _, cid, key, secret) = app.buyer("api@example.com").await;
    fund(&app, uid, "20.00").await;
    let (_, skus) = seed_product(&app.db, "m1", "manual", &[("A", "5", true)]).await;
    manual_stock(&app, skus[0], 10).await;

    let body =
        json!({"sku_id": skus[0], "quantity": 2, "downstream_order_no": "D-1", "trace_id": "t-1"});
    let (s, b) = app
        .upstream(
            "POST",
            "/api/v1/upstream/orders",
            Some(body.clone()),
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["ok"], true, "{b}");
    assert_eq!(b["status"], "paid");
    assert_eq!(b["amount"], "10.00");
    let order_id = b["order_id"].as_i64().unwrap();
    assert_eq!(balance(&app, uid).await, "10.00");
    let refs = downstream_order_refs::Entity::find()
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(refs.len(), 1);
    assert_eq!(
        (refs[0].order_id, refs[0].api_credential_id),
        (order_id, cid)
    );
    assert_eq!(refs[0].downstream_order_no, "D-1");

    // retry of the same downstream number: same order, no second charge
    let (s, b) = app
        .upstream(
            "POST",
            "/api/v1/upstream/orders",
            Some(body),
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!((s, b["order_id"].as_i64()), (200, Some(order_id)), "{b}");
    assert_eq!(balance(&app, uid).await, "10.00");

    // detail with items (effective type), cancel of a paid order refused
    let (s, b) = app
        .upstream(
            "GET",
            &format!("/api/v1/upstream/orders/{order_id}"),
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["items"][0]["fulfillment_type"], "manual");
    assert_eq!(b["items"][0]["quantity"], 2);
    let (s, b) = app
        .upstream(
            "POST",
            &format!("/api/v1/upstream/orders/{order_id}/cancel"),
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (409, Some("cancel_not_allowed")),
        "{b}"
    );

    // insufficient balance: payment fails, the order is canceled and the stock released
    let (s, b) = app
        .upstream(
            "POST",
            "/api/v1/upstream/orders",
            Some(json!({"sku_id": skus[0], "quantity": 4, "downstream_order_no": "D-2"})),
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(
        (b["ok"].as_bool(), b["error_code"].as_str()),
        (Some(false), Some("payment_failed")),
        "{b}"
    );
    let failed = orders::Entity::find_by_id(b["order_id"].as_i64().unwrap())
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.status, "canceled");
    assert_eq!(balance(&app, uid).await, "10.00");
    let sku = product_skus::Entity::find_by_id(skus[0])
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (sku.manual_stock_locked, sku.manual_stock_sold),
        (0, 2),
        "only the paid order consumed stock"
    );

    // someone else's order is not found
    let (_, _, _, key2, secret2) = app.buyer("other@example.com").await;
    let (s, _) = app
        .upstream(
            "GET",
            &format!("/api/v1/upstream/orders/{order_id}"),
            None,
            &key2,
            &secret2,
            0,
        )
        .await;
    assert_eq!(s, 404);
}

/// UPS-02 / UPS-07: a paid storefront order of a mapped product creates a purchase order;
/// the supplier accepting moves the local order to fulfilling, its delivery callback
/// writes the local fulfillment once and marks the order delivered.
#[tokio::test]
async fn ups07_paid_upstream_item_is_procured_and_delivered() {
    let (base, supplier) = start_supplier().await;
    let app = app().await;
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
    supplier.set_products(vec![remote_product(
        9,
        "1.00",
        json!([{"id": 101, "sku_code": "A", "price_amount": "1.00", "stock_quantity": 5, "is_active": true}]),
    )]);
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 9})),
        )
        .await;
    let product_id = data(&res)["local_product_id"].as_i64().unwrap();
    // imported products start inactive and uncategorized
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

    let (uid, token) = app.user("shopper@example.com").await;
    fund(&app, uid, "100.00").await;
    let created = app
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": product_id, "sku_id": sku_id, "quantity": 1}]})),
            Some(&token),
        )
        .await;
    let order_no = data(&created)["order_no"].as_str().unwrap().to_owned();
    let paid = app
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0, "use_balance": true})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&paid)["order_paid"], true, "{paid}");
    let parent = orders::Entity::find()
        .filter(orders::Column::OrderNo.eq(order_no.as_str()))
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

    // order_paid created exactly one purchase order for the child
    let all = procurement_orders::Entity::find()
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(all.len(), 1, "one purchase order");
    assert_eq!(all[0].local_order_id, child.id);

    app.services
        .integration
        .procurement
        .submit(all[0].id)
        .await
        .unwrap();
    let child_row = orders::Entity::find_by_id(child.id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child_row.status, "fulfilling", "accepted → fulfilling");

    let body = json!({
        "event": "order.fulfilled",
        "order_id": 88,
        "order_no": "UP88",
        "downstream_order_no": child.order_no,
        "status": "delivered",
        "fulfillment": {"type": "auto", "status": "delivered", "payload": "CODE-1", "delivery_data": {"k": "v"}},
        "timestamp": 1,
    });
    for _ in 0..2 {
        let (status, _, _) = app
            .send(callback_request(SUP_KEY, SUP_SECRET, &body.to_string(), 0))
            .await;
        assert_eq!(status, 200);
    }
    let rows = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.eq(child.id))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(rows.len(), 1, "idempotent delivery");
    assert_eq!(rows[0].payload, "CODE-1");
    assert_eq!(rows[0].type_, "upstream");
    let child_row = orders::Entity::find_by_id(child.id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child_row.status, "delivered");
    let parent_row = orders::Entity::find_by_id(parent.id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(parent_row.status, "delivered");

    // the buyer sees the delivery masked as manual (UPS-19)
    let detail = app
        .call(
            "GET",
            &format!("/api/v1/orders/{order_no}"),
            None,
            Some(&token),
        )
        .await;
    let child_view = &data(&detail)["children"][0];
    assert_eq!(child_view["fulfillment"]["payload"], "CODE-1", "{detail}");
    assert_ne!(child_view["fulfillment"]["type"], "upstream");
}
