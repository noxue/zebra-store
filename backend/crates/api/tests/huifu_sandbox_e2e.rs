//! Opt-in Zebra Store end-to-end checkout against the separately downloaded Huifu local
//! sandbox. The test uses the real router, Huifu gateway adapter, SQLite transaction layer,
//! sandbox RSA signing, asynchronous Notify delivery and automatic card fulfilment.

#![expect(clippy::unwrap_used, reason = "sandbox integration test")]

mod order_common;

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use chrono::Utc;
use http::{Request, header};
use order_common::{Auth, OrderApp};
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, Set};
use serde_json::{Value, json};
use zs_domain::queue::kinds;
use zs_infra::db::entity::payment_channels;
use zs_infra::payment::http::ReqwestTransport;

const EMAIL: &str = "huifu-sandbox@uuid.com";
const PASSWORD: &str = "01999999-7777-4777-8777-019999999999";

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set"))
}

async fn seed_huifu_channel(
    app: &OrderApp,
    channel_type: &str,
    notify_url: &str,
    return_url: &str,
) -> i64 {
    let now = Utc::now();
    payment_channels::ActiveModel {
        name: Set(format!("Huifu sandbox {channel_type}")),
        icon: Set(String::new()),
        provider_type: Set("huifu".into()),
        channel_type: Set(channel_type.into()),
        interaction_mode: Set("qr".into()),
        fee_rate: Set(Decimal::ZERO),
        fixed_fee: Set(Decimal::ZERO),
        min_amount: Set(Decimal::ZERO),
        max_amount: Set(Decimal::ZERO),
        hide_amount_out_range: Set(false),
        payment_roles: Set(Some(json!(["guest"]))),
        member_levels: Set(Some(json!([]))),
        payment_types: Set(Some(json!(["order"]))),
        config_json: Set(Some(json!({
            "api_base_url": env("HUIFU_SANDBOX_URL"),
            "sys_id": env("HUIFU_SANDBOX_SYS_ID"),
            "product_id": env("HUIFU_SANDBOX_PRODUCT_ID"),
            "huifu_id": env("HUIFU_SANDBOX_HUIFU_ID"),
            "merchant_private_key": fs::read_to_string(env("HUIFU_SANDBOX_PRIVATE_KEY_FILE")).unwrap(),
            "huifu_public_key": fs::read_to_string(env("HUIFU_SANDBOX_PUBLIC_KEY_FILE")).unwrap(),
            "skill_source": "hfps/1.3.1;sandbox/1.0.0",
            "project_id": "ZEBRA-SANDBOX",
            "project_title": "Zebra Store",
            "notify_url": notify_url,
            "return_url": return_url
        }))),
        is_active: Set(true),
        sort_order: Set(0),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id
}

async fn sandbox_post(path: &str, body: Value) -> Value {
    let base = env("HUIFU_SANDBOX_CONTROL_URL");
    let token = env("HUIFU_SANDBOX_ADMIN_TOKEN");
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let session: Value = client
        .get(format!("{}/__admin/session", base.trim_end_matches('/')))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    client
        .post(format!("{}{}", base.trim_end_matches('/'), path))
        .bearer_auth(token)
        .header(
            "X-Huifu-Sandbox-CSRF",
            session["csrf_token"].as_str().unwrap(),
        )
        .json(&body)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires the separately downloaded Huifu local sandbox"]
async fn guest_checkout_huifu_notify_and_card_delivery() {
    let app = Arc::new(OrderApp::with_transport(Arc::new(ReqwestTransport::new())).await);
    let callback_port = std::env::var("HUIFU_SANDBOX_CALLBACK_PORT")
        .unwrap_or_else(|_| "18767".to_owned())
        .parse::<u16>()
        .unwrap();
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", callback_port))
        .await
        .unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = app.router.clone();
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

    let (product, sku) = app
        .product("huifu-sandbox-card", json!({"price_amount": "0.01"}))
        .await;
    app.secrets(product, sku, 1).await;
    let channel = seed_huifu_channel(
        &app,
        "alipay",
        &format!("{base}/api/v1/payments/callback"),
        &format!("{base}/pay"),
    )
    .await;

    let created = app
        .call(
            "POST",
            "/api/v1/guest/orders/create-and-pay",
            Some(json!({
                "email": EMAIL,
                "order_password": PASSWORD,
                "items": [{"product_id": product, "sku_id": sku, "quantity": 1}],
                "channel_id": channel
            })),
            &Auth::None,
        )
        .await;
    assert_eq!(created["status_code"], 0, "checkout failed: {created}");
    assert_eq!(created["data"]["provider_type"], "huifu");
    assert_eq!(created["data"]["interaction_mode"], "qr");
    assert!(
        created["data"]["pay_url"]
            .as_str()
            .is_some_and(|url| url.starts_with(&env("HUIFU_SANDBOX_CONTROL_URL")))
    );
    assert_eq!(
        created["data"]["qr_code"], created["data"]["pay_url"],
        "direct-mode desktop QR content must use the signed Huifu hosted URL"
    );
    let order_no = created["data"]["order_no"].as_str().unwrap().to_owned();
    let payment_id = created["data"]["payment_id"].as_i64().unwrap();
    let gateway_order_no = app.payment(payment_id).await.gateway_order_no;
    assert_eq!(app.secrets_with(product, "reserved").await, 1);

    let simulated = sandbox_post(
        "/__admin/hosting/success",
        json!({"req_seq_id": gateway_order_no}),
    )
    .await;
    assert_eq!(
        simulated["ok"], true,
        "sandbox payment simulation failed: {simulated}"
    );

    let delivered = tokio::time::timeout(
        Duration::from_secs(10),
        sandbox_post(
            "/__admin/deliver",
            json!({
                "channel": "notify",
                "entity_type": "payment",
                "req_seq_id": gateway_order_no,
                "outcome": "success"
            }),
        ),
    )
    .await
    .expect("Huifu Notify must not hang");
    assert_eq!(
        delivered["ok"], true,
        "sandbox delivery failed: {delivered}"
    );
    assert_eq!(
        delivered["notification"]["ack_body"],
        format!("RECV_ORD_ID_{gateway_order_no}")
    );

    let parent = app.order_by_no(&order_no).await;
    assert_eq!(parent.status, "paid");
    assert_eq!(app.payment(payment_id).await.status, "success");
    let children = app.children(parent.id).await;
    assert_eq!(children.len(), 1);
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 1);

    // The sandbox sends the same signed success again. It must acknowledge without a second
    // settlement or fulfilment job.
    let replay = sandbox_post(
        "/__admin/deliver",
        json!({
            "channel": "notify",
            "entity_type": "payment",
            "req_seq_id": gateway_order_no,
            "outcome": "success"
        }),
    )
    .await;
    assert_eq!(replay["ok"], true, "sandbox replay failed: {replay}");
    assert_eq!(app.jobs(kinds::ORDER_AUTO_FULFILL).await.len(), 1);

    app.services
        .order
        .service
        .auto_fulfill(children[0].id)
        .await
        .unwrap();
    assert_eq!(app.secrets_with(product, "used").await, 1);
    assert_eq!(app.order(children[0].id).await.status, "completed");
    assert_eq!(app.order_by_no(&order_no).await.status, "completed");

    let detail = app
        .call(
            "GET",
            &format!("/api/v1/guest/orders/{order_no}"),
            None,
            &Auth::Guest(EMAIL.into(), PASSWORD.into()),
        )
        .await;
    assert_eq!(detail["status_code"], 0, "guest lookup failed: {detail}");
    assert_eq!(detail["data"]["status"], "completed");
    assert!(
        detail["data"]["children"][0]["fulfillment"]["payload"]
            .as_str()
            .is_some_and(|payload| payload.contains("CARD-"))
    );

    let refund_body = json!({"amount": "0.01", "remark": "local sandbox refund"});
    let mut refund = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{}/original-refund", parent.id),
            Some(refund_body.clone()),
        )
        .await;
    assert_eq!(
        refund["status_code"], 0,
        "refund submission failed: {refund}"
    );
    for _ in 0..3 {
        if refund["data"]["attempt"]["status"] == "succeeded" {
            break;
        }
        refund = app
            .admin_call(
                "POST",
                &format!("/api/v1/admin/orders/{}/original-refund", parent.id),
                Some(refund_body.clone()),
            )
            .await;
        assert_eq!(
            refund["status_code"], 0,
            "refund reconciliation failed: {refund}"
        );
    }
    assert_eq!(refund["data"]["attempt"]["status"], "succeeded", "{refund}");
    assert_eq!(refund["data"]["order"]["refunded_amount"], "0.01");
    let refund_replay = app
        .admin_call(
            "POST",
            &format!("/api/v1/admin/orders/{}/original-refund", parent.id),
            Some(refund_body),
        )
        .await;
    assert_eq!(
        refund_replay["status_code"], 0,
        "refund replay: {refund_replay}"
    );
    assert_eq!(refund_replay["data"]["attempt"]["status"], "succeeded");
    assert_eq!(
        refund_replay["data"]["refund_record"]["id"], refund["data"]["refund_record"]["id"],
        "replayed refund must reference the original local refund record"
    );

    // Also exercise a mobile WeChat H5 request. The browser User-Agent determines the
    // hosted checkout request type, while the selected payment method determines T_JSAPI.
    let (wechat_product, wechat_sku) = app
        .product("huifu-sandbox-wechat", json!({"price_amount": "0.01"}))
        .await;
    app.secrets(wechat_product, wechat_sku, 1).await;
    let wechat_channel = seed_huifu_channel(
        &app,
        "wechat",
        &format!("{base}/api/v1/payments/callback"),
        &format!("{base}/pay"),
    )
    .await;
    let request = Request::builder()
        .method("POST")
        .uri("/api/v1/guest/orders/create-and-pay")
        .header(header::ACCEPT_LANGUAGE, "en-US")
        .header(header::CONTENT_TYPE, "application/json")
        .header(
            header::USER_AGENT,
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) MicroMessenger/8.0",
        )
        .header("x-forwarded-for", "203.0.113.7")
        .body(Body::from(
            json!({
                "email": "huifu-wechat-sandbox@uuid.com",
                "order_password": "01999999-7777-4777-8777-019999999998",
                "items": [{"product_id": wechat_product, "sku_id": wechat_sku, "quantity": 1}],
                "channel_id": wechat_channel
            })
            .to_string(),
        ))
        .unwrap();
    let (status, _, body) = app.send(request).await;
    assert!(
        status.is_success(),
        "WeChat H5 checkout HTTP status: {status}"
    );
    let wechat_created: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        wechat_created["status_code"], 0,
        "WeChat H5 checkout failed: {wechat_created}"
    );
    assert_eq!(wechat_created["data"]["provider_type"], "huifu");
    assert!(
        wechat_created["data"]["pay_url"]
            .as_str()
            .is_some_and(|url| url.starts_with(&env("HUIFU_SANDBOX_CONTROL_URL"))),
        "WeChat H5 must return a sandbox hosted pay URL: {wechat_created}"
    );

    server.abort();
}
