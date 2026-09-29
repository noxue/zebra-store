//! zebra-store protocol v1 end to end: two Zebra Store instances in one process, one
//! supplier (serving `/api/v1/zs/*`) and one buyer (connecting with the
//! `zebra-store` adapter), both on local listeners with the test-only
//! `AddressPolicy::AllowPrivate`. All compatibility integration rules apply:
//! SSRF refusal (UPS-01), state machine + ownership of events (UPS-02/03), idempotent
//! orders (UPS-10), change-driven sync with deletion marks (UPS-14/17).
//!
//! Set `ZS_WRITE_EXAMPLES=1` to regenerate `docs/protocol/zebra-store-v1-examples.md`
//! from the captured requests / responses.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, header};
use chrono::Utc;
use integration_common::{IntApp, data, seed_product};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{
    api_credential_rotations, fulfillments, integration_connection_states, orders,
    procurement_orders, product_mappings, product_skus, products, sku_mappings, wallet_accounts,
    zs_change_log, zs_quotes, zs_webhook_events, zs_webhooks,
};
use zs_infra::integration::http::AddressPolicy;
use zs_infra::queue::Worker;
use zs_infra::wire::WireCtx;
use zs_infra::wire::integration::Adapters;
use zs_shared::zs as proto;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// One Zebra Store instance served on a local listener.
struct Site {
    app: IntApp,
    ctx: WireCtx,
    base: String,
}

async fn site(policy: AddressPolicy) -> Site {
    let cfg = integration_common::config();
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = WireCtx::new(&db, &cfg);
    let mut services = zs_infra::wire::services(&ctx);
    services.integration = zs_infra::wire::integration::build_with(
        &ctx,
        &Adapters {
            address_policy: policy,
            ..Adapters::default()
        },
    );
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let router = zs_api::build(zs_api::AppState::new(services.clone(), cfg)).router;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let served = router.clone();
    tokio::spawn(async move { axum::serve(listener, served).await });
    let mut app = IntApp {
        router,
        db: db.clone(),
        services,
        admin_token: String::new(),
        lifecycle: Arc::default(),
        ordering: Arc::new(integration_common::FakeOrdering {
            db,
            fail_payment: Mutex::new(false),
            placed: Mutex::new(0),
        }),
    };
    let login = app
        .call(
            "POST",
            "/api/v1/admin/login",
            Some(json!({"username": "admin", "password": "Admin12345"})),
            None,
        )
        .await;
    app.admin_token = login["data"]["token"].as_str().unwrap().to_owned();
    app.acknowledge_compliance().await;
    Site { app, ctx, base }
}

impl Site {
    /// Runs every due job with the real handlers until none is left.
    async fn drain(&self) {
        let registry = zs_infra::wire::jobs(&self.ctx, &self.app.services);
        let worker = Worker::new(self.app.db.clone(), registry, 1, Duration::from_millis(5));
        let permits = Arc::new(tokio::sync::Semaphore::new(1));
        for _ in 0..40 {
            let started = worker.poll_once(&permits).await.unwrap();
            while permits.available_permits() < 1 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            if started == 0 {
                break;
            }
        }
    }

    async fn fund(&self, user_id: i64, amount: &str) {
        let now = Utc::now();
        wallet_accounts::ActiveModel {
            user_id: Set(user_id),
            balance: Set(amount.parse().unwrap()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.app.db)
        .await
        .unwrap();
    }

    async fn balance(&self, user_id: i64) -> String {
        let row = wallet_accounts::Entity::find()
            .filter(wallet_accounts::Column::UserId.eq(user_id))
            .one(&self.app.db)
            .await
            .unwrap();
        format!("{:.2}", row.map(|r| r.balance).unwrap_or_default())
    }

    /// Request with explicit headers (JWT + extra).
    async fn call_with(
        &self,
        method: &str,
        uri: &str,
        token: &str,
        headers: &[(&str, &str)],
    ) -> Value {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let (_, _, bytes) = self.app.send(req.body(Body::empty()).unwrap()).await;
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }
}

/// A signed zebra-store call; `nonce` fixed for replay tests.
#[derive(Debug, Clone, Default)]
struct ZsCall {
    method: String,
    path_and_query: String,
    body: Option<Value>,
    key: String,
    secret: String,
    nonce: Option<String>,
    idempotency_key: Option<String>,
}

async fn zs(app: &IntApp, c: ZsCall) -> (u16, Value) {
    let bytes = c.body.as_ref().map(Value::to_string).unwrap_or_default();
    let ts = Utc::now().timestamp();
    let nonce = c
        .nonce
        .clone()
        .unwrap_or_else(|| format!("n{}", uuid_like()));
    let (path, query) = c
        .path_and_query
        .split_once('?')
        .unwrap_or((c.path_and_query.as_str(), ""));
    let sig = proto::Signed {
        method: &c.method,
        path,
        query,
        timestamp: ts,
        nonce: &nonce,
        body: bytes.as_bytes(),
    }
    .sign(&c.secret);
    let mut req = Request::builder()
        .method(c.method.as_str())
        .uri(c.path_and_query.as_str())
        .header(proto::HEADER_KEY, c.key.as_str())
        .header(proto::HEADER_TIMESTAMP, ts.to_string())
        .header(proto::HEADER_NONCE, nonce.as_str())
        .header(proto::HEADER_SIGNATURE, sig)
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(k) = &c.idempotency_key {
        req = req.header(proto::HEADER_IDEMPOTENCY_KEY, k);
    }
    let (status, _, out) = app.send(req.body(Body::from(bytes)).unwrap()).await;
    (
        status.as_u16(),
        serde_json::from_slice(&out).unwrap_or(Value::Null),
    )
}

fn uuid_like() -> String {
    format!(
        "{:x}{:x}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default(),
        std::process::id()
    )
}

/// Captured request / response pairs for the examples document.
#[derive(Default)]
struct Examples(Vec<(String, String, Value)>);

impl Examples {
    fn add(&mut self, title: &str, request: &str, response: &Value) {
        self.0
            .push((title.to_owned(), request.to_owned(), response.clone()));
    }

    fn write(&self) {
        if std::env::var("ZS_WRITE_EXAMPLES").is_err() {
            return;
        }
        let mut out = String::from(
            "# Zebra Store 对接协议 v1 — 请求/响应示例\n\n\
             > 由 `backend/crates/api/tests/integration_zebra_store.rs` 端到端测试捕获\n\
             > （`ZS_WRITE_EXAMPLES=1 cargo test -p zs-api --test integration_zebra_store`）。\n\
             > 两个 Zebra Store 实例在同一测试进程中运行：一个为供货方，一个为采购方。\n\
             > 签名头（`ZS-Key` / `ZS-Timestamp` / `ZS-Nonce` / `ZS-Signature`）每次请求都会携带，示例中省略；密钥与卡密已替换为占位符。\n",
        );
        for (title, request, response) in &self.0 {
            let pretty = serde_json::to_string_pretty(response).unwrap();
            out.push_str(&format!(
                "\n## {title}\n\n```http\n{request}\n```\n\n```json\n{pretty}\n```\n"
            ));
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../docs/protocol/zebra-store-v1-examples.md");
        std::fs::write(path, out).unwrap();
    }
}

/// Masks secrets / card codes / volatile values in captured JSON.
fn scrub(v: &Value) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .map(|(k, v)| {
                    let v = match k.as_str() {
                        "nonce" | "ciphertext" => json!("<base64>"),
                        "request_id" => json!("<request-id>"),
                        "api_secret" | "secret" => json!("<secret>"),
                        _ => scrub(v),
                    };
                    (k.clone(), v)
                })
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(scrub).collect()),
        other => other.clone(),
    }
}

async fn add_cards(app: &IntApp, product: i64, sku: i64, cards: &[&str]) {
    let res = app
        .admin(
            "POST",
            "/api/v1/admin/card-secrets/batch",
            Some(json!({"product_id": product, "sku_id": sku, "secrets": [cards.join("\n")]})),
        )
        .await;
    data(&res);
}

async fn state_of(app: &IntApp, conn: i64) -> integration_connection_states::Model {
    integration_connection_states::Entity::find()
        .filter(integration_connection_states::Column::ConnectionId.eq(conn))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}

async fn activate(app: &IntApp, product_id: i64) {
    let cat = app
        .admin(
            "POST",
            "/api/v1/admin/categories",
            Some(json!({"name": {"zh-CN": format!("c{product_id}")}, "slug": format!("c{product_id}"), "parent_id": 0})),
        )
        .await;
    let mut row: products::ActiveModel = products::Entity::find_by_id(product_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    row.is_active = Set(true);
    row.category_id = Set(data(&cat)["id"].as_i64().unwrap());
    row.update(&app.db).await.unwrap();
}

async fn local_sku(app: &IntApp, product_id: i64) -> product_skus::Model {
    product_skus::Entity::find()
        .filter(product_skus::Column::ProductId.eq(product_id))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}

// ---------------------------------------------------------------------------
// End to end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn zebra_store_end_to_end() {
    let mut ex = Examples::default();
    let supplier = site(AddressPolicy::AllowPrivate).await;
    let buyer = site(AddressPolicy::AllowPrivate).await;
    let s = &supplier.app;
    let b = &buyer.app;

    // ---- supplier catalog + the buyer's account there --------------------------------
    let (p1, p1_skus) = seed_product(&s.db, "p1", "auto", &[("A", "5.00", true)]).await;
    add_cards(s, p1, p1_skus[0], &["CARD-1", "CARD-2", "CARD-3"]).await;
    let (p2, _) = seed_product(&s.db, "p2", "manual", &[("M", "8.00", true)]).await;
    let (_p3, p3_skus) = seed_product(&s.db, "p3", "manual", &[("X", "500.00", true)]).await;
    let (sup_uid, sup_token, _cid, key, old_secret) = s.buyer("partner@buyer.example").await;
    supplier.fund(sup_uid, "100.00").await;

    // ---- connection code (spec §3): new secret as pending rotation --------------------
    let res = supplier
        .call_with(
            "POST",
            "/api/v1/api-credential/connection-code",
            &sup_token,
            &[("origin", &supplier.base)],
        )
        .await;
    let code = data(&res)["code"].as_str().unwrap().to_owned();
    assert!(code.starts_with("zsc1_"));
    assert!(data(&res)["rotation_expires_at"].is_string());
    let mine = s
        .call("GET", "/api/v1/api-credential", None, Some(&sup_token))
        .await;
    assert_eq!(data(&mine)["rotation_pending"], true);
    assert_eq!(
        data(&mine)["protocols"],
        json!(["dujiao-next", "zebra-store"])
    );

    // ---- buyer: parse code → handshake → create --------------------------------------
    let parsed = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/parse-code",
            Some(json!({"code": code})),
        )
        .await;
    let p = data(&parsed).clone();
    assert_eq!(p["protocol"], "zebra-store");
    assert_eq!(p["base_url"], supplier.base);
    assert_eq!(p["api_key"], key);
    let new_secret = p["api_secret"].as_str().unwrap().to_owned();
    assert_ne!(new_secret, old_secret);
    ex.add(
        "解析连接码（采购方管理端）",
        "POST /api/v1/admin/site-connections/parse-code\n\n{\"code\": \"zsc1_…\"}",
        &scrub(&parsed),
    );
    let bad = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/parse-code",
            Some(json!({"code": "zsc1_garbage"})),
        )
        .await;
    assert_eq!(bad["status_code"], 400, "{bad}");
    assert!(bad["msg"].as_str().unwrap().contains("zsc1_"), "{bad}");

    let hs = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(json!({"base_url": p["base_url"], "api_key": key, "api_secret": new_secret, "protocol": "zebra-store"})),
        )
        .await;
    let h = data(&hs).clone();
    assert_eq!(h["ok"], true, "{hs}");
    assert_eq!(h["protocol"], "zebra-store");
    assert_eq!(h["version"], "1.0");
    assert_eq!(h["site"]["currency"], "CNY");
    assert_eq!(h["account"]["balance"], "100.00");
    assert_eq!(h["suggested_exchange_rate"], "1");
    assert!(
        h["suggested_callback_url"]
            .as_str()
            .unwrap()
            .ends_with("/api/v1/zs/events")
    );
    // one vocabulary: capability ids everywhere
    for cap in [
        "categories",
        "incremental_changes",
        "push_events",
        "quote",
        "multi_item",
        "idempotency",
        "encrypted_delivery",
    ] {
        assert!(
            h["features"].as_array().unwrap().contains(&json!(cap)),
            "{h}"
        );
    }
    ex.add(
        "握手预检（采购方管理端）",
        "POST /api/v1/admin/site-connections/handshake\n\n{\"base_url\": \"http://127.0.0.1:…\", \"api_key\": \"…\", \"api_secret\": \"…\", \"protocol\": \"zebra-store\"}",
        &scrub(&hs),
    );
    // the first signed request with the new secret promoted the rotation
    let mine = s
        .call("GET", "/api/v1/api-credential", None, Some(&sup_token))
        .await;
    assert_eq!(data(&mine)["rotation_pending"], false);

    let callback = format!("{}/api/v1/zs/events", buyer.base);
    let created = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({
                "name": "Supplier S", "base_url": p["base_url"], "api_key": key,
                "api_secret": new_secret, "protocol": "zebra-store",
                "callback_url": callback, "auto_sync_price": true,
            })),
        )
        .await;
    let conn = data(&created)["id"].as_i64().unwrap();
    let got = b
        .admin(
            "GET",
            &format!("/api/v1/admin/site-connections/{conn}"),
            None,
        )
        .await;
    let c = data(&got).clone();
    assert_eq!(c["status"], "active", "{got}");
    assert_eq!(c["protocol"], "zebra-store");
    assert_eq!(c["supplier_currency"], "CNY");
    assert_eq!(c["sync_mode"], "incremental");
    assert_eq!(c["webhook_status"], "registered");
    assert!(
        c["features"]
            .as_array()
            .unwrap()
            .contains(&json!("push_events"))
    );
    let list = b.admin("GET", "/api/v1/admin/site-connections", None).await;
    assert_eq!(data(&list)[0]["webhook_status"], "registered");
    // the supplier stored our event endpoint
    let hook = zs_webhooks::Entity::find()
        .one(&s.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(hook.url, callback);

    // direct protocol calls with the (now current) secret
    let call = |method: &str, pq: String, body: Option<Value>| ZsCall {
        method: method.to_owned(),
        path_and_query: pq,
        body,
        key: key.clone(),
        secret: new_secret.clone(),
        ..ZsCall::default()
    };
    let (st, hsd) = zs(s, call("GET", "/api/v1/zs/handshake".into(), None)).await;
    assert_eq!(st, 200, "{hsd}");
    assert_eq!(hsd["ok"], true);
    assert_eq!(
        hsd["data"]["features"],
        json!([
            "changes",
            "webhooks",
            "quote",
            "multi_item",
            "idempotency",
            "encrypted_delivery"
        ])
    );
    ex.add("握手 §4", "GET /api/v1/zs/handshake", &scrub(&hsd));

    // ---- change feed + webhook → buyer ----------------------------------------------
    let seq = s.services.integration.zs.snapshot_tick().await.unwrap();
    assert!(
        seq.is_some(),
        "initial snapshot produces product.upserted changes"
    );
    let (st, feed) = zs(
        s,
        call("GET", "/api/v1/zs/catalog/changes?limit=2".into(), None),
    )
    .await;
    assert_eq!(st, 200, "{feed}");
    assert_eq!(feed["data"]["changes"][0]["type"], "product.upserted");
    assert_eq!(feed["data"]["has_more"], true);
    ex.add(
        "变更流 §5（首次快照）",
        "GET /api/v1/zs/catalog/changes?limit=2",
        &scrub(&feed),
    );
    let (st, page) = zs(
        s,
        call("GET", "/api/v1/zs/catalog/products?limit=1".into(), None),
    )
    .await;
    assert_eq!(st, 200, "{page}");
    assert_eq!(page["data"]["items"][0]["id"], p1);
    assert_eq!(page["data"]["items"][0]["skus"][0]["stock_quantity"], 3);
    assert!(page["data"]["items"][0]["version"].is_string());
    assert_eq!(page["data"]["next_cursor"], p1.to_string());
    assert_eq!(page["data"]["has_more"], true);
    ex.add(
        "商品游标列表 §5",
        "GET /api/v1/zs/catalog/products?limit=1",
        &scrub(&page),
    );

    supplier.drain().await;
    let events = zs_webhook_events::Entity::find().all(&s.db).await.unwrap();
    assert!(!events.is_empty());
    assert!(events.iter().all(|e| e.status == "sent"), "{events:?}");
    let first: Value = serde_json::from_str(&events[0].body).unwrap();
    assert_eq!(first["type"], "catalog.changed");
    assert!(first["id"].as_str().unwrap().starts_with("evt_"));
    ex.add(
        "事件推送 §6：catalog.changed（供货方 → 采购方 POST /api/v1/zs/events，带 ZS-Event-Id）",
        "POST {buyer}/api/v1/zs/events",
        &first,
    );
    buyer.drain().await;

    // ---- import both products ---------------------------------------------------------
    let mut locals = Vec::new();
    for upstream in [p1, p2] {
        let res = b
            .admin(
                "POST",
                "/api/v1/admin/product-mappings/import",
                Some(json!({"connection_id": conn, "upstream_product_id": upstream})),
            )
            .await;
        let local = data(&res)["local_product_id"].as_i64().unwrap();
        activate(b, local).await;
        locals.push(local);
    }
    assert_eq!(
        format!("{:.2}", local_sku(b, locals[0]).await.price_amount),
        "5.00"
    );
    assert!(
        state_of(b, conn)
            .await
            .change_cursor
            .parse::<i64>()
            .unwrap()
            > 0,
        "cursor set by the first full sync"
    );

    // ---- supplier changes price / stock / deletes → feed + webhook → buyer ------------
    let mut sku: product_skus::ActiveModel = product_skus::Entity::find_by_id(p1_skus[0])
        .one(&s.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    sku.price_amount = Set("6.00".parse().unwrap());
    sku.update(&s.db).await.unwrap();
    add_cards(s, p1, p1_skus[0], &["CARD-4", "CARD-5"]).await;
    let mut gone: products::ActiveModel = products::Entity::find_by_id(p2)
        .one(&s.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    gone.deleted_at = Set(Some(Utc::now()));
    gone.update(&s.db).await.unwrap();
    let before = state_of(b, conn).await.change_cursor;
    assert!(
        s.services
            .integration
            .zs
            .snapshot_tick()
            .await
            .unwrap()
            .is_some()
    );
    let (_, feed) = zs(
        s,
        call(
            "GET",
            format!("/api/v1/zs/catalog/changes?since={before}"),
            None,
        ),
    )
    .await;
    let kinds: Vec<&str> = feed["data"]["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        vec!["sku.price", "sku.stock", "product.deleted"],
        "{feed}"
    );
    ex.add(
        "变更流 §5（改价 / 补货 / 删除）",
        "GET /api/v1/zs/catalog/changes?since=…",
        &scrub(&feed),
    );
    supplier.drain().await;
    buyer.drain().await;
    let m1 = sku_mappings::Entity::find()
        .filter(sku_mappings::Column::LocalSkuId.eq(local_sku(b, locals[0]).await.id))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(format!("{:.2}", m1.upstream_price), "6.00");
    assert_eq!(m1.upstream_stock, 5);
    assert_eq!(
        format!("{:.2}", local_sku(b, locals[0]).await.price_amount),
        "6.00"
    );
    let m2 = product_mappings::Entity::find()
        .filter(product_mappings::Column::LocalProductId.eq(locals[1]))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m2.upstream_status, "deleted");
    assert!(
        !products::Entity::find_by_id(locals[1])
            .one(&b.db)
            .await
            .unwrap()
            .unwrap()
            .is_active
    );
    assert!(
        state_of(b, conn)
            .await
            .change_cursor
            .parse::<i64>()
            .unwrap()
            > before.parse::<i64>().unwrap()
    );

    // ---- a paid buyer order is procured, delivered encrypted, fulfilled locally -------
    let (shopper, token) = b.user("shopper@buyer.example").await;
    buyer.fund(shopper, "100.00").await;
    let local = local_sku(b, locals[0]).await;
    let created = b
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({"items": [{"product_id": locals[0], "sku_id": local.id, "quantity": 1}]})),
            Some(&token),
        )
        .await;
    let order_no = data(&created)["order_no"].as_str().unwrap().to_owned();
    let paid = b
        .call(
            "POST",
            "/api/v1/payments",
            Some(json!({"order_no": order_no, "channel_id": 0, "use_balance": true})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&paid)["order_paid"], true, "{paid}");
    buyer.drain().await; // procurement:submit → quote + order at the supplier
    let proc = procurement_orders::Entity::find()
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proc.status, "accepted", "{}", proc.error_message);
    assert!(proc.upstream_order_no.starts_with('D') || !proc.upstream_order_no.is_empty());
    assert_eq!(supplier.balance(sup_uid).await, "94.00");
    supplier.drain().await; // auto delivery → order.delivered event → buyer
    buyer.drain().await;
    let proc = procurement_orders::Entity::find_by_id(proc.id)
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(proc.status, "fulfilled");
    let child = orders::Entity::find_by_id(proc.local_order_id)
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(child.status, "delivered");
    let f = fulfillments::Entity::find()
        .filter(fulfillments::Column::OrderId.eq(child.id))
        .all(&b.db)
        .await
        .unwrap();
    assert_eq!(f.len(), 1);
    assert!(f[0].payload.starts_with("CARD-"), "{}", f[0].payload);
    // the event carried the delivery encrypted, never the card in clear
    let delivered: Vec<Value> = zs_webhook_events::Entity::find()
        .filter(zs_webhook_events::Column::Kind.eq("order.delivered"))
        .all(&s.db)
        .await
        .unwrap()
        .iter()
        .map(|e| serde_json::from_str(&e.body).unwrap())
        .collect();
    assert!(!delivered.is_empty());
    assert!(!delivered[0].to_string().contains("CARD-"));
    assert_eq!(
        delivered[0]["data"]["items"][0]["delivery"]["alg"],
        "A256GCM"
    );
    assert_eq!(delivered[0]["data"]["downstream_order_no"], child.order_no);
    ex.add(
        "事件推送 §6/§7：order.delivered（交付已加密）",
        "POST {buyer}/api/v1/zs/events",
        &scrub(&delivered[0]),
    );
    // redelivery of the same event id is deduplicated (UPS-02: no second fulfillment)
    let row = zs_webhook_events::Entity::find()
        .filter(zs_webhook_events::Column::Kind.eq("order.delivered"))
        .one(&s.db)
        .await
        .unwrap()
        .unwrap();
    let mut again: zs_webhook_events::ActiveModel = row.clone().into();
    again.status = Set("pending".into());
    again.update(&s.db).await.unwrap();
    s.services.integration.zs.deliver(row.id).await.unwrap();
    assert_eq!(
        fulfillments::Entity::find()
            .filter(fulfillments::Column::OrderId.eq(child.id))
            .all(&b.db)
            .await
            .unwrap()
            .len(),
        1
    );

    // ---- quote + idempotent order + error codes (direct protocol calls) -------------
    let items = json!([{"sku_id": p1_skus[0], "quantity": 1}]);
    let (st, q) = zs(
        s,
        call(
            "POST",
            "/api/v1/zs/orders/quote".into(),
            Some(json!({"items": items})),
        ),
    )
    .await;
    assert_eq!(st, 200, "{q}");
    assert_eq!(q["data"]["items"][0]["unit_price"], "6.00");
    assert_eq!(q["data"]["items"][0]["available"], true);
    assert_eq!(q["data"]["total"], "6.00");
    assert_eq!(q["data"]["sufficient_balance"], true);
    ex.add(
        "报价 §7",
        &format!(
            "POST /api/v1/zs/orders/quote\n\n{}",
            json!({"items": items})
        ),
        &scrub(&q),
    );
    let quote_id = q["data"]["quote_id"].as_str().unwrap().to_owned();
    let body = json!({"quote_id": quote_id, "items": items, "downstream_order_no": "B-1001", "trace_id": "t-1"});
    let order = |k: &str, body: Value| ZsCall {
        idempotency_key: Some(k.to_owned()),
        ..call("POST", "/api/v1/zs/orders".into(), Some(body))
    };
    let (st, o1) = zs(s, order("buyer-order-1", body.clone())).await;
    assert_eq!(st, 200, "{o1}");
    assert_eq!(o1["data"]["status"], "paid");
    assert_eq!(o1["data"]["downstream_order_no"], "B-1001");
    assert_eq!(o1["data"]["total"], "6.00");
    ex.add(
        "下单 §7（Idempotency-Key: buyer-order-1）",
        &format!("POST /api/v1/zs/orders\nIdempotency-Key: buyer-order-1\n\n{body}"),
        &scrub(&o1),
    );
    assert_eq!(supplier.balance(sup_uid).await, "88.00");
    // idempotent retry: same order, charged once
    let (st, o2) = zs(s, order("buyer-order-1", body.clone())).await;
    assert_eq!(
        (st, &o2["data"]["order_no"]),
        (200, &o1["data"]["order_no"])
    );
    assert_eq!(supplier.balance(sup_uid).await, "88.00");
    // same key, different body → 422 idempotency_conflict
    let (st, e) = zs(
        s,
        order(
            "buyer-order-1",
            json!({"items": items, "downstream_order_no": "B-1002"}),
        ),
    )
    .await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (422, Some("idempotency_conflict")),
        "{e}"
    );
    assert_eq!(e["error"]["retryable"], false);
    assert!(e["error"]["request_id"].is_string());
    ex.add(
        "错误 §8：422 idempotency_conflict",
        "POST /api/v1/zs/orders\nIdempotency-Key: buyer-order-1\n\n（请求体与首次不同）",
        &scrub(&e),
    );
    // same downstream_order_no with another key → same order
    let (st, o3) = zs(s, order("buyer-order-1b", body.clone())).await;
    assert_eq!(
        (st, &o3["data"]["order_no"]),
        (200, &o1["data"]["order_no"])
    );
    // missing key → 400
    let (st, e) = zs(
        s,
        call("POST", "/api/v1/zs/orders".into(), Some(body.clone())),
    )
    .await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (400, Some("invalid_request"))
    );
    // insufficient balance → 402
    let (st, e) = zs(
        s,
        order(
            "buyer-order-2",
            json!({"items": [{"sku_id": p3_skus[0], "quantity": 1}], "downstream_order_no": "B-1003"}),
        ),
    )
    .await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (402, Some("insufficient_balance")),
        "{e}"
    );
    ex.add(
        "错误 §8：402 insufficient_balance",
        "POST /api/v1/zs/orders\nIdempotency-Key: buyer-order-2\n\n{\"items\": [{\"sku_id\": …, \"quantity\": 1}], \"downstream_order_no\": \"B-1003\"}",
        &scrub(&e),
    );
    assert_eq!(supplier.balance(sup_uid).await, "88.00");
    // expired quote → 409
    let (_, q2) = zs(
        s,
        call(
            "POST",
            "/api/v1/zs/orders/quote".into(),
            Some(json!({"items": items})),
        ),
    )
    .await;
    let q2_id = q2["data"]["quote_id"].as_str().unwrap().to_owned();
    let row = zs_quotes::Entity::find()
        .filter(zs_quotes::Column::QuoteId.eq(q2_id.as_str()))
        .one(&s.db)
        .await
        .unwrap()
        .unwrap();
    let mut expired: zs_quotes::ActiveModel = row.into();
    expired.expires_at = Set(Utc::now() - chrono::Duration::seconds(1));
    expired.update(&s.db).await.unwrap();
    let (st, e) = zs(
        s,
        order(
            "buyer-order-3",
            json!({"quote_id": q2_id, "items": items, "downstream_order_no": "B-1004"}),
        ),
    )
    .await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (409, Some("quote_expired")),
        "{e}"
    );
    ex.add(
        "错误 §8：409 quote_expired",
        "POST /api/v1/zs/orders\nIdempotency-Key: buyer-order-3\n\n{\"quote_id\": \"q_…\", …}",
        &scrub(&e),
    );
    // order lookups
    let no = o1["data"]["order_no"].as_str().unwrap();
    let (st, got) = zs(s, call("GET", format!("/api/v1/zs/orders/{no}"), None)).await;
    assert_eq!(
        (st, &got["data"]["order_no"]),
        (200, &o1["data"]["order_no"])
    );
    let (st, got) = zs(
        s,
        call(
            "GET",
            "/api/v1/zs/orders?downstream_order_no=B-1001".into(),
            None,
        ),
    )
    .await;
    assert_eq!(st, 200);
    assert_eq!(got["data"]["items"][0]["order_no"], o1["data"]["order_no"]);
    let (st, e) = zs(
        s,
        call("POST", format!("/api/v1/zs/orders/{no}/cancel"), None),
    )
    .await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (409, Some("order_not_cancelable"))
    );

    // ---- replayed nonce → 401 -------------------------------------------------------
    let fixed = ZsCall {
        nonce: Some("fixed-nonce-0123456789".to_owned()),
        ..call("GET", "/api/v1/zs/handshake".into(), None)
    };
    assert_eq!(zs(s, fixed.clone()).await.0, 200);
    let (st, e) = zs(s, fixed).await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (401, Some("unauthorized"))
    );
    ex.add(
        "错误 §8：401 unauthorized（nonce 重放）",
        "GET /api/v1/zs/handshake\nZS-Nonce: fixed-nonce-0123456789（第二次）",
        &scrub(&e),
    );

    // ---- cursor expired → 410, buyer falls back to a full sync ----------------------
    add_cards(s, p1, p1_skus[0], &["CARD-6"]).await;
    s.services.integration.zs.snapshot_tick().await.unwrap();
    let max = zs_change_log::Entity::find()
        .order_by_desc(zs_change_log::Column::Id)
        .one(&s.db)
        .await
        .unwrap()
        .unwrap()
        .id;
    zs_change_log::Entity::delete_many()
        .filter(zs_change_log::Column::Id.lt(max))
        .exec(&s.db)
        .await
        .unwrap();
    let (st, e) = zs(
        s,
        call("GET", "/api/v1/zs/catalog/changes?since=1".into(), None),
    )
    .await;
    assert_eq!(
        (st, e["error"]["code"].as_str()),
        (410, Some("cursor_expired")),
        "{e}"
    );
    ex.add(
        "错误 §8：410 cursor_expired",
        "GET /api/v1/zs/catalog/changes?since=1",
        &scrub(&e),
    );
    let mut st_row: integration_connection_states::ActiveModel = state_of(b, conn).await.into();
    st_row.change_cursor = Set("1".into());
    st_row.update(&b.db).await.unwrap();
    b.services
        .integration
        .mappings
        .sync_connection_now(conn)
        .await
        .unwrap();
    assert_eq!(state_of(b, conn).await.change_cursor, max.to_string());
    let m1 = sku_mappings::Entity::find_by_id(m1.id)
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m1.upstream_stock, 4, "full sync applied (5 - 2 sold + 1)");

    // ---- key rotation: old + new valid, first use of the new promotes ---------------
    let rotated = s
        .call(
            "POST",
            "/api/v1/api-credential/rotate",
            None,
            Some(&sup_token),
        )
        .await;
    let next = data(&rotated)["api_secret"].as_str().unwrap().to_owned();
    let with = |secret: &str| ZsCall {
        secret: secret.to_owned(),
        ..call("GET", "/api/v1/zs/handshake".into(), None)
    };
    assert_eq!(
        zs(s, with(&new_secret)).await.0,
        200,
        "old secret still valid"
    );
    assert_eq!(zs(s, with(&next)).await.0, 200, "new secret valid");
    assert_eq!(zs(s, with(&new_secret)).await.0, 401, "old secret revoked");
    // expiry promotes automatically
    let rotated = s
        .call(
            "POST",
            "/api/v1/api-credential/rotate",
            None,
            Some(&sup_token),
        )
        .await;
    let third = data(&rotated)["api_secret"].as_str().unwrap().to_owned();
    let mut r: api_credential_rotations::ActiveModel = api_credential_rotations::Entity::find()
        .one(&s.db)
        .await
        .unwrap()
        .unwrap()
        .into();
    r.expires_at = Set(Utc::now() - chrono::Duration::seconds(1));
    r.update(&s.db).await.unwrap();
    assert_eq!(zs(s, with(&next)).await.0, 401, "expired rotation promoted");
    assert_eq!(zs(s, with(&third)).await.0, 200);

    ex.write();
}

// ---------------------------------------------------------------------------
// SSRF and address policy
// ---------------------------------------------------------------------------

/// UPS-01: with the production policy a supplier on a private address is refused
/// before any request, and a buyer cannot register a private webhook URL.
#[tokio::test]
async fn ups01_production_policy_refuses_private_targets() {
    let target = site(AddressPolicy::AllowPrivate).await;
    let prod = site(AddressPolicy::PublicOnly).await;
    let hs = prod
        .app
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(json!({"base_url": target.base, "api_key": "k", "api_secret": "s", "protocol": "zebra-store"})),
        )
        .await;
    let h = data(&hs);
    assert_eq!(h["ok"], false);
    assert!(h["error"].as_str().unwrap().contains("forbidden"), "{hs}");

    let (_, _, _, key, secret) = prod.app.buyer("b@x.example").await;
    for url in [
        "http://127.0.0.1:9/api/v1/zs/events",
        "http://10.0.0.1/x",
        "http://169.254.169.254/latest",
        "http://localhost/x",
    ] {
        let (st, e) = zs(
            &prod.app,
            ZsCall {
                method: "PUT".to_owned(),
                path_and_query: "/api/v1/zs/webhooks".to_owned(),
                body: Some(json!({"url": url})),
                key: key.clone(),
                secret: secret.clone(),
                ..ZsCall::default()
            },
        )
        .await;
        assert_eq!(
            (st, e["error"]["code"].as_str()),
            (400, Some("invalid_request")),
            "{url}"
        );
    }
}

/// `integration.allow_private_addresses` picks the policy; supplier redirects are
/// followed with a bounded policy.
#[tokio::test]
async fn private_address_switch_and_redirects() {
    let mut cfg = integration_common::config();
    assert_eq!(
        Adapters::from_config(&cfg).address_policy,
        AddressPolicy::PublicOnly
    );
    cfg.integration.allow_private_addresses = true;
    assert_eq!(
        Adapters::from_config(&cfg).address_policy,
        AddressPolicy::AllowPrivate
    );

    // a "supplier" that redirects its handshake elsewhere
    let redirect = axum::Router::new().route(
        "/api/v1/zs/handshake",
        axum::routing::get(|| async {
            (
                axum::http::StatusCode::FOUND,
                [(header::LOCATION, "http://127.0.0.1:1/elsewhere")],
            )
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, redirect).await });
    let lan = site(AddressPolicy::AllowPrivate).await;
    let hs = lan
        .app
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(json!({"base_url": base, "api_key": "k", "api_secret": "s", "protocol": "zebra-store"})),
        )
        .await;
    let h = data(&hs);
    assert_eq!(h["ok"], false);
    assert!(!h["error"].as_str().unwrap().contains("302"), "{hs}");
}

/// A connection whose handshake fails is still saved, with the error and the push
/// status recorded.
#[tokio::test]
async fn failed_handshake_does_not_block_saving() {
    let b = site(AddressPolicy::AllowPrivate).await;
    let res = b
        .app
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({"name": "down", "base_url": "http://127.0.0.1:1", "api_key": "k", "api_secret": "s", "protocol": "zebra-store"})),
        )
        .await;
    let id = data(&res)["id"].as_i64().unwrap();
    let got = b
        .app
        .admin("GET", &format!("/api/v1/admin/site-connections/{id}"), None)
        .await;
    let d = data(&got);
    assert_eq!(d["status"], "pending");
    assert_eq!(d["webhook_status"], "failed");
    assert_ne!(d["handshake_error"], "");
    assert_eq!(d["sync_mode"], "full");
}
