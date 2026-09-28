//! Regression tests for catalog checklist items of `bugfix-lessons.md` §21 that had only
//! partial or no coverage: ORD-05 (④ cascade of a clean product delete), MISC-04.

mod catalog_common;
mod common;

use catalog_common::{category, id_of, product, user};
use chrono::Utc;
use common::{TestApp, data};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde_json::json;
use zs_infra::db::entity::{
    card_secret_batches, cart_items, product_mappings, product_skus, products, sku_mappings,
};

/// Live (not soft-deleted) rows of an entity for the product.
macro_rules! live {
    ($app:expr, $entity:ident, $col:ident, $id:expr) => {
        $entity::Entity::find()
            .filter($entity::Column::$col.eq($id))
            .filter($entity::Column::DeletedAt.is_null())
            .count(&$app.db)
            .await
            .unwrap()
    };
}

/// ORD-05 ④: deleting a clean product removes its SKUs, cart lines, card secret batches,
/// product mapping and SKU mappings in the same transaction.
#[tokio::test]
async fn ord_05_clean_delete_cascades_to_dependents() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let created = product(&app, cat, "clean", json!({"fulfillment_type": "auto"})).await;
    let pid = id_of(&created);
    let sku = created["skus"][0]["id"].as_i64().unwrap();
    let uid = user(&app, "cart@example.com", 0).await;
    let now = Utc::now();
    cart_items::ActiveModel {
        user_id: Set(uid),
        product_id: Set(pid),
        sku_id: Set(sku),
        quantity: Set(2),
        fulfillment_type: Set("auto".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    card_secret_batches::ActiveModel {
        product_id: Set(pid),
        sku_id: Set(sku),
        batch_no: Set("B1".into()),
        source: Set("manual".into()),
        total_count: Set(0),
        note: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let mapping = product_mappings::ActiveModel {
        connection_id: Set(1),
        local_product_id: Set(pid),
        upstream_product_id: Set(77),
        upstream_fulfillment_type: Set("auto".into()),
        upstream_status: Set("active".into()),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id;
    sku_mappings::ActiveModel {
        product_mapping_id: Set(mapping),
        local_sku_id: Set(sku),
        upstream_sku_id: Set(701),
        upstream_price: Set(catalog_common::dec("1.00")),
        upstream_stock: Set(5),
        upstream_is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();

    data(&app.delete(&format!("/api/v1/admin/products/{pid}")).await);

    assert_eq!(live!(app, products, Id, pid), 0);
    assert_eq!(live!(app, product_skus, ProductId, pid), 0);
    assert_eq!(live!(app, cart_items, ProductId, pid), 0);
    assert_eq!(live!(app, card_secret_batches, ProductId, pid), 0);
    assert_eq!(live!(app, product_mappings, LocalProductId, pid), 0);
    assert_eq!(live!(app, sku_mappings, ProductMappingId, mapping), 0);
}

/// MISC-04: the admin detail returns every SKU (disabled ones flagged), the storefront
/// detail only the active ones.
#[tokio::test]
async fn misc_04_admin_detail_lists_disabled_skus() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let created = product(
        &app,
        cat,
        "multi",
        json!({"is_active": true, "skus": [
            {"sku_code": "A", "price_amount": 10, "is_active": true},
            {"sku_code": "B", "price_amount": 12, "is_active": false},
        ]}),
    )
    .await;
    let pid = id_of(&created);
    let admin = app.get(&format!("/api/v1/admin/products/{pid}")).await;
    let skus = data(&admin)["skus"].as_array().unwrap().clone();
    assert_eq!(skus.len(), 2, "{admin}");
    let b = skus.iter().find(|s| s["sku_code"] == "B").unwrap();
    assert_eq!(b["is_active"], false);

    let public = app.get("/api/v1/public/products/multi").await;
    let skus = data(&public)["skus"].as_array().unwrap().clone();
    let codes: Vec<&str> = skus.iter().filter_map(|s| s["sku_code"].as_str()).collect();
    assert_eq!(codes, ["A"], "{public}");
}
