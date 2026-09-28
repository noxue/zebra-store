//! Downstream callbacks (`downstream:callback`): signed delivery, retries, reset on
//! new events, parent resolution (UPS-12) and the SSRF guard (UPS-01).

#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

mod integration_common;

use chrono::Utc;
use integration_common::{IntApp, Supplier, jobs_of, seed_order_row, start_supplier};
use sea_orm::EntityTrait;
use serde_json::{Value, json};
use zs_domain::integration::downstream::NewOrderRef;
use zs_domain::queue::kinds;
use zs_infra::db::entity::{downstream_order_refs, fulfillments};
use zs_infra::db::repo::integration::downstream::insert_ref_in;
use zs_infra::integration::http::AddressPolicy;
use zs_shared::sign;

/// A buyer with an approved credential and a paid order carrying a reference to `url`.
async fn setup(app: &IntApp, url: &str) -> (i64, i64, String, String) {
    let (uid, _, cid, key, secret) = app.buyer("down@example.com").await;
    let order = seed_order_row(&app.db, "B-1", uid, "paid", "10.00", None).await;
    let r = insert_ref_in(
        &app.db,
        &NewOrderRef {
            order_id: order,
            api_credential_id: cid,
            downstream_order_no: "DOWN-1".into(),
            callback_url: url.into(),
            trace_id: "t".into(),
        },
        Utc::now(),
    )
    .await
    .unwrap();
    (order, r.id, key, secret)
}

async fn reference(app: &IntApp, id: i64) -> downstream_order_refs::Model {
    downstream_order_refs::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}

fn callback_hits(s: &Supplier) -> Vec<integration_common::Hit> {
    s.hits().into_iter().filter(|h| h.path == "/cb").collect()
}

#[tokio::test]
async fn signed_delivery_and_retries() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let (order, ref_id, key, secret) = setup(&app, &format!("{base}/cb")).await;
    let events = app.services.integration.order_events.clone();
    let downstream = app.services.integration.downstream.clone();

    events.order_status_changed(order).await.unwrap();
    assert_eq!(
        jobs_of(&app.db, kinds::DOWNSTREAM_CALLBACK).await,
        vec![json!({"ref_id": ref_id})]
    );

    // Delivered fulfillment is included (UPS-12).
    let now = Utc::now();
    use sea_orm::{ActiveModelTrait, Set};
    fulfillments::ActiveModel {
        order_id: Set(order),
        type_: Set("auto".into()),
        status: Set("delivered".into()),
        payload: Set("CARD".into()),
        logistics_json: Set(None),
        delivered_by: Set(None),
        delivered_at: Set(Some(now)),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    zs_infra::db::entity::orders::ActiveModel {
        id: Set(order),
        status: Set("delivered".into()),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();

    downstream.send(ref_id).await.unwrap();
    let hits = callback_hits(&supplier);
    assert_eq!(hits.len(), 1);
    let h = &hits[0];
    let ts: i64 = h.headers[sign::HEADER_TIMESTAMP]
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(h.headers[sign::HEADER_API_KEY], key.as_str());
    assert!(sign::verify(
        &secret,
        "POST",
        "/api/v1/upstream/callback",
        h.headers[sign::HEADER_SIGNATURE].to_str().unwrap(),
        ts,
        h.body.as_bytes()
    ));
    let body: Value = serde_json::from_str(&h.body).unwrap();
    assert_eq!(body["event"], "order.fulfilled");
    assert_eq!(body["downstream_order_no"], "DOWN-1");
    assert_eq!(body["status"], "delivered");
    assert_eq!(body["fulfillment"]["payload"], "CARD");
    let r = reference(&app, ref_id).await;
    assert_eq!(r.callback_status, "sent");
    assert!(r.last_callback_at.is_some());

    // Failures: retried with a delay, then marked failed after 5 attempts.
    *supplier.callback_reply.lock().unwrap() = Some((200, json!({"ok": false})));
    for attempt in 1..=5 {
        downstream.send(ref_id).await.unwrap();
        let r = reference(&app, ref_id).await;
        assert_eq!(r.callback_retry_count, attempt);
    }
    let r = reference(&app, ref_id).await;
    assert_eq!(r.callback_status, "failed");

    // UPS-12: a new state change resets the reference and delivers again.
    *supplier.callback_reply.lock().unwrap() = None;
    events.order_status_changed(order).await.unwrap();
    let r = reference(&app, ref_id).await;
    assert_eq!(
        (r.callback_status.as_str(), r.callback_retry_count),
        ("pending", 0)
    );
    downstream.send(ref_id).await.unwrap();
    assert_eq!(reference(&app, ref_id).await.callback_status, "sent");
}

// UPS-12: a child order resolves the parent's reference.
#[tokio::test]
async fn ups12_child_resolves_parent_reference() {
    let (base, _supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let (order, ref_id, _, _) = setup(&app, &format!("{base}/cb")).await;
    let child = seed_order_row(&app.db, "B-1-1", 1, "delivered", "10.00", Some(order)).await;
    app.services
        .integration
        .order_events
        .order_status_changed(child)
        .await
        .unwrap();
    assert_eq!(
        jobs_of(&app.db, kinds::DOWNSTREAM_CALLBACK).await,
        vec![json!({"ref_id": ref_id})]
    );
}

// UPS-01: redirects are not followed; loopback targets are refused in production.
#[tokio::test]
async fn ups01_redirects_and_internal_targets() {
    let (base, supplier) = start_supplier().await;
    let app = IntApp::new().await;
    let (_, ref_id, _, _) = setup(&app, &format!("{base}/redirect")).await;
    app.services
        .integration
        .downstream
        .send(ref_id)
        .await
        .unwrap();
    assert_eq!(supplier.paths(), vec!["/redirect"]);
    assert_eq!(reference(&app, ref_id).await.callback_retry_count, 1);

    let prod = IntApp::with_policy(AddressPolicy::PublicOnly).await;
    let (_, ref_id, _, _) = setup(&prod, &format!("{base}/cb")).await;
    prod.services
        .integration
        .downstream
        .send(ref_id)
        .await
        .unwrap();
    assert!(
        callback_hits(&supplier).is_empty(),
        "loopback must not be reached"
    );
    assert_eq!(reference(&prod, ref_id).await.callback_retry_count, 1);
    for url in [
        "http://10.0.0.1/x",
        "http://169.254.169.254/latest",
        "http://100.64.1.1/",
        "http://localhost/",
    ] {
        let err = zs_domain::integration::downstream::CallbackSender::send(
            &zs_infra::integration::HttpCallbackSender::new(AddressPolicy::PublicOnly),
            url,
            "k",
            "s",
            &zs_domain::integration::protocol::CallbackPayload::default(),
        )
        .await
        .unwrap_err();
        assert_eq!(err.key(), "forbidden address", "{url}");
    }
}

// UPS-10: the unique `(credential, downstream_order_no)` index turns a duplicate into a
// recognisable error; empty numbers never collide.
#[tokio::test]
async fn ups10_downstream_number_is_unique_per_credential() {
    let app = IntApp::new().await;
    let new = |order_id: i64, no: &str, cred: i64| NewOrderRef {
        order_id,
        api_credential_id: cred,
        downstream_order_no: no.into(),
        callback_url: String::new(),
        trace_id: String::new(),
    };
    insert_ref_in(&app.db, &new(1, "X", 7), Utc::now())
        .await
        .unwrap();
    let dup = insert_ref_in(&app.db, &new(2, "X", 7), Utc::now())
        .await
        .unwrap_err();
    assert_eq!(
        dup.key(),
        zs_infra::db::repo::integration::downstream::DUPLICATE_KEY
    );
    insert_ref_in(&app.db, &new(3, "X", 8), Utc::now())
        .await
        .unwrap();
    insert_ref_in(&app.db, &new(4, "", 7), Utc::now())
        .await
        .unwrap();
    insert_ref_in(&app.db, &new(5, "", 7), Utc::now())
        .await
        .unwrap();
}
