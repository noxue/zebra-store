//! Profit ledger, refund claw-back, confirmation job, balances and withdrawals —
//! driven through the order-group port (`services.reseller.ledger`) and the HTTP API
//! (RSL-01, RSL-02, RSL-04, RSL-05).

#![expect(clippy::unwrap_used, reason = "tests")]

mod reseller_common;

use chrono::Duration;
use reseller_common::{App, data, err};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder};
use serde_json::{Value, json};
use zs_domain::reseller::ports::{OrderPaid, OrderRefunded};
use zs_infra::db::entity::{
    reseller_balance_accounts as balances, reseller_ledger_entries as ledger,
};
use zs_shared::money::Amount;

fn amt(s: &str) -> Amount {
    s.parse().unwrap()
}

struct Shop {
    app: App,
    token: String,
    pid: i64,
    ruid: i64,
    product: i64,
    sku: i64,
}

async fn shop() -> Shop {
    let app = App::new().await;
    let (ruid, token, pid) = app.reseller("seller@example.test", "0").await;
    let (product, skus) = app.product("acc", "100.00", &[("100.00", "0")]).await;
    Shop {
        app,
        token,
        pid,
        ruid,
        product,
        sku: skus[0],
    }
}

impl Shop {
    /// A paid order with `profit` (base 100): returns the order id.
    async fn sale(&self, buyer: i64, unit: &str) -> (i64, String) {
        self.app
            .order(
                self.pid,
                self.ruid,
                buyer,
                self.product,
                self.sku,
                "100.00",
                unit,
                "paid",
            )
            .await
    }

    async fn paid(&self, order_id: i64, order_no: &str) -> bool {
        self.app
            .services
            .reseller
            .ledger
            .on_order_paid(&OrderPaid {
                order_id,
                order_no: order_no.into(),
                reseller_id: Some(self.pid),
                currency: "CNY".into(),
                wallet_paid_amount: Amount::ZERO,
                online_paid_amount: amt("130"),
                payment: None,
            })
            .await
            .unwrap()
    }

    async fn refund(&self, order_id: i64, record: i64, amount: &str, before: &str) -> bool {
        self.app
            .services
            .reseller
            .ledger
            .on_order_refunded(&OrderRefunded {
                order_id,
                order_no: String::new(),
                reseller_id: Some(self.pid),
                order_currency: "CNY".into(),
                order_total_amount: amt("130"),
                refund_record_id: record,
                refund_type: "wallet".into(),
                refund_amount: amt(amount),
                refund_currency: "CNY".into(),
                refunded_before: amt(before),
            })
            .await
            .unwrap()
    }

    async fn entries(&self, order_id: i64, kind: &str) -> Vec<ledger::Model> {
        ledger::Entity::find()
            .filter(ledger::Column::OrderId.eq(order_id))
            .filter(ledger::Column::Type.eq(kind))
            .order_by_asc(ledger::Column::Id)
            .all(&self.app.db)
            .await
            .unwrap()
    }

    async fn balance(&self) -> balances::Model {
        balances::Entity::find()
            .filter(balances::Column::ResellerId.eq(self.pid))
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
    }

    fn cache(b: &balances::Model) -> (String, String, String, String) {
        (
            Amount::new(b.available_amount_cache).to_string(),
            Amount::new(b.locked_amount_cache).to_string(),
            Amount::new(b.negative_amount_cache).to_string(),
            b.status.clone(),
        )
    }

    /// Posts profit for a sale and confirms it (available immediately after the window).
    async fn available_profit(&self, unit: &str) -> i64 {
        let (id, no) = self.sale(0, unit).await;
        assert!(self.paid(id, &no).await);
        self.app.advance(Duration::days(8));
        self.app
            .services
            .reseller
            .ledger
            .confirm_due()
            .await
            .unwrap();
        id
    }

    async fn withdraw(&self, amount: &str) -> Value {
        self.app
            .call(
                "POST",
                "/api/v1/reseller/withdraws",
                Some(json!({"amount": amount, "currency": "CNY", "channel": "alipay", "account": "a@b.test"})),
                Some(&self.token),
            )
            .await
    }
}

fn s(v: &str) -> String {
    v.to_owned()
}

// RSL-05: duplicate payment callbacks post one profit entry; pending → confirm job.
#[tokio::test]
async fn rsl05_profit_is_idempotent_and_pending_until_confirmed() {
    let shop = shop().await;
    let (id, no) = shop.sale(0, "130.00").await;
    assert!(shop.paid(id, &no).await);
    assert!(
        !shop.paid(id, &no).await,
        "second callback must not post again"
    );
    let rows = shop.entries(id, "order_profit").await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "pending_confirm");
    assert_eq!(Amount::new(rows[0].amount).to_string(), "30.00");
    assert_eq!(rows[0].idempotency_key, format!("order_profit:{id}"));
    assert_eq!(
        rows[0].available_at.unwrap(),
        shop.app.now() + Duration::days(7)
    );
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("0.00"), s("0.00"), s("0.00"), s("normal"))
    );

    // not yet due
    assert_eq!(
        shop.app
            .services
            .reseller
            .ledger
            .confirm_due()
            .await
            .unwrap(),
        0
    );
    shop.app.advance(Duration::days(7));
    assert_eq!(
        shop.app
            .services
            .reseller
            .ledger
            .confirm_due()
            .await
            .unwrap(),
        1
    );
    // idempotent: a second worker run confirms nothing
    assert_eq!(
        shop.app
            .services
            .reseller
            .ledger
            .confirm_due()
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("30.00"), s("0.00"), s("0.00"), s("normal"))
    );

    // the order view shows the profit as credited
    let list = shop
        .app
        .call("GET", "/api/v1/reseller/orders", None, Some(&shop.token))
        .await;
    assert_eq!(data(&list)[0]["profit_status"], "credited");
    let ledger = shop
        .app
        .call(
            "GET",
            "/api/v1/reseller/ledger-entries",
            None,
            Some(&shop.token),
        )
        .await;
    let row = &data(&ledger)[0];
    assert_eq!(
        (
            row["type"].clone(),
            row["amount"].clone(),
            row["status"].clone()
        ),
        (json!("order_profit"), json!("30.00"), json!("available"))
    );
    assert_eq!(row["order_id"], id);
}

// RSL-07: self-dealing orders (owner buys on own site) post no profit.
#[tokio::test]
async fn rsl07_self_dealing_posts_nothing() {
    let shop = shop().await;
    let (id, no) = shop.sale(shop.ruid, "130.00").await;
    assert!(!shop.paid(id, &no).await);
    assert!(shop.entries(id, "order_profit").await.is_empty());
    // main-site orders (no reseller) are ignored
    let main = shop
        .app
        .services
        .reseller
        .ledger
        .on_order_paid(&OrderPaid {
            order_id: id,
            order_no: no,
            reseller_id: None,
            currency: "CNY".into(),
            wallet_paid_amount: Amount::ZERO,
            online_paid_amount: Amount::ZERO,
            payment: None,
        })
        .await
        .unwrap();
    assert!(!main);
}

// RSL-01: order 130 / profit 30 refunded 52 then 78 → total claw-back exactly 30.
#[tokio::test]
async fn rsl01_partial_refunds_never_over_deduct() {
    let shop = shop().await;
    let (id, no) = shop.sale(0, "130.00").await;
    shop.paid(id, &no).await;
    shop.app.advance(Duration::days(8));
    shop.app
        .services
        .reseller
        .ledger
        .confirm_due()
        .await
        .unwrap();

    assert!(shop.refund(id, 1, "52", "0").await);
    assert!(shop.refund(id, 2, "78", "52").await);
    assert!(
        !shop.refund(id, 2, "78", "52").await,
        "same refund record is idempotent"
    );
    assert!(
        !shop.refund(id, 3, "10", "130").await,
        "nothing left to claw back"
    );
    let rows = shop.entries(id, "refund_deduct").await;
    let amounts: Vec<String> = rows
        .iter()
        .map(|r| Amount::new(r.amount).to_string())
        .collect();
    assert_eq!(amounts, vec!["-12.00", "-18.00"]);
    assert!(rows.iter().all(|r| r.status == "available"));
    assert_eq!(rows[0].idempotency_key, "refund_deduct:1");
    let meta = rows[1].metadata_json.clone().unwrap();
    assert_eq!(
        meta["refund_allocation_json"]["items"][0]["deduct_amount"],
        "18.00"
    );
    assert_eq!(meta["deduct_status"], "available");
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("0.00"), s("0.00"), s("0.00"), s("normal"))
    );
}

// RSL-01: three refunds of 43.33 / 43.33 / 43.34 converge on exactly -30.
#[tokio::test]
async fn rsl01_thirds_converge() {
    let shop = shop().await;
    let (id, no) = shop.sale(0, "130.00").await;
    shop.paid(id, &no).await;
    shop.refund(id, 11, "43.33", "0").await;
    shop.refund(id, 12, "43.33", "43.33").await;
    shop.refund(id, 13, "43.34", "86.66").await;
    let total: sea_orm::prelude::Decimal = shop
        .entries(id, "refund_deduct")
        .await
        .iter()
        .map(|r| r.amount)
        .sum();
    assert_eq!(Amount::new(total).to_string(), "-30.00");
}

// RSL-04: refund inside the confirmation window stays pending (no negative balance);
// confirming later refreshes the caches.
#[tokio::test]
async fn rsl04_refund_in_confirm_window() {
    let shop = shop().await;
    let (id, no) = shop.sale(0, "130.00").await;
    shop.paid(id, &no).await;
    let profit_due = shop.entries(id, "order_profit").await[0]
        .available_at
        .unwrap();
    shop.app.advance(Duration::days(1));
    assert!(shop.refund(id, 7, "65", "0").await);
    let rows = shop.entries(id, "refund_deduct").await;
    assert_eq!(Amount::new(rows[0].amount).to_string(), "-15.00");
    assert_eq!(rows[0].status, "pending_confirm");
    assert_eq!(rows[0].available_at.unwrap(), profit_due);
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("0.00"), s("0.00"), s("0.00"), s("normal"))
    );

    shop.app.advance(Duration::days(8));
    assert_eq!(
        shop.app
            .services
            .reseller
            .ledger
            .confirm_due()
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("15.00"), s("0.00"), s("0.00"), s("normal"))
    );
}

// RSL-02: +100 / -50 available → withdrawing 80 is refused, 50 succeeds.
#[tokio::test]
async fn rsl02_withdraw_checks_net_available() {
    let shop = shop().await;
    let id = shop.available_profit("200.00").await; // profit 100
    assert!(shop.refund(id, 1, "100", "0").await); // -50, available
    assert_eq!(Shop::cache(&shop.balance().await).0, "50.00");
    err(&shop.withdraw("80").await, 400, "可提现余额不足");
    let ok = shop.withdraw("50").await;
    let w = data(&ok);
    assert_eq!(
        (w["amount"].clone(), w["status"].clone()),
        (json!("50.00"), json!("pending"))
    );
    // +100 split into 50 locked + 50 available; the -50 claw-back nets the rest to zero.
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("0.00"), s("50.00"), s("0.00"), s("normal"))
    );
}

// RSL-02: 60 available, withdraw 25 and pay → 35 / 0 / 0 / normal; full withdrawal → zeros.
#[tokio::test]
async fn rsl02_paid_withdrawal_caches() {
    let shop = shop().await;
    shop.available_profit("160.00").await; // profit 60
    let w = shop.withdraw("25").await;
    let wid = data(&w)["id"].as_i64().unwrap();
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("35.00"), s("25.00"), s("0.00"), s("normal"))
    );
    let paid = shop
        .app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/withdraws/{wid}/pay"),
            None,
        )
        .await;
    assert_eq!(data(&paid)["status"], "paid");
    assert!(data(&paid)["processed_by"].is_number());
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("35.00"), s("0.00"), s("0.00"), s("normal"))
    );
    err(
        &shop
            .app
            .admin_call(
                "POST",
                &format!("/api/v1/admin/resellers/withdraws/{wid}/pay"),
                None,
            )
            .await,
        400,
        "请求参数错误",
    );

    let w = shop.withdraw("35").await;
    let wid = data(&w)["id"].as_i64().unwrap();
    data(
        &shop
            .app
            .admin_call(
                "POST",
                &format!("/api/v1/admin/resellers/withdraws/{wid}/pay"),
                None,
            )
            .await,
    );
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("0.00"), s("0.00"), s("0.00"), s("normal"))
    );
    err(
        &shop
            .app
            .admin_call("POST", "/api/v1/admin/resellers/withdraws/9999/pay", None)
            .await,
        404,
        "请求参数错误",
    );
}

// RSL-05: 30 + 50 available, withdraw 60 → 30 locked whole, 50 split 30 locked + 20 available;
// reject unlocks everything.
#[tokio::test]
async fn rsl05_withdraw_split_and_reject() {
    let shop = shop().await;
    let first = shop.available_profit("130.00").await; // 30
    let second = shop.available_profit("150.00").await; // 50
    let w = shop.withdraw("60").await;
    let wid = data(&w)["id"].as_i64().unwrap();
    let rows = ledger::Entity::find()
        .filter(ledger::Column::ResellerId.eq(shop.pid))
        .order_by_asc(ledger::Column::Id)
        .all(&shop.app.db)
        .await
        .unwrap();
    let view: Vec<(Option<i64>, String, String, Option<i64>)> = rows
        .iter()
        .map(|r| {
            (
                r.order_id,
                Amount::new(r.amount).to_string(),
                r.status.clone(),
                r.withdraw_request_id,
            )
        })
        .collect();
    assert_eq!(
        view,
        vec![
            (Some(first), s("30.00"), s("locked"), Some(wid)),
            (Some(second), s("30.00"), s("locked"), Some(wid)),
            (Some(second), s("20.00"), s("available"), None),
        ]
    );
    assert!(
        rows[2]
            .idempotency_key
            .starts_with(&format!("split:{}:", rows[1].id))
    );
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("20.00"), s("60.00"), s("0.00"), s("normal"))
    );

    let rejected = shop
        .app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/resellers/withdraws/{wid}/reject"),
            Some(json!({"reason": " no "})),
        )
        .await;
    assert_eq!(
        (
            data(&rejected)["status"].clone(),
            data(&rejected)["reject_reason"].clone()
        ),
        (json!("rejected"), json!("no"))
    );
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("80.00"), s("0.00"), s("0.00"), s("normal"))
    );
    let unlocked = ledger::Entity::find()
        .filter(ledger::Column::ResellerId.eq(shop.pid))
        .filter(ledger::Column::Status.eq("available"))
        .filter(ledger::Column::WithdrawRequestId.is_null())
        .all(&shop.app.db)
        .await
        .unwrap();
    assert_eq!(unlocked.len(), 3);

    // user lists and admin lists
    let list = shop
        .app
        .call("GET", "/api/v1/reseller/withdraws", None, Some(&shop.token))
        .await;
    assert_eq!(data(&list)[0]["reject_reason"], "no");
    let admin = shop
        .app
        .admin_call(
            "GET",
            "/api/v1/admin/resellers/withdraws?status=rejected&keyword=seller",
            None,
        )
        .await;
    assert_eq!(admin["pagination"]["total"], 1);
    assert_eq!(data(&admin)[0]["processor"]["username"], "admin");
    assert_eq!(
        data(&admin)[0]["profile"]["user"]["email"],
        "seller@example.test"
    );
    let admin_ledger = shop
        .app
        .admin_call(
            "GET",
            &format!(
                "/api/v1/admin/resellers/ledger-entries?reseller_id={}",
                shop.pid
            ),
            None,
        )
        .await;
    assert_eq!(admin_ledger["pagination"]["total"], 3);
    assert!(data(&admin_ledger)[0]["order"]["order_no"].is_string());
    let admin_bal = shop
        .app
        .admin_call("GET", "/api/v1/admin/resellers/balance-accounts", None)
        .await;
    assert_eq!(data(&admin_bal)[0]["available_amount_cache"], "80.00");
}

// RSL-05: two concurrent withdrawals of 60 with 80 available → exactly one succeeds.
#[tokio::test]
async fn rsl05_concurrent_withdrawals() {
    let shop = shop().await;
    shop.available_profit("130.00").await; // 30
    shop.available_profit("150.00").await; // 50
    let (a, b) = tokio::join!(shop.withdraw("60"), shop.withdraw("60"));
    let codes = [
        a["status_code"].as_i64().unwrap(),
        b["status_code"].as_i64().unwrap(),
    ];
    assert!(codes.contains(&0) && codes.contains(&400), "{a} {b}");
    let failed = if codes[0] == 400 { &a } else { &b };
    err(failed, 400, "可提现余额不足");
    assert_eq!(
        Shop::cache(&shop.balance().await),
        (s("20.00"), s("60.00"), s("0.00"), s("normal"))
    );
}

// RSL-02: dashboard flags and withdraw guards per profile / settlement status.
#[tokio::test]
async fn rsl02_dashboard_and_guards() {
    let shop = shop().await;
    // registered before the clock moves (tokens are checked against the wall clock)
    let (_, stranger) = shop.app.user("nobody@example.test").await;
    shop.available_profit("130.00").await;
    let dash = shop
        .app
        .call("GET", "/api/v1/reseller/dashboard", None, Some(&shop.token))
        .await;
    let d = data(&dash);
    assert_eq!(d["withdraw_enabled"], true);
    assert!(d.get("withdraw_disabled_reason").is_none());
    assert_eq!(d["balances"][0]["available_amount"], "30.00");
    assert_eq!(d["profile"]["status"], "active");

    err(&shop.withdraw("0").await, 400, "提现金额不合法");
    err(
        &shop
            .app
            .call(
                "POST",
                "/api/v1/reseller/withdraws",
                Some(json!({"amount": "1", "currency": "CNY", "channel": "", "account": "x"})),
                Some(&shop.token),
            )
            .await,
        400,
        "Channel: 不能为空",
    );

    let url = format!("/api/v1/admin/resellers/profiles/{}", shop.pid);
    data(
        &shop
            .app
            .admin_call("PUT", &url, Some(json!({"settlement_status": "frozen"})))
            .await,
    );
    let dash = shop
        .app
        .call("GET", "/api/v1/reseller/dashboard", None, Some(&shop.token))
        .await;
    assert_eq!(
        (
            data(&dash)["withdraw_enabled"].clone(),
            data(&dash)["withdraw_disabled_reason"].clone()
        ),
        (json!(false), json!("settlement_unavailable"))
    );
    err(&shop.withdraw("10").await, 400, "当前结算状态暂不可提现");

    data(
        &shop
            .app
            .admin_call("POST", &format!("{url}/disable"), Some(json!({})))
            .await,
    );
    let dash = shop
        .app
        .call("GET", "/api/v1/reseller/dashboard", None, Some(&shop.token))
        .await;
    assert_eq!(data(&dash)["withdraw_disabled_reason"], "profile_inactive");
    err(
        &shop.withdraw("10").await,
        400,
        "分销商资格未激活，暂时无法提现",
    );

    let dash = shop
        .app
        .call("GET", "/api/v1/reseller/dashboard", None, Some(&stranger))
        .await;
    assert_eq!(
        data(&dash),
        &json!({"opened": false, "withdraw_enabled": false})
    );
}

// RSL-02: a negative account is frozen for withdrawals even if a new profit arrives.
#[tokio::test]
async fn rsl02_negative_balance_blocks_withdraw() {
    let shop = shop().await;
    let id = shop.available_profit("200.00").await; // +100
    data(&shop.withdraw("100").await);
    assert!(shop.refund(id, 1, "200", "0").await); // -100 available → net -100
    assert_eq!(Shop::cache(&shop.balance().await).3, "negative_balance");
    shop.available_profit("130.00").await; // +30, still negative overall
    err(
        &shop.withdraw("10").await,
        400,
        "提现账户已被冻结，暂时无法提现",
    );
}
