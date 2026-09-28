//! Reconciliation (compliance-gated): run, execute, counts and resolve (UPS-21).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

mod integration_common;

use chrono::Utc;
use integration_common::{IntApp, data, err, jobs_of, start_supplier};
use sea_orm::{ActiveModelTrait, Set};
use serde_json::json;
use zs_domain::queue::kinds;
use zs_infra::db::entity::procurement_orders;

async fn seed_procurement(
    app: &IntApp,
    conn: i64,
    upstream_id: i64,
    status: &str,
    amount: &str,
    no: &str,
) {
    let now = Utc::now();
    procurement_orders::ActiveModel {
        connection_id: Set(conn),
        local_order_id: Set(upstream_id + 1000),
        local_order_no: Set(no.into()),
        upstream_order_id: Set(upstream_id),
        upstream_order_no: Set(format!("UP{upstream_id}")),
        status: Set(status.into()),
        upstream_amount: Set(amount.parse().unwrap()),
        upstream_currency: Set("CNY".into()),
        local_sell_amount: Set("12".parse().unwrap()),
        currency: Set("CNY".into()),
        error_message: Set(String::new()),
        retry_count: Set(0),
        next_retry_at: Set(None),
        upstream_payload: Set(String::new()),
        trace_id: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
}

#[tokio::test]
async fn compliance_gate() {
    let app = IntApp::new().await;
    let res = app
        .admin("GET", "/api/v1/admin/reconciliation/jobs", None)
        .await;
    assert_eq!(res["status_code"], 403, "{res}");
}

// UPS-21: cost vs supplier amount, skipped / errored rows excluded, no re-entry.
#[tokio::test]
async fn ups21_run_execute_resolve() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;
    let conn = app.connection(&base, json!({})).await;
    // Selling price 12 vs purchase 10: never compared.
    seed_procurement(&app, conn, 88, "fulfilled", "10", "L-88").await;
    seed_procurement(&app, conn, 89, "fulfilled", "10", "L-89").await;
    seed_procurement(&app, conn, 0, "pending", "0", "L-0").await;
    seed_procurement(&app, conn, 90, "accepted", "10", "L-90").await;
    {
        let mut orders = supplier.orders.lock().unwrap();
        orders.insert(
            88,
            json!({"order_id": 88, "status": "delivered", "amount": "10.00"}),
        );
        orders.insert(
            89,
            json!({"order_id": 89, "status": "completed", "amount": "11"}),
        );
    }

    let start = (Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
    let end = (Utc::now() + chrono::Duration::hours(1)).to_rfc3339();
    let bad = app
        .admin("POST", "/api/v1/admin/reconciliation/run", Some(json!({"connection_id": conn, "type": "weird", "time_range_start": start, "time_range_end": end})))
        .await;
    err(&bad, 400, "请求参数错误");
    let res = app
        .admin("POST", "/api/v1/admin/reconciliation/run", Some(json!({"connection_id": conn, "type": "full", "time_range_start": start, "time_range_end": end})))
        .await;
    let job = data(&res);
    assert_eq!(job["status"], "pending");
    assert_eq!(job["type"], "full");
    let job_id = job["id"].as_i64().unwrap();
    assert_eq!(
        jobs_of(&app.db, kinds::RECONCILIATION_RUN).await,
        vec![json!({"job_id": job_id})]
    );

    let svc = app.services.integration.reconciliation.clone();
    svc.execute(job_id).await.unwrap();
    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/reconciliation/jobs/{job_id}"),
            None,
        )
        .await;
    let d = data(&res);
    assert_eq!(d["job"]["status"], "completed");
    assert_eq!(d["job"]["total_count"], 2);
    assert_eq!(d["job"]["matched_count"], 1);
    assert_eq!(d["job"]["mismatched_count"], 1);
    let result: serde_json::Value =
        serde_json::from_str(d["job"]["result_json"].as_str().unwrap()).unwrap();
    assert_eq!(result["skipped"], 1);
    assert_eq!(result["errors"], 1);
    assert_eq!(d["items_total"], 1);
    let item = &d["items"][0];
    assert_eq!(item["mismatch_type"], "amount");
    assert_eq!(item["local_amount"], "10.00");
    assert_eq!(item["upstream_amount"], "11.00");
    assert_eq!(item["local_order_no"], "L-89");
    assert_eq!(item["resolved"], false);
    // Mismatches alert the admins.
    let alerts = jobs_of(&app.db, kinds::NOTIFICATION_DISPATCH).await;
    assert_eq!(alerts[0]["biz_type"], "reconciliation");

    // Completed jobs are not re-entered.
    svc.execute(job_id).await.unwrap();
    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/reconciliation/jobs/{job_id}"),
            None,
        )
        .await;
    assert_eq!(data(&res)["items_total"], 1);

    let item_id = item["id"].as_i64().unwrap();
    let res = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/reconciliation/items/{item_id}/resolve"),
            Some(json!({"remark": "ok"})),
        )
        .await;
    assert_eq!(data(&res)["ok"], true);
    let res = app
        .admin(
            "GET",
            &format!("/api/v1/admin/reconciliation/jobs/{job_id}"),
            None,
        )
        .await;
    let item = &data(&res)["items"][0];
    assert_eq!(item["resolved"], true);
    assert_eq!(item["remark"], "ok");
    assert!(item["resolved_by"].as_i64().unwrap() > 0);
    let res = app
        .admin(
            "PUT",
            "/api/v1/admin/reconciliation/items/9999/resolve",
            Some(json!({})),
        )
        .await;
    err(&res, 404, "对账明细不存在");

    let list = app
        .admin(
            "GET",
            &format!(
                "/api/v1/admin/reconciliation/jobs?connection_id={conn}&status=completed&type=full"
            ),
            None,
        )
        .await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
    assert_eq!(data(&list)[0]["connection"]["id"], conn);
    let missing = app
        .admin("GET", "/api/v1/admin/reconciliation/jobs/777", None)
        .await;
    err(&missing, 404, "对账任务不存在");
}
