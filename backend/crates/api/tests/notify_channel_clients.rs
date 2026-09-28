//! `/admin/channel-clients` contract (original `channelclient` admin handler).

mod common;

use common::{TestApp, data};
use sea_orm::EntityTrait;
use serde_json::json;
use zs_infra::db::entity::channel_clients;

const BASE: &str = "/api/v1/admin/channel-clients";

#[tokio::test]
async fn channel_client_crud_contract() {
    let app = TestApp::new().await;

    let created = app
        .post(
            BASE,
            json!({"name": " Telegram Bot ", "channel_type": "telegram_bot", "description": "main",
                   "bot_token": "123456789:ABCDEFGHIJKLMNOP", "callback_url": "https://bot.example.com/cb"}),
        )
        .await;
    let d = data(&created).clone();
    let id = d["id"].as_i64().unwrap();
    assert_eq!(d["name"], "Telegram Bot");
    assert_eq!(d["channel_type"], "telegram_bot");
    assert_eq!(d["status"], 1);
    assert_eq!(d["bot_token_set"], true);
    assert_eq!(d["bot_token"], "1234******************MNOP");
    assert_eq!(d["callback_url"], "https://bot.example.com/cb");
    let key = d["channel_key"].as_str().unwrap().to_owned();
    let secret = d["channel_secret"].as_str().unwrap().to_owned();
    assert_eq!(key.len(), 64);
    assert_eq!(secret.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit()));

    // Secret and bot token are stored encrypted.
    let row = channel_clients::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(row.channel_secret, secret);
    assert!(!row.bot_token.contains("ABCDEFGHIJ"));

    // List and detail expose the decrypted secret to administrators.
    let list = app.get(BASE).await;
    assert_eq!(data(&list).as_array().unwrap().len(), 1);
    assert_eq!(data(&list)[0]["channel_secret"], secret.as_str());
    let got = app.get(&format!("{BASE}/{id}")).await;
    assert_eq!(data(&got)["channel_key"], key.as_str());

    // Update: omitted bot token is kept, "" clears it.
    let kept = app
        .put(
            &format!("{BASE}/{id}"),
            json!({"name": "", "description": "d2"}),
        )
        .await;
    assert_eq!(data(&kept)["name"], "Telegram Bot");
    assert_eq!(data(&kept)["description"], "d2");
    assert_eq!(data(&kept)["bot_token_set"], true);
    let cleared = app
        .put(
            &format!("{BASE}/{id}"),
            json!({"bot_token": "", "callback_url": ""}),
        )
        .await;
    assert_eq!(data(&cleared)["bot_token_set"], false);
    assert_eq!(data(&cleared)["bot_token"], "");
    assert_eq!(data(&cleared)["callback_url"], "");

    // Status toggle.
    let st = app
        .put(&format!("{BASE}/{id}/status"), json!({"status": 0}))
        .await;
    assert_eq!(st["status_code"], 0);
    assert!(st["data"].is_null());
    assert_eq!(data(&app.get(&format!("{BASE}/{id}")).await)["status"], 0);
    let bad = app
        .put(&format!("{BASE}/{id}/status"), json!({"status": 7}))
        .await;
    assert_eq!(bad["status_code"], 400);

    // Reset secret returns a new plaintext secret.
    let reset = app
        .post(&format!("{BASE}/{id}/reset-secret"), json!({}))
        .await;
    let new_secret = data(&reset)["channel_secret"].as_str().unwrap().to_owned();
    assert_ne!(new_secret, secret);
    assert_eq!(data(&reset)["channel_key"], key.as_str());

    // Delete (soft) then 404.
    let del = app.delete(&format!("{BASE}/{id}")).await;
    assert_eq!(del["status_code"], 0);
    let gone = app.get(&format!("{BASE}/{id}")).await;
    assert_eq!(gone["status_code"], 404);
    assert_eq!(data(&app.get(BASE).await).as_array().unwrap().len(), 0);
    let row = channel_clients::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    assert!(row.deleted_at.is_some());
}

#[tokio::test]
async fn channel_client_validation_and_auth() {
    let app = TestApp::new().await;
    let missing = app.post(BASE, json!({"name": "x"})).await;
    assert_eq!(missing["status_code"], 400);
    assert_eq!(app.get(&format!("{BASE}/999")).await["status_code"], 404);
    let anon = app.call("GET", BASE, None, None).await;
    assert_eq!(anon["status_code"], 401);
}
