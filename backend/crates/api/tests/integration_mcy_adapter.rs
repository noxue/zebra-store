//! `mcy-shop` supplier adapter end to end: an in-test OpenApi plugin server
//! (`/plugin/open-api/*`, form bodies, `Api-Id` / `Api-Signature` checked with
//! `Str::generateSignature` semantics — array fields excluded) and our real buyer
//! instance driven through the admin API: handshake → connection → categories /
//! products → import → storefront order → `sku/state` + `amount` + `trade` →
//! delivery → fulfilled. Error paths: bad key, out of stock, insufficient balance
//! (MCY-02), lost `trade` answer never retried (MCY-01), after-sale contents (MCY-03),
//! renamed SKU keeps its mapping (MCY-04).

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Uri, header};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use chrono::Utc;
use integration_common::{IntApp, data, seed_item, seed_order_row};
use md5::{Digest, Md5};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{
    fulfillments, orders, procurement_deliveries, procurement_orders, product_mappings,
    product_skus, products, sku_mappings, wallet_accounts,
};
use zs_infra::integration::http::AddressPolicy;
use zs_infra::queue::Worker;
use zs_infra::wire::WireCtx;
use zs_infra::wire::integration::Adapters;

const API_ID: &str = "1024";
const APP_KEY: &str = "8F3A2C9D1E7B6A54";

// ---------------------------------------------------------------------------
// Mock OpenApi plugin
// ---------------------------------------------------------------------------

struct Sku {
    id: i64,
    name: String,
    cents: i64,
    /// `None` = the plugin answers `null` (unlimited).
    stock: Option<i64>,
}

#[derive(Default)]
struct Plugin {
    balance_cents: i64,
    skus: Vec<Sku>,
    trades: Vec<Vec<(String, String)>>,
    /// Committed purchases `(trade_no, contents)`.
    orders: Vec<(String, String)>,
    seq: u64,
    drop_after_commit: bool,
    after_sale: bool,
    requests: Vec<String>,
}

type Mock = Arc<Mutex<Plugin>>;

fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                out.push(u8::from_str_radix(&raw[i + 1..i + 3], 16).unwrap());
                i += 2;
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8(out).unwrap()
}

fn parse_form(body: &[u8]) -> Vec<(String, String)> {
    std::str::from_utf8(body)
        .unwrap()
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

/// mcy-shop `Str::generateSignature` (`kernel/Util/Str.php:97-107`): ksort, drop `sign`,
/// empty strings and array values, urldecoded query + `&key=`.
fn mcy_signature(pairs: &[(String, String)], key: &str) -> String {
    let mut post: Vec<(String, String)> = Vec::new();
    for (k, v) in pairs {
        if k.contains('[') {
            continue; // array value: excluded
        }
        post.retain(|(pk, _)| pk != k);
        post.push((k.clone(), v.clone()));
    }
    post.retain(|(k, v)| k != "sign" && !v.is_empty());
    post.sort_by(|a, b| a.0.cmp(&b.0));
    let q: Vec<String> = post.iter().map(|(k, v)| format!("{k}={v}")).collect();
    hex::encode(Md5::digest(format!("{}&key={key}", q.join("&"))))
}

fn ok(data: Value) -> Response {
    axum::Json(json!({"code": 200, "msg": "success", "data": data})).into_response()
}

fn fail(msg: &str) -> Response {
    axum::Json(json!({"code": 400, "msg": msg})).into_response()
}

fn flat(pairs: &[(String, String)], key: &str) -> String {
    pairs
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn money(c: i64) -> String {
    format!("{}.{:02}", c / 100, c % 100)
}

fn item(p: &Plugin) -> Value {
    json!({
        "id": 20, "name": "会员卡", "introduce": "<p>会员说明</p>",
        "picture_url": "", "category": {"name": "会员"},
        "widget": "[{\"title\":\"QQ号\",\"name\":\"qq\",\"placeholder\":\"\",\"type\":\"text\",\"regex\":\"\",\"error\":\"\",\"data\":\"\"}]",
        "sku": p.skus.iter().map(|s| json!({
            "id": s.id, "name": s.name, "stock_price": money(s.cents), "stock": s.stock,
        })).collect::<Vec<_>>(),
    })
}

fn broken() -> Response {
    let (mut tx, body) = http_body_util::channel::Channel::<Bytes, std::io::Error>::new(1);
    tokio::spawn(async move {
        tx.send_data(Bytes::from_static(b"{\"code\":200"))
            .await
            .unwrap();
        tx.abort(std::io::Error::other("supplier connection reset"));
    });
    Response::builder()
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::new(body))
        .unwrap()
}

async fn open_api(State(m): State<Mock>, uri: Uri, headers: HeaderMap, body: Bytes) -> Response {
    let pairs = parse_form(&body);
    let endpoint = uri
        .path()
        .trim_start_matches("/plugin/open-api/")
        .to_owned();
    let mut p = m.lock().unwrap();
    p.requests.push(endpoint.clone());
    let h = |k: &str| {
        headers
            .get(k)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    if h("Api-Id") != API_ID {
        return fail("API-ID不存在");
    }
    if h("Api-Signature") != mcy_signature(&pairs, APP_KEY) {
        return fail("签名错误");
    }
    let sku_id: i64 = flat(&pairs, "sku_id").parse().unwrap_or(0);
    let qty: i64 = flat(&pairs, "quantity").parse().unwrap_or(0);
    let enough = |p: &Plugin| {
        p.skus
            .iter()
            .find(|s| s.id == sku_id)
            .is_some_and(|s| s.stock.is_none_or(|q| q >= qty))
    };
    match endpoint.as_str() {
        "connect" => ok(json!({"username": "mcybuyer", "balance": money(p.balance_cents)})),
        "items" => ok(json!([item(&p)])),
        "item" => {
            if flat(&pairs, "id") == "20" {
                ok(item(&p))
            } else {
                fail("商品不存在")
            }
        }
        "sku/state" => ok(json!({"state": enough(&p)})),
        "amount" => match p.skus.iter().find(|s| s.id == sku_id) {
            Some(s) => ok(json!({"amount": money(s.cents * qty)})),
            None => fail("SKU不存在"),
        },
        "trade" => {
            p.trades.push(pairs.clone());
            let Some(unit) = p.skus.iter().find(|s| s.id == sku_id).map(|s| s.cents) else {
                return fail("SKU不存在");
            };
            if !enough(&p) {
                return fail("库存不足");
            }
            if p.balance_cents < unit * qty {
                return fail("余额不足");
            }
            p.balance_cents -= unit * qty;
            if let Some(s) = p.skus.iter_mut().find(|s| s.id == sku_id)
                && let Some(q) = s.stock.as_mut()
            {
                *q -= qty;
            }
            p.seq += 1;
            let contents = if p.after_sale {
                "库存不足，请申请售后".to_owned()
            } else {
                (0..qty)
                    .map(|i| format!("MCY-{}-{i}", p.seq))
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            p.orders.push((flat(&pairs, "trade_no"), contents.clone()));
            if p.drop_after_commit {
                return broken();
            }
            ok(json!({"contents": contents}))
        }
        _ => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn start_mock() -> (String, Mock) {
    let plugin = Plugin {
        balance_cents: 10_000,
        skus: vec![
            Sku {
                id: 201,
                name: "月卡".into(),
                cents: 600,
                stock: Some(5),
            },
            Sku {
                id: 202,
                name: "年卡".into(),
                cents: 6_000,
                stock: None,
            },
        ],
        ..Plugin::default()
    };
    let mock: Mock = Arc::new(Mutex::new(plugin));
    let app = Router::new()
        .route("/plugin/open-api/{*rest}", post(open_api))
        .with_state(mock.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (base, mock)
}

// ---------------------------------------------------------------------------
// Buyer (our instance)
// ---------------------------------------------------------------------------

struct Site {
    app: IntApp,
    ctx: WireCtx,
}

async fn site() -> Site {
    let cfg = integration_common::config();
    let db = zs_infra::db::connect(&cfg.database).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let ctx = WireCtx::new(&db, &cfg);
    let mut services = zs_infra::wire::services(&ctx);
    // `integration.allow_private_addresses`: the mock listens on loopback.
    services.integration = zs_infra::wire::integration::build_with(
        &ctx,
        &Adapters {
            address_policy: AddressPolicy::AllowPrivate,
            ..Adapters::default()
        },
    );
    zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
    let router = zs_api::build(zs_api::AppState::new(services.clone(), cfg)).router;
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
    Site { app, ctx }
}

impl Site {
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

    async fn connect(&self, base: &str) -> i64 {
        let res = self
            .app
            .admin(
                "POST",
                "/api/v1/admin/site-connections",
                Some(json!({
                    "name": "MCY", "base_url": base, "api_key": API_ID, "api_secret": APP_KEY,
                    "protocol": "mcy-shop", "auto_sync_price": true,
                })),
            )
            .await;
        data(&res)["id"].as_i64().unwrap()
    }

    async fn import(&self, conn: i64) -> i64 {
        let res = self
            .app
            .admin(
                "POST",
                "/api/v1/admin/product-mappings/import",
                Some(json!({"connection_id": conn, "upstream_product_id": 20, "auto_create_category": true})),
            )
            .await;
        let id = data(&res)["local_product_id"].as_i64().unwrap();
        let mut row: products::ActiveModel = products::Entity::find_by_id(id)
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
            .into();
        row.is_active = Set(true);
        row.update(&self.app.db).await.unwrap();
        id
    }

    async fn sku(&self, product_id: i64, code: &str) -> product_skus::Model {
        product_skus::Entity::find()
            .filter(product_skus::Column::ProductId.eq(product_id))
            .filter(product_skus::Column::SkuCode.eq(code))
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
    }

    async fn procure(&self, order_no: &str, product: i64, sku: i64, qty: i32) -> i64 {
        let oid = seed_order_row(&self.app.db, order_no, 1, "paid", "10.00", None).await;
        seed_item(&self.app.db, oid, product, sku, qty, "upstream").await;
        self.app
            .services
            .integration
            .order_events
            .order_paid(oid)
            .await
            .unwrap();
        let p = procurement_orders::Entity::find()
            .filter(procurement_orders::Column::LocalOrderId.eq(oid))
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap();
        self.submit(p.id).await;
        p.id
    }

    async fn submit(&self, id: i64) {
        self.app
            .services
            .integration
            .procurement
            .submit(id)
            .await
            .unwrap();
    }

    async fn procurement(&self, id: i64) -> procurement_orders::Model {
        procurement_orders::Entity::find_by_id(id)
            .one(&self.app.db)
            .await
            .unwrap()
            .unwrap()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn mcy_shop_end_to_end() {
    let (base, mock) = start_mock().await;
    let buyer = site().await;
    let b = &buyer.app;

    let res = b
        .admin("GET", "/api/v1/admin/site-connections/protocols", None)
        .await;
    let meta = data(&res)
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "mcy-shop")
        .unwrap()
        .clone();
    assert_eq!(meta["name"]["zh-CN"], "萌次元商城");
    assert_eq!(meta["capabilities"], json!(["categories"]));
    let keys: Vec<&str> = meta["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["key"].as_str().unwrap())
        .collect();
    assert_eq!(keys, vec!["base_url", "api_key", "api_secret", "currency"]);
    assert_eq!(meta["fields"][1]["label"]["zh-CN"], "API-ID（用户 ID）");

    // ---- bad key → refused, message kept -------------------------------------------------
    let bad = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(json!({"base_url": base, "api_key": API_ID, "api_secret": "WRONG", "protocol": "mcy-shop"})),
        )
        .await;
    assert_eq!(data(&bad)["ok"], false);
    assert!(
        data(&bad)["error"].as_str().unwrap().contains("签名错误"),
        "{bad}"
    );
    let hs = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(json!({"base_url": base, "api_key": API_ID, "api_secret": APP_KEY, "protocol": "mcy-shop"})),
        )
        .await;
    assert_eq!(data(&hs)["ok"], true, "{hs}");
    assert_eq!(data(&hs)["site"]["name"], "mcybuyer");
    assert_eq!(data(&hs)["account"]["balance"], "100.00");

    // ---- connection, categories, products --------------------------------------------------
    let conn = buyer.connect(&base).await;
    let got = b
        .admin(
            "GET",
            &format!("/api/v1/admin/site-connections/{conn}"),
            None,
        )
        .await;
    assert_eq!(data(&got)["status"], "active", "{got}");
    assert_eq!(data(&got)["supplier_currency"], "CNY");
    let cats = b
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-categories?connection_id={conn}"),
            None,
        )
        .await;
    assert!(data(&cats).to_string().contains("会员"), "{cats}");
    let page = b
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-products?connection_id={conn}&page=1&page_size=20"),
            None,
        )
        .await;
    let p = &data(&page)["items"][0];
    assert_eq!(p["id"], 20);
    assert_eq!(p["price_amount"], "6.00");
    assert_eq!(p["skus"][0]["id"], 201);
    assert_eq!(p["skus"][0]["stock_quantity"], 5);
    assert_eq!(p["skus"][1]["stock_quantity"], -1, "null stock = unlimited");

    // ---- import + storefront order ------------------------------------------------------------
    let product = buyer.import(conn).await;
    let month = buyer.sku(product, "月卡").await;
    assert_eq!(format!("{:.2}", month.price_amount), "6.00");
    let (shopper, token) = b.user("shopper@buyer.example").await;
    buyer.fund(shopper, "100.00").await;
    let created = b
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({
                "items": [{"product_id": product, "sku_id": month.id, "quantity": 2}],
                "manual_form_data": {product.to_string(): {"qq": "10001"}},
            })),
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
    buyer.drain().await;
    let proc = procurement_orders::Entity::find()
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    // `contents` of the trade is persisted with the trade number and delivered right
    // away (the plugin has no order lookup: nothing may live only in memory).
    assert_eq!(proc.status, "fulfilled", "{}", proc.error_message);
    let held = procurement_deliveries::Entity::find()
        .filter(procurement_deliveries::Column::ProcurementOrderId.eq(proc.id))
        .one(&b.db)
        .await
        .unwrap()
        .expect("synchronous delivery persisted");
    assert_eq!(held.payload, "MCY-1-0\nMCY-1-1");
    {
        let m = mock.lock().unwrap();
        let t = &m.trades[0];
        assert_eq!(flat(t, "sku_id"), "201");
        assert_eq!(flat(t, "quantity"), "2");
        assert_eq!(flat(t, "qq"), "10001");
        let trade_no = flat(t, "trade_no");
        assert_eq!(trade_no.len(), 24);
        assert_eq!(proc.upstream_order_no, trade_no);
        assert_eq!(m.balance_cents, 10_000 - 1_200);
        let tail: Vec<&str> = m
            .requests
            .iter()
            .rev()
            .take(3)
            .map(String::as_str)
            .collect();
        assert_eq!(tail, vec!["trade", "amount", "sku/state"]);
    }
    assert_eq!(format!("{:.2}", proc.upstream_amount), "12.00");
    b.services
        .integration
        .procurement
        .poll(proc.id)
        .await
        .unwrap();
    let proc = buyer.procurement(proc.id).await;
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
    assert_eq!(f[0].payload, "MCY-1-0\nMCY-1-1");

    // ---- MCY-04: a renamed SKU keeps its mapping (id based), price follows ----------------
    {
        let mut m = mock.lock().unwrap();
        m.skus[0].name = "月卡 Plus".into();
        m.skus[0].cents = 650;
    }
    let mapping = product_mappings::Entity::find()
        .filter(product_mappings::Column::LocalProductId.eq(product))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    let synced = b
        .admin(
            "POST",
            &format!("/api/v1/admin/product-mappings/{}/sync", mapping.id),
            None,
        )
        .await;
    data(&synced);
    let m = sku_mappings::Entity::find()
        .filter(sku_mappings::Column::LocalSkuId.eq(month.id))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.upstream_sku_id, 201);
    assert_eq!(format!("{:.2}", m.upstream_price), "6.50");
    assert_eq!(m.upstream_stock, 3);
    let local = product_skus::Entity::find_by_id(month.id)
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(format!("{:.2}", local.price_amount), "6.50");
}

#[tokio::test]
async fn mcy_shop_error_paths() {
    let (base, mock) = start_mock().await;
    let buyer = site().await;
    let conn = buyer.connect(&base).await;
    let product = buyer.import(conn).await;
    let month = buyer.sku(product, "月卡").await.id;
    let year = buyer.sku(product, "年卡").await.id;

    // ---- out of stock: sku/state refuses, no trade ---------------------------------------
    mock.lock().unwrap().skus[0].stock = Some(0);
    let p = buyer.procure("L-OOS", product, month, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "rejected");
    assert!(
        row.error_message.contains("sku/state"),
        "{}",
        row.error_message
    );
    assert!(mock.lock().unwrap().trades.is_empty());

    // ---- MCY-02: the supplier's message survives and the refusal is final -----------------
    mock.lock().unwrap().balance_cents = 100;
    let p = buyer.procure("L-BAL", product, year, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "rejected");
    assert!(
        row.error_message.contains("余额不足"),
        "{}",
        row.error_message
    );
    assert_eq!(mock.lock().unwrap().trades.len(), 1);

    // ---- MCY-01: lost trade answer → manual check, never retried --------------------------
    {
        let mut m = mock.lock().unwrap();
        m.balance_cents = 10_000;
        m.drop_after_commit = true;
    }
    let p = buyer.procure("L-LOST", product, year, 1).await;
    let row = buyer.procurement(p).await;
    // may have been charged: held for manual review, the local order is not rolled back
    assert_eq!(row.status, "manual_review", "{}", row.error_message);
    assert!(
        row.error_message.contains("check the order manually"),
        "{}",
        row.error_message
    );
    assert_eq!(mock.lock().unwrap().trades.len(), 2);
    assert_eq!(mock.lock().unwrap().orders.len(), 1);
    mock.lock().unwrap().drop_after_commit = false;
    buyer.submit(p).await;
    assert_eq!(mock.lock().unwrap().trades.len(), 2, "no second trade");
    assert_eq!(mock.lock().unwrap().balance_cents, 10_000 - 6_000);

    // ---- MCY-03: after-sale contents are not a delivery -----------------------------------
    {
        let mut m = mock.lock().unwrap();
        m.after_sale = true;
        m.balance_cents = 10_000;
    }
    let p = buyer.procure("L-AS", product, year, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "manual_review", "{}", row.error_message);
    assert!(!row.upstream_order_no.is_empty(), "trade number kept");
    assert!(
        row.error_message.contains("no goods"),
        "{}",
        row.error_message
    );
    buyer
        .app
        .services
        .integration
        .procurement
        .poll(p)
        .await
        .unwrap();
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "manual_review", "awaits a manual check");
    assert!(
        fulfillments::Entity::find()
            .filter(fulfillments::Column::OrderId.eq(row.local_order_id))
            .all(&buyer.app.db)
            .await
            .unwrap()
            .is_empty()
    );
}
