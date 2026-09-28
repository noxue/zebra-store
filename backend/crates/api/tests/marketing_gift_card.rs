//! Gift card admin endpoints and the redemption helpers for the wallet group.

#![expect(clippy::unwrap_used, reason = "tests")]

mod catalog_common;
mod common;

use catalog_common::{raw_json, user};
use chrono::Utc;
use common::{TestApp, data};
use sea_orm::TransactionTrait;
use serde_json::{Value, json};
use zs_domain::settings::SettingsExt;
use zs_infra::db::repo::marketing::gift_card::{lock_for_redeem, mark_redeemed};

const CARDS: &str = "/api/v1/admin/gift-cards";

fn status(v: &Value) -> i64 {
    v["status_code"].as_i64().unwrap()
}

async fn generate(app: &TestApp, body: Value) -> Value {
    data(&app.post(&format!("{CARDS}/generate"), body).await).clone()
}

async fn card_ids(app: &TestApp, query: &str) -> Vec<i64> {
    let res = app.get(&format!("{CARDS}?{query}")).await;
    data(&res)
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["id"].as_i64().unwrap())
        .collect()
}

#[tokio::test]
async fn generate_list_update_delete() {
    let app = TestApp::new().await;
    let res = generate(
        &app,
        json!({"name": " 春节 ", "quantity": 3, "amount": "50", "expires_at": ""}),
    )
    .await;
    assert_eq!(res["created"], 3);
    let batch = &res["batch"];
    assert!(batch["batch_no"].as_str().unwrap().starts_with("GCB"));
    assert_eq!(batch["amount"], "50.00");
    assert_eq!(batch["currency"], "CNY");
    assert_eq!(batch["quantity"], 3);
    assert!(batch["created_by"].as_i64().unwrap() > 0);

    for bad in [
        json!({"name": "x", "quantity": 10001, "amount": "1"}),
        json!({"name": "x", "quantity": -1, "amount": "1"}),
        json!({"name": "x", "quantity": 1, "amount": "0"}),
    ] {
        assert_eq!(
            app.post(&format!("{CARDS}/generate"), bad).await["msg"],
            "礼品卡参数不合法"
        );
    }
    assert_eq!(
        app.post(
            &format!("{CARDS}/generate"),
            json!({"name": "x", "quantity": 1})
        )
        .await["msg"],
        // Original `RespondBindError` on `generateRequest.Amount binding:"required"`.
        "Amount: 不能为空"
    );
    let bad_time = app
        .post(
            &format!("{CARDS}/generate"),
            json!({"name": "x", "quantity": 1, "amount": "1", "expires_at": "soon"}),
        )
        .await;
    assert_eq!(status(&bad_time), 400);

    let list = app.get(CARDS).await;
    let cards = data(&list).as_array().unwrap().clone();
    assert_eq!(cards.len(), 3);
    let first = &cards[0];
    assert!(first["code"].as_str().unwrap().starts_with("GC"));
    assert_eq!(
        first["code"].as_str().unwrap(),
        first["code"].as_str().unwrap().to_uppercase()
    );
    assert_eq!(first["name"], "春节");
    assert_eq!(first["status"], "active");
    assert_eq!(first["is_expired"], false);
    assert_eq!(first["batch"]["batch_no"], batch["batch_no"]);
    assert!(first.get("redeemed_user").is_none());
    let codes: std::collections::HashSet<&str> =
        cards.iter().map(|c| c["code"].as_str().unwrap()).collect();
    assert_eq!(codes.len(), 3, "codes are unique");

    let id = first["id"].as_i64().unwrap();
    let updated = app
        .put(
            &format!("{CARDS}/{id}"),
            json!({"name": "新名字", "status": "disabled", "expires_at": "2099-01-01T00:00:00Z"}),
        )
        .await;
    let u = data(&updated);
    assert_eq!(
        (u["name"].as_str(), u["status"].as_str()),
        (Some("新名字"), Some("disabled"))
    );
    assert!(u["expires_at"].as_str().unwrap().starts_with("2099-01-01"));
    let cleared = app
        .put(&format!("{CARDS}/{id}"), json!({"expires_at": ""}))
        .await;
    assert!(data(&cleared)["expires_at"].is_null());
    let past = app
        .put(
            &format!("{CARDS}/{id}"),
            json!({"expires_at": "2000-01-01T00:00:00Z"}),
        )
        .await;
    assert_eq!(past["msg"], "礼品卡参数不合法");
    let redeem = app
        .put(&format!("{CARDS}/{id}"), json!({"status": "redeemed"}))
        .await;
    assert_eq!(status(&redeem), 400);
    assert_eq!(
        status(&app.put(&format!("{CARDS}/999"), json!({"name": "x"})).await),
        404
    );

    assert_eq!(
        data(&app.delete(&format!("{CARDS}/{id}")).await),
        &json!({"deleted": true})
    );
    assert_eq!(
        app.delete(&format!("{CARDS}/{id}")).await["msg"],
        "礼品卡不存在"
    );
}

#[tokio::test]
async fn currency_follows_site_config() {
    let app = TestApp::new().await;
    // settings are written the way the content group stores them
    let ctx = zs_infra::wire::WireCtx::new(&app.db, &common::test_config());
    ctx.settings
        .put("site_config", &json!({"currency": "usd"}))
        .await
        .unwrap();
    let res = generate(&app, json!({"name": "USD", "quantity": 1, "amount": "5"})).await;
    assert_eq!(res["batch"]["currency"], "USD");
}

#[tokio::test]
async fn filters_batch_status_export_and_redeem() {
    let app = TestApp::new().await;
    let a = generate(&app, json!({"name": "A", "quantity": 2, "amount": "10"})).await;
    let past = (Utc::now() - chrono::Duration::days(1)).to_rfc3339();
    generate(
        &app,
        json!({"name": "B", "quantity": 1, "amount": "20", "expires_at": past}),
    )
    .await;
    let batch_a = a["batch"]["batch_no"].as_str().unwrap().to_owned();

    let all = card_ids(&app, "").await;
    assert_eq!(all.len(), 3);
    assert_eq!(card_ids(&app, "status=expired").await.len(), 1);
    assert_eq!(card_ids(&app, "status=active").await.len(), 2);
    assert_eq!(
        card_ids(&app, &format!("batch_no={}", batch_a.to_lowercase()))
            .await
            .len(),
        2
    );
    let expired = app.get(&format!("{CARDS}?status=expired")).await;
    assert_eq!(data(&expired)[0]["is_expired"], true);
    assert_eq!(
        status(&app.get(&format!("{CARDS}?redeemed_user_id=0")).await),
        400
    );
    assert_eq!(
        status(&app.get(&format!("{CARDS}?created_from=bad")).await),
        400
    );

    // redemption helpers (the wallet group runs them in its own transaction)
    let buyer = user(&app, "buyer@example.com", 0).await;
    let list = app.get(&format!("{CARDS}?status=active")).await;
    let code = data(&list)[0]["code"].as_str().unwrap().to_lowercase();
    let card_id = data(&list)[0]["id"].as_i64().unwrap();
    let txn = app.db.begin().await.unwrap();
    let card = lock_for_redeem(&txn, &format!(" {code} "), Utc::now())
        .await
        .unwrap();
    assert_eq!(card.id, card_id);
    mark_redeemed(&txn, card.id, buyer, Some(77), Utc::now())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    let txn = app.db.begin().await.unwrap();
    let again = lock_for_redeem(&txn, &code, Utc::now()).await.unwrap_err();
    assert_eq!(again.key(), "error.gift_card_redeemed");
    assert_eq!(
        mark_redeemed(&txn, card_id, buyer, None, Utc::now())
            .await
            .unwrap_err()
            .key(),
        "error.gift_card_redeemed"
    );
    drop(txn);
    let expired_code = data(&expired)[0]["code"].as_str().unwrap().to_owned();
    let txn = app.db.begin().await.unwrap();
    assert_eq!(
        lock_for_redeem(&txn, &expired_code, Utc::now())
            .await
            .unwrap_err()
            .key(),
        "error.gift_card_expired"
    );
    assert_eq!(
        lock_for_redeem(&txn, "NOPE", Utc::now())
            .await
            .unwrap_err()
            .key(),
        "error.gift_card_not_found"
    );
    drop(txn);

    let redeemed = app.get(&format!("{CARDS}?status=redeemed")).await;
    let r = &data(&redeemed)[0];
    assert_eq!(r["redeemed_user_id"], buyer);
    assert_eq!(r["wallet_txn_id"], 77);
    assert_eq!(
        r["redeemed_user"],
        json!({"id": buyer, "email": "buyer@example.com", "display_name": "buyer"})
    );
    assert_eq!(
        card_ids(&app, &format!("redeemed_user_id={buyer}")).await,
        vec![card_id]
    );

    // batch status skips redeemed cards; redeemed cards cannot be deleted
    let res = app
        .patch(
            &format!("{CARDS}/batch-status"),
            json!({"ids": all, "status": "disabled"}),
        )
        .await;
    assert_eq!(data(&res)["affected"], 2);
    let bad = app
        .patch(
            &format!("{CARDS}/batch-status"),
            json!({"ids": all, "status": "redeemed"}),
        )
        .await;
    assert_eq!(status(&bad), 400);
    let empty = app
        .patch(
            &format!("{CARDS}/batch-status"),
            json!({"ids": [0], "status": "active"}),
        )
        .await;
    assert_eq!(status(&empty), 400);
    assert_eq!(
        app.delete(&format!("{CARDS}/{card_id}")).await["msg"],
        "礼品卡参数不合法"
    );

    // export
    let csv = raw_json(
        &app,
        "POST",
        &format!("{CARDS}/export"),
        json!({"ids": all, "format": "csv"}),
    )
    .await;
    assert_eq!(csv.headers["content-type"], "text/csv; charset=utf-8");
    assert!(
        csv.headers["content-disposition"]
            .to_str()
            .unwrap()
            .starts_with("attachment; filename=\"gift_cards_")
    );
    let mut lines = csv.body.lines();
    assert_eq!(
        lines.next(),
        Some(
            "id,batch_no,name,code,amount,currency,status,redeemed_user_id,redeemed_at,expires_at,created_at"
        )
    );
    assert_eq!(lines.count(), 3);
    let txt = raw_json(
        &app,
        "POST",
        &format!("{CARDS}/export"),
        json!({"ids": [card_id], "format": "txt"}),
    )
    .await;
    assert_eq!(txt.body, code.to_uppercase());
    let none = raw_json(
        &app,
        "POST",
        &format!("{CARDS}/export"),
        json!({"ids": [9999], "format": "txt"}),
    )
    .await;
    assert_eq!(none.json()["status_code"], 404);
    let bad = raw_json(
        &app,
        "POST",
        &format!("{CARDS}/export"),
        json!({"ids": [card_id], "format": "pdf"}),
    )
    .await;
    assert_eq!(bad.json()["msg"], "礼品卡参数不合法");
}
