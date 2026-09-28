//! Product mappings against a mock supplier: import (UPS-08/14/20), sync
//! (UPS-04/08/13/14), unavailable / deleted products, stock sync job (UPS-06/17),
//! pre-order stock guard (UPS-06), re-pricing (UPS-05), batch actions.

#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

mod integration_common;

use integration_common::{IntApp, Supplier, data, err, remote_product, start_supplier};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use zs_infra::db::entity::{product_mappings, product_skus, products, sku_mappings};

fn skus2() -> Value {
    json!([
        {"id": 101, "sku_code": "A", "spec_values": {"zh-CN": "a"}, "price_amount": "1.00", "stock_quantity": 5, "is_active": true},
        {"id": 102, "sku_code": "B", "spec_values": {"zh-CN": "b"}, "price_amount": "2.00", "stock_quantity": -1, "is_active": true},
    ])
}

async fn setup(extra: Value) -> (IntApp, Supplier, i64) {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let conn = app.connection(&base, extra).await;
    (app, supplier, conn)
}

async fn import(app: &IntApp, conn: i64, up: i64) -> Value {
    app.admin(
        "POST",
        "/api/v1/admin/product-mappings/import",
        Some(json!({"connection_id": conn, "upstream_product_id": up})),
    )
    .await
}

async fn local_skus(app: &IntApp, product_id: i64) -> Vec<product_skus::Model> {
    product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .all(&app.db)
        .await
        .unwrap()
}

async fn sku_maps(app: &IntApp, mapping_id: i64) -> Vec<sku_mappings::Model> {
    sku_mappings::Entity::find()
        .filter(sku_mappings::Column::ProductMappingId.eq(mapping_id))
        .all(&app.db)
        .await
        .unwrap()
}

async fn product(app: &IntApp, id: i64) -> products::Model {
    products::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}

async fn mapping(app: &IntApp, id: i64) -> product_mappings::Model {
    product_mappings::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}

// UPS-05 / UPS-08 / UPS-14 / UPS-20: import clones product + SKUs + mappings in one go.
#[tokio::test]
async fn import_creates_inactive_upstream_product() {
    let (app, supplier, conn) = setup(json!({"exchange_rate": "7.2", "price_markup_percent": 50, "price_rounding_mode": "ceil_int"})).await;
    supplier.set_products(vec![remote_product(9, "1.00", skus2())]);

    let res = import(&app, conn, 9).await;
    let m = data(&res);
    assert_eq!(m["upstream_product_id"], 9);
    assert_eq!(m["upstream_fulfillment_type"], "auto");
    assert_eq!(m["upstream_status"], "active");
    let pid = m["local_product_id"].as_i64().unwrap();

    let p = product(&app, pid).await;
    assert_eq!(p.fulfillment_type, "upstream");
    assert!(p.is_mapped);
    assert!(!p.is_active, "imported products stay offline");
    // 1.00 × 7.2 = 7.20 cost, +50 % = 10.80 → ceil 11
    assert_eq!(p.price_amount.normalize().to_string(), "11");
    assert_eq!(p.cost_price_amount.normalize().to_string(), "7.2");
    assert!(p.slug.starts_with(&format!("upstream-{conn}-9-")));
    // Images and content images were downloaded from the supplier (same origin only).
    let images: Vec<String> = serde_json::from_value(p.images.unwrap()).unwrap();
    assert!(images[0].starts_with("/uploads/"), "{images:?}");
    let content = p.content_json.unwrap().to_string();
    assert!(
        content.contains("/uploads/") && !content.contains("/images/c.gif"),
        "{content}"
    );

    let skus = local_skus(&app, pid).await;
    assert_eq!(skus.len(), 2);
    let maps = sku_maps(&app, m["id"].as_i64().unwrap()).await;
    assert_eq!(maps.len(), 2);
    let b = maps.iter().find(|s| s.upstream_sku_id == 102).unwrap();
    assert_eq!(b.upstream_stock, -1);
    assert_eq!(b.upstream_price.normalize().to_string(), "2");

    // Already mapped.
    let again = import(&app, conn, 9).await;
    err(&again, 400, "该上游商品已存在映射");
    // Unknown / deleted supplier product.
    supplier.deleted.lock().unwrap().push(10);
    err(&import(&app, conn, 10).await, 404, "上游商品不存在");
    err(&import(&app, conn, 11).await, 404, "上游商品不存在");
    // Unknown connection.
    err(&import(&app, 999, 9).await, 404, "站点连接不存在");

    // Detail with SKU mappings; list with connection and product.
    let mid = m["id"].as_i64().unwrap();
    let detail = app
        .admin(
            "GET",
            &format!("/api/v1/admin/product-mappings/{mid}"),
            None,
        )
        .await;
    assert_eq!(data(&detail)["sku_mappings"].as_array().unwrap().len(), 2);
    let list = app
        .admin(
            "GET",
            "/api/v1/admin/product-mappings?upstream_status=active&product_status=active",
            None,
        )
        .await;
    let items = data(&list).as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["connection"]["id"], conn);
    assert_eq!(items[0]["product"]["id"], pid);
    assert_eq!(items[0]["product"]["skus"].as_array().unwrap().len(), 2);
    let bad = app
        .admin(
            "GET",
            "/api/v1/admin/product-mappings?upstream_status=weird",
            None,
        )
        .await;
    err(&bad, 400, "无效的上游状态参数");
    let bad = app
        .admin(
            "GET",
            "/api/v1/admin/product-mappings?product_status=weird",
            None,
        )
        .await;
    err(&bad, 400, "无效的商品状态参数");
    let found = app
        .admin(
            "GET",
            "/api/v1/admin/product-mappings?search=%E4%B8%8A%E6%B8%B8%E5%95%86%E5%93%81",
            None,
        )
        .await;
    assert_eq!(data(&found).as_array().unwrap().len(), 1);
    let none = app
        .admin("GET", "/api/v1/admin/product-mappings?search=zzz", None)
        .await;
    assert_eq!(data(&none).as_array().unwrap().len(), 0);
}

// UPS-08 (2): an invalid supplier SKU price is not imported at zero.
#[tokio::test]
async fn ups08_import_skips_unpriced_skus() {
    let (app, supplier, conn) = setup(json!({})).await;
    supplier.set_products(vec![remote_product(
        9,
        "N/A",
        json!([
            {"id": 101, "sku_code": "A", "price_amount": "3.00", "stock_quantity": 5, "is_active": true},
            {"id": 102, "sku_code": "B", "price_amount": "abc", "stock_quantity": 5, "is_active": true},
        ]),
    )]);
    let res = import(&app, conn, 9).await;
    let pid = data(&res)["local_product_id"].as_i64().unwrap();
    let skus = local_skus(&app, pid).await;
    assert_eq!(skus.len(), 1);
    assert_eq!(skus[0].price_amount.normalize().to_string(), "3");
    assert_eq!(
        product(&app, pid)
            .await
            .price_amount
            .normalize()
            .to_string(),
        "3"
    );

    supplier.set_products(vec![remote_product(
        10,
        "x",
        json!([{"id": 1, "price_amount": "y", "is_active": true}]),
    )]);
    let res = import(&app, conn, 10).await;
    err(&res, 400, "商品价格或币种不合法");
    // UPS-20: nothing was written for the refused import.
    assert_eq!(
        products::Entity::find().all(&app.db).await.unwrap().len(),
        1
    );
}

// UPS-08 / UPS-14 (2) / UPS-04 / UPS-13: sync aligns SKUs, prices and tiers.
#[tokio::test]
async fn sync_aligns_skus_prices_and_tiers() {
    let (app, supplier, conn) = setup(json!({"exchange_rate": "2", "auto_sync_price": true})).await;
    let mut p = remote_product(9, "1.00", skus2());
    p["wholesale_prices"] = json!([{"sku_id": 101, "min_quantity": 5, "unit_price": "0.80"}]);
    supplier.set_products(vec![p.clone()]);
    let res = import(&app, conn, 9).await;
    let mid = data(&res)["id"].as_i64().unwrap();
    let pid = data(&res)["local_product_id"].as_i64().unwrap();
    let skus = local_skus(&app, pid).await;
    let sku_a = skus.iter().find(|s| s.sku_code == "A").unwrap().id;
    // UPS-04: the tier points at the local SKU, not the supplier's id.
    let tiers = product(&app, pid).await.wholesale_prices.unwrap();
    assert_eq!(tiers[0]["sku_id"], sku_a);
    assert_eq!(tiers[0]["unit_price"], "1.60");

    // Supplier: A price invalid (stock 9), B removed, C new; no tiers sent; schema added.
    p["skus"] = json!([
        {"id": 101, "sku_code": "A", "price_amount": "abc", "stock_quantity": 9, "is_active": true},
        {"id": 103, "sku_code": "C", "price_amount": "3.00", "stock_quantity": 4, "is_active": true},
    ]);
    p["wholesale_prices"] = json!([]);
    p["manual_form_schema"] = json!({"fields": [{"key": "account"}]});
    p["fulfillment_type"] = "manual".into();
    supplier.set_products(vec![p.clone()]);
    let res = app
        .admin(
            "POST",
            &format!("/api/v1/admin/product-mappings/{mid}/sync"),
            None,
        )
        .await;
    assert_eq!(data(&res)["synced"], true);

    let skus = local_skus(&app, pid).await;
    let a = skus.iter().find(|s| s.sku_code == "A").unwrap();
    let b = skus.iter().find(|s| s.sku_code == "B").unwrap();
    let c = skus.iter().find(|s| s.sku_code == "C").unwrap();
    assert_eq!(
        a.price_amount.normalize().to_string(),
        "2",
        "UPS-08: invalid price keeps the local price"
    );
    assert!(!b.is_active, "UPS-14: removed supplier SKU is disabled");
    assert!(c.is_active);
    assert_eq!(c.price_amount.normalize().to_string(), "6");
    let maps = sku_maps(&app, mid).await;
    assert_eq!(
        maps.iter()
            .find(|m| m.upstream_sku_id == 101)
            .unwrap()
            .upstream_stock,
        9
    );
    assert_eq!(
        maps.iter()
            .find(|m| m.upstream_sku_id == 102)
            .unwrap()
            .upstream_stock,
        0
    );
    assert_eq!(
        maps.iter()
            .find(|m| m.upstream_sku_id == 103)
            .unwrap()
            .upstream_stock,
        4
    );
    // UPS-13: no supplier tiers → local tiers kept.
    let tiers = product(&app, pid).await.wholesale_prices.unwrap();
    assert_eq!(tiers[0]["sku_id"], sku_a);
    let prod = product(&app, pid).await;
    assert_eq!(
        prod.manual_form_schema_json.unwrap()["fields"][0]["key"],
        "account"
    );
    assert_eq!(mapping(&app, mid).await.upstream_fulfillment_type, "manual");
    // Product price = lowest active SKU price (A 2.00, C 6.00).
    assert_eq!(prod.price_amount.normalize().to_string(), "2");

    // A valid new price follows with auto_sync_price.
    p["skus"][0]["price_amount"] = "5.00".into();
    supplier.set_products(vec![p]);
    data(
        &app.admin(
            "POST",
            &format!("/api/v1/admin/product-mappings/{mid}/sync"),
            None,
        )
        .await,
    );
    let a = local_skus(&app, pid)
        .await
        .into_iter()
        .find(|s| s.sku_code == "A")
        .unwrap();
    assert_eq!(a.price_amount.normalize().to_string(), "10");
    assert_eq!(a.cost_price_amount.normalize().to_string(), "10");
}

// UPS-14 (1): delisted → inactive (product off); deleted → mapping disabled; relisting
// does not re-activate the local product.
#[tokio::test]
async fn ups14_unavailable_and_deleted() {
    let (app, supplier, conn) = setup(json!({})).await;
    let mut p = remote_product(9, "1.00", skus2());
    supplier.set_products(vec![p.clone()]);
    let res = import(&app, conn, 9).await;
    let mid = data(&res)["id"].as_i64().unwrap();
    let pid = data(&res)["local_product_id"].as_i64().unwrap();
    use sea_orm::{ActiveModelTrait, Set};
    products::ActiveModel {
        id: Set(pid),
        is_active: Set(true),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();

    p["is_active"] = false.into();
    supplier.set_products(vec![p.clone()]);
    data(
        &app.admin(
            "POST",
            &format!("/api/v1/admin/product-mappings/{mid}/sync"),
            None,
        )
        .await,
    );
    assert!(!product(&app, pid).await.is_active);
    let m = mapping(&app, mid).await;
    assert_eq!(m.upstream_status, "inactive");
    assert!(m.is_active);
    assert!(local_skus(&app, pid).await.iter().all(|s| !s.is_active));
    assert!(
        sku_maps(&app, mid)
            .await
            .iter()
            .all(|s| !s.upstream_is_active && s.upstream_stock == 0)
    );

    // Relisted: mapping active again, local product stays offline (admin decides).
    p["is_active"] = true.into();
    supplier.set_products(vec![p]);
    data(
        &app.admin(
            "POST",
            &format!("/api/v1/admin/product-mappings/{mid}/sync"),
            None,
        )
        .await,
    );
    assert_eq!(mapping(&app, mid).await.upstream_status, "active");
    assert!(!product(&app, pid).await.is_active);

    supplier.deleted.lock().unwrap().push(9);
    data(
        &app.admin(
            "POST",
            &format!("/api/v1/admin/product-mappings/{mid}/sync"),
            None,
        )
        .await,
    );
    let m = mapping(&app, mid).await;
    assert_eq!(m.upstream_status, "deleted");
    assert!(!m.is_active);
}

// UPS-06 / UPS-17: batch stock sync with pagination completeness.
#[tokio::test]
async fn ups06_stock_sync_job() {
    let (app, supplier, conn) = setup(json!({"auto_sync_price": true})).await;
    let products_list: Vec<Value> = (1..=3)
        .map(|i| remote_product(i, "1.00", json!([{"id": 100 + i, "sku_code": "S", "price_amount": "1.00", "stock_quantity": 1, "is_active": true}])))
        .collect();
    supplier.set_products(products_list.clone());
    let mut mids = Vec::new();
    for i in 1..=3 {
        mids.push(data(&import(&app, conn, i).await)["id"].as_i64().unwrap());
    }
    app.set_setting(
        "upstream_sync_config",
        json!({"sync_page_size": 10, "sync_max_pages": 10}),
    )
    .await;

    // Supplier total says 12 but the second page is empty → incomplete; product 3
    // missing from the listing must NOT be marked deleted (UPS-06).
    let mut changed = products_list.clone();
    changed[0]["skus"][0]["stock_quantity"] = 42.into();
    changed.truncate(2);
    supplier.set_products(changed);
    *supplier.total_override.lock().unwrap() = Some(12);
    *supplier.truncate_after_page.lock().unwrap() = Some(1);
    let svc = &app.services.integration.mappings;
    let setting = svc.sync_setting().await;
    svc.sync_all_stock(&setting).await.unwrap();
    assert_eq!(sku_maps(&app, mids[0]).await[0].upstream_stock, 42);
    assert_eq!(mapping(&app, mids[2]).await.upstream_status, "active");
    let q = supplier
        .hits()
        .into_iter()
        .find(|h| h.path == "/api/v1/upstream/products")
        .unwrap()
        .query;
    assert!(q.contains("include_inactive=true"), "{q}");

    // Complete listing without product 3 and supplier echoing includes_inactive → deleted.
    *supplier.total_override.lock().unwrap() = None;
    *supplier.truncate_after_page.lock().unwrap() = None;
    // Force a full sync by building a fresh service view (first run of a new process).
    let fresh = IntAppSync::full_sync(&app).await;
    fresh.sync_all_stock(&setting).await.unwrap();
    assert_eq!(mapping(&app, mids[2]).await.upstream_status, "deleted");

    // Old supplier without the echo: missing products are only logged.
    *supplier.echo_inactive.lock().unwrap() = false;
    let (app2, supplier2, conn2) = setup(json!({})).await;
    *supplier2.echo_inactive.lock().unwrap() = false;
    supplier2.set_products(products_list.clone());
    let m2 = data(&import(&app2, conn2, 3).await)["id"].as_i64().unwrap();
    supplier2.set_products(vec![products_list[0].clone()]);
    let svc2 = &app2.services.integration.mappings;
    svc2.sync_all_stock(&svc2.sync_setting().await)
        .await
        .unwrap();
    assert_eq!(mapping(&app2, m2).await.upstream_status, "active");
}

/// Helper giving a mapping service without in-memory sync history.
struct IntAppSync;

impl IntAppSync {
    async fn full_sync(app: &IntApp) -> zs_app::integration::mapping::MappingService {
        let cfg = integration_common::config();
        let ctx = zs_infra::wire::WireCtx::new(&app.db, &cfg);
        zs_infra::wire::integration::build_with(
            &ctx,
            &zs_infra::wire::integration::Adapters {
                address_policy: zs_infra::integration::http::AddressPolicy::AllowPrivate,
                ordering: None,
                lifecycle: None,
                extra_adapters: Vec::new(),
            },
        )
        .mappings
    }
}

// UPS-06: pre-order stock guard (cached → realtime sync → fail-open).
#[tokio::test]
async fn ups06_pre_order_stock_guard() {
    let (app, supplier, conn) = setup(json!({})).await;
    let mut p = remote_product(
        9,
        "1.00",
        json!([{"id": 101, "sku_code": "A", "price_amount": "1.00", "stock_quantity": 1, "is_active": true}]),
    );
    supplier.set_products(vec![p.clone()]);
    let pid = data(&import(&app, conn, 9).await)["local_product_id"]
        .as_i64()
        .unwrap();
    let sku = local_skus(&app, pid).await[0].id;
    let events = app.services.integration.order_events.clone();

    events.ensure_upstream_stock(sku, 1).await.unwrap();
    // Cached 1 < 3 → realtime sync finds 5 → ok.
    p["skus"][0]["stock_quantity"] = 5.into();
    supplier.set_products(vec![p.clone()]);
    events.ensure_upstream_stock(sku, 3).await.unwrap();
    // Cached 5 < 6 → realtime sync finds 2 → insufficient.
    p["skus"][0]["stock_quantity"] = 2.into();
    supplier.set_products(vec![p.clone()]);
    let e = events.ensure_upstream_stock(sku, 6).await.unwrap_err();
    assert_eq!(e.key(), "error.upstream_stock_insufficient");
    // Supplier unreachable → fail-open.
    let res = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}"),
            Some(json!({"base_url": "http://127.0.0.1:9"})),
        )
        .await;
    data(&res);
    events.ensure_upstream_stock(sku, 6).await.unwrap();
    // Unmapped SKU and disabled check pass.
    events.ensure_upstream_stock(424242, 99).await.unwrap();
    app.set_setting(
        "upstream_sync_config",
        json!({"pre_order_stock_check_enabled": false}),
    )
    .await;
    events.ensure_upstream_stock(sku, 99).await.unwrap();
}

// UPS-05 (2): changing the exchange rate re-prices mapped products; cost follows (UPS-08 ④).
#[tokio::test]
async fn ups05_rate_change_reprices() {
    let (app, supplier, conn) = setup(json!({})).await;
    supplier.set_products(vec![remote_product(9, "1.00", json!([{"id": 101, "sku_code": "A", "price_amount": "1.00", "stock_quantity": 1, "is_active": true}]))]);
    let pid = data(&import(&app, conn, 9).await)["local_product_id"]
        .as_i64()
        .unwrap();
    let res = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}"),
            Some(json!({"exchange_rate": "6.9"})),
        )
        .await;
    data(&res);
    let sku = &local_skus(&app, pid).await[0];
    assert_eq!(sku.price_amount.normalize().to_string(), "6.9");
    assert_eq!(sku.cost_price_amount.normalize().to_string(), "6.9");
    assert_eq!(
        product(&app, pid)
            .await
            .price_amount
            .normalize()
            .to_string(),
        "6.9"
    );

    let res = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}"),
            Some(json!({"price_markup_percent": 100})),
        )
        .await;
    data(&res);
    let re = app
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{conn}/reapply-markup"),
            None,
        )
        .await;
    assert_eq!(data(&re)["updated_products"], 1);
    let sku = &local_skus(&app, pid).await[0];
    assert_eq!(sku.price_amount.normalize().to_string(), "13.8");
    assert_eq!(sku.cost_price_amount.normalize().to_string(), "6.9");
}

#[tokio::test]
async fn browse_batch_and_delete() {
    let (app, supplier, conn) = setup(json!({})).await;
    supplier.set_products(
        (1..=3)
            .map(|i| {
                let mut p = remote_product(i, "1.00", skus2());
                p["category_id"] = if i < 3 { 5.into() } else { 6.into() };
                p
            })
            .collect(),
    );
    *supplier.categories.lock().unwrap() = vec![
        json!({"id": 4, "parent_id": 0, "slug": "games", "name": {"zh-CN": "游戏"}}),
        json!({"id": 5, "parent_id": 4, "slug": "steam", "name": {"zh-CN": "Steam"}}),
        json!({"id": 6, "parent_id": 0, "slug": "other", "name": {"zh-CN": "其他"}}),
    ];

    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-products?connection_id={conn}&page=1&page_size=2"),
            None,
        )
        .await;
    assert_eq!(data(&res)["total"], 3);
    assert_eq!(data(&res)["items"].as_array().unwrap().len(), 2);
    assert_eq!(data(&res)["mapped_ids"], json!([]));
    let res = app
        .admin("GET", "/api/v1/admin/upstream-products", None)
        .await;
    err(&res, 400, "请求参数错误");
    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-categories?connection_id={conn}"),
            None,
        )
        .await;
    assert_eq!(data(&res)["supported"], true);
    assert_eq!(data(&res)["categories"].as_array().unwrap().len(), 3);

    // Batch import with automatic categories (parent first).
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/batch-import",
            Some(json!({"connection_id": conn, "upstream_product_ids": [1, 2, 404], "auto_create_category": true})),
        )
        .await;
    let d = data(&res);
    assert_eq!(d["total"], 3);
    assert_eq!(d["success_count"], 2);
    assert_eq!(d["results"][2]["success"], false);
    let cats = zs_infra::db::entity::categories::Entity::find()
        .all(&app.db)
        .await
        .unwrap();
    let steam = cats.iter().find(|c| c.slug == "steam").unwrap();
    let games = cats.iter().find(|c| c.slug == "games").unwrap();
    assert_eq!(steam.parent_id, games.id);

    // By category: product 3 lives in supplier category 6.
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/batch-import-by-category",
            Some(json!({"connection_id": conn, "upstream_category_id": 6, "auto_create_category": true})),
        )
        .await;
    let d = data(&res);
    assert_eq!(d["total"], 1);
    assert_eq!(d["success_count"], 1);
    assert_eq!(d["category_name"], "其他");

    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-products?connection_id={conn}"),
            None,
        )
        .await;
    let mut ids: Vec<i64> = data(&res)["mapped_ids"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect();
    ids.sort_unstable();
    assert_eq!(ids, vec![1, 2, 3]);

    let list = app
        .admin("GET", "/api/v1/admin/product-mappings", None)
        .await;
    let mids: Vec<i64> = data(&list)
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_i64().unwrap())
        .collect();
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/batch-status",
            Some(json!({"ids": mids, "is_active": false})),
        )
        .await;
    assert_eq!(data(&res)["success_count"], 3);
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/batch-sync",
            Some(json!({"ids": [mids[0], 9999]})),
        )
        .await;
    assert_eq!(data(&res), &json!({"total": 2, "success_count": 1}));
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/batch-delete",
            Some(json!({"ids": []})),
        )
        .await;
    err(&res, 400, "IDs: 最小值为 1");

    // Delete: local product loses the mapping and becomes an inactive manual product.
    let pid = mapping(&app, mids[0]).await.local_product_id;
    let res = app
        .admin(
            "DELETE",
            &format!("/api/v1/admin/product-mappings/{}", mids[0]),
            None,
        )
        .await;
    assert_eq!(data(&res)["deleted"], true);
    let p = product(&app, pid).await;
    assert!(!p.is_mapped && !p.is_active);
    assert_eq!(p.fulfillment_type, "manual");
    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/product-mappings/{}", mids[0]),
            None,
        )
        .await;
    err(&res, 404, "商品映射不存在");
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/product-mappings/batch-delete",
            Some(json!({"ids": [mids[1], mids[0]]})),
        )
        .await;
    assert_eq!(data(&res), &json!({"total": 2, "success_count": 1}));
}
