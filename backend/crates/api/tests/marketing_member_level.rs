//! Member level endpoints, level prices, user levels and automatic upgrades.

#![expect(clippy::unwrap_used, reason = "tests")]

mod catalog_common;
mod common;

use catalog_common::{user, user_row};
use common::{TestApp, data};
use serde_json::{Value, json};
use zs_shared::money::Amount;

const LEVELS: &str = "/api/v1/admin/member-levels";
const PRICES: &str = "/api/v1/admin/member-level-prices";

fn status(v: &Value) -> i64 {
    v["status_code"].as_i64().unwrap()
}

async fn level(app: &TestApp, body: Value) -> i64 {
    data(&app.post(LEVELS, body).await)["id"].as_i64().unwrap()
}

#[tokio::test]
async fn level_crud_contract() {
    let app = TestApp::new().await;
    let res = app
        .post(LEVELS, json!({"name": {"zh-CN": "默认"}, "slug": "default", "discount_rate": 100, "is_default": true, "sort_order": 0}))
        .await;
    let d = data(&res);
    assert_eq!(d["discount_rate"], "100.00");
    assert_eq!(d["recharge_threshold"], "0.00");
    assert_eq!(d["is_default"], true);
    assert_eq!(d["is_active"], true);
    let default_id = d["id"].as_i64().unwrap();

    let dup = app
        .post(
            LEVELS,
            json!({"name": {}, "slug": "default", "sort_order": 9}),
        )
        .await;
    assert_eq!(
        (status(&dup), dup["msg"].as_str()),
        (400, Some("会员等级标识已存在"))
    );
    // PRC-06: unique sort order among active levels
    let clash = app
        .post(LEVELS, json!({"name": {}, "slug": "vip", "sort_order": 0}))
        .await;
    assert_eq!(clash["msg"], "该排序权重已被其他启用会员等级使用");
    let inactive = level(
        &app,
        json!({"name": {}, "slug": "inactive", "sort_order": 0, "is_active": false}),
    )
    .await;
    let vip = level(&app, json!({"name": {"zh-CN": "VIP"}, "slug": "vip", "discount_rate": 90, "sort_order": 10, "is_default": true})).await;
    let res = app.get(LEVELS).await;
    assert_eq!(res["pagination"]["page_size"], 50);
    let list = data(&res).as_array().unwrap();
    assert_eq!(list[0]["id"], vip);
    let defaults: Vec<i64> = list
        .iter()
        .filter(|l| l["is_default"] == true)
        .map(|l| l["id"].as_i64().unwrap())
        .collect();
    assert_eq!(defaults, vec![vip], "only one default level");
    let only_inactive = app.get(&format!("{LEVELS}?is_active=false")).await;
    assert_eq!(data(&only_inactive)[0]["id"], inactive);

    // update: missing level → 404; activating into a used sort order → 400; omitted is_active kept
    let missing = app
        .put(&format!("{LEVELS}/999"), json!({"name": {}, "slug": "x"}))
        .await;
    assert_eq!(status(&missing), 404);
    let clash = app
        .put(
            &format!("{LEVELS}/{inactive}"),
            json!({"name": {}, "slug": "inactive", "sort_order": 10, "is_active": true}),
        )
        .await;
    assert_eq!(status(&clash), 400);
    let kept = app
        .put(
            &format!("{LEVELS}/{inactive}"),
            json!({"name": {"zh-CN": "改"}, "slug": "inactive", "sort_order": 10}),
        )
        .await;
    assert_eq!(data(&kept)["is_active"], false);
    assert_eq!(data(&kept)["name"]["zh-CN"], "改");

    // cannot delete the default level; others can be deleted and their slug reused
    let res = app.delete(&format!("{LEVELS}/{vip}")).await;
    assert_eq!(res["msg"], "默认会员等级不能删除");
    assert_eq!(
        data(&app.delete(&format!("{LEVELS}/{default_id}")).await),
        &json!({"deleted": true})
    );
    level(
        &app,
        json!({"name": {}, "slug": "default", "sort_order": 1}),
    )
    .await;

    // public list: active only, numbers, no status
    let public = app
        .call("GET", "/api/v1/public/member-levels", None, None)
        .await;
    let first = &data(&public)[0];
    assert_eq!(first["discount_rate"], 90.0);
    assert!(first.get("is_active").is_none());
    assert_eq!(data(&public).as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn level_prices_and_user_levels() {
    let app = TestApp::new().await;
    let vip = level(
        &app,
        json!({"name": {}, "slug": "vip", "sort_order": 1, "is_default": true}),
    )
    .await;
    let res = app
        .post(
            &format!("{PRICES}/batch"),
            json!({"prices": [
                {"member_level_id": vip, "product_id": 5, "price_amount": 8},
                {"member_level_id": vip, "product_id": 5, "sku_id": 3, "price_amount": "7.5"}
            ]}),
        )
        .await;
    assert_eq!(data(&res), &json!({"saved": true}));
    let list = app.get(&format!("{PRICES}?product_id=5")).await;
    let rows = data(&list).as_array().unwrap().clone();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        (rows[0]["sku_id"].as_i64(), rows[0]["price_amount"].as_str()),
        (Some(0), Some("8.00"))
    );
    assert_eq!(status(&app.get(PRICES).await), 400);
    let bad = app
        .post(
            &format!("{PRICES}/batch"),
            json!({"prices": [{"member_level_id": vip, "product_id": 5}]}),
        )
        .await;
    assert_eq!(status(&bad), 400);
    // delete then upsert the same key again (unique index includes deleted rows)
    let id = rows[0]["id"].as_i64().unwrap();
    assert_eq!(
        data(&app.delete(&format!("{PRICES}/{id}")).await),
        &json!({"deleted": true})
    );
    data(
        &app.post(
            &format!("{PRICES}/batch"),
            json!({"prices": [{"member_level_id": vip, "product_id": 5, "price_amount": 6}]}),
        )
        .await,
    );
    let list = app.get(&format!("{PRICES}?product_id=5")).await;
    assert_eq!(data(&list)[0]["price_amount"], "6.00");

    // user level management
    let u = user(&app, "a@example.com", 0).await;
    let u2 = user(&app, "b@example.com", 0).await;
    let res = app
        .put(
            &format!("/api/v1/admin/users/{u}/member-level"),
            json!({"member_level_id": 999}),
        )
        .await;
    assert_eq!(status(&res), 404);
    let res = app
        .put(
            "/api/v1/admin/users/9999/member-level",
            json!({"member_level_id": vip}),
        )
        .await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (500, Some("更新用户会员等级失败"))
    );
    assert_eq!(
        data(
            &app.put(
                &format!("/api/v1/admin/users/{u}/member-level"),
                json!({"member_level_id": vip})
            )
            .await
        ),
        &json!({"updated": true})
    );
    assert_eq!(user_row(&app, u).await.member_level_id, vip);
    let res = app.post(&format!("{LEVELS}/backfill"), json!({})).await;
    assert_eq!(data(&res), &json!({"affected": 1}));
    assert_eq!(user_row(&app, u2).await.member_level_id, vip);
    // PRC-12: default level assignment never overwrites an existing level
    let u3 = user(&app, "c@example.com", 42).await;
    app.services
        .marketing
        .member_level
        .assign_default_level(u3)
        .await
        .unwrap();
    assert_eq!(user_row(&app, u3).await.member_level_id, 42);
}

// QA-A07 (live QA I-7): the message is translated, not the raw key.
#[tokio::test]
async fn backfill_requires_default_level() {
    let app = TestApp::new().await;
    let res = app.post(&format!("{LEVELS}/backfill"), json!({})).await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (400, Some("尚未设置默认会员等级"))
    );
}

// QA-A13 (live QA I-13, original gap): a level still assigned to a user cannot
// be deleted; once nobody uses it, it can.
#[tokio::test]
async fn qa_a13_level_in_use_cannot_be_deleted() {
    let app = TestApp::new().await;
    level(
        &app,
        json!({"name": {}, "slug": "base", "sort_order": 0, "is_default": true}),
    )
    .await;
    let gold = level(&app, json!({"name": {}, "slug": "gold", "sort_order": 5})).await;
    let u = user(&app, "g@example.com", gold).await;
    let res = app.delete(&format!("{LEVELS}/{gold}")).await;
    assert_eq!(
        (status(&res), res["msg"].as_str()),
        (400, Some("该会员等级仍有用户使用，请先调整这些用户的等级"))
    );
    assert_eq!(user_row(&app, u).await.member_level_id, gold);
    app.put(
        &format!("/api/v1/admin/users/{u}/member-level"),
        json!({"member_level_id": 0}),
    )
    .await;
    let res = app.delete(&format!("{LEVELS}/{gold}")).await;
    assert_eq!(status(&res), 0, "{res}");
}

// PRC-06, PRC-12
#[tokio::test]
async fn upgrades_are_atomic_and_never_sideways() {
    let app = TestApp::new().await;
    let svc = &app.services.marketing.member_level;
    let base = level(
        &app,
        json!({"name": {}, "slug": "base", "sort_order": 0, "is_default": true}),
    )
    .await;
    let vip = level(
        &app,
        json!({"name": {}, "slug": "vip", "sort_order": 5, "spend_threshold": 100}),
    )
    .await;
    let gold = level(
        &app,
        json!({"name": {}, "slug": "gold", "sort_order": 10, "spend_threshold": 1000}),
    )
    .await;

    let u = user(&app, "u@example.com", base).await;
    let mut handles = Vec::new();
    for _ in 0..10 {
        let svc = svc.clone();
        handles.push(tokio::spawn(async move {
            svc.on_order_paid(u, Amount::from(10)).await
        }));
    }
    for h in handles {
        h.await.unwrap().unwrap();
    }
    let row = user_row(&app, u).await;
    assert_eq!(row.total_spent.to_string().parse::<f64>().unwrap(), 100.0);
    assert_eq!(row.member_level_id, vip);

    // a user already at gold keeps gold
    let rich = user(&app, "r@example.com", gold).await;
    svc.on_order_paid(rich, Amount::from(100)).await.unwrap();
    assert_eq!(user_row(&app, rich).await.member_level_id, gold);

    // recharge threshold also upgrades; zero/negative amounts are ignored
    let r = user(&app, "re@example.com", base).await;
    let recharge = level(
        &app,
        json!({"name": {}, "slug": "recharge", "sort_order": 20, "recharge_threshold": 50}),
    )
    .await;
    svc.on_recharge_completed(r, Amount::from(-5))
        .await
        .unwrap();
    assert_eq!(user_row(&app, r).await.member_level_id, base);
    svc.on_recharge_completed(r, Amount::from(50))
        .await
        .unwrap();
    assert_eq!(user_row(&app, r).await.member_level_id, recharge);

    // PRC-07: disabled level gets no member price
    data(&app.post(&format!("{PRICES}/batch"), json!({"prices": [{"member_level_id": vip, "product_id": 9, "sku_id": 1, "price_amount": 8}]})).await);
    let p = svc
        .resolve_price(vip, 9, 1, Amount::from(10))
        .await
        .unwrap();
    assert_eq!((p.price, p.discount), (Amount::from(8), Amount::from(2)));
    data(&app.put(&format!("{LEVELS}/{vip}"), json!({"name": {}, "slug": "vip", "sort_order": 5, "spend_threshold": 100, "is_active": false})).await);
    let p = svc
        .resolve_price(vip, 9, 1, Amount::from(10))
        .await
        .unwrap();
    assert_eq!((p.price, p.discount), (Amount::from(10), Amount::ZERO));
    let none = svc.resolve_price(0, 9, 1, Amount::from(10)).await.unwrap();
    assert_eq!(none.price, Amount::from(10));
}

// QA-A20 (live QA I-20): out-of-range discount rates and negative thresholds are
// refused instead of being stored silently.
#[tokio::test]
async fn qa_a20_level_values_are_range_checked() {
    let app = TestApp::new().await;
    for bad in [
        json!({"name": {}, "slug": "a", "discount_rate": 120}),
        json!({"name": {}, "slug": "b", "discount_rate": -5}),
        json!({"name": {}, "slug": "c", "spend_threshold": -1}),
    ] {
        let res = app.post(LEVELS, bad.clone()).await;
        assert_eq!(status(&res), 400, "{bad}: {res}");
    }
    let ok = app
        .post(
            LEVELS,
            json!({"name": {}, "slug": "d", "discount_rate": 100}),
        )
        .await;
    assert_eq!(status(&ok), 0, "{ok}");
}
