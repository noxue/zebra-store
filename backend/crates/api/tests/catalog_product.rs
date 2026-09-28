//! Admin product endpoints follow the original contract and its bug-fix lessons.

#![expect(clippy::unwrap_used, reason = "tests")]

mod catalog_common;
mod common;

use catalog_common::{category, id_of, insert_secrets, order_item, payment_channel, product};
use common::{TestApp, data};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter};
use serde_json::{Value, json};
use zs_infra::db::entity::{product_skus, products};

const PRODUCTS: &str = "/api/v1/admin/products";

fn status(v: &Value) -> i64 {
    v["status_code"].as_i64().unwrap()
}

async fn sku_rows(app: &TestApp, product_id: i64) -> Vec<product_skus::Model> {
    product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .all(&app.db)
        .await
        .unwrap()
}

#[tokio::test]
async fn product_crud_contract() {
    let app = TestApp::new().await;
    let cat = category(&app, "cards", 0).await;
    let created = product(
        &app,
        cat,
        "steam",
        json!({"cost_price_amount": 4.5, "tags": ["hot"]}),
    )
    .await;
    assert_eq!(created["slug"], "steam");
    assert_eq!(created["price_amount"], "10.00");
    assert_eq!(created["cost_price_amount"], "4.50");
    assert_eq!(created["purchase_type"], "member");
    assert_eq!(created["stock_display_mode"], "exact");
    assert_eq!(created["fulfillment_type"], "manual");
    assert_eq!(created["manual_stock_total"], 5);
    assert_eq!(created["payment_channel_ids"], "");
    assert_eq!(created["wholesale_prices"], json!([]));
    assert_eq!(created["manual_form_schema"], json!({"fields": []}));
    assert_eq!(created["category"]["slug"], "cards");
    assert_eq!(created["skus"][0]["sku_code"], "DEFAULT");
    assert_eq!(created["skus"][0]["price_amount"], "10.00");
    assert_eq!(created["skus"][0]["manual_stock_total"], 5);
    assert_eq!(created["is_active"], true);
    let id = id_of(&created);

    // duplicate slug
    let dup = app
        .post(PRODUCTS, json!({"category_id": cat, "slug": "steam", "title": {"zh-CN": "x"}, "price_amount": 1}))
        .await;
    assert_eq!(
        (status(&dup), dup["msg"].as_str().unwrap()),
        (400, "Slug 已存在")
    );
    // required fields
    let missing = app
        .post(
            PRODUCTS,
            json!({"category_id": cat, "slug": "x", "title": {}}),
        )
        .await;
    // Original `RespondBindError` on `CreateProductRequest.PriceAmount binding:"required"`.
    assert_eq!(missing["msg"], "PriceAmount: 不能为空");

    // list + detail
    let list = app.get(&format!("{PRODUCTS}?page=1&page_size=10")).await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
    assert_eq!(list["pagination"]["total"], 1);
    let detail = app.get(&format!("{PRODUCTS}/{id}")).await;
    assert_eq!(data(&detail)["id"], id);
    let missing = app.get(&format!("{PRODUCTS}/9999")).await;
    assert_eq!(
        (status(&missing), missing["msg"].as_str().unwrap()),
        (404, "商品不存在")
    );

    // full update (slug taken by another product → slug_used)
    product(&app, cat, "other", json!({})).await;
    let used = app
        .put(&format!("{PRODUCTS}/{id}"), json!({"category_id": cat, "slug": "other", "title": {"zh-CN": "x"}, "price_amount": 1}))
        .await;
    assert_eq!(used["msg"], "Slug 已被其他资源使用");
    let updated = app
        .put(
            &format!("{PRODUCTS}/{id}"),
            json!({"category_id": cat, "slug": "steam-2", "title": {"zh-CN": "新"}, "price_amount": "12.5",
                   "manual_stock_total": -1, "stock_display_mode": "range"}),
        )
        .await;
    let u = data(&updated);
    assert_eq!(u["slug"], "steam-2");
    assert_eq!(u["price_amount"], "12.50");
    assert_eq!(u["manual_stock_total"], -1);
    assert_eq!(u["stock_display_mode"], "range");
    assert_eq!(u["skus"][0]["manual_stock_total"], -1);

    // quick update
    let patched = app
        .patch(
            &format!("{PRODUCTS}/{id}"),
            json!({"sort_order": 9, "is_active": false}),
        )
        .await;
    assert_eq!(data(&patched)["sort_order"], 9);
    assert_eq!(data(&patched)["is_active"], false);
    let empty = app.patch(&format!("{PRODUCTS}/{id}"), json!({})).await;
    assert_eq!(status(&empty), 400);

    // delete; the slug becomes reusable
    let deleted = app.delete(&format!("{PRODUCTS}/{id}")).await;
    assert!(data(&deleted).is_null());
    assert_eq!(status(&app.get(&format!("{PRODUCTS}/{id}")).await), 404);
    product(&app, cat, "steam-2", json!({})).await;

    // auth
    let anon = app.call("GET", PRODUCTS, None, None).await;
    assert_eq!(status(&anon), 401);
}

// ORD-10
#[tokio::test]
async fn purchase_limits_and_enum_validation() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let bad = app
        .post(
            PRODUCTS,
            json!({"category_id": cat, "slug": "a", "title": {}, "price_amount": 1,
                               "min_purchase_quantity": 5, "max_purchase_quantity": 3}),
        )
        .await;
    assert_eq!(bad["msg"], "单次购买数量下限不能大于上限");
    let p = product(
        &app,
        cat,
        "b",
        json!({"min_purchase_quantity": -3, "max_purchase_quantity": 2}),
    )
    .await;
    assert_eq!(
        (
            p["min_purchase_quantity"].as_i64(),
            p["max_purchase_quantity"].as_i64()
        ),
        (Some(0), Some(2))
    );
    let purchase = app
        .post(PRODUCTS, json!({"category_id": cat, "slug": "c", "title": {}, "price_amount": 1, "purchase_type": "vip"}))
        .await;
    assert_eq!(purchase["msg"], "商品购买身份不合法");
    let fulfill = app
        .post(PRODUCTS, json!({"category_id": cat, "slug": "c", "title": {}, "price_amount": 1, "fulfillment_type": "x"}))
        .await;
    assert_eq!(fulfill["msg"], "交付信息不合法");
    let display = app
        .post(PRODUCTS, json!({"category_id": cat, "slug": "c", "title": {}, "price_amount": 1, "stock_display_mode": "x"}))
        .await;
    assert_eq!(display["msg"], "请求参数错误");
    let stock = app
        .post(PRODUCTS, json!({"category_id": cat, "slug": "c", "title": {}, "price_amount": 1, "manual_stock_total": -2}))
        .await;
    assert_eq!(stock["msg"], "人工库存参数不合法");
    let price = app
        .post(
            PRODUCTS,
            json!({"category_id": cat, "slug": "c", "title": {}, "price_amount": -1}),
        )
        .await;
    assert_eq!(price["msg"], "商品价格或币种不合法");
    let schema = app
        .post(PRODUCTS, json!({"category_id": cat, "slug": "c", "title": {}, "price_amount": 1,
                               "manual_form_schema": {"fields": [{"key": "Bad-Key", "type": "text"}]}}))
        .await;
    assert_eq!(schema["msg"], "人工交付表单配置不合法");
    let ok_schema = product(
        &app,
        cat,
        "d",
        json!({"manual_form_schema": {"fields": [{"key": "qq", "type": "select", "options": ["b", "a", "a"]}]}}),
    )
    .await;
    assert_eq!(
        ok_schema["manual_form_schema"]["fields"][0]["options"],
        json!(["a", "b"])
    );
    // auto products drop the manual form
    let auto = product(
        &app,
        cat,
        "e",
        json!({"fulfillment_type": "auto",
        "manual_form_schema": {"fields": [{"key": "qq", "type": "text"}]}}),
    )
    .await;
    assert_eq!(auto["manual_form_schema"], json!({}));
}

// ORD-09
#[tokio::test]
async fn category_rules_for_assignment_and_activation() {
    let app = TestApp::new().await;
    let parent = category(&app, "parent", 0).await;
    let child = category(&app, "child", parent).await;
    let leaf = category(&app, "leaf", 0).await;

    let on_parent = app
        .post(
            PRODUCTS,
            json!({"category_id": parent, "slug": "p", "title": {}, "price_amount": 1}),
        )
        .await;
    assert_eq!(on_parent["msg"], "当前分类不可直接挂载商品，请选择末级分类");
    let missing = app
        .post(
            PRODUCTS,
            json!({"category_id": 999, "slug": "p", "title": {}, "price_amount": 1}),
        )
        .await;
    assert_eq!(status(&missing), 400);

    let p = id_of(&product(&app, child, "p1", json!({"is_active": false})).await);
    // quick PATCH cannot move a product under a parent category
    let moved = app
        .patch(&format!("{PRODUCTS}/{p}"), json!({"category_id": parent}))
        .await;
    assert_eq!(moved["msg"], "当前分类不可直接挂载商品，请选择末级分类");
    // uncategorised product cannot be activated
    let zero = app
        .patch(&format!("{PRODUCTS}/{p}"), json!({"category_id": 0}))
        .await;
    data(&zero);
    let activate = app
        .patch(&format!("{PRODUCTS}/{p}"), json!({"is_active": true}))
        .await;
    assert_eq!(status(&activate), 400);
    data(
        &app.patch(
            &format!("{PRODUCTS}/{p}"),
            json!({"category_id": leaf, "is_active": true}),
        )
        .await,
    );

    // inactive category blocks activation
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{leaf}/active"),
            json!({"is_active": false}),
        )
        .await,
    );
    let inactive = app
        .patch(&format!("{PRODUCTS}/{p}"), json!({"is_active": true}))
        .await;
    assert_eq!(status(&inactive), 400);
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{leaf}/active"),
            json!({"is_active": true}),
        )
        .await,
    );

    // batch-status reports per-item failures
    let a = id_of(&product(&app, leaf, "a", json!({"is_active": false})).await);
    let b = id_of(&product(&app, leaf, "b", json!({"is_active": false})).await);
    data(
        &app.patch(&format!("{PRODUCTS}/{b}"), json!({"category_id": 0}))
            .await,
    );
    let res = app
        .post(
            &format!("{PRODUCTS}/batch-status"),
            json!({"ids": [a, b, 9999], "is_active": true}),
        )
        .await;
    let d = data(&res);
    assert_eq!(
        (d["total"].as_i64(), d["success_count"].as_i64()),
        (Some(3), Some(1))
    );
    let failed = d["failed_items"].as_array().unwrap();
    assert_eq!(failed[0]["id"], b);
    assert_eq!(failed[0]["error_code"], "product_category_invalid");
    assert_eq!(
        failed[0]["message"],
        "当前分类不可直接挂载商品，请选择末级分类"
    );
    assert_eq!(failed[1]["error_code"], "product_not_found");
    let empty = app
        .post(&format!("{PRODUCTS}/batch-status"), json!({"ids": []}))
        .await;
    assert_eq!(status(&empty), 400);

    // batch-category
    let res = app
        .post(
            &format!("{PRODUCTS}/batch-category"),
            json!({"ids": [a, b], "category_id": child}),
        )
        .await;
    assert_eq!(data(&res)["success_count"], 2);
    let detail = app.get(&format!("{PRODUCTS}/{a}")).await;
    assert_eq!(data(&detail)["category_id"], child);
}

// PRC-10, PRC-02, PRC-05
#[tokio::test]
async fn wholesale_prices_semantics() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let p = product(
        &app,
        cat,
        "w",
        json!({"price_amount": 100, "wholesale_prices": [{"min_quantity": 10, "unit_price": 70}, {"min_quantity": 5, "unit_price": 80}]}),
    )
    .await;
    let id = id_of(&p);
    assert_eq!(
        p["wholesale_prices"],
        json!([{"min_quantity": 5, "unit_price": "80.00"}, {"min_quantity": 10, "unit_price": "70.00"}])
    );

    let base =
        json!({"category_id": cat, "slug": "w", "title": {"zh-CN": "w"}, "price_amount": 100});
    let kept = app.put(&format!("{PRODUCTS}/{id}"), base.clone()).await;
    assert_eq!(data(&kept)["wholesale_prices"].as_array().unwrap().len(), 2);
    let mut clear = base.clone();
    clear["wholesale_prices"] = json!([]);
    let cleared = app.put(&format!("{PRODUCTS}/{id}"), clear).await;
    assert_eq!(data(&cleared)["wholesale_prices"], json!([]));

    let url = format!("{PRODUCTS}/{id}/wholesale-prices");
    for bad in [
        json!([{"min_quantity": 0, "unit_price": 80}]),
        json!([{"min_quantity": 5, "unit_price": 0}]),
        json!([{"min_quantity": 5, "unit_price": 80}, {"min_quantity": 5, "unit_price": 70}]),
        json!([{"min_quantity": 5, "unit_price": 80}, {"min_quantity": 10, "unit_price": 90}]),
        json!([{"min_quantity": 5, "unit_price": 80}, {"min_quantity": 10, "unit_price": 80}]),
        json!([{"sku_code": "NOPE", "min_quantity": 5, "unit_price": 80}]),
    ] {
        let res = app.patch(&url, json!({"wholesale_prices": bad})).await;
        assert_eq!(res["msg"], "批发价配置不合法", "{res}");
    }
    let missing = app.patch(&url, json!({})).await;
    assert_eq!(missing["msg"], "WholesalePrices: 不能为空");
    let ok = app
        .patch(&url, json!({"wholesale_prices": [{"sku_code": "default", "min_quantity": 5, "unit_price": 80}]}))
        .await;
    let tiers = &data(&ok)["wholesale_prices"];
    assert_eq!(tiers[0]["sku_code"], "DEFAULT");
    assert!(tiers[0]["sku_id"].as_i64().unwrap() > 0);
    // the PATCH touched nothing else
    assert_eq!(data(&ok)["title"]["zh-CN"], "w");
    assert_eq!(
        status(
            &app.patch(
                &format!("{PRODUCTS}/9999/wholesale-prices"),
                json!({"wholesale_prices": []})
            )
            .await
        ),
        404
    );

    // SKU tiers are validated after the SKUs are persisted; a bad tier rolls everything back
    let multi = product(
        &app,
        cat,
        "multi",
        json!({"skus": [{"sku_code": "A", "price_amount": 100}, {"sku_code": "B", "price_amount": 100}],
               "wholesale_prices": [{"sku_code": "A", "min_quantity": 5, "unit_price": 70}, {"sku_code": "B", "min_quantity": 5, "unit_price": 60}]}),
    )
    .await;
    let tiers = multi["wholesale_prices"].as_array().unwrap();
    assert_eq!(tiers.len(), 2);
    assert!(tiers.iter().all(|t| t["sku_id"].as_i64().unwrap() > 0));
    let a_id = multi["skus"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["sku_code"] == "A")
        .unwrap()["id"]
        .clone();
    let mismatch = app
        .patch(&format!("{PRODUCTS}/{}/wholesale-prices", id_of(&multi)),
            json!({"wholesale_prices": [{"sku_id": a_id, "sku_code": "B", "min_quantity": 5, "unit_price": 70}]}))
        .await;
    assert_eq!(mismatch["msg"], "批发价配置不合法");
    let rolled_back = app
        .post(
            PRODUCTS,
            json!({"category_id": cat, "slug": "rb", "title": {}, "price_amount": 1,
            "wholesale_prices": [{"sku_code": "GHOST", "min_quantity": 2, "unit_price": 1}]}),
        )
        .await;
    assert_eq!(rolled_back["msg"], "批发价配置不合法");
    let count = products::Entity::find()
        .filter(products::Column::Slug.eq("rb"))
        .count(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0, "failed create must not leave a product behind");
}

// ORD-11, ORD-06
#[tokio::test]
async fn sku_sync_semantics() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let p = product(
        &app,
        cat,
        "s",
        json!({"skus": [
            {"sku_code": "A", "price_amount": 30, "cost_price_amount": 10, "manual_stock_total": 5},
            {"sku_code": "B", "price_amount": 20, "cost_price_amount": 12, "manual_stock_total": 3},
            {"sku_code": "C", "price_amount": 5, "cost_price_amount": 1, "manual_stock_total": 9, "is_active": false}
        ]}),
    )
    .await;
    let id = id_of(&p);
    assert_eq!(p["price_amount"], "20.00", "price = min active SKU price");
    assert_eq!(
        p["cost_price_amount"], "10.00",
        "cost = min active SKU cost"
    );
    assert_eq!(p["manual_stock_total"], 8);
    let dup = app
        .post(
            PRODUCTS,
            json!({"category_id": cat, "slug": "d", "title": {}, "price_amount": 1,
            "skus": [{"sku_code": "A", "price_amount": 1}, {"sku_code": "a", "price_amount": 1}]}),
        )
        .await;
    assert_eq!(dup["msg"], "请求参数错误");

    let body = |skus: Value| json!({"category_id": cat, "slug": "s", "title": {}, "price_amount": 1, "skus": skus});
    // remove A, then add A again: hard delete lets the code be reused
    data(
        &app.put(
            &format!("{PRODUCTS}/{id}"),
            body(json!([{"sku_code": "B", "price_amount": 20}])),
        )
        .await,
    );
    let readded = app
        .put(&format!("{PRODUCTS}/{id}"), body(json!([{"sku_code": "B", "price_amount": 20}, {"sku_code": "A", "price_amount": 25, "manual_stock_total": -1}])))
        .await;
    assert_eq!(data(&readded)["manual_stock_total"], -1);
    assert_eq!(sku_rows(&app, id).await.len(), 2);
    // foreign SKU id rejected
    let foreign = app
        .put(
            &format!("{PRODUCTS}/{id}"),
            body(json!([{"id": 99999, "sku_code": "Z", "price_amount": 1}])),
        )
        .await;
    assert_eq!(status(&foreign), 400);

    // single-spec round trips keep exactly one SKU row
    let single = json!({"category_id": cat, "slug": "s", "title": {}, "price_amount": 7, "manual_stock_total": 2});
    data(&app.put(&format!("{PRODUCTS}/{id}"), single.clone()).await);
    data(
        &app.put(
            &format!("{PRODUCTS}/{id}"),
            body(
                json!([{"sku_code": "X", "price_amount": 3}, {"sku_code": "Y", "price_amount": 4}]),
            ),
        )
        .await,
    );
    let back = app.put(&format!("{PRODUCTS}/{id}"), single).await;
    let rows = sku_rows(&app, id).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0].is_active);
    assert_eq!(data(&back)["skus"][0]["price_amount"], "7.00");
    assert_eq!(data(&back)["skus"][0]["manual_stock_total"], 2);
}

// DLV-04
#[tokio::test]
async fn auto_sku_with_card_secrets_cannot_be_disabled() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let p = product(
        &app,
        cat,
        "auto",
        json!({"fulfillment_type": "auto", "skus": [{"sku_code": "A", "price_amount": 10}, {"sku_code": "B", "price_amount": 10}]}),
    )
    .await;
    let id = id_of(&p);
    let skus = p["skus"].as_array().unwrap();
    let a = skus.iter().find(|s| s["sku_code"] == "A").unwrap()["id"]
        .as_i64()
        .unwrap();
    let b = skus.iter().find(|s| s["sku_code"] == "B").unwrap()["id"]
        .as_i64()
        .unwrap();
    insert_secrets(&app, id, a, "available", 3).await;
    insert_secrets(&app, id, b, "used", 2).await;
    let body = |a_active: bool, b_active: bool| {
        json!({"category_id": cat, "slug": "auto", "title": {"zh-CN": "changed"}, "price_amount": 1, "fulfillment_type": "auto",
               "skus": [{"id": a, "sku_code": "A", "price_amount": 10, "is_active": a_active},
                        {"id": b, "sku_code": "B", "price_amount": 10, "is_active": b_active}]})
    };
    let res = app
        .put(&format!("{PRODUCTS}/{id}"), body(false, true))
        .await;
    assert_eq!(res["msg"], "该 SKU 仍有关联卡密库存，不能直接停用或删除");
    let unchanged = app.get(&format!("{PRODUCTS}/{id}")).await;
    assert_ne!(data(&unchanged)["title"]["zh-CN"], "changed");
    // only used secrets → allowed
    data(
        &app.put(&format!("{PRODUCTS}/{id}"), body(true, false))
            .await,
    );

    // DLV-08: admin detail shows per-SKU auto stock
    let detail = app.get(&format!("{PRODUCTS}/{id}")).await;
    let d = data(&detail);
    assert_eq!(d["auto_stock_available"], 3);
    assert_eq!(d["auto_stock_sold"], 2);
    let sku_a = d["skus"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == a)
        .unwrap()
        .clone();
    assert_eq!(sku_a["auto_stock_available"], 3);
}

// ORD-05
#[tokio::test]
async fn delete_guards_and_cascade() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let stocked = id_of(&product(&app, cat, "stocked", json!({"fulfillment_type": "auto"})).await);
    insert_secrets(&app, stocked, 0, "available", 1).await;
    let res = app.delete(&format!("{PRODUCTS}/{stocked}")).await;
    assert_eq!(res["msg"], "该商品仍有可用或预占的卡密库存，无法删除");
    let reserved =
        id_of(&product(&app, cat, "reserved", json!({"fulfillment_type": "auto"})).await);
    insert_secrets(&app, reserved, 0, "reserved", 1).await;
    assert_eq!(
        status(&app.delete(&format!("{PRODUCTS}/{reserved}")).await),
        400
    );
    let sold = id_of(&product(&app, cat, "sold", json!({})).await);
    order_item(&app, sold).await;
    let res = app.delete(&format!("{PRODUCTS}/{sold}")).await;
    assert_eq!(res["msg"], "该商品已有成交记录，无法删除");

    let clean = id_of(&product(&app, cat, "clean", json!({"fulfillment_type": "auto"})).await);
    insert_secrets(&app, clean, 0, "used", 2).await;
    data(
        &app.post(
            "/api/v1/admin/member-levels",
            json!({"name": {"zh-CN": "VIP"}, "slug": "vip", "sort_order": 1}),
        )
        .await,
    );
    let levels = app.get("/api/v1/admin/member-levels").await;
    let level_id = data(&levels)[0]["id"].as_i64().unwrap();
    data(&app.post("/api/v1/admin/member-level-prices/batch",
        json!({"prices": [{"member_level_id": level_id, "product_id": clean, "price_amount": 5}]})).await);
    let res = app
        .post(
            &format!("{PRODUCTS}/batch-delete"),
            json!({"ids": [clean, stocked]}),
        )
        .await;
    let d = data(&res);
    assert_eq!(
        (d["total"].as_i64(), d["success_count"].as_i64()),
        (Some(2), Some(1))
    );
    assert_eq!(d["failed_ids"], json!([stocked]));
    let live_skus = product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(clean))
        .filter(product_skus::Column::DeletedAt.is_null())
        .count(&app.db)
        .await
        .unwrap();
    assert_eq!(live_skus, 0);
    assert_eq!(catalog_common::count_secrets(&app, clean, "used").await, 0);
    let prices = app
        .get(&format!(
            "/api/v1/admin/member-level-prices?product_id={clean}"
        ))
        .await;
    assert_eq!(data(&prices), &json!([]));
    let ok_delete = app
        .post(
            &format!("{PRODUCTS}/batch-delete"),
            json!({"ids": [reserved + 1000]}),
        )
        .await;
    assert_eq!(data(&ok_delete)["failed_ids"], json!([reserved + 1000]));
}

// PAY-17
#[tokio::test]
async fn payment_channels_are_filtered_on_save() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let active = payment_channel(&app, true).await;
    let inactive = payment_channel(&app, false).await;
    let p = product(
        &app,
        cat,
        "pc",
        json!({"payment_channel_ids": [active, inactive, 999, active, 0]}),
    )
    .await;
    assert_eq!(p["payment_channel_ids"], format!("[{active}]"));
    let none = product(&app, cat, "pc2", json!({"payment_channel_ids": [inactive]})).await;
    assert_eq!(none["payment_channel_ids"], "");
}

// ORD-12, DLV-07 (numeric search), DB-09
#[tokio::test]
async fn admin_filters() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let other = category(&app, "o", 0).await;
    let mut autos = Vec::new();
    for (slug, n) in [("auto0", 0), ("auto3", 3), ("auto6", 6)] {
        let id = id_of(&product(&app, cat, slug, json!({"fulfillment_type": "auto"})).await);
        insert_secrets(&app, id, 0, "available", n).await;
        autos.push(id);
    }
    let unlimited = id_of(&product(&app, cat, "unl", json!({"manual_stock_total": -1})).await);
    let empty = id_of(
        &product(
            &app,
            other,
            "empty",
            json!({"manual_stock_total": 0, "is_active": false}),
        )
        .await,
    );
    let stocked = id_of(
        &product(
            &app,
            other,
            "stocked",
            json!({"manual_stock_total": 9, "title": {"zh-CN": "测试商品"}}),
        )
        .await,
    );

    let ids = |v: &Value| -> Vec<i64> {
        let mut ids: Vec<i64> = data(v)
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["id"].as_i64().unwrap())
            .collect();
        ids.sort_unstable();
        ids
    };
    let low = app.get(&format!("{PRODUCTS}?stock_status=low")).await;
    assert_eq!(ids(&low), vec![autos[0], autos[1], empty]);
    let normal = app.get(&format!("{PRODUCTS}?stock_staus=normal")).await;
    assert_eq!(ids(&normal), vec![autos[2], stocked]);
    let unl = app.get(&format!("{PRODUCTS}?stock_status=unlimited")).await;
    assert_eq!(ids(&unl), vec![unlimited]);

    let inactive = app.get(&format!("{PRODUCTS}?is_active=0")).await;
    assert_eq!(ids(&inactive), vec![empty]);
    let bad = app.get(&format!("{PRODUCTS}?is_active=maybe")).await;
    assert_eq!(status(&bad), 400);
    let by_cat = app.get(&format!("{PRODUCTS}?category_id={other}")).await;
    assert_eq!(ids(&by_cat), vec![empty, stocked]);
    let by_type = app.get(&format!("{PRODUCTS}?fulfillment_type=auto")).await;
    assert_eq!(ids(&by_type).len(), 3);
    let by_title = app
        .get(&format!("{PRODUCTS}?search={}", "%E6%B5%8B%E8%AF%95"))
        .await;
    assert_eq!(ids(&by_title), vec![stocked]);
    let by_id = app
        .get(&format!("{PRODUCTS}?search={empty}&is_active=true"))
        .await;
    assert!(
        ids(&by_id).is_empty(),
        "numeric search keeps the other filters"
    );
    let by_id = app.get(&format!("{PRODUCTS}?search={empty}")).await;
    assert!(ids(&by_id).contains(&empty));
    let by_sku = app.get(&format!("{PRODUCTS}?search=default")).await;
    assert_eq!(ids(&by_sku).len(), 6);

    data(
        &app.patch(
            &format!("{PRODUCTS}/{stocked}/wholesale-prices"),
            json!({"wholesale_prices": [{"min_quantity": 2, "unit_price": 1}]}),
        )
        .await,
    );
    let with = app.get(&format!("{PRODUCTS}?wholesale=1")).await;
    assert_eq!(ids(&with), vec![stocked]);
    let without = app
        .get(&format!("{PRODUCTS}?has_wholesale_prices=none"))
        .await;
    assert_eq!(ids(&without).len(), 5);
}
