//! Wallet account, transactions and admin adjustment endpoints.

mod wallet_common;

use serde_json::json;
use wallet_common::{App, data, err, keys};

const BAD: &str = "Invalid request parameters";

#[tokio::test]
async fn user_wallet_requires_login_and_starts_empty() {
    let app = App::new().await;
    let res = app.call("GET", "/api/v1/wallet", None, None).await;
    assert_eq!(res["status_code"], 401);
    let (_, token) = app.user("u1@example.com").await;
    let res = app.call("GET", "/api/v1/wallet", None, Some(&token)).await;
    assert_eq!(data(&res), &json!({"balance": "0.00"}));
    let res = app
        .call("GET", "/api/v1/wallet/transactions", None, Some(&token))
        .await;
    assert_eq!(data(&res), &json!([]));
    assert_eq!(res["pagination"]["total"], 0);
    assert_eq!(res["pagination"]["page_size"], 20);
}

/// WAL-02: explicit operation, mandatory remark, operator recorded on the transaction.
#[tokio::test]
async fn wal_02_admin_adjust_rules_and_audit() {
    let app = App::new().await;
    let (uid, token) = app.user("adj@example.com").await;
    let adjust = format!("/api/v1/admin/users/{uid}/wallet/adjust");

    // No operation → 400 (never defaults to "add").
    let res = app
        .admin_call(
            "POST",
            &adjust,
            Some(json!({"amount": "10.00", "remark": "x"})),
        )
        .await;
    err(&res, 400, BAD);
    let res = app
        .admin_call(
            "POST",
            &adjust,
            Some(json!({"amount": "10", "operation": "set", "remark": "x"})),
        )
        .await;
    err(&res, 400, BAD);
    // Remark trimmed empty → dedicated message.
    let res = app
        .admin_call(
            "POST",
            &adjust,
            Some(json!({"amount": "10.00", "operation": "add", "remark": "   "})),
        )
        .await;
    err(
        &res,
        400,
        "A remark is required for wallet balance adjustments",
    );
    let res = app
        .admin_call(
            "POST",
            &adjust,
            Some(json!({"amount": null, "operation": "add", "remark": "r"})),
        )
        .await;
    err(&res, 400, "Amount: is required");
    for bad in [json!("abc"), json!("0"), json!("-5")] {
        let res = app
            .admin_call(
                "POST",
                &adjust,
                Some(json!({"amount": bad, "operation": "add", "remark": "r"})),
            )
            .await;
        err(&res, 400, BAD);
    }
    assert_eq!(app.balance(uid).await, "0.00");

    let res = app
        .admin_call(
            "POST",
            &adjust,
            Some(json!({"amount": "100", "operation": "add", "remark": " bonus "})),
        )
        .await;
    let d = data(&res);
    assert_eq!(d["account"]["balance"], "100.00");
    assert_eq!(d["account"]["user_id"], uid);
    let txn = &d["transaction"];
    assert_eq!(txn["type"], "admin_adjust");
    assert_eq!(txn["direction"], "in");
    assert_eq!(txn["amount"], "100.00");
    assert_eq!(txn["balance_before"], "0.00");
    assert_eq!(txn["balance_after"], "100.00");
    assert_eq!(txn["remark"], "bonus");
    assert_eq!(txn["currency"], "CNY");
    assert_eq!(txn["operator_admin_id"], 1);
    assert!(txn.get("order_id").is_none());
    assert!(
        txn["reference"]
            .as_str()
            .unwrap()
            .starts_with(&format!("admin_adjust:{uid}:"))
    );

    // Balance never goes negative; QA-A12: the message says why (live QA I-12).
    let res = app
        .admin_call(
            "POST",
            &adjust,
            Some(json!({"amount": "150", "operation": "subtract", "remark": "r"})),
        )
        .await;
    err(&res, 400, "Insufficient wallet balance");
    let res = app
        .admin_call("POST", &adjust, Some(json!({"amount": 30.5, "operation": "SUBTRACT", "remark": "fix", "currency": "usd"})))
        .await;
    assert_eq!(data(&res)["account"]["balance"], "69.50");
    assert_eq!(data(&res)["transaction"]["direction"], "out");
    assert_eq!(data(&res)["transaction"]["currency"], "USD");

    // The user sees whitelisted transaction fields, newest first.
    let res = app
        .call("GET", "/api/v1/wallet/transactions", None, Some(&token))
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        keys(&rows[0]),
        [
            "amount",
            "balance_after",
            "created_at",
            "direction",
            "id",
            "remark",
            "type"
        ]
    );
    assert_eq!(rows[0]["amount"], "30.50");
    assert_eq!(rows[0]["balance_after"], "69.50");
    let res = app.call("GET", "/api/v1/wallet", None, Some(&token)).await;
    assert_eq!(data(&res)["balance"], "69.50");

    // Admin transaction filters.
    let res = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/users/{uid}/wallet/transactions?direction=out"),
            None,
        )
        .await;
    assert_eq!(data(&res).as_array().unwrap().len(), 1);
    assert_eq!(data(&res)[0]["operator_admin_id"], 1);
    let res = app
        .admin_call(
            "GET",
            &format!("/api/v1/admin/users/{uid}/wallet/transactions?type=recharge"),
            None,
        )
        .await;
    assert_eq!(res["pagination"]["total"], 0);
}

#[tokio::test]
async fn admin_user_wallet_view_and_errors() {
    let app = App::new().await;
    let (uid, _) = app.user("view@example.com").await;
    let res = app
        .admin_call("GET", &format!("/api/v1/admin/users/{uid}/wallet"), None)
        .await;
    let d = data(&res);
    assert_eq!(d["user"]["email"], "view@example.com");
    assert!(d["user"].get("password_hash").is_none());
    assert_eq!(d["account"]["balance"], "0.00");
    assert_eq!(d["account"]["user_id"], uid);

    let res = app
        .admin_call("GET", "/api/v1/admin/users/abc/wallet", None)
        .await;
    err(&res, 400, "Invalid user ID");
    let res = app
        .admin_call("GET", "/api/v1/admin/users/9999/wallet", None)
        .await;
    err(&res, 404, "User not found");
    let res = app
        .admin_call(
            "POST",
            "/api/v1/admin/users/0/wallet/adjust",
            Some(json!({"amount": "1", "operation": "add", "remark": "r"})),
        )
        .await;
    err(&res, 400, "Invalid user ID");
}

/// Wallet admin routes are compliance gated **[C]**.
#[tokio::test]
async fn admin_wallet_routes_are_compliance_gated() {
    let app = App::without_ack().await;
    let (uid, _) = app.user("gate@example.com").await;
    for (method, uri, body) in [
        ("GET", format!("/api/v1/admin/users/{uid}/wallet"), None),
        (
            "GET",
            format!("/api/v1/admin/users/{uid}/wallet/transactions"),
            None,
        ),
        (
            "POST",
            format!("/api/v1/admin/users/{uid}/wallet/adjust"),
            Some(json!({"amount": "1", "operation": "add", "remark": "r"})),
        ),
        ("GET", "/api/v1/admin/wallet/recharges".to_owned(), None),
    ] {
        let res = app.admin_call(method, &uri, body).await;
        assert_eq!(res["status_code"], 403, "{uri}: {res}");
    }
    assert_eq!(app.balance(uid).await, "0.00");
    // Without an admin token the routes are 401.
    let res = app
        .call("GET", "/api/v1/admin/wallet/recharges", None, None)
        .await;
    assert_eq!(res["status_code"], 401);
}
