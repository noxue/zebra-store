//! Regression tests for checklist items of `bugfix-lessons.md` §21 on site integration
//! that had no coverage: UPS-09, UPS-15, RISK-03.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

mod integration_common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::Request;
use integration_common::{IntApp, data, remote_product, start_supplier};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use zs_infra::db::entity::{api_credentials, products, sku_mappings};

/// UPS-09: a mapped product always stays `upstream`, whatever an admin update sends.
#[tokio::test]
async fn ups_09_mapped_product_fulfillment_type_stays_upstream() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let conn = app.connection(&base, json!({})).await;
    supplier.set_products(vec![remote_product(
        9,
        "1.00",
        json!([{"id": 101, "sku_code": "A", "spec_values": {"zh-CN": "a"}, "price_amount": "1.00",
            "stock_quantity": 5, "is_active": true}]),
    )]);
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 9})),
        )
        .await;
    let pid = data(&res)["local_product_id"].as_i64().unwrap();
    let row = products::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.fulfillment_type, "upstream");

    // imports arrive without a local category; the admin assigns one while editing
    let cat = app
        .admin(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"name": {"zh-CN": "cat"}, "slug": "cat", "parent_id": 0})),
        )
        .await;
    let body = json!({
        "category_id": data(&cat)["id"],
        "slug": row.slug,
        "title": row.title_json,
        "price_amount": row.price_amount.to_string(),
        "fulfillment_type": "auto",
    });
    let updated = app
        .admin("PUT", &format!("/api/v1/admin/products/{pid}"), Some(body))
        .await;
    // (the admin DTO may display the upstream's real type, UPS-19; storage is what counts)
    assert_eq!(data(&updated)["id"], pid, "{updated}");
    let reread = app
        .admin("GET", &format!("/api/v1/admin/products/{pid}"), None)
        .await;
    assert_eq!(data(&reread)["id"], pid);
    let stored = products::Entity::find_by_id(pid)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.fulfillment_type, "upstream");
    assert!(stored.is_mapped);
}

/// Signed `GET /api/v1/channel{path}` with a fresh channel client.
async fn channel_get(app: &IntApp, creds: &(String, String), path: &str) -> Value {
    let full = format!("/api/v1/channel{path}");
    let ts = chrono::Utc::now().timestamp();
    let signature = zs_shared::sign::sign(&creds.1, "GET", &full, ts, b"");
    let req = Request::builder()
        .method("GET")
        .uri(&full)
        .header("Dujiao-Next-Channel-Key", &creds.0)
        .header("Dujiao-Next-Channel-Timestamp", ts.to_string())
        .header("Dujiao-Next-Channel-Signature", signature)
        .body(Body::empty())
        .unwrap();
    let (_, _, body) = app.send(req).await;
    serde_json::from_slice(&body).unwrap()
}

/// UPS-15: the bot/channel catalog shares the storefront stock decoration of mapped
/// products: upstream SKU stock 5 and -1 (unlimited) → the product is unlimited (-1),
/// each SKU shows its upstream stock; with every SKU mapping inactive → out of stock.
#[tokio::test]
async fn ups_15_channel_catalog_uses_upstream_stock() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let conn = app.connection(&base, json!({})).await;
    supplier.set_products(vec![remote_product(
        15,
        "2.00",
        json!([
            {"id": 151, "sku_code": "A", "spec_values": {"zh-CN": "a"}, "price_amount": "2.00",
                "stock_quantity": 5, "is_active": true},
            {"id": 152, "sku_code": "B", "spec_values": {"zh-CN": "b"}, "price_amount": "3.00",
                "stock_quantity": -1, "is_active": true}
        ]),
    )]);
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/import",
            Some(json!({"connection_id": conn, "upstream_product_id": 15})),
        )
        .await;
    let pid = data(&res)["local_product_id"].as_i64().unwrap();
    // imports arrive inactive and uncategorised; list it like an admin would
    let cat = app
        .admin(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"name": {"zh-CN": "cat"}, "slug": "ups15", "parent_id": 0})),
        )
        .await;
    products::Entity::update_many()
        .col_expr(products::Column::IsActive, Expr::value(true))
        .col_expr(
            products::Column::CategoryId,
            Expr::value(data(&cat)["id"].as_i64().unwrap()),
        )
        .filter(products::Column::Id.eq(pid))
        .exec(&app.db)
        .await
        .unwrap();
    let client = app
        .admin(
            "POST",
            "/api/v1/admin/channel-clients",
            Some(json!({"name": "Bot", "channel_type": "telegram_bot"})),
        )
        .await;
    let creds = (
        data(&client)["channel_key"].as_str().unwrap().to_owned(),
        data(&client)["channel_secret"].as_str().unwrap().to_owned(),
    );

    let detail = channel_get(&app, &creds, &format!("/catalog/products/{pid}")).await;
    let d = data(&detail);
    assert_eq!(d["stock_count"], -1, "{detail}");
    assert_eq!(d["stock_status"], "unlimited", "{detail}");
    let skus = d["skus"].as_array().unwrap();
    let count_of = |code: &str| {
        skus.iter()
            .find(|s| s["sku_code"] == code)
            .map(|s| (s["stock_count"].clone(), s["stock_status"].clone()))
            .unwrap_or_else(|| panic!("{code}: {detail}"))
    };
    let (count, status) = count_of("A");
    assert_eq!(count, json!(5));
    assert_ne!(status, json!("out_of_stock"), "{detail}");
    assert_eq!(count_of("B").0, json!(-1));
    let list = channel_get(&app, &creds, "/catalog/products").await;
    let item = data(&list)["items"]
        .as_array()
        .and_then(|l| l.iter().find(|p| p["id"] == pid).cloned())
        .unwrap_or_else(|| panic!("{list}"));
    assert_eq!(item["stock_status"], "unlimited", "{list}");

    // every SKU mapping inactive → sold out (not "unlimited" from stale stock)
    sku_mappings::Entity::update_many()
        .col_expr(sku_mappings::Column::UpstreamIsActive, Expr::value(false))
        .exec(&app.db)
        .await
        .unwrap();
    let detail = channel_get(&app, &creds, &format!("/catalog/products/{pid}")).await;
    assert_eq!(data(&detail)["stock_status"], "out_of_stock", "{detail}");
}

/// Signed upstream call from `ip`.
async fn upstream_from(app: &IntApp, ip: &str, key: &str, secret: &str) -> u16 {
    let path = "/api/v1/upstream/ping";
    let ts = chrono::Utc::now().timestamp();
    let sig = zs_shared::sign::sign(secret, "POST", path, ts, b"");
    let mut req = Request::builder()
        .method("POST")
        .uri(path)
        .header(zs_shared::sign::HEADER_API_KEY, key)
        .header(zs_shared::sign::HEADER_TIMESTAMP, ts.to_string())
        .header(zs_shared::sign::HEADER_SIGNATURE, sig)
        .body(Body::empty())
        .unwrap();
    let addr: SocketAddr = format!("{ip}:40000").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    app.send(req).await.0.as_u16()
}

/// RISK-03 ①: the pre-auth limit key is `IP|API key` (header truncated to 128 bytes, like
/// the original `KeyByIPAndHeader`): a victim's key is exhausted only from the caller's
/// own IP, never from another address, and padding a key past 128 bytes does not buy a
/// fresh bucket. (The original also gives each distinct key its own bucket, so rotating
/// random keys is bounded by the auth failures, not by this limiter.)
#[tokio::test]
async fn risk_03_rate_limit_key_is_bound_to_the_ip() {
    let app = IntApp::new().await;
    let (_, _, _, key, secret) = app.buyer("victim@example.com").await;
    let attacker = "203.0.113.7";
    let mut statuses = Vec::new();
    for _ in 0..61 {
        statuses.push(upstream_from(&app, attacker, &key, "wrong-secret").await);
    }
    assert!(statuses[..60].iter().all(|s| *s == 401), "{statuses:?}");
    assert_eq!(statuses[60], 429);
    // the owner, calling from elsewhere, is unaffected
    assert_eq!(
        upstream_from(&app, "198.51.100.9", &key, &secret).await,
        200
    );

    let long_a = format!("{}A", "k".repeat(128));
    let long_b = format!("{}B", "k".repeat(128));
    for _ in 0..60 {
        upstream_from(&app, "192.0.2.1", &long_a, "x").await;
    }
    assert_eq!(upstream_from(&app, "192.0.2.1", &long_b, "x").await, 429);
}

/// RISK-03 ②③: authentication writes only `last_used_at`, at most once a minute, so a
/// concurrent admin disable is never overwritten by the request's stale snapshot.
#[tokio::test]
async fn risk_03_last_used_at_is_a_throttled_single_column_write() {
    let app = IntApp::new().await;
    let (_, _, cid, key, secret) = app.buyer("touch@example.com").await;
    let stored = |app: &IntApp| {
        let db = app.db.clone();
        async move {
            api_credentials::Entity::find_by_id(cid)
                .one(&db)
                .await
                .unwrap()
                .unwrap()
        }
    };
    assert_eq!(upstream_from(&app, "10.1.1.1", &key, &secret).await, 200);
    let first = stored(&app).await.last_used_at.unwrap();
    assert_eq!(upstream_from(&app, "10.1.1.1", &key, &secret).await, 200);
    assert_eq!(
        stored(&app).await.last_used_at,
        Some(first),
        "throttled within 60 s"
    );

    // make the next request touch again, racing with an admin disable
    api_credentials::Entity::update_many()
        .col_expr(
            api_credentials::Column::LastUsedAt,
            Expr::value(Option::<chrono::DateTime<chrono::Utc>>::None),
        )
        .filter(api_credentials::Column::Id.eq(cid))
        .exec(&app.db)
        .await
        .unwrap();
    let calls = (0..8).map(|_| upstream_from(&app, "10.1.1.2", &key, &secret));
    let status_url = format!("/api/v1/admin/api-credentials/{cid}/status");
    let disable = app.admin("PUT", &status_url, Some(json!({"is_active": false})));
    let (_, disabled) = tokio::join!(run_all(calls), disable);
    data(&disabled);
    let row = stored(&app).await;
    assert!(!row.is_active, "the disable survives concurrent requests");
    assert_eq!(upstream_from(&app, "10.1.1.3", &key, &secret).await, 403);
}

async fn run_all<F: std::future::Future<Output = u16>>(calls: impl Iterator<Item = F>) -> Vec<u16> {
    let mut out = Vec::new();
    let handles: Vec<F> = calls.collect();
    for h in handles {
        out.push(h.await);
    }
    out
}
