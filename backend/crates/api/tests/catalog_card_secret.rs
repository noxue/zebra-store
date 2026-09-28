//! Card secret endpoints and the ordering ports (stock / reservation) offered to the order group.

#![expect(clippy::unwrap_used, reason = "tests")]

mod catalog_common;
mod common;

use std::sync::Arc;

use catalog_common::{
    category, count_secrets, id_of, insert_secrets, multipart, product, raw_get, raw_json,
};
use chrono::Utc;
use common::{TestApp, data};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::{Value, json};
use zs_domain::catalog::ordering::{CatalogOrdering, StockMove, StockTarget};
use zs_infra::db::entity::{card_secrets, product_skus, products};
use zs_infra::db::repo::catalog::ordering::SeaCatalogOrdering;

const SECRETS: &str = "/api/v1/admin/card-secrets";

fn status(v: &Value) -> i64 {
    v["status_code"].as_i64().unwrap()
}

async fn auto_product(app: &TestApp, cat: i64, slug: &str, skus: Value) -> (i64, Vec<i64>) {
    let p = product(
        app,
        cat,
        slug,
        json!({"fulfillment_type": "auto", "skus": skus}),
    )
    .await;
    // the create response lists active SKUs only; the admin detail lists all of them
    let detail = app
        .get(&format!("/api/v1/admin/products/{}", id_of(&p)))
        .await;
    let ids = data(&detail)["skus"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_i64().unwrap())
        .collect();
    (id_of(&p), ids)
}

// DLV-04, DLV-10
#[tokio::test]
async fn batch_create_resolves_sku_and_deduplicates() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let (multi, skus) = auto_product(
        &app,
        cat,
        "multi",
        json!([{"sku_code": "A", "price_amount": 1}, {"sku_code": "B", "price_amount": 1}]),
    )
    .await;
    let res = app
        .post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": multi, "secrets": ["x"]}),
        )
        .await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (400, Some("卡密参数不合法"))
    );
    let res = app
        .post(&format!("{SECRETS}/batch"), json!({"product_id": multi, "sku_id": skus[0], "secrets": ["a\na\nb", " b "], "note": " n "}))
        .await;
    let d = data(&res);
    assert_eq!(d["created"], 2);
    assert!(d["batch_no"].as_str().unwrap().starts_with("BATCH-"));
    let res = app
        .post(&format!("{SECRETS}/batch"), json!({"product_id": multi, "sku_id": skus[1], "secrets": ["a\na\nb"], "deduplicate": false, "batch_no": "MY-BATCH"}))
        .await;
    assert_eq!(
        (
            data(&res)["created"].as_i64(),
            data(&res)["batch_no"].as_str()
        ),
        (Some(3), Some("MY-BATCH"))
    );

    // single active SKU is picked automatically; disabled/foreign SKUs are rejected
    let (single, single_skus) = auto_product(
        &app,
        cat,
        "single",
        json!([{"sku_code": "A", "price_amount": 1}, {"sku_code": "B", "price_amount": 1, "is_active": false}]),
    )
    .await;
    let res = app
        .post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": single, "secrets": ["q"]}),
        )
        .await;
    data(&res);
    let rows = card_secrets::Entity::find()
        .filter(card_secrets::Column::ProductId.eq(single))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(rows[0].sku_id, single_skus[0]);
    let res = app
        .post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": single, "sku_id": single_skus[1], "secrets": ["q"]}),
        )
        .await;
    assert_eq!(status(&res), 400);
    let res = app
        .post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": single, "sku_id": skus[0], "secrets": ["q"]}),
        )
        .await;
    assert_eq!(status(&res), 400);
    let res = app
        .post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": 999, "secrets": ["q"]}),
        )
        .await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (404, Some("商品不存在"))
    );
    let res = app
        .post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": single, "secrets": [" ", ""]}),
        )
        .await;
    assert_eq!(status(&res), 400);
}

// DLV-06, DLV-10
#[tokio::test]
async fn import_csv_and_txt() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let (id, _) = auto_product(
        &app,
        cat,
        "p",
        json!([{"sku_code": "A", "price_amount": 1}]),
    )
    .await;
    let pid = id.to_string();
    let url = format!("{SECRETS}/import");

    let res = multipart(
        &app,
        &url,
        &[("product_id", &pid)],
        Some("\u{feff}id,secret\n1,AAA\n2,BBB\n3,AAA\n"),
    )
    .await;
    assert_eq!(data(&res)["created"], 2);
    let res = multipart(
        &app,
        &url,
        &[("product_id", &pid), ("deduplicate", "false")],
        Some("X\nX\nY\n"),
    )
    .await;
    assert_eq!(data(&res)["created"], 3);
    let res = multipart(
        &app,
        &url,
        &[("product_id", &pid), ("deduplicate", "abc")],
        Some("Z\n"),
    )
    .await;
    assert_eq!(status(&res), 400);
    let res = multipart(&app, &url, &[("product_id", &pid)], None).await;
    assert_eq!(status(&res), 400);
    let res = multipart(&app, &url, &[("product_id", "0")], Some("Z\n")).await;
    assert_eq!(status(&res), 400);

    // large import is chunked inside one transaction
    let big: String = (0..10_000).map(|i| format!("BIG-{i}\n")).collect();
    let res = multipart(
        &app,
        &url,
        &[("product_id", &pid), ("note", "bulk")],
        Some(&big),
    )
    .await;
    assert_eq!(data(&res)["created"], 10_000);
    assert_eq!(count_secrets(&app, id, "available").await, 10_005);

    let tpl = raw_get(&app, &format!("{SECRETS}/template")).await;
    assert_eq!(tpl.body, "secret\nCARD-AAA-0001\nCARD-BBB-0002\n");
    assert_eq!(
        tpl.headers["content-disposition"],
        "attachment; filename=\"card-secrets-template.csv\""
    );
}

// DLV-07
#[tokio::test]
async fn list_filters_bulk_targets_and_batches() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let (id, skus) = auto_product(
        &app,
        cat,
        "p",
        json!([{"sku_code": "A", "price_amount": 1}]),
    )
    .await;
    let first = data(&app.post(&format!("{SECRETS}/batch"),
        json!({"product_id": id, "secrets": (0..10).map(|i| format!("Key-{i}")).collect::<Vec<_>>(), "batch_no": "ABC-1"})).await).clone();
    let second = data(
        &app.post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": id, "secrets": ["other-1", "other-2"], "batch_no": "ZZZ-2"}),
        )
        .await,
    )
    .clone();
    let batch_one = first["batch_id"].as_i64().unwrap();

    let list = app
        .get(&format!(
            "{SECRETS}?product_id={id}&secret=KEY-1&page_size=5"
        ))
        .await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(data(&list)[0]["batch"]["batch_no"], "ABC-1");
    assert_eq!(data(&list)[0]["sku_id"], skus[0]);
    let by_batch_no = app
        .get(&format!("{SECRETS}?status=available&batch_no=abc"))
        .await;
    assert_eq!(by_batch_no["pagination"]["total"], 10);
    let bad = app.get(&format!("{SECRETS}?sku_id={}", skus[0])).await;
    assert_eq!(status(&bad), 400);
    let bad = app.get(&format!("{SECRETS}?product_id=x")).await;
    assert_eq!(status(&bad), 400);

    // empty criteria never mean "everything"
    let res = app
        .post(&format!("{SECRETS}/batch-delete"), json!({}))
        .await;
    assert_eq!(status(&res), 400);
    assert_eq!(count_secrets(&app, id, "available").await, 12);
    let res = app
        .post(
            &format!("{SECRETS}/batch-delete"),
            json!({"filter": {"batch_no": "none"}}),
        )
        .await;
    assert_eq!(status(&res), 404);

    // delete 2 of batch one by ids, use 3 by filter; realtime batch counts
    let ids: Vec<i64> = data(
        &app.get(&format!("{SECRETS}?batch_id={batch_one}&page_size=100"))
            .await,
    )
    .as_array()
    .unwrap()
    .iter()
    .map(|s| s["id"].as_i64().unwrap())
    .collect();
    let res = app
        .post(
            &format!("{SECRETS}/batch-delete"),
            json!({"ids": [ids[0], ids[1]]}),
        )
        .await;
    assert_eq!(data(&res)["affected"], 2);
    let res = app
        .patch(
            &format!("{SECRETS}/batch-status"),
            json!({"ids": [ids[2], ids[3], ids[4]], "status": "used"}),
        )
        .await;
    assert_eq!(data(&res)["affected"], 3);
    let res = app
        .patch(
            &format!("{SECRETS}/batch-status"),
            json!({"batch_id": batch_one, "status": "bogus"}),
        )
        .await;
    assert_eq!(status(&res), 400);
    let batches = app.get(&format!("{SECRETS}/batches?product_id={id}")).await;
    let b = data(&batches)
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == batch_one)
        .unwrap()
        .clone();
    assert_eq!(
        (
            b["total_count"].as_i64(),
            b["used_count"].as_i64(),
            b["available_count"].as_i64()
        ),
        (Some(8), Some(3), Some(5))
    );
    assert_eq!(b["name"], "");
    // batch target only touches that batch
    let res = app
        .patch(
            &format!("{SECRETS}/batch-status"),
            json!({"batch_id": second["batch_id"], "status": "reserved"}),
        )
        .await;
    assert_eq!(data(&res)["affected"], 2);
    let stats = app.get(&format!("{SECRETS}/stats?product_id={id}")).await;
    assert_eq!(
        data(&stats),
        &json!({"total": 10, "available": 5, "reserved": 2, "used": 3})
    );
    let stats = app.get(&format!("{SECRETS}/stats?product_id=0")).await;
    assert_eq!(status(&stats), 400);

    // single update
    let res = app
        .put(
            &format!("{SECRETS}/{}", ids[5]),
            json!({"secret": " NEW ", "status": "used"}),
        )
        .await;
    assert_eq!(
        (data(&res)["secret"].as_str(), data(&res)["status"].as_str()),
        (Some("NEW"), Some("used"))
    );
    let res = app.put(&format!("{SECRETS}/{}", ids[5]), json!({})).await;
    assert_eq!(status(&res), 400);
    let res = app
        .put(&format!("{SECRETS}/{}", ids[0]), json!({"secret": "x"}))
        .await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (404, Some("卡密不存在"))
    );
}

// DLV-01
#[tokio::test]
async fn export_and_export_available() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let (id, _) = auto_product(
        &app,
        cat,
        "p",
        json!([{"sku_code": "A", "price_amount": 1}]),
    )
    .await;
    data(
        &app.post(
            &format!("{SECRETS}/batch"),
            json!({"product_id": id, "secrets": ["S1", "S2", "S3", "S4", "S5"]}),
        )
        .await,
    );

    let csv = raw_json(
        &app,
        "POST",
        &format!("{SECRETS}/export"),
        json!({"format": "csv"}),
    )
    .await;
    assert_eq!(csv.headers["content-type"], "text/csv; charset=utf-8");
    assert!(
        csv.body
            .starts_with("id,secret,status,product_id,sku_id,order_id,batch_id,created_at\n")
    );
    assert_eq!(
        csv.body.lines().count(),
        6,
        "no criteria exports every current row"
    );
    let txt = raw_json(
        &app,
        "POST",
        &format!("{SECRETS}/export"),
        json!({"format": "TXT", "filter": {"secret": "s1"}}),
    )
    .await;
    assert_eq!(txt.body, "S1");
    assert!(
        txt.headers["content-disposition"]
            .to_str()
            .unwrap()
            .ends_with(".txt\"")
    );
    let bad = raw_json(
        &app,
        "POST",
        &format!("{SECRETS}/export"),
        json!({"format": "xls"}),
    )
    .await;
    assert_eq!(bad.json()["status_code"], 400);

    let url = format!("{SECRETS}/export-available");
    let short = raw_json(
        &app,
        "POST",
        &url,
        json!({"product_id": id, "limit": 6, "format": "txt"}),
    )
    .await;
    assert_eq!(short.json()["msg"], "卡密库存不足");
    assert_eq!(count_secrets(&app, id, "available").await, 5);
    let out = raw_json(
        &app,
        "POST",
        &url,
        json!({"product_id": id, "limit": 2, "format": "txt"}),
    )
    .await;
    assert_eq!(out.body, "S1\nS2");
    assert_eq!(out.headers["x-exported-count"], "2");
    assert_eq!(count_secrets(&app, id, "used").await, 2);
    let del = raw_json(
        &app,
        "POST",
        &url,
        json!({"product_id": id, "limit": 1, "format": "csv", "delete_after_export": true}),
    )
    .await;
    assert!(del.body.contains(",S3,available,"));
    assert_eq!(count_secrets(&app, id, "available").await, 2);

    // concurrent exports never hand out the same secret
    let (a, b) = tokio::join!(
        raw_json(
            &app,
            "POST",
            &url,
            json!({"product_id": id, "limit": 2, "format": "txt"})
        ),
        raw_json(
            &app,
            "POST",
            &url,
            json!({"product_id": id, "limit": 2, "format": "txt"})
        ),
    );
    let ok = [&a, &b].iter().filter(|r| r.body.starts_with('S')).count();
    assert_eq!(ok, 1, "{} / {}", a.body, b.body);
    let none = raw_json(
        &app,
        "POST",
        &url,
        json!({"product_id": id, "limit": 1, "format": "txt"}),
    )
    .await;
    assert_eq!(none.json()["status_code"], 404);

    let manual = id_of(&product(&app, cat, "manual", json!({})).await);
    let res = raw_json(
        &app,
        "POST",
        &url,
        json!({"product_id": manual, "limit": 1, "format": "txt"}),
    )
    .await;
    assert_eq!(res.json()["msg"], "卡密参数不合法");
}

async fn stock(app: &TestApp, id: i64) -> (i32, i32, i32) {
    let row = products::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    (
        row.manual_stock_total,
        row.manual_stock_locked,
        row.manual_stock_sold,
    )
}

// ORD-03, ORD-06
#[tokio::test]
async fn manual_stock_movements_use_conditional_updates() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let ordering = SeaCatalogOrdering::new(app.db.clone());
    let zero = id_of(&product(&app, cat, "zero", json!({"manual_stock_total": 0})).await);
    let err = ordering
        .move_manual_stock(StockTarget::Product(zero), StockMove::Reserve, 1)
        .await
        .unwrap_err();
    assert_eq!(err.key(), "error.manual_stock_insufficient");

    let unlimited = product(&app, cat, "inf", json!({"manual_stock_total": -1})).await;
    let sku = unlimited["skus"][0]["id"].as_i64().unwrap();
    assert_eq!(
        ordering
            .move_manual_stock(StockTarget::Sku(sku), StockMove::Reserve, 100)
            .await
            .unwrap(),
        0
    );
    let row = product_skus::Entity::find_by_id(sku)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.manual_stock_total, -1);

    let five = id_of(&product(&app, cat, "five", json!({"manual_stock_total": 5})).await);
    ordering
        .move_manual_stock(StockTarget::Product(five), StockMove::Reserve, 3)
        .await
        .unwrap();
    assert_eq!(stock(&app, five).await, (2, 3, 0));
    ordering
        .move_manual_stock(StockTarget::Product(five), StockMove::Release, 3)
        .await
        .unwrap();
    assert_eq!(stock(&app, five).await, (5, 0, 0));
    // legacy unreserved order: consume takes from total
    ordering
        .move_manual_stock(StockTarget::Product(five), StockMove::Consume, 2)
        .await
        .unwrap();
    assert_eq!(stock(&app, five).await, (3, 0, 2));

    let one = id_of(&product(&app, cat, "one", json!({"manual_stock_total": 1})).await);
    let ordering = Arc::new(ordering);
    let (a, b) = tokio::join!(
        ordering.move_manual_stock(StockTarget::Product(one), StockMove::Reserve, 1),
        ordering.move_manual_stock(StockTarget::Product(one), StockMove::Reserve, 1),
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    assert_eq!(stock(&app, one).await, (0, 1, 0));
}

// DLV-03
#[tokio::test]
async fn secret_reservation_never_double_books() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let (id, skus) = auto_product(
        &app,
        cat,
        "p",
        json!([{"sku_code": "A", "price_amount": 1}]),
    )
    .await;
    insert_secrets(&app, id, skus[0], "available", 1).await;
    let ordering = Arc::new(SeaCatalogOrdering::new(app.db.clone()));
    let now = Utc::now();
    let (a, b) = tokio::join!(
        ordering.reserve_secrets(id, skus[0], 1, 101, now),
        ordering.reserve_secrets(id, skus[0], 1, 102, now),
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let err = a.err().or(b.err()).unwrap();
    assert_eq!(err.key(), "error.card_secret_insufficient");

    insert_secrets(&app, id, skus[0], "available", 10).await;
    let mut handles = Vec::new();
    for order in 0..10 {
        let ordering = ordering.clone();
        handles.push(tokio::spawn(async move {
            ordering
                .reserve_secrets(id, 0, 1, 200 + order, Utc::now())
                .await
        }));
    }
    let mut reserved = Vec::new();
    for h in handles {
        reserved.extend(h.await.unwrap().unwrap());
    }
    reserved.sort_unstable();
    reserved.dedup();
    assert_eq!(reserved.len(), 10, "every order got a distinct secret");
    assert!(ordering.reserve_secrets(id, 0, 1, 999, now).await.is_err());

    // release returns reserved secrets; mark_used requires every id to flip
    assert_eq!(ordering.release_secrets(200, now).await.unwrap(), 1);
    let ids = ordering.reserve_secrets(id, 0, 1, 300, now).await.unwrap();
    assert_eq!(ordering.mark_secrets_used(&ids, 300, now).await.unwrap(), 1);
    assert!(ordering.mark_secrets_used(&ids, 301, now).await.is_err());
    assert_eq!(
        ordering
            .order_secrets(300, Some("used"))
            .await
            .unwrap()
            .len(),
        1
    );

    // orderable only when product and category are active
    assert!(ordering.orderable_product(id).await.unwrap().is_some());
    data(
        &app.patch(
            &format!("/api/v1/admin/categories/{cat}/active"),
            json!({"is_active": false}),
        )
        .await,
    );
    assert!(ordering.orderable_product(id).await.unwrap().is_none());
}

/// QA-A10 / QA-A11 (live QA I-10, I-11; original gaps): a re-import skips secrets
/// already in stock, and a sold secret can never be put back on sale.
#[tokio::test]
async fn qa_a10_a11_reimport_and_sold_secret_guards() {
    let app = TestApp::new().await;
    let cat = category(&app, "c", 0).await;
    let (id, skus) = auto_product(
        &app,
        cat,
        "dup",
        json!([{"sku_code": "A", "price_amount": 1}]),
    )
    .await;
    let batch = |secrets: Value| json!({"product_id": id, "sku_id": skus[0], "secrets": secrets});
    let res = app
        .post(&format!("{SECRETS}/batch"), batch(json!(["S1\nS2\nS3"])))
        .await;
    assert_eq!(data(&res)["created"], 3);
    // re-import: only the new secret is added
    let res = app
        .post(&format!("{SECRETS}/batch"), batch(json!(["S1\nS2\nS4"])))
        .await;
    assert_eq!(data(&res)["created"], 1, "{res}");
    // everything already in stock → refused with a clear message
    let res = app
        .post(&format!("{SECRETS}/batch"), batch(json!(["S1", "S4"])))
        .await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (400, Some("这些卡密已全部存在，未导入新卡密"))
    );
    // CSV import de-duplicates against the stock as well
    let (pid, sid) = (id.to_string(), skus[0].to_string());
    let res = multipart(
        &app,
        &format!("{SECRETS}/import"),
        &[("product_id", &pid), ("sku_id", &sid)],
        Some("secret\nS2\nS5\n"),
    )
    .await;
    assert_eq!(data(&res)["created"], 1, "{res}");
    assert_eq!(count_secrets(&app, id, "available").await, 5);

    // QA-A11: mark one sold, then try to put it back on sale
    let rows = card_secrets::Entity::find()
        .filter(card_secrets::Column::ProductId.eq(id))
        .all(&app.db)
        .await
        .unwrap();
    let (sold, other) = (rows[0].id, rows[1].id);
    let res = app
        .patch(
            &format!("{SECRETS}/batch-status"),
            json!({"ids": [sold], "status": "used"}),
        )
        .await;
    assert_eq!(data(&res)["affected"], 1);
    let res = app
        .put(&format!("{SECRETS}/{sold}"), json!({"status": "available"}))
        .await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (400, Some("已售出的卡密不能修改状态"))
    );
    let res = app
        .patch(
            &format!("{SECRETS}/batch-status"),
            json!({"ids": [sold, other], "status": "available"}),
        )
        .await;
    assert_eq!(data(&res)["affected"], 1, "only the unsold one: {res}");
    let row = card_secrets::Entity::find_by_id(sold)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "used");
}
