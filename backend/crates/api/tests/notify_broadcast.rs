//! Telegram broadcasts (`/admin/telegram-bot/*`, job `telegram:broadcast`)
//! and bot callbacks (job `bot:notify`).

mod common;
mod notify_common;

use common::data;
use notify_common::{Mock, app_with, channel, channel_client, ok_data, start_mock};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
use serde_json::json;
use zs_domain::notify::channel::{BotNotifyPayload, bot_events};
use zs_infra::db::entity::jobs;
use zs_infra::notify::safe_http::AddressPolicy;

const BROADCASTS: &str = "/api/v1/admin/telegram-bot/broadcasts";

#[tokio::test]
async fn broadcast_lifecycle() {
    let mock = Mock::default();
    let base = start_mock(&mock).await;
    let app = app_with(&base, AddressPolicy::AllowPrivate).await;

    let body = json!({"title": "Sale", "recipient_type": "all", "message_html": "<b>50% off</b>"});
    // No active bot client with a token yet.
    assert_eq!(app.post(BROADCASTS, body.clone()).await["status_code"], 400);

    let creds = channel_client(&app, "99:BOT", "").await;
    // No Telegram users bound yet.
    assert_eq!(app.post(BROADCASTS, body.clone()).await["status_code"], 400);

    for id in ["5001", "5002", "666"] {
        let r = channel(
            &app,
            &creds,
            "POST",
            "/identities/telegram/provision",
            Some(json!({"channel_user_id": id, "username": format!("u{id}")})),
        )
        .await;
        ok_data(&r);
    }

    let users = app
        .get("/api/v1/admin/telegram-bot/users?keyword=u500&page=1&page_size=10")
        .await;
    assert_eq!(users["pagination"]["total"], 2);
    let u = &data(&users)[0];
    assert!(u["user_id"].as_i64().unwrap() > 0);
    assert!(u["user_email"].as_str().unwrap().ends_with("@login.local"));
    assert!(u["telegram_user_id"].is_string());
    assert!(u["bound_at"].is_string());
    assert!(u["user_created_at"].is_string());
    let by_tg = app
        .get("/api/v1/admin/telegram-bot/users?telegram_user_id=666")
        .await;
    assert_eq!(by_tg["pagination"]["total"], 1);

    // Validation.
    assert_eq!(
        app.post(
            BROADCASTS,
            json!({"title": "x", "recipient_type": "group", "message_html": "m"})
        )
        .await["status_code"],
        400
    );
    assert_eq!(
        app.post(
            BROADCASTS,
            json!({"title": "x", "recipient_type": "specific", "message_html": "m"})
        )
        .await["status_code"],
        400
    );

    let created = app.post(BROADCASTS, body).await;
    let b = data(&created).clone();
    let id = b["id"].as_i64().unwrap();
    assert_eq!(b["title"], "Sale");
    assert_eq!(b["recipient_type"], "all");
    assert_eq!(b["status"], "pending");
    assert_eq!(b["recipient_count"], 3);
    assert_eq!(b["success_count"], 0);
    assert_eq!(b["filters"], json!({}));
    assert!(b["started_at"].is_null());
    assert!(
        b.get("recipient_chat_ids").is_none(),
        "chat ids are never exposed"
    );
    let queued = jobs::Entity::find()
        .filter(jobs::Column::Kind.eq("telegram:broadcast"))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(queued.len(), 1);

    // The job delivers the broadcast.
    app.services.notify.broadcasts.process(id).await.unwrap();
    let sends: Vec<_> = mock
        .hits()
        .into_iter()
        .filter(|h| h.path == "/bot99:BOT/sendMessage")
        .collect();
    assert_eq!(sends.len(), 3);
    assert_eq!(sends[0].json()["parse_mode"], "HTML");
    assert_eq!(sends[0].json()["text"], "<b>50% off</b>");
    let done = data(&app.get(&format!("{BROADCASTS}/{id}")).await).clone();
    assert_eq!(done["status"], "completed");
    assert_eq!(done["success_count"], 2);
    assert_eq!(done["failed_count"], 1);
    assert!(done["last_error"].as_str().unwrap().contains("blocked"));
    assert!(done["completed_at"].is_string());

    // Specific recipients with a photo attachment use sendPhoto (NTF-06).
    let user_id = data(&by_tg)[0]["user_id"].as_i64().unwrap();
    let photo = app
        .post(
            BROADCASTS,
            json!({"title": "Pic", "recipient_type": "specific", "user_ids": [user_id, user_id], "message_html": "hi",
                   "attachment_url": "https://cdn.example.com/a.png", "attachment_name": "a.png"}),
        )
        .await;
    let pid = data(&photo)["id"].as_i64().unwrap();
    assert_eq!(data(&photo)["recipient_count"], 1);
    assert_eq!(
        data(&photo)["filters"]["selected_user_ids"],
        json!([user_id])
    );
    app.services.notify.broadcasts.process(pid).await.unwrap();
    let photo_hit = mock
        .hits()
        .into_iter()
        .find(|h| h.path == "/bot99:BOT/sendPhoto")
        .unwrap();
    assert_eq!(photo_hit.json()["photo"], "https://cdn.example.com/a.png");
    assert_eq!(photo_hit.json()["caption"], "hi");
    assert_eq!(
        data(&app.get(&format!("{BROADCASTS}/{pid}")).await)["status"],
        "failed",
        "only recipient blocked the bot"
    );

    let list = app.get(&format!("{BROADCASTS}?status=completed")).await;
    assert_eq!(list["pagination"]["total"], 1);
    assert_eq!(data(&list)[0]["id"], id);
    let all = app.get(BROADCASTS).await;
    assert_eq!(all["pagination"]["total"], 2);
    assert_eq!(data(&all)[0]["id"], pid, "newest first");

    // The original registers no DELETE route for broadcasts.
    assert_eq!(
        app.get(&format!("{BROADCASTS}/999999")).await["status_code"],
        404
    );
}

// NTF-06 (2) + UPS-01: bot callbacks are signed, path-rebuilt and SSRF-guarded.
#[tokio::test]
async fn ntf06_bot_notify_callbacks() {
    let mock = Mock::default();
    let base = start_mock(&mock).await;
    let app = app_with(&base, AddressPolicy::AllowPrivate).await;
    let creds = channel_client(&app, "", &format!("{base}/hook?token=x")).await;

    let payload = BotNotifyPayload {
        event_type: bot_events::ORDER_PAID.into(),
        order_id: 12,
        telegram_user_id: "5001".into(),
        ..BotNotifyPayload::default()
    };
    app.services.notify.bot.enqueue(&payload).await.unwrap();
    let queued = jobs::Entity::find()
        .filter(jobs::Column::Kind.eq("bot:notify"))
        .all(&app.db)
        .await
        .unwrap();
    assert_eq!(queued[0].max_attempts, 3);

    app.services.notify.bot.handle(&payload).await.unwrap();
    let hit = mock.hits().pop().unwrap();
    assert_eq!(hit.path, "/internal/order-paid");
    assert_eq!(hit.query, "");
    assert_eq!(
        hit.json(),
        json!({"order_id": 12, "telegram_user_id": "5001"})
    );
    assert_eq!(hit.headers["dujiao-next-channel-key"], creds.1.as_str());
    let ts: i64 = hit.headers["dujiao-next-channel-timestamp"]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    let sig = hit.headers["dujiao-next-channel-signature"]
        .to_str()
        .unwrap();
    assert!(zs_shared::sign::verify(
        &creds.2,
        "POST",
        "/internal/order-paid",
        sig,
        ts,
        hit.body.as_bytes()
    ));

    // UPS-01: with the production policy a loopback callback is refused (and not retried).
    let guarded = app_with(&base, AddressPolicy::PublicOnly).await;
    channel_client(&guarded, "", &format!("{base}/hook")).await;
    let before = mock.hits().len();
    guarded.services.notify.bot.handle(&payload).await.unwrap();
    assert_eq!(
        mock.hits().len(),
        before,
        "no request reached the loopback server"
    );
}
