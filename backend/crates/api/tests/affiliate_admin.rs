//! Admin affiliate pages: users (status, batch), commissions and withdrawal review
//! (compliance gated).

#![expect(clippy::unwrap_used, reason = "tests")]

mod wallet_common;

use sea_orm::EntityTrait;
use serde_json::{Value, json};
use wallet_common::{App, data, err};
use zs_infra::db::entity::affiliate_commissions as commissions;

const BAD: &str = "Invalid request parameters";

async fn setup(app: &App) -> (i64, String, i64) {
    app.set_setting(
        "affiliate_config",
        json!({"enabled": true, "commission_rate": 10, "confirm_days": 0, "min_withdraw_amount": 0, "withdraw_channels": []}),
    )
    .await;
    let (uid, token) = app.user("Promoter@Example.com").await;
    let res = app
        .call("POST", "/api/v1/affiliate/open", None, Some(&token))
        .await;
    let pid = data(&res)["id"].as_i64().unwrap();
    // One referred paid order with a 20.00 affiliate base → 2.00 commission.
    let (buyer, _) = app.user("buyer@example.com").await;
    let product = app.product("p1", true).await;
    let order = app.order("DJADMIN1", buyer, "20", None, Some(pid)).await;
    app.order_item(order, product, "20", "0").await;
    app.services
        .affiliate
        .service
        .handle_order_paid(order)
        .await
        .unwrap();
    let _ = uid;
    (pid, token, order)
}

async fn withdraw(app: &App, token: &str, amount: &str) -> i64 {
    let res = app
        .call(
            "POST",
            "/api/v1/affiliate/withdraws",
            Some(json!({"amount": amount, "channel": "alipay", "account": "acc-1"})),
            Some(token),
        )
        .await;
    data(&res)["id"].as_i64().unwrap()
}

#[tokio::test]
async fn users_list_with_stats_and_status_changes() {
    let app = App::new().await;
    let (pid, _, _) = setup(&app).await;
    let res = app
        .admin_call(
            "GET",
            "/api/v1/admin/affiliates/users?keyword=promoter",
            None,
        )
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    assert_eq!(row["profile"]["id"], pid);
    assert_eq!(row["profile"]["status"], "active");
    assert_eq!(row["profile"]["user"]["email"], "Promoter@Example.com");
    assert!(row["profile"]["code"].is_string());
    // The original stats struct has no JSON tags: PascalCase keys.
    assert_eq!(row["stats"]["ClickCount"], 0);
    assert_eq!(row["stats"]["ValidOrderCount"], 1);
    assert_eq!(row["stats"]["AvailableCommission"], "2.00");
    assert_eq!(row["stats"]["PendingCommission"], "0.00");
    let code = row["profile"]["code"].as_str().unwrap().to_owned();
    let res = app
        .admin_call(
            "GET",
            &format!(
                "/api/v1/admin/affiliates/users?code={}",
                code.to_lowercase()
            ),
            None,
        )
        .await;
    assert_eq!(res["pagination"]["total"], 1);
    let res = app
        .admin_call(
            "GET",
            "/api/v1/admin/affiliates/users?status=disabled",
            None,
        )
        .await;
    assert_eq!(res["pagination"]["total"], 0);

    let status = format!("/api/v1/admin/affiliates/users/{pid}/status");
    err(
        &app.admin_call("PATCH", &status, Some(json!({"status": "banned"})))
            .await,
        400,
        BAD,
    );
    err(
        &app.admin_call("PATCH", &status, Some(json!({}))).await,
        400,
        "Status: is required",
    );
    err(
        &app.admin_call(
            "PATCH",
            "/api/v1/admin/affiliates/users/999/status",
            Some(json!({"status": "active"})),
        )
        .await,
        404,
        BAD,
    );
    let res = app
        .admin_call("PATCH", &status, Some(json!({"status": "disabled"})))
        .await;
    assert_eq!(data(&res)["status"], "disabled");
    // Disabled affiliates are not attributed.
    assert_eq!(
        app.services
            .affiliate
            .service
            .resolve_order_snapshot(0, &code, "")
            .await
            .unwrap(),
        None
    );

    let batch = "/api/v1/admin/affiliates/users/batch-status";
    err(
        &app.admin_call(
            "PATCH",
            batch,
            Some(json!({"profile_ids": [], "status": "active"})),
        )
        .await,
        400,
        BAD,
    );
    let res = app
        .admin_call(
            "PATCH",
            batch,
            Some(json!({"profile_ids": [pid, pid, 0, 12345], "status": "active"})),
        )
        .await;
    assert_eq!(data(&res), &json!({"updated": 1}));
}

#[tokio::test]
async fn commission_and_withdraw_review() {
    let app = App::new().await;
    let (pid, token, _) = setup(&app).await;
    let res = app
        .admin_call(
            "GET",
            "/api/v1/admin/affiliates/commissions?order_no=ADMIN1&keyword=promoter",
            None,
        )
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["order"]["order_no"], "DJADMIN1");
    assert_eq!(
        rows[0]["affiliate_profile"]["user"]["email"],
        "Promoter@Example.com"
    );
    assert_eq!(rows[0]["base_amount"], "20.00");
    assert_eq!(rows[0]["rate_percent"], "10.00");
    assert_eq!(rows[0]["commission_amount"], "2.00");
    let res = app
        .admin_call(
            "GET",
            &format!(
                "/api/v1/admin/affiliates/commissions?affiliate_profile_id={pid}&status=withdrawn"
            ),
            None,
        )
        .await;
    assert_eq!(res["pagination"]["total"], 0);

    // Reject releases the frozen commissions.
    let w1 = withdraw(&app, &token, "2").await;
    let res = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/affiliates/withdraws/{w1}/reject"),
            Some(json!({"reason": "bad account"})),
        )
        .await;
    let d = data(&res);
    assert_eq!(d["status"], "rejected");
    assert_eq!(d["reject_reason"], "bad account");
    assert_eq!(d["processor"]["username"], "admin");
    assert!(d["processed_at"].is_string());
    let row = commissions::Entity::find()
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.withdraw_request_id, None);
    assert_eq!(row.status, "available");
    // Reviewing twice is refused.
    err(
        &app.admin_call(
            "POST",
            &format!("/api/v1/admin/affiliates/withdraws/{w1}/pay"),
            None,
        )
        .await,
        400,
        BAD,
    );

    // Pay marks commissions withdrawn.
    let w2 = withdraw(&app, &token, "2").await;
    let res = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/affiliates/withdraws/{w2}/pay"),
            None,
        )
        .await;
    assert_eq!(data(&res)["status"], "paid");
    let row = commissions::Entity::find()
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "withdrawn");
    let res = app
        .call("GET", "/api/v1/affiliate/dashboard", None, Some(&token))
        .await;
    assert_eq!(data(&res)["withdrawn_commission"], "2.00");
    assert_eq!(data(&res)["available_commission"], "0.00");

    let res = app
        .admin_call(
            "GET",
            "/api/v1/admin/affiliates/withdraws?keyword=acc-1&status=paid",
            None,
        )
        .await;
    let rows = data(&res).as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["affiliate_profile"]["id"], pid);
    err(
        &app.admin_call("POST", "/api/v1/admin/affiliates/withdraws/999/pay", None)
            .await,
        404,
        BAD,
    );
    let res = app
        .call("GET", "/api/v1/affiliate/withdraws", None, Some(&token))
        .await;
    let mine: Vec<Value> = data(&res).as_array().unwrap().clone();
    assert_eq!(mine.len(), 2);
    assert_eq!(mine[1]["reject_reason"], "bad account");
    assert!(mine[0].get("reject_reason").is_none());
}

/// Finance pages are compliance gated; the user list is not.
#[tokio::test]
async fn finance_routes_are_compliance_gated() {
    let app = App::without_ack().await;
    for (method, uri) in [
        ("GET", "/api/v1/admin/affiliates/commissions"),
        ("GET", "/api/v1/admin/affiliates/withdraws"),
        ("POST", "/api/v1/admin/affiliates/withdraws/1/pay"),
    ] {
        let res = app.admin_call(method, uri, None).await;
        assert_eq!(res["status_code"], 403, "{uri}: {res}");
    }
    let res = app
        .admin_call("GET", "/api/v1/admin/affiliates/users", None)
        .await;
    assert_eq!(data(&res), &json!([]));
}
