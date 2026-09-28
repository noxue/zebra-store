//! Public catalog endpoints: visibility, shape, stock masking and price decoration.

#![expect(clippy::unwrap_used, reason = "tests")]

mod catalog_common;
mod common;

use catalog_common::{category, id_of, insert_secrets, product};
use chrono::Utc;
use common::{TestApp, data};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{post_products, posts};

const PUBLIC: &str = "/api/v1/public/products";

fn slugs(v: &Value) -> Vec<String> {
    let mut s: Vec<String> = data(v)
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["slug"].as_str().unwrap().to_owned())
        .collect();
    s.sort();
    s
}

// ORD-09 (3)
#[tokio::test]
async fn visibility_follows_product_and_category_state() {
    let app = TestApp::new().await;
    let parent = category(&app, "parent", 0).await;
    let child = category(&app, "child", parent).await;
    let other = category(&app, "other", 0).await;
    product(&app, child, "in-child", json!({})).await;
    product(&app, other, "in-other", json!({})).await;
    product(&app, other, "hidden", json!({"is_active": false})).await;

    let all = app.call("GET", PUBLIC, None, None).await;
    assert_eq!(slugs(&all), vec!["in-child", "in-other"]);
    assert_eq!(all["pagination"]["total"], 2);
    let by_parent = app
        .call("GET", &format!("{PUBLIC}?category_id={parent}"), None, None)
        .await;
    assert_eq!(slugs(&by_parent), vec!["in-child"]);
    let bogus = app
        .call("GET", &format!("{PUBLIC}?category_id=abc"), None, None)
        .await;
    assert!(slugs(&bogus).is_empty());
    let search = app
        .call("GET", &format!("{PUBLIC}?search=OTHER"), None, None)
        .await;
    assert_eq!(slugs(&search), vec!["in-other"]);

    let hidden = app
        .call("GET", &format!("{PUBLIC}/hidden"), None, None)
        .await;
    assert_eq!(
        (hidden["status_code"].as_i64(), hidden["msg"].as_str()),
        (Some(404), Some("商品不存在"))
    );

    // disabling the parent hides the child's products (list and detail)
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{child}/active"),
            json!({"is_active": false}),
        )
        .await,
    );
    let after = app.call("GET", PUBLIC, None, None).await;
    assert_eq!(slugs(&after), vec!["in-other"]);
    let detail = app
        .call("GET", &format!("{PUBLIC}/in-child"), None, None)
        .await;
    assert_eq!(detail["status_code"], 404);
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{child}/active"),
            json!({"is_active": true}),
        )
        .await,
    );
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{parent}/active"),
            json!({"is_active": false}),
        )
        .await,
    );
    let by_parent = app
        .call("GET", &format!("{PUBLIC}?category_id={parent}"), None, None)
        .await;
    assert!(slugs(&by_parent).is_empty());
}

// MISC-02, DLV-02, PRC-13, PRC-14, PRC-08
#[tokio::test]
async fn detail_shape_and_prices() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let p = product(
        &app,
        cat,
        "multi",
        json!({"price_amount": 59.9, "cost_price_amount": 1, "instructions": {"zh-CN": "secret"},
               "skus": [
                 {"sku_code": "A", "price_amount": 89.9, "cost_price_amount": 3, "sort_order": 100, "manual_stock_total": 4},
                 {"sku_code": "B", "price_amount": 49.9, "sort_order": 10, "manual_stock_total": 2},
                 {"sku_code": "OFF", "price_amount": 1, "is_active": false}
               ],
               "wholesale_prices": [{"min_quantity": 5, "unit_price": 40}]}),
    )
    .await;
    let id = id_of(&p);
    data(
        &app.post(
            "/api/v1/admin/promotions",
            json!({"name": "minus10", "type": "fixed", "scope_ref_id": id, "value": 10}),
        )
        .await,
    );
    data(
        &app.post(
            "/api/v1/admin/promotions",
            json!({"name": "future", "type": "special_price", "scope_ref_id": id, "value": 1,
               "starts_at": (Utc::now() + chrono::Duration::days(1)).to_rfc3339()}),
        )
        .await,
    );
    let level = data(
        &app.post(
            "/api/v1/admin/member-levels",
            json!({"name": {"zh-CN": "VIP"}, "slug": "vip", "discount_rate": 90, "sort_order": 5}),
        )
        .await,
    )["id"]
        .clone();
    let off_level = data(
        &app.post(
            "/api/v1/admin/member-levels",
            json!({"name": {"zh-CN": "OFF"}, "slug": "off", "sort_order": 6, "is_active": false}),
        )
        .await,
    )["id"]
        .clone();
    data(
        &app.post(
            "/api/v1/admin/member-level-prices/batch",
            json!({"prices": [
                {"member_level_id": level, "product_id": id, "price_amount": 80},
                {"member_level_id": off_level, "product_id": id, "price_amount": 70}
            ]}),
        )
        .await,
    );
    let t = Utc::now();
    let post = posts::ActiveModel {
        slug: Set("guide".into()),
        type_: Set("blog".into()),
        title_json: Set(Some(json!({"zh-CN": "指南"}))),
        thumbnail: Set(String::new()),
        is_published: Set(true),
        published_at: Set(Some(t)),
        created_at: Set(t),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    post_products::ActiveModel {
        post_id: Set(post.id),
        product_id: Set(id),
        sort: Set(0),
        created_at: Set(t),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();

    let res = app
        .call("GET", &format!("{PUBLIC}/multi"), None, None)
        .await;
    let d = data(&res);
    assert_eq!(
        d["price_amount"], "89.90",
        "display price = first active SKU"
    );
    assert_eq!(d["promotion_price_amount"], "79.90");
    assert_eq!(d["promotion_name"], "minus10");
    assert_eq!(d["promotion_type"], "fixed");
    assert_eq!(
        d["promotion_rules"].as_array().unwrap().len(),
        1,
        "future promotion not effective"
    );
    assert_eq!(d["promotion_rules"][0]["value"], "10.00");
    assert_eq!(
        d["wholesale_prices"],
        json!([{"min_quantity": 5, "unit_price": "40.00"}])
    );
    assert_eq!(
        d["member_prices"],
        json!([{"member_level_id": level, "sku_id": 0, "price_amount": "80.00"}])
    );
    let skus = d["skus"].as_array().unwrap();
    assert_eq!(skus.len(), 2, "inactive SKUs are not public");
    assert_eq!(skus[0]["sku_code"], "A");
    assert_eq!(skus[1]["promotion_price_amount"], "39.90");
    assert_eq!(d["manual_stock_available"], 6);
    assert_eq!(d["stock_status"], "in_stock");
    assert_eq!(d["stock_display"], "exact");
    assert_eq!(d["stock_quantity_hidden"], false);
    assert_eq!(
        d["category"],
        json!({"id": cat, "parent_id": 0, "slug": "c", "name": {"zh-CN": "c"}, "sort_order": 0})
    );
    assert_eq!(d["related_posts"][0]["slug"], "guide");
    assert_eq!(d["related_posts"][0]["type"], "blog");
    for forbidden in [
        "cost_price_amount",
        "instructions",
        "manual_stock_locked",
        "is_active",
        "created_at",
    ] {
        assert!(d.get(forbidden).is_none(), "{forbidden} must not be public");
    }
    assert!(skus[0].get("cost_price_amount").is_none());
    assert!(d.get("payment_channel_ids").is_none());

    // the list uses the same shape but no related posts
    let list = app.call("GET", PUBLIC, None, None).await;
    assert_eq!(data(&list)[0]["promotion_price_amount"], "79.90");
    assert!(data(&list)[0].get("related_posts").is_none());
}

// ORD-08, DLV-08
#[tokio::test]
async fn stock_is_masked_outside_exact_mode() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let hidden = id_of(
        &product(
            &app,
            cat,
            "hidden",
            json!({"fulfillment_type": "auto", "stock_display_mode": "hidden"}),
        )
        .await,
    );
    insert_secrets(&app, hidden, 0, "available", 37).await;
    let ranged = id_of(
        &product(
            &app,
            cat,
            "ranged",
            json!({"fulfillment_type": "auto", "stock_display_mode": "range"}),
        )
        .await,
    );
    insert_secrets(&app, ranged, 0, "available", 37).await;
    let exact = id_of(&product(&app, cat, "exact", json!({"fulfillment_type": "auto"})).await);
    insert_secrets(&app, exact, 0, "available", 37).await;
    product(
        &app,
        cat,
        "empty",
        json!({"manual_stock_total": 0, "stock_display_mode": "status"}),
    )
    .await;
    product(
        &app,
        cat,
        "infinite",
        json!({"manual_stock_total": -1, "stock_display_mode": "status"}),
    )
    .await;

    let get = |slug: &'static str| {
        let app = &app;
        async move {
            data(
                &app.call("GET", &format!("{PUBLIC}/{slug}"), None, None)
                    .await,
            )
            .clone()
        }
    };
    let h = get("hidden").await;
    assert_eq!(
        (
            h["auto_stock_available"].as_i64(),
            h["stock_display"].as_str()
        ),
        (Some(1), Some("hidden"))
    );
    assert_eq!(h["skus"][0]["auto_stock_available"], 1);
    assert_eq!(h["stock_quantity_hidden"], true);
    let r = get("ranged").await;
    assert_eq!(r["stock_display"], "range_21_50");
    assert_eq!(
        (r["stock_range_min"].as_i64(), r["stock_range_max"].as_i64()),
        (Some(21), Some(50))
    );
    let e = get("exact").await;
    assert_eq!(e["auto_stock_available"], 37);
    assert_eq!(e["skus"][0]["auto_stock_available"], 37);
    let empty = get("empty").await;
    assert_eq!(
        (
            empty["stock_status"].as_str(),
            empty["is_sold_out"].as_bool()
        ),
        (Some("out_of_stock"), Some(true))
    );
    assert_eq!(empty["manual_stock_available"], 0);
    let inf = get("infinite").await;
    assert_eq!(
        (
            inf["stock_status"].as_str(),
            inf["manual_stock_available"].as_i64()
        ),
        (Some("unlimited"), Some(-1))
    );
    assert_eq!(inf["skus"][0]["manual_stock_total"], -1);
}
