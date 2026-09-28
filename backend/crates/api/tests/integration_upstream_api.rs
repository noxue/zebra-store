//! The upstream API we serve (`/api/v1/upstream/*`): HMAC auth, error bodies with real
//! HTTP codes, rate limit, catalog projection and order endpoints (UPS-10/14/18/19).

mod integration_common;

use integration_common::{IntApp, data, seed_product};
use serde_json::json;

#[tokio::test]
async fn ups10_authentication_failures() {
    let app = IntApp::new().await;
    let (uid, token, cid, key, secret) = app.buyer("a@example.com").await;

    // Missing headers.
    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/v1/upstream/ping")
        .body(axum::body::Body::empty())
        .unwrap();
    let (status, _, body) = app.send(req).await;
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(status, 401);
    assert_eq!(
        body,
        json!({"ok": false, "error_code": "missing_auth_headers", "error_message": "missing authentication headers"})
    );

    // Wrong signature.
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, "wrong", 0)
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (401, Some("invalid_signature"))
    );
    // ±60 s window: 61 s skew is refused.
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, -61)
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (401, Some("timestamp_expired"))
    );
    let (s, _) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, -55)
        .await;
    assert_eq!(s, 200);
    // Unknown key.
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, "nope", &secret, 0)
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (403, Some("invalid_api_key"))
    );

    // Disabled credential.
    let off = app
        .call(
            "PUT",
            "/api/v1/api-credential/status",
            Some(json!({"is_active": false})),
            Some(&token),
        )
        .await;
    data(&off);
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, 0)
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (403, Some("invalid_api_key"))
    );
    let on = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/api-credentials/{cid}/status"),
            Some(json!({"is_active": true})),
        )
        .await;
    data(&on);

    // Disabled owner.
    use sea_orm::{ActiveModelTrait, Set};
    zs_infra::db::entity::users::ActiveModel {
        id: Set(uid),
        status: Set("disabled".into()),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, 0)
        .await;
    assert_eq!((s, b["error_code"].as_str()), (403, Some("user_disabled")));
}

// UPS-18: a pending credential's key cannot call the API.
#[tokio::test]
async fn ups18_pending_key_is_refused() {
    let app = IntApp::new().await;
    let (uid, token) = app.user("p@example.com").await;
    data(
        &app.call("POST", "/api/v1/api-credential/apply", None, Some(&token))
            .await,
    );
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let row = zs_infra::db::entity::api_credentials::Entity::find()
        .filter(zs_infra::db::entity::api_credentials::Column::UserId.eq(uid))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &row.api_key, "", 0)
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (403, Some("invalid_api_key"))
    );
}

// UPS-10: 60 requests per minute per IP|key; the 61st is 429.
#[tokio::test]
async fn ups10_rate_limit() {
    let app = IntApp::new().await;
    let (_, _, _, key, secret) = app.buyer("rl@example.com").await;
    for i in 0..60 {
        let (s, _) = app
            .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, 0)
            .await;
        assert_eq!(s, 200, "request {i}");
    }
    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, 0)
        .await;
    assert_eq!(s, 429, "{b}");
}

#[tokio::test]
async fn ping_and_catalog() {
    let app = IntApp::new().await;
    let (uid, _, _, key, secret) = app.buyer("c@example.com").await;
    app.set_setting(
        "site_config",
        json!({"site_name": "Zebra", "currency": "usd"}),
    )
    .await;

    let (s, b) = app
        .upstream("POST", "/api/v1/upstream/ping", None, &key, &secret, 0)
        .await;
    assert_eq!(s, 200);
    assert_eq!(b["ok"], true);
    assert_eq!(b["site_name"], "Zebra");
    assert_eq!(b["protocol_version"], "1.0");
    assert_eq!(b["user_id"], uid);
    assert_eq!(b["balance"], "0.00");
    assert_eq!(b["currency"], "USD");

    let (pid, skus) = seed_product(
        &app.db,
        "p1",
        "manual",
        &[("A", "12.5", true), ("B", "20", false)],
    )
    .await;
    let (off_id, _) = seed_product(&app.db, "p2", "manual", &[("A", "1", true)]).await;
    use sea_orm::{ActiveModelTrait, Set};
    zs_infra::db::entity::products::ActiveModel {
        id: Set(off_id),
        is_active: Set(false),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();

    let (s, b) = app
        .upstream("GET", "/api/v1/upstream/categories", None, &key, &secret, 0)
        .await;
    assert_eq!(s, 200);
    assert_eq!(b["ok"], true);
    assert_eq!(b["categories"].as_array().unwrap().len(), 2);
    assert!(b["categories"][0].get("is_active").is_none());

    let (s, b) = app
        .upstream(
            "GET",
            "/api/v1/upstream/products?page=1&page_size=500",
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(s, 200, "{b}");
    assert_eq!(b["page_size"], 50);
    assert_eq!(b["total"], 1);
    assert_eq!(b["includes_inactive"], false);
    let p = &b["items"][0];
    assert_eq!(p["id"], pid);
    assert_eq!(p["price_amount"], "10.00");
    assert_eq!(p["fulfillment_type"], "manual");
    // Only active SKUs, real remaining stock with the upstream threshold (20).
    assert_eq!(p["skus"].as_array().unwrap().len(), 1);
    assert_eq!(p["skus"][0]["id"], skus[0]);
    assert_eq!(p["skus"][0]["price_amount"], "12.50");
    assert_eq!(p["skus"][0]["stock_quantity"], 30);
    assert_eq!(p["skus"][0]["stock_status"], "in_stock");
    assert!(p.get("cost_price_amount").is_none());

    // include_inactive echoes and lists delisted products (UPS-14).
    let (_, b) = app
        .upstream(
            "GET",
            "/api/v1/upstream/products?include_inactive=true",
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(b["total"], 2);
    assert_eq!(b["includes_inactive"], true);

    // Delisted detail is 200 with is_active=false; unknown is 404 product_not_found.
    let (s, b) = app
        .upstream(
            "GET",
            &format!("/api/v1/upstream/products/{off_id}"),
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(b["product"]["is_active"], false);
    let (s, b) = app
        .upstream(
            "GET",
            "/api/v1/upstream/products/9999",
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (404, Some("product_not_found"))
    );
}

// UPS-19 (1): a mapped product exposes the supplier's type and the synced stock.
#[tokio::test]
async fn ups19_mapped_product_is_masked() {
    let app = IntApp::new().await;
    let (_, _, _, key, secret) = app.buyer("m@example.com").await;
    let (pid, skus) = seed_product(&app.db, "mapped", "upstream", &[("A", "5", true)]).await;
    use sea_orm::{ActiveModelTrait, Set};
    zs_infra::db::entity::products::ActiveModel {
        id: Set(pid),
        is_mapped: Set(true),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let now = chrono::Utc::now();
    let m = zs_infra::db::entity::product_mappings::ActiveModel {
        connection_id: Set(1),
        local_product_id: Set(pid),
        upstream_product_id: Set(77),
        upstream_fulfillment_type: Set("auto".into()),
        upstream_status: Set("active".into()),
        is_active: Set(true),
        last_synced_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    zs_infra::db::entity::sku_mappings::ActiveModel {
        product_mapping_id: Set(m.id),
        local_sku_id: Set(skus[0]),
        upstream_sku_id: Set(5),
        upstream_price: Set("4".parse().unwrap()),
        upstream_stock: Set(7),
        upstream_is_active: Set(true),
        stock_synced_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let (_, b) = app
        .upstream(
            "GET",
            &format!("/api/v1/upstream/products/{pid}"),
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(b["product"]["fulfillment_type"], "auto");
    assert_eq!(b["product"]["skus"][0]["stock_quantity"], 7);
    assert_eq!(b["product"]["skus"][0]["stock_status"], "low_stock");
    assert!(!b.to_string().contains("\"upstream\""));
}

#[tokio::test]
async fn create_order_validation_and_idempotency() {
    let app = IntApp::new().await;
    let (_, _, _, key, secret) = app.buyer("o@example.com").await;
    let (_, skus) = seed_product(
        &app.db,
        "o1",
        "manual",
        &[("A", "5", true), ("B", "5", false)],
    )
    .await;
    let order = |body: serde_json::Value| {
        let app = &app;
        let (key, secret) = (key.clone(), secret.clone());
        async move {
            app.upstream(
                "POST",
                "/api/v1/upstream/orders",
                Some(body),
                &key,
                &secret,
                0,
            )
            .await
        }
    };

    // UPS-10: internal / non-http callback URLs are refused.
    for url in [
        "http://10.0.0.1/x",
        "http://localhost/",
        "ftp://a.com",
        "http://127.0.0.1:6379/",
    ] {
        let (s, b) = order(json!({"sku_id": skus[0], "quantity": 1, "callback_url": url})).await;
        assert_eq!(
            (s, b["error_code"].as_str()),
            (400, Some("invalid_callback_url")),
            "{url}"
        );
    }
    let (s, b) = order(json!({"sku_id": skus[0], "quantity": 0})).await;
    assert_eq!((s, b["error_code"].as_str()), (400, Some("bad_request")));
    let (s, b) = order(json!({"sku_id": 99999, "quantity": 1})).await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("sku_unavailable"))
    );
    let (s, b) = order(json!({"sku_id": skus[1], "quantity": 1})).await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (400, Some("sku_unavailable"))
    );

    // UPS-10: the same downstream order number yields the same order.
    let body = json!({"sku_id": skus[0], "quantity": 1, "downstream_order_no": "D-1", "callback_url": "https://buyer.example.com/cb"});
    let (s, first) = order(body.clone()).await;
    assert_eq!(s, 200, "{first}");
    assert_eq!(first["ok"], true);
    assert_eq!(first["amount"], "10.00");
    let (s, second) = order(body).await;
    assert_eq!(s, 200);
    assert_eq!(second["order_no"], first["order_no"]);
    assert_eq!(*app.ordering.placed.lock().unwrap(), 1);

    // Wallet failure: ok:false, canceled, HTTP 200.
    *app.ordering.fail_payment.lock().unwrap() = true;
    let (s, b) =
        order(json!({"sku_id": skus[0], "quantity": 1, "downstream_order_no": "D-2"})).await;
    assert_eq!(s, 200);
    assert_eq!(b["ok"], false);
    assert_eq!(b["status"], "canceled");
    assert_eq!(b["error_code"], "payment_failed");

    // Order detail / cancel.
    let id = first["order_id"].as_i64().unwrap();
    let (s, b) = app
        .upstream(
            "GET",
            &format!("/api/v1/upstream/orders/{id}"),
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(s, 200);
    assert_eq!(b["order_no"], first["order_no"]);
    assert_eq!(b["refunded_amount"], "0.00");
    let (s, b) = app
        .upstream(
            "GET",
            "/api/v1/upstream/orders/424242",
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (404, Some("order_not_found"))
    );
    let (s, b) = app
        .upstream(
            "POST",
            &format!("/api/v1/upstream/orders/{id}/cancel"),
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(
        (s, b["error_code"].as_str()),
        (409, Some("cancel_not_allowed"))
    );
    let (s, b) = app
        .upstream("GET", "/api/v1/upstream/orders/abc", None, &key, &secret, 0)
        .await;
    assert_eq!((s, b["error_code"].as_str()), (400, Some("bad_request")));
}
