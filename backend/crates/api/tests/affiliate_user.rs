//! Storefront affiliate flow: open, click tracking, commissions (order hooks), dashboard,
//! withdrawals, and the confirm job (AFF-01).

#![expect(clippy::unwrap_used, reason = "tests")]

mod wallet_common;

use chrono::Utc;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, TransactionTrait};
use serde_json::{Value, json};
use wallet_common::{App, data, dec, err, keys};
use zs_infra::db::entity::affiliate_commissions as commissions;
use zs_shared::money::Amount;

const BAD: &str = "Invalid request parameters";

async fn enable(app: &App, confirm_days: i64) {
    app.set_setting(
        "affiliate_config",
        json!({"enabled": true, "commission_rate": 10, "confirm_days": confirm_days, "min_withdraw_amount": 5, "withdraw_channels": ["Alipay", "USDT"]}),
    )
    .await;
}

async fn open(app: &App, token: &str) -> Value {
    data(
        &app.call("POST", "/api/v1/affiliate/open", None, Some(token))
            .await,
    )
    .clone()
}

async fn click(app: &App, code: &str, visitor: &str, path: &str) -> Value {
    app.call(
        "POST",
        "/api/v1/public/affiliate/click",
        Some(json!({"affiliate_code": code, "visitor_key": visitor, "landing_path": path, "referrer": "https://ref"})),
        None,
    )
    .await
}

/// Seeds a paid parent order (two children) referred by `profile_id`; returns its id.
async fn referred_order(app: &App, no: &str, buyer: i64, profile_id: i64) -> i64 {
    let yes = app.product(&format!("{no}-a"), true).await;
    let no_aff = app.product(&format!("{no}-b"), false).await;
    let parent = app.order(no, buyer, "150", None, Some(profile_id)).await;
    let c1 = app
        .order(
            &format!("{no}-1"),
            buyer,
            "100",
            Some(parent),
            Some(profile_id),
        )
        .await;
    let c2 = app
        .order(
            &format!("{no}-2"),
            buyer,
            "50",
            Some(parent),
            Some(profile_id),
        )
        .await;
    app.order_item(c1, yes, "100", "20").await;
    app.order_item(c2, no_aff, "50", "0").await;
    parent
}

#[tokio::test]
async fn open_requires_program_and_is_idempotent() {
    let app = App::new().await;
    let (_, token) = app.user("aff@example.com").await;
    let res = app
        .call("POST", "/api/v1/affiliate/open", None, Some(&token))
        .await;
    err(&res, 400, "Access denied");
    assert_eq!(
        app.call("POST", "/api/v1/affiliate/open", None, None).await["status_code"],
        401
    );
    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    assert_eq!(data(&res)["opened"], false);
    assert_eq!(data(&res)["pending_commission"], "0.00");

    enable(&app, 0).await;
    let profile = open(&app, &token).await;
    assert_eq!(keys(&profile), ["code", "created_at", "id", "status"]);
    let code = profile["code"].as_str().unwrap().to_owned();
    assert_eq!(code.len(), 8);
    assert!(
        code.bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    );
    assert_eq!(profile["status"], "active");
    assert_eq!(open(&app, &token).await["code"], code);

    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    let d = data(&res);
    assert_eq!(d["opened"], true);
    assert_eq!(d["affiliate_code"], code);
    assert_eq!(d["promotion_path"], format!("/?aff={code}"));
}

#[tokio::test]
async fn clicks_are_tracked_and_deduplicated() {
    let app = App::new().await;
    enable(&app, 0).await;
    let (_, token) = app.user("clicks@example.com").await;
    let code = open(&app, &token).await["code"]
        .as_str()
        .unwrap()
        .to_owned();

    assert_eq!(
        data(&click(&app, &code.to_lowercase(), "v1", "/").await),
        &json!({"ok": true})
    );
    click(&app, &code, "v1", "/").await; // same visitor + path within 10 minutes
    click(&app, &code, "v1", "/p/2").await;
    click(&app, &code, "", "/").await; // anonymous visitors are not deduplicated
    click(&app, "UNKNOWN1", "v1", "/").await;
    err(
        &app.call(
            "POST",
            "/api/v1/public/affiliate/click",
            Some(json!({"visitor_key": "x"})),
            None,
        )
        .await,
        400,
        "AffiliateCode: is required",
    );
    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    assert_eq!(data(&res)["click_count"], 3);
}

#[tokio::test]
async fn commissions_follow_orders_and_confirmation() {
    let app = App::new().await;
    enable(&app, 7).await;
    let (_, token) = app.user("promoter@example.com").await;
    let (buyer, _) = app.user("buyer@example.com").await;
    let profile = open(&app, &token).await;
    let pid = profile["id"].as_i64().unwrap();
    let code = profile["code"].as_str().unwrap().to_owned();
    click(&app, &code, "visitor-1", "/").await;

    // Attribution for the order group: visitor click wins, self-referral ignored.
    let svc = &app.services.affiliate.service;
    assert_eq!(
        svc.resolve_order_snapshot(buyer, "", "visitor-1")
            .await
            .unwrap(),
        Some((pid, code.clone()))
    );
    let promoter = app.user_row(profile_user(&app, pid).await).await.id;
    assert_eq!(
        svc.resolve_order_snapshot(promoter, &code, "")
            .await
            .unwrap(),
        None
    );

    let order = referred_order(&app, "DJ1", buyer, pid).await;
    svc.handle_order_paid(order).await.unwrap();
    svc.handle_order_paid(order).await.unwrap(); // idempotent
    let res = app
        .call("GET", "/api/v1/affiliate/commissions", None, Some(&token))
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        keys(&rows[0]),
        [
            "commission_amount",
            "commission_type",
            "confirm_at",
            "created_at",
            "id",
            "status"
        ]
    );
    // Base = 100 − 20 coupon (the non-affiliate product is excluded); 10 % → 8.00.
    assert_eq!(rows[0]["commission_amount"], "8.00");
    assert_eq!(rows[0]["status"], "pending_confirm");
    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    assert_eq!(data(&res)["pending_commission"], "8.00");
    assert_eq!(data(&res)["valid_order_count"], 1);
    assert_eq!(data(&res)["conversion_rate"], 100.0);

    // Not yet due: the confirm job leaves it pending.
    assert_eq!(svc.confirm_due().await.unwrap(), 0);
    // Make it due, then two concurrent confirmations change it exactly once (AFF-01).
    make_due(&app).await;
    let (a, b) = tokio::join!(svc.confirm_due(), svc.confirm_due());
    assert_eq!(a.unwrap() + b.unwrap(), 1);
    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    assert_eq!(data(&res)["available_commission"], "8.00");
    assert_eq!(data(&res)["pending_commission"], "0.00");

    // Self-referred orders earn nothing.
    let own = referred_order(&app, "DJ2", promoter, pid).await;
    svc.handle_order_paid(own).await.unwrap();
    let res = app
        .call("GET", "/api/v1/affiliate/commissions", None, Some(&token))
        .await;
    assert_eq!(res["pagination"]["total"], 1);
}

async fn profile_user(app: &App, profile_id: i64) -> i64 {
    zs_infra::db::entity::affiliate_profiles::Entity::find_by_id(profile_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .user_id
}

async fn make_due(app: &App) {
    use sea_orm::sea_query::Expr;
    commissions::Entity::update_many()
        .col_expr(
            commissions::Column::ConfirmAt,
            Expr::value(Some(Utc::now() - chrono::Duration::minutes(1))),
        )
        .filter(commissions::Column::Status.eq("pending_confirm"))
        .exec(&app.db)
        .await
        .unwrap();
}

#[tokio::test]
async fn withdrawals_freeze_and_split_commissions() {
    let app = App::new().await;
    enable(&app, 0).await;
    let (_, token) = app.user("wd@example.com").await;
    let (buyer, _) = app.user("wdbuyer@example.com").await;
    let pid = open(&app, &token).await["id"].as_i64().unwrap();
    let svc = &app.services.affiliate.service;
    for no in ["W1", "W2"] {
        let order = referred_order(&app, no, buyer, pid).await;
        svc.handle_order_paid(order).await.unwrap();
    }
    // Two available commissions of 8.00 each.
    let post = |body: Value| {
        let app = &app;
        let token = token.clone();
        async move {
            app.call(
                "POST",
                "/api/v1/affiliate/withdraws",
                Some(body),
                Some(&token),
            )
            .await
        }
    };
    for bad in [
        json!({"amount": "4.99", "channel": "alipay", "account": "a"}),
        json!({"amount": "10", "channel": "bank", "account": "a"}),
        json!({"amount": "abc", "channel": "alipay", "account": "a"}),
        json!({"amount": "16.01", "channel": "alipay", "account": "a"}),
    ] {
        err(&post(bad).await, 400, BAD);
    }
    // `binding:"required"` failures name the Go field (RespondBindError)
    err(
        &post(json!({"amount": "10", "channel": "alipay", "account": ""})).await,
        400,
        "Account: is required",
    );
    let res = post(json!({"amount": "10", "channel": "ALIPAY", "account": "me@alipay"})).await;
    let w = data(&res);
    assert_eq!(
        keys(w),
        ["account", "amount", "channel", "created_at", "id", "status"]
    );
    assert_eq!(w["amount"], "10.00");
    assert_eq!(w["status"], "pending_review");
    // 8 + 2 frozen, 6 remains available (split row).
    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    assert_eq!(data(&res)["available_commission"], "6.00");
    let rows = commissions::Entity::find().all(&app.db).await.unwrap();
    assert_eq!(rows.len(), 3);
    let bound: Vec<_> = rows
        .iter()
        .filter(|r| r.withdraw_request_id.is_some())
        .collect();
    assert_eq!(bound.len(), 2);
    let total_bound: sea_orm::prelude::Decimal = bound.iter().map(|r| r.commission_amount).sum();
    assert_eq!(total_bound, dec("10"));
    assert!(
        rows.iter()
            .any(|r| r.commission_type.starts_with("sp") && r.commission_amount == dec("6"))
    );

    err(
        &post(json!({"amount": "6.01", "channel": "usdt", "account": "T"})).await,
        400,
        BAD,
    );
    let res = app
        .call("GET", "/api/v1/affiliate/withdraws", None, Some(&token))
        .await;
    assert_eq!(res["pagination"]["total"], 1);

    // A user without a profile cannot withdraw.
    let (_, stranger) = app.user("nobody@example.com").await;
    let res = app
        .call(
            "POST",
            "/api/v1/affiliate/withdraws",
            Some(json!({"amount": "5", "channel": "alipay", "account": "x"})),
            Some(&stranger),
        )
        .await;
    err(&res, 400, BAD);
}

/// Refund clawback port used inside the order group's refund transaction.
#[tokio::test]
async fn refund_clawback_is_proportional() {
    let app = App::new().await;
    enable(&app, 7).await;
    let (_, token) = app.user("cb@example.com").await;
    let (buyer, _) = app.user("cbbuyer@example.com").await;
    let pid = open(&app, &token).await["id"].as_i64().unwrap();
    let order = referred_order(&app, "CB1", buyer, pid).await;
    app.services
        .affiliate
        .service
        .handle_order_paid(order)
        .await
        .unwrap();
    let total: Amount = "150".parse().unwrap();

    let txn = app.db.begin().await.unwrap();
    let changed = zs_infra::db::repo::affiliate::clawback_on_refund(
        &txn,
        order,
        total,
        "75".parse().unwrap(),
        Amount::ZERO,
        "",
        Utc::now(),
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(changed, 1);
    let row = commissions::Entity::find()
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (row.commission_amount, row.base_amount),
        (dec("4"), dec("40"))
    );

    let txn = app.db.begin().await.unwrap();
    zs_infra::db::repo::affiliate::clawback_on_refund(
        &txn,
        order,
        total,
        "75".parse().unwrap(),
        "75".parse().unwrap(),
        "refund",
        Utc::now(),
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    let row = commissions::Entity::find()
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "rejected");
    assert_eq!(row.invalid_reason, "refund");
    assert_eq!(row.confirm_at, None);

    // Cancellation rejects open commissions.
    let order2 = referred_order(&app, "CB2", buyer, pid).await;
    let svc = &app.services.affiliate.service;
    svc.handle_order_paid(order2).await.unwrap();
    svc.handle_order_canceled(order2, "").await.unwrap();
    let row = commissions::Entity::find()
        .filter(commissions::Column::OrderId.eq(order2))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (row.status.as_str(), row.invalid_reason.as_str()),
        ("rejected", "order_canceled")
    );
}
