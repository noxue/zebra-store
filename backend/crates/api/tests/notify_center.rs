//! Notification center: test send, delivery logs, `notification:dispatch`
//! and the periodic alert check, with Telegram / Feishu mocked.

mod common;
mod notify_common;

use common::data;
use notify_common::{Mock, app_with, set_setting, start_mock};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use zs_domain::notify::center::{DispatchPayload, NotifyEvent};
use zs_infra::db::entity::jobs;
use zs_infra::notify::safe_http::AddressPolicy;

const LOGS: &str = "/api/v1/admin/settings/notification-center/logs";
const TEST: &str = "/api/v1/admin/settings/notification-center/test";

#[tokio::test]
async fn test_send_and_logs_contract() {
    let mock = Mock::default();
    let base = start_mock(&mock).await;
    let app = app_with(&base, AddressPolicy::AllowPrivate).await;
    set_setting(
        &app,
        "telegram_auth_config",
        json!({"bot_token": "42:TEST"}),
    )
    .await;

    let empty = app.get(LOGS).await;
    assert_eq!(data(&empty), &json!([]));
    assert_eq!(empty["pagination"]["total"], 0);

    let sent = app
        .post(TEST, json!({"channel": " TELEGRAM ", "target": " -1001234567 ", "scene": "order_paid_success", "locale": "en-US"}))
        .await;
    assert_eq!(data(&sent), &json!({"sent": true}));
    let hits = mock.hits();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "/bot42:TEST/sendMessage");
    let body = hits[0].json();
    assert_eq!(body["chat_id"], "-1001234567");
    assert_eq!(body["disable_web_page_preview"], true);
    let text = body["text"].as_str().unwrap();
    assert!(text.starts_with("Order Payment Succeeded\n\nCustomer: Alex Zhang"));
    assert!(text.contains("Order No: DJ202603230001"));

    // Feishu: tenant token then the IM message.
    set_setting(
        &app,
        "notification_center_config",
        json!({"channels": {"feishu": {"enabled": false, "app_id": "cli_x", "app_secret": "sec", "receive_id_type": "open_id"}}}),
    )
    .await;
    let feishu = app
        .post(
            "/api/v1/admin/settings/notifications/test",
            json!({"channel": "feishu", "target": "ou_1"}),
        )
        .await;
    assert_eq!(data(&feishu)["sent"], true);
    let hits = mock.hits();
    assert_eq!(
        hits[1].path,
        "/open-apis/auth/v3/tenant_access_token/internal"
    );
    assert_eq!(
        hits[1].json(),
        json!({"app_id": "cli_x", "app_secret": "sec"})
    );
    assert_eq!(hits[2].path, "/open-apis/im/v1/messages");
    assert_eq!(hits[2].query, "receive_id_type=open_id");
    assert_eq!(hits[2].headers["authorization"], "Bearer t-mock");
    let msg = hits[2].json();
    assert_eq!(msg["receive_id"], "ou_1");
    assert_eq!(msg["msg_type"], "text");
    let content: serde_json::Value =
        serde_json::from_str(msg["content"].as_str().unwrap()).unwrap();
    assert!(
        content["text"]
            .as_str()
            .unwrap()
            .starts_with("系统异常告警\n\n告警类型：低库存商品")
    );

    // Email with SMTP disabled fails and is logged as failed.
    let email = app
        .post(
            TEST,
            json!({"channel": "email", "target": "ops@example.com"}),
        )
        .await;
    assert_eq!(email["status_code"], 500);
    assert_eq!(email["msg"], "通知发送失败");

    // Validation.
    assert_eq!(
        app.post(TEST, json!({"channel": "sms", "target": "x"}))
            .await["status_code"],
        400
    );
    assert_eq!(
        app.post(TEST, json!({"channel": "email"})).await["status_code"],
        400
    );

    let logs = app.get(LOGS).await;
    assert_eq!(logs["pagination"]["total"], 3);
    let rows = data(&logs).as_array().unwrap();
    assert_eq!(rows[0]["channel"], "email", "newest first");
    assert_eq!(rows[0]["status"], "failed");
    assert!(!rows[0]["error_message"].as_str().unwrap().is_empty());
    let tg = &rows[2];
    assert_eq!(tg["channel"], "telegram");
    assert_eq!(tg["recipient"], "-1001234567");
    assert_eq!(tg["event_type"], "order_paid_success");
    assert_eq!(tg["status"], "success");
    assert_eq!(tg["is_test"], true);
    assert_eq!(tg["locale"], "en-US");
    assert_eq!(tg["title"], "Order Payment Succeeded");
    assert_eq!(tg["variables"]["order_no"], "DJ202603230001");
    assert!(tg["created_at"].as_str().unwrap().ends_with('Z'));

    let failed = app
        .get(&format!("{LOGS}?status=failed&channel=email&is_test=true"))
        .await;
    assert_eq!(failed["pagination"]["total"], 1);
    let none = app.get(&format!("{LOGS}?is_test=false")).await;
    assert_eq!(none["pagination"]["total"], 0);
    assert_eq!(
        app.get(&format!("{LOGS}?is_test=maybe")).await["status_code"],
        400
    );
    assert_eq!(
        app.get(&format!("{LOGS}?created_from=yesterday")).await["status_code"],
        400
    );
}

#[tokio::test]
async fn dispatch_job_delivers_and_dedupes() {
    let mock = Mock::default();
    let base = start_mock(&mock).await;
    let app = app_with(&base, AddressPolicy::AllowPrivate).await;
    set_setting(
        &app,
        "telegram_auth_config",
        json!({"bot_token": "42:TEST"}),
    )
    .await;
    set_setting(
        &app,
        "notification_center_config",
        json!({"channels": {"telegram": {"enabled": true, "recipients": ["-1001234567", "666666"]}}}),
    )
    .await;

    // Producers enqueue through the Notifier port.
    let event = NotifyEvent {
        event_type: "wallet_recharge_success".into(),
        biz_type: "wallet_recharge".into(),
        biz_id: 5,
        data: json!({"recharge_no": "RC5", "amount": "9.90", "currency": "CNY"})
            .as_object()
            .cloned()
            .unwrap(),
        ..NotifyEvent::default()
    };
    app.services
        .notify
        .notifier
        .notify(event.clone())
        .await
        .unwrap();
    let payload = event.into_payload().unwrap();
    let queued = jobs::Entity::find()
        .filter(jobs::Column::Kind.eq("notification:dispatch"))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].max_attempts, 5);

    app.services.notify.center.dispatch(&payload).await.unwrap();
    app.services.notify.center.dispatch(&payload).await.unwrap();
    let paths = mock.paths();
    assert_eq!(paths.len(), 2, "two recipients, second dispatch deduped");
    let text = mock.hits()[0].json()["text"].as_str().unwrap().to_owned();
    assert!(text.contains("充值单号：RC5"));
    assert!(text.contains("充值金额：9.90 CNY"));

    let unknown = DispatchPayload {
        event_type: "nope".into(),
        ..DispatchPayload::default()
    };
    app.services.notify.center.dispatch(&unknown).await.unwrap();
    assert_eq!(mock.paths().len(), 2);
}

#[tokio::test]
async fn alert_check_reports_low_stock_once_per_interval() {
    let mock = Mock::default();
    let base = start_mock(&mock).await;
    let app = app_with(&base, AddressPolicy::AllowPrivate).await;
    set_setting(
        &app,
        "telegram_auth_config",
        json!({"bot_token": "42:TEST"}),
    )
    .await;
    set_setting(
        &app,
        "notification_center_config",
        json!({"channels": {"telegram": {"enabled": true, "recipients": ["-1001234567"]}}, "inventory_alert_interval_seconds": 3600}),
    )
    .await;
    let cat = data(
        &app.post(
            "/api/v1/admin/categories",
            json!({"name": {"zh-CN": "c"}, "slug": "c"}),
        )
        .await,
    )["id"]
        .as_i64()
        .unwrap();
    app.post(
        "/api/v1/admin/products",
        json!({"category_id": cat, "slug": "low", "title": {"zh-CN": "低库存商品A"}, "price_amount": 1,
               "fulfillment_type": "manual", "manual_stock_total": 2, "is_active": true}),
    )
    .await;

    app.services.notify.center.run_alert_check().await.unwrap();
    let hits = mock.hits();
    assert_eq!(hits.len(), 1);
    let text = hits[0].json()["text"].as_str().unwrap().to_owned();
    assert!(text.starts_with("系统异常告警"));
    assert!(text.contains("告警类型：低库存商品"));
    assert!(text.contains("1. 低库存商品A [人工交付] 剩余 2（低库存）"));
    // NTF-04 ①: inside the interval nothing new is sent.
    app.services.notify.center.run_alert_check().await.unwrap();
    assert_eq!(mock.hits().len(), 1);
}
