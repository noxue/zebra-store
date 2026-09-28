//! `acg-faka` supplier adapter end to end: an in-test acg-faka 「共享店铺」 server
//! (`/shared/*`, form bodies, `SharedValidation` md5 signature re-implemented from the
//! PHP semantics, balance-paid `trade`, `query`) and our real buyer instance driven
//! through the admin API: handshake → connection → categories / products → import →
//! storefront order → procurement `trade` → `query` poll → cards delivered.
//! Error paths: bad key / unknown merchant, out of stock, insufficient balance, a
//! `trade` whose answer is lost (never bought twice, ACG-02), manual delivery confirmed
//! by a change of the `query` content (ACG-04).

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{Uri, header};
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

const APP_ID: &str = "1024";
const APP_KEY: &str = "8F3A2C9D1E7B6A54";

// ---------------------------------------------------------------------------
// Mock acg-faka supplier
// ---------------------------------------------------------------------------

struct Commodity {
    id: i64,
    code: &'static str,
    name: &'static str,
    /// `(race, retail price, our buy price in cents)`; empty = no races.
    races: Vec<(&'static str, &'static str, i64)>,
    /// Buy price of a commodity without races (cents).
    factory_cents: i64,
    widget: Value,
    delivery_way: i64,
}

fn catalog() -> Vec<Commodity> {
    vec![
        Commodity {
            id: 12,
            code: "GAME12",
            name: "游戏点卡",
            races: vec![("月卡", "10.00", 850), ("季卡", "28.00", 2500)],
            factory_cents: 0,
            widget: json!([{"cn": "游戏账号", "name": "Account", "placeholder": "账号", "type": "text", "regex": "^[a-z0-9]+$", "error": "账号格式错误", "dict": ""}]),
            delivery_way: 0,
        },
        Commodity {
            id: 13,
            code: "PLAIN13",
            name: "普通卡",
            races: Vec::new(),
            factory_cents: 500,
            widget: json!([]),
            delivery_way: 0,
        },
        Commodity {
            id: 14,
            code: "MANUAL14",
            name: "代充服务",
            races: Vec::new(),
            factory_cents: 300,
            widget: json!([]),
            delivery_way: 1,
        },
    ]
}

struct MockOrder {
    trade_no: String,
    request_no: String,
    secret: String,
}

#[derive(Default)]
struct Shop {
    balance_cents: i64,
    /// Unsold cards per `(commodity, race)`.
    cards: HashMap<(i64, String), Vec<String>>,
    /// Stock column of manual-delivery commodities.
    manual_stock: i64,
    orders: Vec<MockOrder>,
    seq: u64,
    /// Commit the trade, then break the connection (lost answer).
    drop_after_commit: bool,
    /// Break the connection of every trade without committing it (the buyer cannot
    /// tell the difference from a lost answer).
    reset_trades: bool,
    requests: Vec<(String, Vec<(String, String)>)>,
}

type Mock = Arc<Mutex<Shop>>;

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

/// `application/x-www-form-urlencoded` pairs in order (PHP reads the same body).
fn parse_form(body: &[u8]) -> Vec<(String, String)> {
    let raw = std::str::from_utf8(body).unwrap();
    raw.split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

enum PhpValue {
    Flat(String),
    Nested(Vec<(String, String)>),
}

/// `$_POST` as PHP builds it: top-level keys in first-appearance order, `a[b]=v`
/// nested (last value wins per key).
fn php_post(pairs: &[(String, String)]) -> Vec<(String, PhpValue)> {
    let mut post: Vec<(String, PhpValue)> = Vec::new();
    for (name, value) in pairs {
        let (top, inner) = match name.split_once('[') {
            Some((t, rest)) => (t.to_owned(), Some(rest.trim_end_matches(']').to_owned())),
            None => (name.clone(), None),
        };
        let slot = match post.iter().position(|(k, _)| *k == top) {
            Some(i) => i,
            None => {
                let empty = if inner.is_some() {
                    PhpValue::Nested(Vec::new())
                } else {
                    PhpValue::Flat(String::new())
                };
                post.push((top.clone(), empty));
                post.len() - 1
            }
        };
        match (inner, &mut post[slot].1) {
            (None, v) => *v = PhpValue::Flat(value.clone()),
            (Some(k), PhpValue::Nested(list)) => {
                list.retain(|(ik, _)| *ik != k);
                list.push((k, value.clone()));
            }
            (Some(k), v) => *v = PhpValue::Nested(vec![(k, value.clone())]),
        }
    }
    post
}

/// `Str::generateSignature($_POST, $appKey)` (acg-faka `app/Util/Str.php:103-113`).
fn php_signature(pairs: &[(String, String)], key: &str) -> String {
    let mut post = php_post(pairs);
    post.retain(|(k, _)| k != "sign");
    post.sort_by(|a, b| a.0.cmp(&b.0));
    post.retain(|(_, v)| !matches!(v, PhpValue::Flat(s) if s.is_empty()));
    let mut parts = Vec::new();
    for (k, v) in &post {
        match v {
            PhpValue::Flat(s) => parts.push(format!("{k}={s}")),
            PhpValue::Nested(list) => {
                for (ik, iv) in list {
                    parts.push(format!("{k}[{ik}]={iv}"));
                }
            }
        }
    }
    hex::encode(Md5::digest(format!("{}&key={key}", parts.join("&"))))
}

fn ok(data: Value) -> Response {
    axum::Json(json!({"code": 200, "msg": "success", "data": data})).into_response()
}

fn fail(msg: &str) -> Response {
    axum::Json(json!({"code": 0, "msg": msg})).into_response()
}

fn cents(c: i64) -> String {
    format!("{}.{:02}", c / 100, c % 100)
}

fn flat(pairs: &[(String, String)], key: &str) -> String {
    pairs
        .iter()
        .rev()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

fn stock_of(shop: &Shop, c: &Commodity, race: &str) -> i64 {
    if c.delivery_way == 1 {
        return shop.manual_stock;
    }
    shop.cards
        .get(&(c.id, race.to_owned()))
        .map_or(0, |v| i64::try_from(v.len()).unwrap())
}

fn total_stock(shop: &Shop, c: &Commodity) -> i64 {
    if c.races.is_empty() {
        stock_of(shop, c, "")
    } else {
        c.races.iter().map(|(r, _, _)| stock_of(shop, c, r)).sum()
    }
}

fn row(shop: &Shop, c: &Commodity) -> Value {
    let config: String = if c.races.is_empty() {
        String::new()
    } else {
        let mut s = "[category]\n".to_owned();
        for (r, retail, _) in &c.races {
            s.push_str(&format!("{r}={retail}\n"));
        }
        s
    };
    json!({
        "id": c.id, "category_id": 3, "name": c.name, "description": "<p>说明</p>",
        "cover": "/favicon.ico", "price": 10, "user_price": 9, "status": 1, "code": c.code,
        "sort": 0, "delivery_way": c.delivery_way, "contact_type": 0, "password_status": 0,
        "widget": c.widget.to_string(), "minimum": 0, "maximum": 0, "config": config,
        "stock": total_stock(shop, c), "tags": "热门,点卡",
    })
}

fn detail(shop: &Shop, c: &Commodity) -> Value {
    let mut v = row(shop, c);
    let config = if c.races.is_empty() {
        json!({})
    } else {
        let category: serde_json::Map<String, Value> = c
            .races
            .iter()
            .map(|(r, retail, _)| ((*r).to_owned(), json!(retail)))
            .collect();
        let factory: serde_json::Map<String, Value> = c
            .races
            .iter()
            // acg-faka answers float numbers
            .map(|(r, _, f)| ((*r).to_owned(), json!((*f as f64) / 100.0)))
            .collect();
        json!({"category": category, "category_factory": factory})
    };
    v["config"] = config;
    v["widget"] = c.widget.clone();
    v["factory_price"] = json!((c.factory_cents as f64) / 100.0);
    v["tags"] = json!(["热门", "点卡"]);
    v
}

/// Answer whose body breaks off after the headers: the client sees the connection
/// break after the supplier committed the trade.
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

async fn shared(State(m): State<Mock>, uri: Uri, body: Bytes) -> Response {
    let pairs = parse_form(&body);
    let path = uri.path().to_owned();
    let mut shop = m.lock().unwrap();
    shop.requests.push((path.clone(), pairs.clone()));
    // SharedValidation: user by app_id, hash_equals(generateSignature($_POST, key), sign)
    if flat(&pairs, "app_id") != APP_ID {
        return fail("商户ID不存在");
    }
    if flat(&pairs, "sign") != php_signature(&pairs, APP_KEY) {
        return fail("密钥错误");
    }
    let catalog = catalog();
    let by_code = |code: &str| catalog.iter().find(|c| c.code == code);
    match path.as_str() {
        "/shared/authentication/connect" => ok(json!({
            "shopName": "异次元测试站",
            "balance": (shop.balance_cents as f64) / 100.0,
        })),
        "/shared/commodity/items" => ok(json!([{
            "id": 3, "name": "游戏", "sort": 0, "icon": "", "status": 1, "pid": 0,
            "children": catalog.iter().map(|c| row(&shop, c)).collect::<Vec<_>>(),
        }])),
        "/shared/commodity/item" => match by_code(&flat(&pairs, "code")) {
            Some(c) => ok(detail(&shop, c)),
            None => fail("商品不存在"),
        },
        "/shared/commodity/stock" => match by_code(&flat(&pairs, "code")) {
            Some(c) => ok(json!({"stock": stock_of(&shop, c, &flat(&pairs, "race")).to_string()})),
            None => fail("商品不存在"),
        },
        "/shared/commodity/query" => {
            let no = flat(&pairs, "tradeNo");
            match shop.orders.iter().find(|o| o.trade_no == no) {
                Some(o) => ok(json!({"secret": o.secret, "widget": null, "status": 1})),
                None => fail("订单不存在"),
            }
        }
        "/shared/commodity/trade" => {
            if shop.reset_trades {
                return broken();
            }
            let Some(c) = by_code(&flat(&pairs, "shared_code")) else {
                return fail("商品不存在");
            };
            let num: i64 = flat(&pairs, "num").parse().unwrap_or(0);
            if num <= 0 {
                return fail("至少购买1个");
            }
            let race = flat(&pairs, "race");
            let unit = if c.races.is_empty() {
                c.factory_cents
            } else {
                match c.races.iter().find(|(r, _, _)| *r == race) {
                    Some((_, _, f)) => *f,
                    None => return fail("请选择商品种类"),
                }
            };
            for w in c.widget.as_array().unwrap() {
                let name = w["name"].as_str().unwrap();
                let value = flat(&pairs, name);
                let valid = !value.is_empty()
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
                if !valid {
                    return fail(w["error"].as_str().unwrap());
                }
            }
            let request_no = flat(&pairs, "request_no");
            if request_no.len() > 19 {
                return fail("SQLSTATE[22001]: Data too long for column 'request_no'");
            }
            if !request_no.is_empty() && shop.orders.iter().any(|o| o.request_no == request_no) {
                return fail("The request ID already exists");
            }
            if c.delivery_way == 0 && stock_of(&shop, c, &race) < num {
                return fail("库存不足");
            }
            let amount = unit * num;
            if shop.balance_cents < amount {
                return fail("余额不足");
            }
            // committed: balance, cards, order
            shop.balance_cents -= amount;
            let secret = if c.delivery_way == 0 {
                let cards = shop.cards.get_mut(&(c.id, race.clone())).unwrap();
                let taken: Vec<String> = cards.drain(..usize::try_from(num).unwrap()).collect();
                taken.join("\n")
            } else {
                shop.manual_stock -= num;
                "付款后请联系客服 QQ 10000 领取".to_owned()
            };
            shop.seq += 1;
            let trade_no = format!("{}", 583_920_174_628_300_000_u64 + shop.seq);
            shop.orders.push(MockOrder {
                trade_no: trade_no.clone(),
                request_no,
                secret: secret.clone(),
            });
            if shop.drop_after_commit {
                return broken();
            }
            let left = stock_of(&shop, c, &race);
            ok(json!({
                "url": format!("https://acg.example/user/personal/purchaseRecord?tradeNo={trade_no}"),
                "amount": cents(amount), "tradeNo": trade_no, "secret": secret,
                "leave_message": null, "stock": left.to_string(),
            }))
        }
        _ => (axum::http::StatusCode::NOT_FOUND, "<html>404</html>").into_response(),
    }
}

async fn start_mock() -> (String, Mock) {
    let mut shop = Shop {
        balance_cents: 10_000,
        manual_stock: 50,
        ..Shop::default()
    };
    shop.cards.insert(
        (12, "月卡".into()),
        vec!["M1".into(), "M2".into(), "M3".into()],
    );
    shop.cards.insert((12, "季卡".into()), vec!["Q1".into()]);
    shop.cards.insert(
        (13, String::new()),
        vec!["P1".into(), "P2".into(), "P3".into()],
    );
    let mock: Mock = Arc::new(Mutex::new(shop));
    let app = Router::new()
        .route("/shared/{*rest}", post(shared))
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
                    "name": "ACG", "base_url": base, "api_key": APP_ID, "api_secret": APP_KEY,
                    "protocol": "acg-faka", "extra": {"currency": "CNY"},
                })),
            )
            .await;
        data(&res)["id"].as_i64().unwrap()
    }

    async fn import(&self, conn: i64, upstream: i64) -> i64 {
        let res = self
            .app
            .admin(
                "POST",
                "/api/v1/admin/product-mappings/import",
                Some(json!({"connection_id": conn, "upstream_product_id": upstream, "auto_create_category": true})),
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

    async fn skus(&self, product_id: i64) -> Vec<product_skus::Model> {
        product_skus::Entity::find()
            .filter(product_skus::Column::ProductId.eq(product_id))
            .all(&self.app.db)
            .await
            .unwrap()
    }

    /// A paid local order of one upstream SKU, submitted to the supplier once.
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

fn trades(mock: &Mock) -> Vec<Vec<(String, String)>> {
    mock.lock()
        .unwrap()
        .requests
        .iter()
        .filter(|(p, _)| p == "/shared/commodity/trade")
        .map(|(_, f)| f.clone())
        .collect()
}

fn field(fields: &[(String, String)], key: &str) -> String {
    flat(fields, key)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn acg_faka_end_to_end() {
    let (base, mock) = start_mock().await;
    let buyer = site().await;
    let b = &buyer.app;

    // ---- the protocol registry drives the admin form ---------------------------------
    let res = b
        .admin("GET", "/api/v1/admin/site-connections/protocols", None)
        .await;
    let meta = data(&res)
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "acg-faka")
        .unwrap()
        .clone();
    assert_eq!(meta["name"]["zh-CN"], "异次元发卡");
    assert_eq!(meta["name"]["en-US"], "ACG-Faka");
    assert!(
        meta["description"]["zh-TW"]
            .as_str()
            .unwrap()
            .contains("異次元")
    );
    assert_eq!(meta["capabilities"], json!(["categories"]));
    assert_eq!(meta["supports_connection_code"], false);
    let fields: Vec<(&str, &str, bool)> = meta["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["key"].as_str().unwrap(),
                f["kind"].as_str().unwrap(),
                f["required"].as_bool().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        fields,
        vec![
            ("base_url", "url", true),
            ("api_key", "text", true),
            ("api_secret", "secret", true),
            ("currency", "text", false)
        ]
    );

    // ---- handshake pre-check: wrong key / unknown merchant / success -------------------
    let probe = |key: &str, id: &str| json!({"base_url": base, "api_key": id, "api_secret": key, "protocol": "acg-faka"});
    let bad = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(probe("WRONG-KEY", APP_ID)),
        )
        .await;
    assert_eq!(data(&bad)["ok"], false);
    assert!(
        data(&bad)["error"].as_str().unwrap().contains("密钥错误"),
        "{bad}"
    );
    let bad = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(probe(APP_KEY, "9999")),
        )
        .await;
    assert!(
        data(&bad)["error"]
            .as_str()
            .unwrap()
            .contains("商户ID不存在"),
        "{bad}"
    );
    let hs = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections/handshake",
            Some(probe(APP_KEY, APP_ID)),
        )
        .await;
    let h = data(&hs);
    assert_eq!(h["ok"], true, "{hs}");
    assert_eq!(h["protocol"], "acg-faka");
    assert_eq!(h["site"]["name"], "异次元测试站");
    assert_eq!(h["site"]["currency"], "CNY");
    assert_eq!(h["account"]["balance"], "100.00");
    assert_eq!(h["features"], json!(["categories"]));

    // ---- a connection with a wrong key is saved but stays pending ---------------------
    let wrong = b
        .admin(
            "POST",
            "/api/v1/admin/site-connections",
            Some(json!({"name": "bad", "base_url": base, "api_key": APP_ID, "api_secret": "nope", "protocol": "acg-faka"})),
        )
        .await;
    let wrong_id = data(&wrong)["id"].as_i64().unwrap();
    let got = b
        .admin(
            "GET",
            &format!("/api/v1/admin/site-connections/{wrong_id}"),
            None,
        )
        .await;
    assert_eq!(data(&got)["status"], "pending");
    assert!(
        data(&got)["handshake_error"]
            .as_str()
            .unwrap()
            .contains("密钥错误")
    );

    // ---- connection + ping ---------------------------------------------------------------
    let conn = buyer.connect(&base).await;
    let got = b
        .admin(
            "GET",
            &format!("/api/v1/admin/site-connections/{conn}"),
            None,
        )
        .await;
    let c = data(&got);
    assert_eq!(c["status"], "active", "{got}");
    assert_eq!(c["protocol"], "acg-faka");
    assert_eq!(c["supplier_currency"], "CNY");
    assert_eq!(c["sync_mode"], "full");
    assert_eq!(c["webhook_status"], "unsupported");
    assert_eq!(c["extra"], json!({"currency": "CNY"}));
    let ping = b
        .admin(
            "POST",
            &format!("/api/v1/admin/site-connections/{conn}/ping"),
            None,
        )
        .await;
    assert_eq!(data(&ping)["site_name"], "异次元测试站");
    assert_eq!(data(&ping)["balance"], "100.00");

    // ---- categories + products (buy prices from `item`, ACG-03) -------------------------
    let cats = b
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-categories?connection_id={conn}"),
            None,
        )
        .await;
    let cats = data(&cats).to_string();
    assert!(cats.contains("游戏") && cats.contains("acg-3"), "{cats}");
    let page = b
        .admin(
            "GET",
            &format!("/api/v1/admin/upstream-products?connection_id={conn}&page=1&page_size=2"),
            None,
        )
        .await;
    let p = data(&page);
    assert_eq!(p["total"], 3, "{page}");
    let items = p["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    let game = &items[0];
    assert_eq!(game["id"], 12);
    assert_eq!(game["title"]["zh-CN"], "游戏点卡");
    assert_eq!(game["price_amount"], "8.50");
    assert_eq!(game["fulfillment_type"], "auto");
    assert_eq!(game["images"], json!([]), "placeholder cover dropped");
    let skus = game["skus"].as_array().unwrap();
    assert_eq!(skus.len(), 2);
    assert_eq!(skus[0]["sku_code"], "月卡");
    assert_eq!(
        skus[0]["price_amount"], "8.50",
        "buy price, not retail 10.00"
    );
    assert_eq!(skus[0]["stock_quantity"], 3);
    assert_eq!(skus[1]["price_amount"], "25.00");
    assert_eq!(skus[1]["stock_quantity"], 1);
    assert_eq!(skus[0]["id"].as_i64().unwrap() >> 20, 12);
    assert_eq!(items[1]["skus"][0]["id"], 13 << 20);
    assert_eq!(items[1]["skus"][0]["price_amount"], "5.00");

    // ---- import (auto category) -------------------------------------------------------
    let product = buyer.import(conn, 12).await;
    let local = buyer.skus(product).await;
    assert_eq!(local.len(), 2);
    let month = local.iter().find(|s| s.sku_code == "月卡").unwrap().clone();
    assert_eq!(format!("{:.2}", month.price_amount), "8.50");
    let m = sku_mappings::Entity::find()
        .filter(sku_mappings::Column::LocalSkuId.eq(month.id))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.upstream_stock, 3);
    let row = products::Entity::find_by_id(product)
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert!(
        row.category_id > 0,
        "category created from the supplier tree"
    );
    let schema = serde_json::to_value(&row.manual_form_schema_json).unwrap();
    assert!(schema.to_string().contains("\"account\""), "{schema}");

    // ---- storefront order → trade → query → delivered -------------------------------
    let (shopper, token) = b.user("shopper@buyer.example").await;
    buyer.fund(shopper, "100.00").await;
    let created = b
        .call(
            "POST",
            "/api/v1/orders",
            Some(json!({
                "items": [{"product_id": product, "sku_id": month.id, "quantity": 2}],
                "manual_form_data": {product.to_string(): {"account": "player1"}},
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
    buyer.drain().await; // procurement:submit → trade (cards in the answer)
    let proc = procurement_orders::Entity::find()
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    // The cards returned by `trade` are stored with the trade number and delivered in
    // the same run: no `query` round trip is needed.
    assert_eq!(proc.status, "fulfilled", "{}", proc.error_message);
    let held = procurement_deliveries::Entity::find()
        .filter(procurement_deliveries::Column::ProcurementOrderId.eq(proc.id))
        .one(&b.db)
        .await
        .unwrap()
        .expect("synchronous delivery persisted");
    assert_eq!(held.payload, "M1\nM2");
    assert!(
        mock.lock()
            .unwrap()
            .requests
            .iter()
            .all(|(path, _)| !path.ends_with("/query")),
        "delivered without polling"
    );
    let sent = trades(&mock);
    assert_eq!(sent.len(), 1);
    let t = &sent[0];
    assert_eq!(field(t, "shared_code"), "GAME12");
    assert_eq!(field(t, "race"), "月卡");
    assert_eq!(field(t, "num"), "2");
    assert_eq!(
        field(t, "Account"),
        "player1",
        "widget sent under its own name"
    );
    let request_no = field(t, "request_no");
    assert_eq!(request_no.len(), 19, "ACG-02: fits char(19)");
    // ACG-01: the key never travels, every request carried a valid signature
    assert!(
        mock.lock()
            .unwrap()
            .requests
            .iter()
            .all(|(_, f)| f.iter().all(|(k, _)| k != "app_key"))
    );
    assert_eq!(mock.lock().unwrap().balance_cents, 10_000 - 1_700);
    assert_eq!(proc.upstream_order_no, "583920174628300001");
    assert_eq!(format!("{:.2}", proc.upstream_amount), "17.00");

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
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].payload, "M1\nM2");

    // ---- full sync keeps buy prices and live stock --------------------------------------
    b.services
        .integration
        .mappings
        .sync_connection_now(conn)
        .await
        .unwrap();
    let m = sku_mappings::Entity::find()
        .filter(sku_mappings::Column::LocalSkuId.eq(month.id))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(m.upstream_stock, 1);
    assert_eq!(format!("{:.2}", m.upstream_price), "8.50");
    let mapping = product_mappings::Entity::find()
        .filter(product_mappings::Column::LocalProductId.eq(product))
        .one(&b.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(mapping.upstream_status, "active");
}

#[tokio::test]
async fn acg_faka_error_paths() {
    let (base, mock) = start_mock().await;
    let buyer = site().await;
    let conn = buyer.connect(&base).await;
    let plain = buyer.import(conn, 13).await;
    let plain_sku = buyer.skus(plain).await[0].id;

    // ---- out of stock: permanent, nothing bought ---------------------------------------
    mock.lock()
        .unwrap()
        .cards
        .insert((13, String::new()), Vec::new());
    let p = buyer.procure("L-OOS", plain, plain_sku, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "rejected");
    assert!(
        row.error_message.contains("库存不足"),
        "{}",
        row.error_message
    );
    assert!(mock.lock().unwrap().orders.is_empty());

    // ---- insufficient balance: permanent, message kept ----------------------------------
    {
        let mut shop = mock.lock().unwrap();
        shop.cards
            .insert((13, String::new()), vec!["P1".into(), "P2".into()]);
        shop.balance_cents = 100;
    }
    let p = buyer.procure("L-BAL", plain, plain_sku, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "rejected");
    assert!(
        row.error_message.contains("余额不足"),
        "{}",
        row.error_message
    );
    assert_eq!(mock.lock().unwrap().balance_cents, 100);

    // ---- ACG-02: the trade is committed but its answer is lost --------------------------
    {
        let mut shop = mock.lock().unwrap();
        shop.balance_cents = 10_000;
        shop.drop_after_commit = true;
    }
    let p = buyer.procure("L-LOST", plain, plain_sku, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "failed", "a lost answer is retried later");
    assert!(
        row.error_message.contains("upstream request error"),
        "{}",
        row.error_message
    );
    assert_eq!(mock.lock().unwrap().orders.len(), 1);
    assert_eq!(mock.lock().unwrap().balance_cents, 9_500);
    // the retry reuses the request_no: the supplier refuses it, no second purchase; the
    // purchase may have been charged, so it is held for manual review (no rollback)
    mock.lock().unwrap().drop_after_commit = false;
    buyer.submit(p).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "manual_review", "{}", row.error_message);
    assert!(
        row.error_message.contains("request_no"),
        "{}",
        row.error_message
    );
    assert!(
        row.error_message.contains("manually"),
        "{}",
        row.error_message
    );
    let sent = trades(&mock);
    let lost: Vec<String> = sent
        .iter()
        .filter(|f| field(f, "shared_code") == "PLAIN13")
        .map(|f| field(f, "request_no"))
        .collect();
    assert_eq!(lost.len(), 4, "oos, balance, lost, retry");
    assert_eq!(lost[2], lost[3], "same request_no on retry");
    assert_eq!(mock.lock().unwrap().orders.len(), 1, "never bought twice");
    assert_eq!(mock.lock().unwrap().balance_cents, 9_500);
    // a held purchase is never submitted again by the workers
    buyer.submit(p).await;
    assert_eq!(trades(&mock).len(), sent.len());
    // the admin list surfaces it
    let listed = buyer
        .app
        .admin(
            "GET",
            "/api/v1/admin/procurement-orders?status=manual_review",
            None,
        )
        .await;
    assert_eq!(listed["pagination"]["total"], 1, "{listed}");
    // admin retry: same request_no again, refused again → still held, still one order
    let retried = buyer
        .app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{p}/retry"),
            None,
        )
        .await;
    assert_eq!(retried["status_code"], 0, "{retried}");
    buyer.drain().await;
    assert_eq!(buyer.procurement(p).await.status, "manual_review");
    assert_eq!(mock.lock().unwrap().orders.len(), 1, "never bought twice");
    // admin cancel = mark failed → canceled (local order rolled back)
    let canceled = buyer
        .app
        .admin(
            "POST",
            &format!("/api/v1/admin/procurement-orders/{p}/cancel"),
            None,
        )
        .await;
    assert_eq!(canceled["status_code"], 0, "{canceled}");
    assert_eq!(buyer.procurement(p).await.status, "canceled");

    // ---- ACG-04: manual delivery notices are not deliveries ------------------------------
    let manual = buyer.import(conn, 14).await;
    let manual_sku = buyer.skus(manual).await[0].id;
    let p = buyer.procure("L-MAN", manual, manual_sku, 1).await;
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "accepted", "{}", row.error_message);
    assert_eq!(row.upstream_order_id, 0);
    assert!(!row.upstream_order_no.is_empty());
    buyer
        .app
        .services
        .integration
        .procurement
        .poll(p)
        .await
        .unwrap();
    let row = buyer.procurement(p).await;
    assert_eq!(row.status, "accepted", "waits for a manual check");
    let delivered = |local: i64| {
        let db = buyer.app.db.clone();
        async move {
            fulfillments::Entity::find()
                .filter(fulfillments::Column::OrderId.eq(local))
                .all(&db)
                .await
                .unwrap()
        }
    };
    assert!(delivered(row.local_order_id).await.is_empty());
    // the trade-time notice is kept as the baseline
    let baseline = procurement_deliveries::Entity::find()
        .filter(procurement_deliveries::Column::ProcurementOrderId.eq(p))
        .one(&buyer.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(baseline.payload, "付款后请联系客服 QQ 10000 领取");
    assert_eq!(baseline.status, "unconfirmed");
    let set_secret = |secret: &str| {
        let mut shop = mock.lock().unwrap();
        let order = shop
            .orders
            .iter_mut()
            .find(|o| o.trade_no == row.upstream_order_no)
            .unwrap();
        order.secret = secret.to_owned();
    };
    let procurement = buyer.app.services.integration.procurement.clone();
    // a known pending notice that differs from the baseline is still no delivery
    set_secret("正在发货中，请耐心等待，如有疑问，请联系客服。");
    procurement.poll(p).await.unwrap();
    assert_eq!(buyer.procurement(p).await.status, "accepted");
    assert!(delivered(row.local_order_id).await.is_empty());
    // the seller delivers by hand: `query` now shows the card → delivered exactly once
    set_secret("REAL-CARD-1\r\nREAL-CARD-2");
    procurement.poll(p).await.unwrap();
    assert_eq!(buyer.procurement(p).await.status, "fulfilled");
    procurement.poll(p).await.unwrap();
    procurement.sync_accepted().await.unwrap();
    let f = delivered(row.local_order_id).await;
    assert_eq!(f.len(), 1, "delivered exactly once");
    assert_eq!(f[0].payload, "REAL-CARD-1\nREAL-CARD-2");
}

/// Money safety of transport failures: a trade that may have reached the supplier and
/// never got an answer is held for manual review once its retries are used up (no
/// refund); a supplier that cannot even be connected to is rejected and rolled back.
#[tokio::test]
async fn acg_faka_transport_failures() {
    let (base, mock) = start_mock().await;
    let buyer = site().await;
    let conn = buyer.connect(&base).await;
    let plain = buyer.import(conn, 13).await;
    let plain_sku = buyer.skus(plain).await[0].id;
    let submit_until_settled = |p: i64| {
        let buyer = &buyer;
        async move {
            for _ in 0..10 {
                if buyer.procurement(p).await.status != "failed" {
                    break;
                }
                buyer.submit(p).await;
            }
            buyer.procurement(p).await
        }
    };

    // ---- sent, answer lost on every attempt → retried with the same request_no, then
    // manual review (the local order is not rolled back)
    mock.lock().unwrap().reset_trades = true;
    let p = buyer.procure("L-RESET", plain, plain_sku, 1).await;
    assert_eq!(buyer.procurement(p).await.status, "failed");
    let row = submit_until_settled(p).await;
    assert_eq!(row.status, "manual_review", "{}", row.error_message);
    assert!(
        row.error_message.starts_with("[result unknown]"),
        "{}",
        row.error_message
    );
    let sent: Vec<String> = trades(&mock)
        .iter()
        .map(|f| field(f, "request_no"))
        .collect();
    assert!(sent.len() > 1);
    assert!(sent.iter().all(|r| *r == sent[0]), "same request_no");
    let local = orders::Entity::find_by_id(row.local_order_id)
        .one(&buyer.app.db)
        .await
        .unwrap()
        .unwrap();
    assert_ne!(local.status, "canceled", "not rolled back");
    assert_ne!(local.status, "refunded", "not rolled back");
    mock.lock().unwrap().reset_trades = false;

    // ---- the supplier cannot be reached at all → nothing was sent: rejected + rollback
    let dead = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let dead_url = format!("http://{}", dead.local_addr().unwrap());
    drop(dead);
    let res = buyer
        .app
        .admin(
            "PUT",
            &format!("/api/v1/admin/site-connections/{conn}"),
            Some(json!({"base_url": dead_url})),
        )
        .await;
    data(&res);
    let before = trades(&mock).len();
    let p = buyer.procure("L-DOWN", plain, plain_sku, 1).await;
    let row = submit_until_settled(p).await;
    assert_eq!(row.status, "rejected", "{}", row.error_message);
    assert!(!row.error_message.starts_with("[result unknown]"));
    assert_eq!(trades(&mock).len(), before, "nothing reached the supplier");
}
