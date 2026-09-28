//! Wallet recharges: channel listing, creation, queries, the WAL-01 terminal-state matrix,
//! WAL-03 capture fallback, completion hooks and the recharge-aware settlement.

mod wallet_common;

use std::sync::Arc;

use chrono::Utc;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
use wallet_common::{App, QueryMode, data, dec, err, input, keys, paid_input};
use zs_domain::payment::settlement::PaymentSettlement;
use zs_domain::payment::types::PaymentStatus;
use zs_domain::queue::kinds;
use zs_infra::db::entity::payments;

async fn setup() -> (App, i64, String, i64) {
    let app = App::new().await;
    let (uid, token) = app.user("payer@example.com").await;
    let channel = app.channel("epay", "alipay", json!(["wallet"]), "1").await;
    (app, uid, token, channel)
}

async fn create(app: &App, token: &str, channel: i64, amount: &str) -> Value {
    let res = app
        .call(
            "POST",
            "/api/v1/wallet/recharge",
            Some(json!({"amount": amount, "channel_id": channel, "remark": "top up"})),
            Some(token),
        )
        .await;
    data(&res).clone()
}

#[tokio::test]
async fn payment_channels_only_list_wallet_channels() {
    let (app, _, token, channel) = setup().await;
    app.channel("epay", "wxpay", json!(["order"]), "0").await;
    let res = app
        .call(
            "POST",
            "/api/v1/wallet/payment-channels",
            Some(json!({"amount": "10"})),
            Some(&token),
        )
        .await;
    let list = data(&res).as_array().unwrap().clone();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0]["id"], channel);
    assert_eq!(list[0]["provider_type"], "epay");
    // Fee fields are only disclosed when customers pay the fee (PAY-03).
    assert!(list[0].get("fee_rate").is_none());
    let res = app
        .call(
            "POST",
            "/api/v1/wallet/payment-channels",
            Some(json!({})),
            Some(&token),
        )
        .await;
    err(&res, 400, "Amount: is required");
    for bad in [json!({"amount": "x"}), json!({"amount": "0"})] {
        let res = app
            .call(
                "POST",
                "/api/v1/wallet/payment-channels",
                Some(bad),
                Some(&token),
            )
            .await;
        err(&res, 400, "Invalid request parameters");
    }
}

#[tokio::test]
async fn create_recharge_shapes_and_queries() {
    let (app, uid, token, channel) = setup().await;
    app.set_setting("payment_config", json!({"customer_fee_enabled": true}))
        .await;
    let d = create(&app, &token, channel, "100").await;
    let no = d["recharge_no"].as_str().unwrap().to_owned();
    assert!(no.starts_with("WR"));
    assert_eq!(d["recharge_status"], "pending");
    assert_eq!(d["status"], "pending");
    assert_eq!(d["provider_type"], "epay");
    assert_eq!(d["pay_url"], format!("https://pay.test/{no}"));
    assert_eq!(d["fee_policy"], "customer_surcharge");
    assert_eq!(d["account"], json!({"balance": "0.00"}));
    let recharge = &d["recharge"];
    assert_eq!(
        keys(recharge),
        [
            "amount",
            "created_at",
            "currency",
            "fee_amount",
            "fee_rate",
            "id",
            "paid_at",
            "payable_amount",
            "recharge_no",
            "remark",
            "status"
        ]
    );
    assert_eq!(recharge["amount"], "100.00");
    assert_eq!(recharge["payable_amount"], "101.00");
    assert_eq!(recharge["fee_rate"], "1.00");
    assert_eq!(recharge["fee_amount"], "1.00");
    assert_eq!(recharge["paid_at"], Value::Null);
    assert_eq!(recharge["remark"], "top up");
    // Internal fields never leak.
    assert!(d.get("provider_payload").is_none() && d.get("gateway_order_no").is_none());

    let payment_id = d["payment_id"].as_i64().unwrap();
    let row = app.payment_row(payment_id).await;
    assert_eq!(row.order_id, 0);
    assert_eq!(row.status, "pending");
    assert_eq!(row.amount, dec("101"));
    assert_eq!(row.gateway_order_no, format!("DJPTEST{payment_id}"));
    // WAL-01: the timeout job is scheduled for the payment.
    let jobs = app.jobs(kinds::WALLET_RECHARGE_EXPIRE).await;
    assert_eq!(jobs.len(), 1);
    assert!(
        jobs[0]
            .payload
            .contains(&format!("\"payment_id\":{payment_id}"))
    );
    assert!(jobs[0].run_at > Utc::now() + chrono::Duration::minutes(14));

    // Lists, stats and detail.
    create(&app, &token, channel, "5").await;
    let res = app
        .call(
            "GET",
            "/api/v1/wallet/recharges?page_size=1",
            None,
            Some(&token),
        )
        .await;
    assert_eq!(res["pagination"]["total"], 2);
    assert_eq!(data(&res)[0]["amount"], "5.00");
    let res = app
        .call(
            "GET",
            &format!("/api/v1/wallet/recharges?recharge_no={no}"),
            None,
            Some(&token),
        )
        .await;
    assert_eq!(res["pagination"]["total"], 1);
    let res = app
        .call("GET", "/api/v1/wallet/recharges/stats", None, Some(&token))
        .await;
    assert_eq!(
        data(&res),
        &json!({"total": 2, "by_status": {"pending": 2}})
    );
    let res = app
        .call(
            "GET",
            &format!("/api/v1/wallet/recharges/{no}"),
            None,
            Some(&token),
        )
        .await;
    assert_eq!(data(&res)["payment_id"], payment_id);
    let res = app
        .call("GET", "/api/v1/wallet/recharges/WRNOPE", None, Some(&token))
        .await;
    err(&res, 404, "Payment not found");

    // Another user cannot see it.
    let (_, other) = app.user("other@example.com").await;
    let res = app
        .call(
            "GET",
            &format!("/api/v1/wallet/recharges/{no}"),
            None,
            Some(&other),
        )
        .await;
    err(&res, 404, "Payment not found");

    // Admin list with user, channel name and payment status.
    let res = app
        .admin_call(
            "GET",
            &format!(
                "/api/v1/admin/wallet/recharges?user_keyword=PAYER&status=PENDING&user_id={uid}"
            ),
            None,
        )
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["user"]["email"], "payer@example.com");
    assert_eq!(rows[0]["channel_name"], "epay-alipay");
    assert_eq!(rows[0]["payment_status"], "pending");
    assert_eq!(rows[0]["provider_type"], "epay");
    let res = app
        .admin_call("GET", "/api/v1/admin/wallet/recharges?user_id=abc", None)
        .await;
    err(&res, 400, "Invalid request parameters");
    let res = app
        .admin_call(
            "GET",
            "/api/v1/admin/wallet/recharges?created_from=yesterday",
            None,
        )
        .await;
    err(&res, 400, "Invalid request parameters");
}

#[tokio::test]
async fn create_recharge_validations() {
    let (app, _, token, channel) = setup().await;
    let order_only = app.channel("epay", "wxpay", json!(["order"]), "0").await;
    let post = |body: Value| {
        let app = &app;
        let token = token.clone();
        async move {
            app.call("POST", "/api/v1/wallet/recharge", Some(body), Some(&token))
                .await
        }
    };
    err(
        &post(json!({"amount": "10"})).await,
        400,
        "ChannelID: is required",
    );
    err(
        &post(json!({"amount": "0", "channel_id": channel})).await,
        400,
        "Invalid request parameters",
    );
    err(
        &post(json!({"amount": "10", "channel_id": 999})).await,
        404,
        "Payment channel not found",
    );
    err(
        &post(json!({"amount": "10", "channel_id": order_only})).await,
        400,
        "This payment channel is not available for wallet recharge",
    );
    // PAY-11: the wallet recharge channel whitelist is enforced on creation.
    app.set_setting(
        "wallet_config",
        json!({"recharge_channel_ids": [order_only]}),
    )
    .await;
    err(
        &post(json!({"amount": "10", "channel_id": channel})).await,
        400,
        "This payment channel is not available for wallet recharge",
    );
    app.set_setting("wallet_config", json!({"recharge_channel_ids": []}))
        .await;
    // Gateway failure marks the recharge failed.
    *app.gateway.fail_start.lock().unwrap() = true;
    err(
        &post(json!({"amount": "10", "channel_id": channel})).await,
        400,
        "Payment gateway request failed",
    );
    let res = app
        .call("GET", "/api/v1/wallet/recharges/stats", None, Some(&token))
        .await;
    assert_eq!(data(&res)["by_status"], json!({"failed": 1}));
    assert!(app.jobs(kinds::WALLET_RECHARGE_EXPIRE).await.is_empty());
}

/// WAL-01: success credits once; duplicates never credit twice; member level and
/// notifications run once.
#[tokio::test]
async fn wal_01_success_credits_once_with_hooks() {
    let (app, uid, token, channel) = setup().await;
    let d = create(&app, &token, channel, "50").await;
    let pid = d["payment_id"].as_i64().unwrap();
    let svc = &app.services.wallet.recharge;
    let settled = svc
        .settle(&paid_input(pid, channel, "50.00", "CNY"))
        .await
        .unwrap();
    assert!(settled.newly_succeeded);
    assert_eq!(settled.recharge.as_ref().unwrap().status, "success");
    for _ in 0..2 {
        let again = svc
            .settle(&paid_input(pid, channel, "50.00", "CNY"))
            .await
            .unwrap();
        assert!(!again.newly_succeeded);
    }
    assert_eq!(app.balance(uid).await, "50.00");
    let res = app
        .call("GET", "/api/v1/wallet/transactions", None, Some(&token))
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["type"], "recharge");
    assert_eq!(rows[0]["remark"], "top up");
    // Member level hook: total_recharged accumulated exactly once.
    assert_eq!(app.user_row(uid).await.total_recharged, dec("50"));
    // Notification queued once.
    let jobs = app.jobs(kinds::NOTIFICATION_DISPATCH).await;
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].payload.contains("wallet_recharge_success"));
    let res = app
        .call(
            "GET",
            &format!(
                "/api/v1/wallet/recharges/{}",
                d["recharge_no"].as_str().unwrap()
            ),
            None,
            Some(&token),
        )
        .await;
    assert_eq!(data(&res)["recharge_status"], "success");
    assert_eq!(data(&res)["status"], "success");
    assert!(data(&res)["recharge"]["paid_at"].is_string());
    // Success then expire job → still success (WAL-01).
    svc.expire(pid).await.unwrap();
    assert_eq!(app.payment_row(pid).await.status, "success");
    assert_eq!(app.balance(uid).await, "50.00");
}

/// WAL-01: expired then a late success credits once; late pending/failed never reopen.
#[tokio::test]
async fn wal_01_terminal_state_matrix() {
    let (app, uid, token, channel) = setup().await;
    let svc = &app.services.wallet.recharge;

    // Expired, then pending/failed callbacks: nothing changes.
    let d = create(&app, &token, channel, "10").await;
    let pid = d["payment_id"].as_i64().unwrap();
    svc.expire(pid).await.unwrap();
    assert_eq!(app.payment_row(pid).await.status, "expired");
    for status in [PaymentStatus::Pending, PaymentStatus::Failed] {
        let s = svc
            .settle(&input(pid, channel, status, "10.00", "CNY"))
            .await
            .unwrap();
        assert_eq!(s.payment.status, PaymentStatus::Expired);
        assert_eq!(s.recharge.unwrap().status, "expired");
    }
    // Expired, then success → credited once.
    svc.settle(&paid_input(pid, channel, "10.00", "CNY"))
        .await
        .unwrap();
    svc.settle(&paid_input(pid, channel, "10.00", "CNY"))
        .await
        .unwrap();
    assert_eq!(app.balance(uid).await, "10.00");

    // Failed, then success → credited once.
    let d = create(&app, &token, channel, "7").await;
    let pid = d["payment_id"].as_i64().unwrap();
    let s = svc
        .settle(&input(pid, channel, PaymentStatus::Failed, "0", ""))
        .await
        .unwrap();
    assert_eq!(s.recharge.unwrap().status, "failed");
    svc.settle(&paid_input(pid, channel, "7.00", "CNY"))
        .await
        .unwrap();
    assert_eq!(app.balance(uid).await, "17.00");

    // Success then pending → stays success.
    let s = svc
        .settle(&input(pid, channel, PaymentStatus::Pending, "0", ""))
        .await
        .unwrap();
    assert_eq!(s.payment.status, PaymentStatus::Success);
    assert_eq!(s.recharge.unwrap().status, "success");

    // PAY-04: an underpaid or wrong-currency success is rejected.
    let d = create(&app, &token, channel, "20").await;
    let pid = d["payment_id"].as_i64().unwrap();
    assert!(
        svc.settle(&paid_input(pid, channel, "1.00", "CNY"))
            .await
            .is_err()
    );
    assert!(
        svc.settle(&paid_input(pid, channel, "20.00", "USD"))
            .await
            .is_err()
    );
    assert!(
        svc.settle(&paid_input(pid, channel + 100, "20.00", "CNY"))
            .await
            .is_err()
    );
    assert_eq!(app.payment_row(pid).await.status, "pending");
    assert_eq!(app.balance(uid).await, "17.00");
}

/// WAL-01: the timeout job only touches recharge payments.
#[tokio::test]
async fn wal_01_expire_skips_order_payments() {
    let (app, _, _, channel) = setup().await;
    let now = Utc::now();
    let pid = payments::ActiveModel {
        order_id: Set(5),
        channel_id: Set(channel),
        provider_type: Set("epay".into()),
        channel_type: Set("alipay".into()),
        interaction_mode: Set("redirect".into()),
        amount: Set(dec("5")),
        fee_rate: Set(dec("0")),
        fixed_fee: Set(dec("0")),
        fee_amount: Set(dec("0")),
        fee_policy: Set("none".into()),
        currency: Set("CNY".into()),
        status: Set("pending".into()),
        exception_code: Set(String::new()),
        provider_ref: Set(String::new()),
        gateway_order_no: Set(String::new()),
        provider_payload: Set(None),
        pay_url: Set(String::new()),
        qr_code: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id;
    app.services.wallet.recharge.expire(pid).await.unwrap();
    assert_eq!(app.payment_row(pid).await.status, "pending");
    // Missing payments are skipped without error (no retry storm).
    app.services.wallet.recharge.expire(987_654).await.unwrap();
}

/// WAL-03: channels without active query return the stored payment; others are captured.
#[tokio::test]
async fn wal_03_capture_falls_back_to_current_status() {
    let (app, uid, token, channel) = setup().await;
    let d = create(&app, &token, channel, "30").await;
    let pid = d["payment_id"].as_i64().unwrap();
    let uri = format!("/api/v1/wallet/recharge/payments/{pid}/capture");

    let res = app.call("POST", &uri, None, Some(&token)).await;
    assert_eq!(data(&res)["status"], "pending");
    assert_eq!(data(&res)["recharge_status"], "pending");

    let (_, other) = app.user("thief@example.com").await;
    err(
        &app.call("POST", &uri, None, Some(&other)).await,
        404,
        "Payment not found",
    );
    err(
        &app.call(
            "POST",
            "/api/v1/wallet/recharge/payments/abc/capture",
            None,
            Some(&token),
        )
        .await,
        400,
        "Invalid payment request",
    );

    *app.gateway.query.lock().unwrap() = QueryMode::Paid;
    let res = app.call("POST", &uri, None, Some(&token)).await;
    assert_eq!(data(&res)["status"], "success");
    assert_eq!(data(&res)["recharge_status"], "success");
    assert_eq!(data(&res)["account"]["balance"], "30.00");
    // A second capture does not credit again.
    app.call("POST", &uri, None, Some(&token)).await;
    assert_eq!(app.balance(uid).await, "30.00");
}

/// Gateway callbacks reach the wallet through the recharge-aware settlement; order
/// payments are delegated to the wrapped settlement.
#[tokio::test]
async fn recharge_aware_settlement_routes_by_business() {
    let (app, uid, token, channel) = setup().await;
    let d = create(&app, &token, channel, "12").await;
    let pid = d["payment_id"].as_i64().unwrap();
    let inner: Arc<dyn PaymentSettlement> = Arc::new(
        zs_infra::db::repo::payment::settlement::PaymentRowSettlement::new(
            app.db.clone(),
            app.ctx.clock.clone(),
        ),
    );
    let settlement =
        zs_infra::wire::wallet::recharge_settlement(&app.ctx, &app.services.wallet, inner);
    let paid = settlement
        .settle(paid_input(pid, channel, "12.00", "CNY"))
        .await
        .unwrap();
    assert_eq!(paid.status, PaymentStatus::Success);
    assert_eq!(app.balance(uid).await, "12.00");
    // Concurrent duplicate notifications still credit once.
    let tasks: Vec<_> = (0..5)
        .map(|_| {
            let s = settlement.clone();
            tokio::spawn(async move { s.settle(paid_input(pid, channel, "12.00", "CNY")).await })
        })
        .collect();
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    assert_eq!(app.balance(uid).await, "12.00");
}
