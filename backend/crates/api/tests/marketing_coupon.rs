//! Coupon and promotion endpoints, coupon evaluation and the usage ledger.

#![expect(clippy::unwrap_used, reason = "tests")]

mod common;

use std::sync::Arc;

use chrono::Utc;
use common::{TestApp, data};
use serde_json::{Value, json};
use zs_app::marketing::coupon::Buyer;
use zs_domain::marketing::coupon::{CouponClaim, CouponLedger, EligibilityItem};
use zs_infra::db::repo::marketing::coupon::SeaCouponRepo;
use zs_shared::money::Amount;

const COUPONS: &str = "/api/v1/admin/coupons";
const PROMOTIONS: &str = "/api/v1/admin/promotions";

fn status(v: &Value) -> i64 {
    v["status_code"].as_i64().unwrap()
}

async fn coupon(app: &TestApp, body: Value) -> Value {
    data(&app.post(COUPONS, body).await).clone()
}

#[tokio::test]
async fn coupon_crud_contract() {
    let app = TestApp::new().await;
    let c = coupon(
        &app,
        json!({"code": "SAVE10", "type": "percent", "value": 10, "max_discount": "5", "scope_ref_ids": [1, 2],
               "payment_roles": ["Member"], "member_levels": [0, 3, 3], "starts_at": "2026-01-01T00:00:00+08:00",
               "per_item_discount": true}),
    )
    .await;
    assert_eq!(c["code"], "SAVE10");
    assert_eq!(c["type"], "percent");
    assert_eq!(c["value"], "10.00");
    assert_eq!(c["max_discount"], "5.00");
    assert_eq!(c["scope_type"], "product");
    assert_eq!(c["scope_ref_ids"], "[1,2]");
    assert_eq!(c["payment_roles"], json!(["member"]));
    assert_eq!(c["member_levels"], json!([3]));
    assert_eq!(
        c["per_item_discount"], false,
        "per_item only applies to fixed coupons"
    );
    assert_eq!(c["disabled_wholesale_price"], false);
    assert_eq!(c["is_active"], true);
    assert_eq!(c["used_count"], 0);
    assert!(
        c["starts_at"]
            .as_str()
            .unwrap()
            .starts_with("2025-12-31T16:00:00")
    );
    let id = c["id"].as_i64().unwrap();

    let dup = app
        .post(
            COUPONS,
            json!({"code": "SAVE10", "type": "fixed", "value": 1, "scope_ref_ids": [1]}),
        )
        .await;
    assert_eq!(
        (status(&dup), dup["msg"].as_str()),
        (400, Some("优惠券不合法"))
    );
    // Original `RespondBindError`: required fields (Go zero value included) are named.
    for (bad, msg) in [
        (
            json!({"code": "A", "type": "fixed", "value": 1}),
            "ScopeRefIDs: 不能为空",
        ),
        (
            json!({"code": "A", "type": "fixed", "value": 0, "scope_ref_ids": [1]}),
            "Value: 不能为空",
        ),
        (
            json!({"code": "A", "type": "fixed", "value": 1, "scope_ref_ids": [1], "starts_at": "tomorrow"}),
            "请求参数错误",
        ),
    ] {
        assert_eq!(app.post(COUPONS, bad).await["msg"], msg);
    }
    let bad_pct = app
        .post(
            COUPONS,
            json!({"code": "A", "type": "percent", "value": 101, "scope_ref_ids": [1]}),
        )
        .await;
    assert_eq!(bad_pct["msg"], "优惠券不合法");
    let bad_role = app.post(COUPONS, json!({"code": "A", "type": "fixed", "value": 1, "scope_ref_ids": [1], "payment_roles": ["admin"]})).await;
    assert_eq!(bad_role["msg"], "优惠券不合法");
    let no_scope = app
        .post(
            COUPONS,
            json!({"code": "A", "type": "fixed", "value": 1, "scope_ref_ids": []}),
        )
        .await;
    assert_eq!(no_scope["msg"], "优惠券不适用于该商品");

    // update keeps omitted flags
    data(
        &app.put(
            &format!("{COUPONS}/{id}"),
            json!({"code": "SAVE10", "type": "fixed", "value": 3, "scope_ref_ids": [2],
        "disabled_wholesale_price": true, "is_active": false}),
        )
        .await,
    );
    let updated = app
        .put(
            &format!("{COUPONS}/{id}"),
            json!({"code": "SAVE-NEW", "type": "fixed", "value": 3, "scope_ref_ids": [2]}),
        )
        .await;
    let u = data(&updated);
    assert_eq!(
        (
            u["code"].as_str(),
            u["disabled_wholesale_price"].as_bool(),
            u["is_active"].as_bool()
        ),
        (Some("SAVE-NEW"), Some(true), Some(false))
    );
    let missing = app
        .put(
            &format!("{COUPONS}/999"),
            json!({"code": "X", "type": "fixed", "value": 3, "scope_ref_ids": [2]}),
        )
        .await;
    assert_eq!(
        (status(&missing), missing["msg"].as_str()),
        (404, Some("优惠券不存在"))
    );

    let deleted = app.delete(&format!("{COUPONS}/{id}")).await;
    assert_eq!(data(&deleted), &json!({"deleted": true}));
    assert_eq!(status(&app.delete(&format!("{COUPONS}/{id}")).await), 404);
    // the code can be reused after deletion
    coupon(
        &app,
        json!({"code": "SAVE-NEW", "type": "fixed", "value": 1, "scope_ref_ids": [1]}),
    )
    .await;
}

// PRC-15
#[tokio::test]
async fn coupon_list_filters_match_whole_scope_ids() {
    let app = TestApp::new().await;
    let a = coupon(
        &app,
        json!({"code": "A", "type": "fixed", "value": 1, "scope_ref_ids": [11, 21]}),
    )
    .await;
    let b = coupon(&app, json!({"code": "B", "type": "fixed", "value": 1, "scope_ref_ids": [1, 5], "is_active": false})).await;
    let c = coupon(
        &app,
        json!({"code": "C", "type": "fixed", "value": 1, "scope_ref_ids": [7, 1, 9]}),
    )
    .await;
    let ids = |v: &Value| {
        data(v)
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["id"].as_i64().unwrap())
            .collect::<Vec<_>>()
    };
    let res = app.get(&format!("{COUPONS}?scope_ref_id=1")).await;
    assert_eq!(
        ids(&res),
        vec![c["id"].as_i64().unwrap(), b["id"].as_i64().unwrap()]
    );
    assert_eq!(
        status(&app.get(&format!("{COUPONS}?scope_ref_id=abc")).await),
        400
    );
    assert_eq!(status(&app.get(&format!("{COUPONS}?id=0")).await), 400);
    let res = app.get(&format!("{COUPONS}?is_active=false")).await;
    assert_eq!(ids(&res), vec![b["id"].as_i64().unwrap()]);
    let res = app.get(&format!("{COUPONS}?code=A")).await;
    assert_eq!(ids(&res), vec![a["id"].as_i64().unwrap()]);
    assert_eq!(
        res["pagination"],
        json!({"page": 1, "page_size": 20, "total": 1, "total_page": 1})
    );
}

fn items(product_id: i64, qty: i32, total: i64) -> Vec<EligibilityItem> {
    vec![EligibilityItem {
        product_id,
        quantity: qty,
        total_price: Amount::from(total),
        wholesale_discount: Amount::ZERO,
    }]
}

// PRC-01
#[tokio::test]
async fn ledger_enforces_limits_under_concurrency() {
    let app = TestApp::new().await;
    let limited = coupon(&app, json!({"code": "ONCE", "type": "fixed", "value": 5, "usage_limit": 1, "scope_ref_ids": [1]})).await;
    let ledger = Arc::new(SeaCouponRepo::new(app.db.clone()));
    let coupon_id = limited["id"].as_i64().unwrap();
    let mut handles = Vec::new();
    for order in 0..20 {
        let ledger = ledger.clone();
        handles.push(tokio::spawn(async move {
            ledger
                .claim(
                    &CouponClaim {
                        coupon_id,
                        user_id: order + 1,
                        order_id: 1000 + order,
                        discount_amount: Amount::from(5),
                    },
                    Utc::now(),
                )
                .await
        }));
    }
    let mut ok = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(()) => ok += 1,
            Err(e) => assert_eq!(e.key(), "error.coupon_usage_limit"),
        }
    }
    assert_eq!(ok, 1);
    let res = app.get(&format!("{COUPONS}?id={coupon_id}")).await;
    assert_eq!(data(&res)[0]["used_count"], 1);

    // per-user limit
    let per_user = coupon(&app, json!({"code": "PU", "type": "fixed", "value": 5, "per_user_limit": 1, "scope_ref_ids": [1]})).await;
    let pu = per_user["id"].as_i64().unwrap();
    let mut handles = Vec::new();
    for order in 0..10 {
        let ledger = ledger.clone();
        handles.push(tokio::spawn(async move {
            ledger
                .claim(
                    &CouponClaim {
                        coupon_id: pu,
                        user_id: 7,
                        order_id: 2000 + order,
                        discount_amount: Amount::from(5),
                    },
                    Utc::now(),
                )
                .await
        }));
    }
    let mut ok = 0;
    for h in handles {
        if h.await.unwrap().is_ok() {
            ok += 1;
        }
    }
    assert_eq!(ok, 1);

    // evaluation sees the usage; release (cancel) gives it back
    let svc = &app.services.marketing.coupon;
    let buyer = Buyer {
        user_id: 7,
        is_guest: false,
        member_level_id: 0,
    };
    let err = svc.apply("PU", buyer, &items(1, 1, 100)).await.unwrap_err();
    assert_eq!(err.key(), "error.coupon_per_user_limit");
    let mut released = 0;
    for o in 2000..2010 {
        released += ledger.release(o, Utc::now()).await.unwrap();
    }
    assert_eq!(released, 1);
    let applied = svc.apply("PU", buyer, &items(1, 1, 100)).await.unwrap();
    assert_eq!(applied.discount, Amount::from(5));
    let missing = ledger
        .claim(
            &CouponClaim {
                coupon_id: 999,
                user_id: 1,
                order_id: 1,
                discount_amount: Amount::ZERO,
            },
            Utc::now(),
        )
        .await
        .unwrap_err();
    assert_eq!(missing.key(), "error.coupon_not_found");
}

// PRC-03 through the service (repository-backed)
#[tokio::test]
async fn coupon_evaluation_through_service() {
    let app = TestApp::new().await;
    coupon(&app, json!({"code": "ITEM", "type": "fixed", "value": 5, "per_item_discount": true, "max_discount": 10, "scope_ref_ids": [100]})).await;
    coupon(&app, json!({"code": "OFF", "type": "fixed", "value": 5, "scope_ref_ids": [100], "is_active": false})).await;
    let svc = &app.services.marketing.coupon;
    let guest = Buyer {
        user_id: 0,
        is_guest: true,
        member_level_id: 0,
    };
    let applied = svc
        .apply(" ITEM ", guest, &items(100, 3, 375))
        .await
        .unwrap();
    assert_eq!(
        (applied.discount, applied.eligible.quantity),
        (Amount::from(10), 3)
    );
    assert_eq!(
        svc.apply("OFF", guest, &items(100, 1, 10))
            .await
            .unwrap_err()
            .key(),
        "error.coupon_inactive"
    );
    assert_eq!(
        svc.apply("NOPE", guest, &items(100, 1, 10))
            .await
            .unwrap_err()
            .key(),
        "error.coupon_not_found"
    );
    assert_eq!(
        svc.apply("ITEM", guest, &items(5, 1, 10))
            .await
            .unwrap_err()
            .key(),
        "error.coupon_scope_invalid"
    );
    assert_eq!(
        svc.apply(" ", guest, &items(5, 1, 10))
            .await
            .unwrap_err()
            .key(),
        "error.coupon_invalid"
    );
}

#[tokio::test]
async fn promotion_crud_contract() {
    let app = TestApp::new().await;
    let created = app
        .post(PROMOTIONS, json!({"name": " Summer Sale ", "type": "PERCENT", "scope_ref_id": 5, "value": 20, "min_amount": 100}))
        .await;
    let p = data(&created);
    assert_eq!(p["name"], "Summer Sale");
    assert_eq!(p["type"], "percent");
    assert_eq!(p["scope_type"], "product");
    assert_eq!(p["value"], "20.00");
    assert_eq!(p["min_amount"], "100.00");
    assert_eq!(p["is_active"], true);
    assert!(p["starts_at"].is_null());
    let id = p["id"].as_i64().unwrap();
    for (bad, msg) in [
        (
            json!({"name": "x", "type": "percent", "scope_ref_id": 5, "value": 101}),
            "活动价规则不合法",
        ),
        (
            json!({"name": "x", "type": "bogus", "scope_ref_id": 5, "value": 1}),
            "活动价规则不合法",
        ),
        (
            json!({"name": "x", "type": "fixed", "scope_ref_id": 5, "value": 1,
                "starts_at": "2026-02-01T00:00:00Z", "ends_at": "2026-01-01T00:00:00Z"}),
            "活动价规则不合法",
        ),
        (
            json!({"name": "x", "type": "fixed", "value": 1}),
            "ScopeRefID: 不能为空",
        ),
    ] {
        assert_eq!(app.post(PROMOTIONS, bad).await["msg"], msg);
    }
    app.post(
        PROMOTIONS,
        json!({"name": "Other", "type": "special_price", "scope_ref_id": 6, "value": 9}),
    )
    .await;

    let list = app.get(&format!("{PROMOTIONS}?name=summer")).await;
    assert_eq!(
        data(&list).as_array().unwrap().len(),
        1,
        "name filter is case-insensitive"
    );
    let list = app.get(&format!("{PROMOTIONS}?scope_ref_id=6")).await;
    assert_eq!(data(&list)[0]["name"], "Other");
    let list = app.get(&format!("{PROMOTIONS}?scope_ref_id=abc")).await;
    assert_eq!(
        list["pagination"]["total"], 2,
        "a malformed scope_ref_id is ignored"
    );

    let updated = app
        .put(&format!("{PROMOTIONS}/{id}"), json!({"name": "S", "type": "fixed", "scope_ref_id": 5, "value": 3, "is_active": false}))
        .await;
    assert_eq!(
        (
            data(&updated)["type"].as_str(),
            data(&updated)["is_active"].as_bool()
        ),
        (Some("fixed"), Some(false))
    );
    let kept = app
        .put(
            &format!("{PROMOTIONS}/{id}"),
            json!({"name": "S", "type": "fixed", "scope_ref_id": 5, "value": 3}),
        )
        .await;
    assert_eq!(
        data(&kept)["is_active"],
        false,
        "omitted is_active keeps the stored flag"
    );
    let missing = app
        .put(
            &format!("{PROMOTIONS}/999"),
            json!({"name": "S", "type": "fixed", "scope_ref_id": 5, "value": 3}),
        )
        .await;
    assert_eq!(missing["msg"], "活动价不存在");
    assert_eq!(
        data(&app.delete(&format!("{PROMOTIONS}/{id}")).await),
        &json!({"deleted": true})
    );
    assert_eq!(
        app.delete(&format!("{PROMOTIONS}/{id}")).await["status_code"],
        404
    );

    // effective promotions: tiered match through the service (PRC-08)
    for (min, value) in [(0, 5), (100, 10), (300, 20)] {
        data(&app.post(PROMOTIONS, json!({"name": format!("t{min}"), "type": "percent", "scope_ref_id": 42, "value": value, "min_amount": min})).await);
    }
    data(
        &app.post(
            PROMOTIONS,
            json!({"name": "later", "type": "special_price", "scope_ref_id": 42, "value": 1,
        "starts_at": (Utc::now() + chrono::Duration::days(1)).to_rfc3339()}),
        )
        .await,
    );
    let svc = &app.services.marketing.promotion;
    let price = |q| async move {
        svc.apply(42, "50".parse().unwrap(), q)
            .await
            .unwrap()
            .unwrap()
            .1
            .to_string()
    };
    assert_eq!(price(1).await, "47.50");
    assert_eq!(price(2).await, "45.00");
    assert_eq!(price(6).await, "40.00");
}
