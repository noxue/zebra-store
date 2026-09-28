//! Regression tests for admin checklist items of `bugfix-lessons.md` §21 that had only
//! partial coverage: ADM-01.

#![expect(
    clippy::unwrap_used,
    reason = "integration tests: failures should abort the test"
)]

mod payment_common;

use payment_common::PayApp;
use sea_orm::EntityTrait;
use serde_json::{Value, json};
use zs_infra::db::entity::payment_channels;

fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    &v["data"]
}

async fn stored_config(app: &PayApp, id: i64) -> Value {
    payment_channels::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .config_json
        .unwrap_or(Value::Null)
}

/// ADM-01: channel secrets are masked in every admin response; updating with the mask
/// keeps the stored secret, `null` clears it and a new value replaces it.
#[tokio::test]
async fn adm_01_channel_secrets_masked_and_merged() {
    let app = PayApp::offline().await;
    // `private_key` is a secret too (only used by epay v2), so it can be cleared on a v1 channel
    let config = |key: Value| {
        json!({"gateway_url": "https://pay.example.com", "merchant_id": "1001", "merchant_key": key,
            "private_key": "pk-1",
            "notify_url": "https://shop/api/v1/payments/callback", "return_url": "https://shop/pay"})
    };
    let created = app
        .call(
            "POST",
            "/api/v1/admin/payment-channels",
            Some(
                json!({"name": "epay", "provider_type": "epay", "channel_type": "alipay",
                "interaction_mode": "redirect", "config_json": config(json!("secret-key-1"))}),
            ),
        )
        .await;
    let id = data(&created)["id"].as_i64().unwrap();
    assert_eq!(data(&created)["config_json"]["merchant_key"], "••••••••");
    assert_eq!(
        stored_config(&app, id).await["merchant_key"],
        "secret-key-1"
    );

    let got = app
        .call("GET", &format!("/api/v1/admin/payment-channels/{id}"), None)
        .await;
    assert_eq!(data(&got)["config_json"]["merchant_key"], "••••••••");
    let list = app
        .call("GET", "/api/v1/admin/payment-channels", None)
        .await;
    assert!(!list.to_string().contains("secret-key-1"), "{list}");

    // the mask sent back keeps the stored secret
    let uri = format!("/api/v1/admin/payment-channels/{id}");
    let res = app
        .call(
            "PUT",
            &uri,
            Some(json!({"config_json": config(json!("••••••••"))})),
        )
        .await;
    assert_eq!(data(&res)["config_json"]["merchant_key"], "••••••••");
    assert_eq!(
        stored_config(&app, id).await["merchant_key"],
        "secret-key-1"
    );

    // a new value replaces it
    let res = app
        .call(
            "PUT",
            &uri,
            Some(json!({"config_json": config(json!("secret-key-2"))})),
        )
        .await;
    data(&res);
    assert_eq!(
        stored_config(&app, id).await["merchant_key"],
        "secret-key-2"
    );

    // an explicit null clears it; an omitted secret is kept
    let mut cleared = config(json!("••••••••"));
    cleared["private_key"] = Value::Null;
    data(
        &app.call("PUT", &uri, Some(json!({"config_json": cleared})))
            .await,
    );
    let stored = stored_config(&app, id).await;
    assert!(stored.get("private_key").is_none(), "{stored}");
    assert_eq!(stored["merchant_key"], "secret-key-2");
    let mut omitted = config(json!("••••••••"));
    omitted.as_object_mut().unwrap().remove("merchant_key");
    omitted.as_object_mut().unwrap().remove("private_key");
    data(
        &app.call("PUT", &uri, Some(json!({"config_json": omitted})))
            .await,
    );
    assert_eq!(
        stored_config(&app, id).await["merchant_key"],
        "secret-key-2"
    );
}
