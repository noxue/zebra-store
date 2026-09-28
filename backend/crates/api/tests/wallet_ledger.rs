//! The wallet ledger port used by other groups inside their own transactions:
//! conditional balance updates, reference idempotency and allocation rounds (PAY-02),
//! all on a single SQLite connection (DB-01).

#![expect(clippy::unwrap_used, reason = "tests")]

mod wallet_common;

use std::time::{Duration, Instant};

use chrono::Utc;
use sea_orm::TransactionTrait;
use wallet_common::App;
use zs_domain::wallet::ports::{BalanceChangeRequest, LedgerError};
use zs_domain::wallet::rules::underpaid_reference;
use zs_domain::wallet::txn_type;
use zs_infra::db::repo::wallet::ledger;
use zs_shared::money::Amount;

fn amt(v: &str) -> Amount {
    v.parse().unwrap()
}

fn change(user_id: i64, delta: &str, kind: &str, reference: &str) -> BalanceChangeRequest {
    BalanceChangeRequest {
        user_id,
        delta: amt(delta),
        kind: kind.to_owned(),
        reference: reference.to_owned(),
        remark: String::new(),
        currency: "CNY".to_owned(),
        operator_admin_id: None,
        order_id: None,
    }
}

async fn credit(app: &App, user_id: i64, amount: &str, reference: &str) {
    let txn = app.db.begin().await.unwrap();
    ledger::credit(
        &txn,
        &change(user_id, amount, txn_type::RECHARGE, reference),
        Utc::now(),
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
}

/// Concurrent debits never overspend: 20 × 10 against a balance of 100 → exactly 10 win.
#[tokio::test]
async fn concurrent_debits_never_overspend() {
    let app = App::new().await;
    let (uid, _) = app.user("debit@example.com").await;
    credit(&app, uid, "100", "seed").await;
    let started = Instant::now();
    let tasks: Vec<_> = (0..20)
        .map(|i| {
            let db = app.db.clone();
            tokio::spawn(async move {
                let txn = db.begin().await.unwrap();
                let req = change(
                    uid,
                    "0",
                    txn_type::ORDER_PAY,
                    &format!("order:{i}:order_pay"),
                );
                let out = ledger::debit(&txn, &req, amt("10"), Utc::now()).await;
                match out {
                    Ok(_) => {
                        txn.commit().await.unwrap();
                        true
                    }
                    Err(e) => {
                        assert_eq!(e, LedgerError::InsufficientBalance);
                        false
                    }
                }
            })
        })
        .collect();
    let mut wins = 0;
    for t in tasks {
        if t.await.unwrap() {
            wins += 1;
        }
    }
    // DB-01: a single connection never deadlocks.
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(wins, 10);
    assert_eq!(app.balance(uid).await, "0.00");
}

/// DB-01 (3): 20 concurrent credits on one connection finish quickly with the right sum.
#[tokio::test]
async fn db_01_concurrent_credits_complete() {
    let app = App::new().await;
    let (uid, _) = app.user("credit@example.com").await;
    let started = Instant::now();
    let tasks: Vec<_> = (0..20)
        .map(|i| {
            let db = app.db.clone();
            tokio::spawn(async move {
                let txn = db.begin().await.unwrap();
                ledger::credit(
                    &txn,
                    &change(uid, "1.25", txn_type::RECHARGE, &format!("r:{i}")),
                    Utc::now(),
                )
                .await
                .unwrap();
                txn.commit().await.unwrap();
            })
        })
        .collect();
    for t in tasks {
        t.await.unwrap();
    }
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(app.balance(uid).await, "25.00");
}

/// PAY-02: the underpaid credit is idempotent per payment reference.
#[tokio::test]
async fn pay_02_underpaid_credit_is_idempotent() {
    let app = App::new().await;
    let (uid, _) = app.user("under@example.com").await;
    for _ in 0..2 {
        let txn = app.db.begin().await.unwrap();
        let mut req = change(
            uid,
            "10",
            txn_type::ORDER_UNDERPAID_CREDIT,
            &underpaid_reference(77),
        );
        req.order_id = Some(5);
        let (account, t) = ledger::credit(&txn, &req, Utc::now()).await.unwrap();
        txn.commit().await.unwrap();
        assert_eq!(account.balance, amt("10"));
        assert_eq!(t.reference, "payment:77:underpaid_credit");
        assert_eq!(t.kind, "order_underpaid_credit");
    }
    assert_eq!(app.balance(uid).await, "10.00");
    // Guests have no wallet; zero credits are invalid.
    let txn = app.db.begin().await.unwrap();
    assert_eq!(
        ledger::credit(&txn, &change(0, "1", txn_type::RECHARGE, "g"), Utc::now())
            .await
            .unwrap_err(),
        LedgerError::NotSupportedForGuest
    );
    assert_eq!(
        ledger::credit(&txn, &change(uid, "0", txn_type::RECHARGE, "z"), Utc::now())
            .await
            .unwrap_err(),
        LedgerError::InvalidAmount
    );
}

/// PAY-02: "use balance → release → use again" debits twice and releases once, with
/// round-numbered references.
#[tokio::test]
async fn pay_02_allocation_rounds() {
    let app = App::new().await;
    let (uid, _) = app.user("rounds@example.com").await;
    credit(&app, uid, "20", "seed").await;
    let order = 42;

    let txn = app.db.begin().await.unwrap();
    let used = ledger::apply_order_balance(&txn, order, uid, amt("15"), "CNY", Utc::now())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(used, amt("15"));
    assert_eq!(app.balance(uid).await, "5.00");

    let txn = app.db.begin().await.unwrap();
    let released = ledger::release_order_balance(
        &txn,
        &ledger::Release {
            order_id: order,
            user_id: uid,
            amount: amt("15"),
            kind: txn_type::ORDER_REFUND,
            currency: "CNY",
            remark: "",
        },
        Utc::now(),
    )
    .await
    .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(released, amt("15"));
    assert_eq!(app.balance(uid).await, "20.00");

    // Second round: only the balance available is used (min(balance, total)).
    let txn = app.db.begin().await.unwrap();
    let used = ledger::apply_order_balance(&txn, order, uid, amt("30"), "CNY", Utc::now())
        .await
        .unwrap();
    txn.commit().await.unwrap();
    assert_eq!(used, amt("20"));
    assert_eq!(app.balance(uid).await, "0.00");

    let txn = app.db.begin().await.unwrap();
    let pays = ledger::count_order_transactions(&txn, order, txn_type::ORDER_PAY)
        .await
        .unwrap();
    let refunds = ledger::count_order_transactions(&txn, order, txn_type::ORDER_REFUND)
        .await
        .unwrap();
    assert_eq!((pays, refunds), (2, 1));
    let first = ledger::transaction_by_reference(&txn, "order:42:order_pay")
        .await
        .unwrap()
        .unwrap();
    let second = ledger::transaction_by_reference(&txn, "order:42:order_pay:2")
        .await
        .unwrap()
        .unwrap();
    assert_eq!((first.amount, second.amount), (amt("15"), amt("20")));
    assert_eq!(second.balance_before, amt("20"));
    assert_eq!(second.balance_after, Amount::ZERO);
    // An empty wallet is simply not used.
    let used = ledger::apply_order_balance(&txn, 43, uid, amt("5"), "CNY", Utc::now())
        .await
        .unwrap();
    assert_eq!(used, Amount::ZERO);
}
