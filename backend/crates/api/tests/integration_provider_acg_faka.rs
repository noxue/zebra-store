//! acg-faka (异次元发卡) 共享店铺 provider compat (`/shared/*`): requests are built and
//! signed exactly like the acg-faka PHP client (`app/Service/Bind/Shared.php`:
//! form_params incl. `app_key`, nested `sku[…]`, md5 over ksort'ed fields), against
//! the real router, order group and wallet on in-memory SQLite.
//!
//! Lessons (docs/protocol/third-party/provider-compat.md §7): PRV-01 scalar `app_id` +
//! exact signature comparison, PRV-02 only open (auto-delivery, active) products are
//! addressable, PRV-03 duplicate `request_no` never charges twice, PRV-04 `draftCard`
//! never evaluates filters, PRV-05 zero-priced trades refused, PRV-06 rate limit,
//! PRV-07 disabled / unapproved / re-approved credentials, IP allowlist; found live
//! against acg-faka 3.7.9 (docs/protocol/third-party/interop-report.md): PRV-08
//! localized spec value = one race name, PRV-09 `stock` is a string on replays.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use std::collections::BTreeMap;

use axum::body::Body;
use axum::http::{Request, header};
use chrono::Utc;
use integration_common::{IntApp, data, seed_product};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, IntoActiveModel, QueryFilter, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::{member_levels, product_skus, products, users, wallet_accounts};

// ---------------------------------------------------------------------------
// The acg-faka client, reimplemented
// ---------------------------------------------------------------------------

/// PHP `urlencode` (RFC 1738 `http_build_query` of Guzzle `form_params`).
fn urlencode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(char::from(b)),
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// acg-faka `Str::generateSignature`: ksort on top-level keys, top-level `''` dropped,
/// nested `sku[k]` kept in insertion order, `urldecode(http_build_query(..))`.
fn php_sign(fields: &[(&str, &str)], key: &str) -> String {
    let mut top: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (name, value) in fields {
        if *name == "sign" {
            continue;
        }
        let top_key = name.split('[').next().unwrap().to_owned();
        let nested = name.contains('[');
        if !nested && value.is_empty() {
            top.remove(&top_key);
            continue;
        }
        let entry = top.entry(top_key).or_default();
        if !nested {
            entry.clear();
        }
        entry.push(((*name).to_owned(), (*value).to_owned()));
    }
    let query: Vec<String> = top
        .values()
        .flatten()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    zs_shared::sign::md5_hex(format!("{}&key={key}", query.join("&")).as_bytes())
}

/// `Shared::request`: business fields + `app_id` + `app_key` (in clear, like the
/// PHP client) + `sign`.
fn acg_body(fields: &[(&str, &str)], app_id: &str, app_key: &str, send_key: bool) -> String {
    let mut all: Vec<(&str, &str)> = fields.to_vec();
    all.push(("app_id", app_id));
    if send_key {
        all.push(("app_key", app_key));
    }
    let sign = php_sign(&all, app_key);
    all.iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .chain(std::iter::once(format!("sign={sign}")))
        .collect::<Vec<_>>()
        .join("&")
}

async fn post_raw(app: &IntApp, path: &str, body: String) -> (u16, Value) {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(body))
        .unwrap();
    let (status, _, bytes) = app.send(req).await;
    (
        status.as_u16(),
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

struct Client<'a> {
    app: &'a IntApp,
    app_id: String,
    app_key: String,
}

impl Client<'_> {
    async fn call(&self, endpoint: &str, fields: &[(&str, &str)]) -> Value {
        let body = acg_body(fields, &self.app_id, &self.app_key, true);
        let (status, v) = post_raw(self.app, endpoint, body).await;
        assert_eq!(status, 200, "{endpoint}: {v}");
        v
    }
}

fn ok_data(v: &Value) -> &Value {
    assert_eq!(v["code"], 200, "expected success, got {v}");
    &v["data"]
}

fn fail_msg(v: &Value) -> &str {
    assert_eq!(v["code"], 0, "expected failure, got {v}");
    assert!(v.get("data").is_none(), "{v}");
    v["msg"].as_str().unwrap()
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

async fn fund(app: &IntApp, user_id: i64, amount: &str) {
    let now = Utc::now();
    wallet_accounts::ActiveModel {
        user_id: Set(user_id),
        balance: Set(amount.parse().unwrap()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
}

async fn balance(app: &IntApp, user_id: i64) -> String {
    let row = wallet_accounts::Entity::find()
        .filter(wallet_accounts::Column::UserId.eq(user_id))
        .one(&app.db)
        .await
        .unwrap();
    format!("{:.2}", row.map(|r| r.balance).unwrap_or_default())
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

/// Gives the user a 90 % member level.
async fn member_90(app: &IntApp, user_id: i64) {
    let now = Utc::now();
    let level = member_levels::ActiveModel {
        name_json: Set(Some(json!({"zh-CN": "金牌"}))),
        slug: Set("gold".into()),
        icon: Set(String::new()),
        discount_rate: Set("90".parse().unwrap()),
        recharge_threshold: Set(0.into()),
        spend_threshold: Set(0.into()),
        is_default: Set(false),
        sort_order: Set(10),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
    let mut user = users::Entity::find_by_id(user_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    user.member_level_id = Set(level.id);
    user.update(&app.db).await.unwrap();
}

async fn set_spec(app: &IntApp, sku_id: i64, spec: Value) {
    let mut sku = product_skus::Entity::find_by_id(sku_id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    sku.spec_values_json = Set(Some(spec));
    sku.update(&app.db).await.unwrap();
}

/// A trade exactly as `Shared::trade` sends it for a product without races.
async fn trade(c: &Client<'_>, code: &str, request_no: &str, num: &str) -> Value {
    c.call(
        "/shared/commodity/trade",
        &[
            ("shared_code", code),
            ("contact", "buyer@example.com"),
            ("num", num),
            ("card_id", "0"),
            ("device", "0"),
            ("password", ""),
            ("race", ""),
            ("request_no", request_no),
        ],
    )
    .await
}

async fn issue_key(app: &IntApp, token: &str) -> Value {
    let res = app
        .call(
            "POST",
            "/api/v1/api-credential/compat/issue",
            None,
            Some(token),
        )
        .await;
    data(&res).clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

// The helper reproduces the spec vectors (acg-faka.md §2.4), so every request below is
// signed like the PHP client.
#[test]
fn php_client_signing_matches_spec_vectors() {
    const KEY: &str = "8F3A2C9D1E7B6A54";
    assert_eq!(
        php_sign(&[("app_id", "1024"), ("app_key", KEY)], KEY),
        "07bf581b48cc04485a688ec49f481368"
    );
    assert_eq!(
        php_sign(&[("app_id", "1024")], KEY),
        "1095131eb01a23659cff325ba248290b"
    );
    let v3 = [
        ("shared_code", "ABCDEF1234567890"),
        ("contact", "buyer@example.com"),
        ("num", "2"),
        ("card_id", "0"),
        ("device", "0"),
        ("password", ""),
        ("race", "月卡"),
        ("request_no", "123456789012345678"),
        ("sku[区服]", "亚服"),
        ("sku[版本]", "标准"),
    ];
    let body = acg_body(&v3, "1024", KEY, true);
    assert_eq!(
        body,
        "shared_code=ABCDEF1234567890&contact=buyer%40example.com&num=2&card_id=0&device=0&password=&race=%E6%9C%88%E5%8D%A1&request_no=123456789012345678&sku%5B%E5%8C%BA%E6%9C%8D%5D=%E4%BA%9A%E6%9C%8D&sku%5B%E7%89%88%E6%9C%AC%5D=%E6%A0%87%E5%87%86&app_id=1024&app_key=8F3A2C9D1E7B6A54&sign=f7f8a9ce8e0ed44908af0afacb33c593"
    );
}

#[tokio::test]
async fn acg_faka_downstream_end_to_end() {
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;

    // ---- catalog: auto single SKU, auto with two races, manual, expensive auto -------
    let (p1, p1_skus) = seed_product(&app.db, "p1", "auto", &[("A", "10.00", true)]).await;
    add_cards(&app, p1, p1_skus[0], &["CARD-1", "CARD-2", "CARD-3"]).await;
    let (p2, p2_skus) = seed_product(
        &app.db,
        "p2",
        "auto",
        &[("M", "20.00", true), ("Q", "50.00", true)],
    )
    .await;
    set_spec(&app, p2_skus[0], json!({"时长": "月卡"})).await;
    set_spec(&app, p2_skus[1], json!({"时长": "季卡"})).await;
    add_cards(&app, p2, p2_skus[0], &["M-1"]).await;
    add_cards(&app, p2, p2_skus[1], &["Q-1"]).await;
    let (p3, _) = seed_product(&app.db, "p3", "manual", &[("X", "5.00", true)]).await;
    let (p4, p4_skus) = seed_product(&app.db, "p4", "auto", &[("B", "500.00", true)]).await;
    add_cards(&app, p4, p4_skus[0], &["BIG-1"]).await;

    let (uid, token, _cid, _key, _secret) = app.buyer("acg@buyer.example").await;
    fund(&app, uid, "100.00").await;
    member_90(&app, uid).await;
    let app_id = uid.to_string();

    // ---- before issuing a compat key the app_id is unknown ---------------------------
    let pre = Client {
        app: &app,
        app_id: app_id.clone(),
        app_key: "WHATEVER".into(),
    };
    assert_eq!(
        fail_msg(&pre.call("/shared/authentication/connect", &[]).await),
        "商户ID不存在"
    );

    // ---- personal center: issue + view ------------------------------------------------
    let view = issue_key(&app, &token).await;
    assert_eq!(view["app_id"], app_id);
    assert_eq!(view["is_active"], true);
    assert_eq!(view["protocols"][0]["id"], "acg-faka");
    assert_eq!(view["protocols"][0]["enabled"], true);
    let app_key = view["app_key"].as_str().unwrap().to_owned();
    assert_eq!(app_key.len(), 32);
    let mine = app
        .call("GET", "/api/v1/api-credential/compat", None, Some(&token))
        .await;
    assert_eq!(data(&mine)["app_key"], app_key);
    let c = Client {
        app: &app,
        app_id: app_id.clone(),
        app_key: app_key.clone(),
    };

    // ---- connect ---------------------------------------------------------------------
    let v = c.call("/shared/authentication/connect", &[]).await;
    assert_eq!(v["msg"], "success");
    let d = ok_data(&v);
    assert!(!d["shopName"].as_str().unwrap().is_empty());
    assert_eq!(d["balance"].as_f64(), Some(100.0));
    // faka-bridge style: app_key not posted, only app_id + sign (acg-faka.md §2.3 V2)
    let (_, v) = post_raw(
        &app,
        "/shared/authentication/connect",
        acg_body(&[], &app_id, &app_key, false),
    )
    .await;
    assert_eq!(v["code"], 200, "{v}");

    // ---- items: auto products only, caller price vs retail ---------------------------
    let v = c.call("/shared/commodity/items", &[]).await;
    assert!(
        v.get("msg").is_none(),
        "items has no msg like the original: {v}"
    );
    let tree = ok_data(&v).as_array().unwrap();
    let rows: Vec<&Value> = tree
        .iter()
        .flat_map(|cat| {
            assert_eq!(cat["status"], 1);
            assert_eq!(cat["pid"], 0);
            cat["children"].as_array().unwrap().iter()
        })
        .collect();
    let codes: Vec<&str> = rows.iter().map(|r| r["code"].as_str().unwrap()).collect();
    assert!(codes.contains(&p1.to_string().as_str()));
    assert!(codes.contains(&p2.to_string().as_str()));
    // PRV-02: manual fulfilment cannot be delivered inside `trade` → not offered
    assert!(!codes.contains(&p3.to_string().as_str()), "{codes:?}");
    let row1 = rows.iter().find(|r| r["id"] == p1).unwrap();
    assert_eq!(row1["price"].as_f64(), Some(10.0));
    assert_eq!(row1["user_price"].as_f64(), Some(9.0));
    assert_eq!(row1["stock"], 3);
    assert_eq!(row1["delivery_way"], 0);
    assert_eq!(row1["widget"], "[]");
    assert_eq!(row1["config"], "");
    assert!(row1.get("factory_price").is_none());
    let row2 = rows.iter().find(|r| r["id"] == p2).unwrap();
    assert_eq!(row2["config"], "[category]\n月卡=20.00\n季卡=50.00\n");
    assert_eq!(row2["stock"], 2);

    // ---- item: caller price in factory_price / category_factory ----------------------
    let v = c
        .call(
            "/shared/commodity/item",
            &[("code", &p1.to_string()), ("sharedCode", &p1.to_string())],
        )
        .await;
    let d = ok_data(&v);
    assert_eq!(d["name"], "商品 p1");
    assert_eq!(d["factory_price"].as_f64(), Some(9.0));
    assert_eq!(d["widget"], json!([]));
    let v = c
        .call("/shared/commodity/item", &[("code", &p2.to_string())])
        .await;
    let d = ok_data(&v);
    assert_eq!(d["factory_price"].as_f64(), Some(0.0));
    assert_eq!(
        d["config"]["category"],
        json!({"月卡": "20.00", "季卡": "50.00"})
    );
    assert_eq!(
        d["config"]["category_factory"],
        json!({"月卡": "18.00", "季卡": "45.00"})
    );
    // acg-faka ≤ 3.1.1 sends only sharedCode and expects a tree
    let v = c
        .call("/shared/commodity/item", &[("sharedCode", &p1.to_string())])
        .await;
    assert_eq!(ok_data(&v)[0]["children"][0]["code"], p1.to_string());
    // PRV-02: manual / unknown / malformed codes are refused
    let v = c
        .call("/shared/commodity/item", &[("code", &p3.to_string())])
        .await;
    assert_eq!(fail_msg(&v), "该商品未开放对接");
    for bad in ["999999", "0", "01", "abc"] {
        let v = c.call("/shared/commodity/item", &[("code", bad)]).await;
        assert_eq!(fail_msg(&v), "商品不存在", "{bad}");
    }

    // ---- inventory / stock / inventoryState / valuation ------------------------------
    let v = c
        .call(
            "/shared/commodity/inventory",
            &[("sharedCode", &p2.to_string()), ("race", "季卡")],
        )
        .await;
    let d = ok_data(&v);
    assert_eq!(d["count"], 1);
    assert_eq!(d["is_category"], true);
    assert_eq!(
        d["config"],
        "[category]\n月卡=20.00\n季卡=50.00\n[category_factory]\n月卡=18.00\n季卡=45.00\n"
    );
    let v = c
        .call(
            "/shared/commodity/stock",
            &[("code", &p1.to_string()), ("race", "")],
        )
        .await;
    assert_eq!(ok_data(&v), &json!({"stock": "3"}));
    let v = c
        .call(
            "/shared/commodity/stock",
            &[
                ("code", &p2.to_string()),
                ("race", "月卡"),
                ("sku[区服]", "亚服"),
            ],
        )
        .await;
    assert_eq!(ok_data(&v)["stock"], "1");
    let v = c
        .call(
            "/shared/commodity/inventoryState",
            &[
                ("shared_code", &p1.to_string()),
                ("card_id", "0"),
                ("num", "3"),
                ("race", ""),
            ],
        )
        .await;
    assert_eq!(ok_data(&v), &json!([]));
    let v = c
        .call(
            "/shared/commodity/inventoryState",
            &[
                ("shared_code", &p1.to_string()),
                ("card_id", "0"),
                ("num", "4"),
                ("race", ""),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "库存不足");
    let v = c
        .call(
            "/shared/commodity/valuation",
            &[
                ("code", &p1.to_string()),
                ("num", "2"),
                ("race", ""),
                ("card_id", "0"),
            ],
        )
        .await;
    assert_eq!(
        ok_data(&v),
        &json!({"price": "18.00", "currency_code": "CNY"})
    );
    let v = c
        .call(
            "/shared/commodity/valuation",
            &[("code", &p2.to_string()), ("num", "1"), ("race", "不存在")],
        )
        .await;
    assert_eq!(fail_msg(&v), "商品种类不存在");

    // ---- trade: wallet payment + synchronous card secrets -----------------------------
    let p1s = p1.to_string();
    let v = trade(&c, &p1s, "202609250000000001", "2").await;
    assert_eq!(v["msg"], "success");
    let d = ok_data(&v).clone();
    assert_eq!(d["amount"], "18.00");
    let mut cards: Vec<&str> = d["secret"].as_str().unwrap().split('\n').collect();
    cards.sort_unstable();
    assert_eq!(cards.len(), 2, "{d}");
    assert!(cards.iter().all(|c| c.starts_with("CARD-")));
    assert_eq!(d["stock"], "1");
    let trade_no = d["tradeNo"].as_str().unwrap().to_owned();
    assert!(
        d["url"]
            .as_str()
            .unwrap()
            .ends_with(&format!("/orders/{trade_no}"))
    );
    assert_eq!(balance(&app, uid).await, "82.00");

    // PRV-03 / UPS-10: the same request_no answers the same order, charged once
    let again = trade(&c, &p1s, "202609250000000001", "2").await;
    assert_eq!(ok_data(&again)["tradeNo"], trade_no);
    assert_eq!(ok_data(&again)["secret"], d["secret"]);
    // PRV-09: `stock` stays a string on a replay (was `null`); 1 card left
    assert_eq!(ok_data(&again)["stock"], "1");
    assert_eq!(balance(&app, uid).await, "82.00");

    // query (own order): paid + delivered secrets
    let v = c
        .call("/shared/commodity/query", &[("tradeNo", &trade_no)])
        .await;
    let q = ok_data(&v);
    assert_eq!(q["status"], 1);
    assert_eq!(q["secret"], d["secret"]);
    assert_eq!(q["widget"], Value::Null);
    let v = c
        .call("/shared/commodity/query", &[("tradeNo", "nope")])
        .await;
    assert_eq!(fail_msg(&v), "订单不存在");

    // race trade on the multi-SKU product
    let v = c
        .call(
            "/shared/commodity/trade",
            &[
                ("shared_code", &p2.to_string()),
                ("num", "1"),
                ("race", "季卡"),
                ("request_no", "202609250000000002"),
                ("card_id", "0"),
                ("sku[区服]", "亚服"),
            ],
        )
        .await;
    assert_eq!(ok_data(&v)["secret"], "Q-1");
    assert_eq!(ok_data(&v)["amount"], "45.00");
    assert_eq!(balance(&app, uid).await, "37.00");

    // out of stock (1 card left, 5 asked) — no charge
    let v = trade(&c, &p1s, "202609250000000003", "5").await;
    assert_eq!(fail_msg(&v), "库存不足");
    // insufficient balance (450.00 > 37.00) — no order, no charge
    let v = c
        .call(
            "/shared/commodity/trade",
            &[
                ("shared_code", &p4.to_string()),
                ("num", "1"),
                ("race", ""),
                ("request_no", "202609250000000004"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "余额不足");
    assert_eq!(balance(&app, uid).await, "37.00");
    // PRV-02: manual products cannot be traded
    let v = c
        .call(
            "/shared/commodity/trade",
            &[
                ("shared_code", &p3.to_string()),
                ("num", "1"),
                ("request_no", "202609250000000005"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "该商品未开放对接");
    // pre-selected cards are not supported
    let v = c
        .call(
            "/shared/commodity/trade",
            &[
                ("shared_code", &p1s),
                ("num", "1"),
                ("card_id", "7"),
                ("request_no", "202609250000000006"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "该商品不支持预选卡密");
    let v = c
        .call(
            "/shared/commodity/trade",
            &[("shared_code", &p1s), ("num", "1")],
        )
        .await;
    assert_eq!(fail_msg(&v), "request_no 不能为空");

    // PRV-04: draftCard never evaluates filters (no card secret oracle)
    let v = c
        .call(
            "/shared/commodity/draftCard",
            &[
                ("code", &p1s),
                ("search-secret", "CARD"),
                ("page", "1"),
                ("limit", "10"),
            ],
        )
        .await;
    assert_eq!(ok_data(&v), &json!({"list": [], "total": 0}));
    let v = c
        .call(
            "/shared/commodity/draft",
            &[("code", &p1s), ("card_id", "1")],
        )
        .await;
    assert_eq!(ok_data(&v), &json!({"draft_premium": 0}));

    // another buyer cannot read this order
    let (uid2, token2, _, _, _) = app.buyer("other@buyer.example").await;
    let other = issue_key(&app, &token2).await;
    let c2 = Client {
        app: &app,
        app_id: uid2.to_string(),
        app_key: other["app_key"].as_str().unwrap().to_owned(),
    };
    let v = c2
        .call("/shared/commodity/query", &[("tradeNo", &trade_no)])
        .await;
    assert_eq!(fail_msg(&v), "订单不存在");
    // …and the first buyer's key does not sign for the second buyer
    let forged = Client {
        app: &app,
        app_id: uid2.to_string(),
        app_key: app_key.clone(),
    };
    assert_eq!(
        fail_msg(&forged.call("/shared/authentication/connect", &[]).await),
        "密钥错误"
    );
}

// PRV-01: signature and app_id checks.
#[tokio::test]
async fn prv01_signature_and_app_id_rules() {
    let app = IntApp::new().await;
    let (uid, token, _, _, _) = app.buyer("sig@buyer.example").await;
    let key = issue_key(&app, &token).await["app_key"]
        .as_str()
        .unwrap()
        .to_owned();
    let app_id = uid.to_string();

    // wrong key
    let (_, v) = post_raw(
        &app,
        "/shared/authentication/connect",
        acg_body(&[], &app_id, "WRONGKEY", false),
    )
    .await;
    assert_eq!(fail_msg(&v), "密钥错误");
    // a field changed after signing
    let body = acg_body(&[("num", "1")], &app_id, &key, true).replace("num=1", "num=9");
    let (_, v) = post_raw(&app, "/shared/commodity/draft", body).await;
    assert_eq!(fail_msg(&v), "密钥错误");
    // missing / magic-hash / uppercase signatures
    for sign in ["", "0", "0e462097431906509019562988736854"] {
        let body = format!("app_id={app_id}&sign={sign}");
        let (_, v) = post_raw(&app, "/shared/authentication/connect", body).await;
        assert_eq!(fail_msg(&v), "密钥错误", "{sign}");
    }
    let good = php_sign(&[("app_id", &app_id)], &key);
    let (_, v) = post_raw(
        &app,
        "/shared/authentication/connect",
        format!("app_id={app_id}&sign={}", good.to_uppercase()),
    )
    .await;
    assert_eq!(fail_msg(&v), "密钥错误");
    let (_, v) = post_raw(
        &app,
        "/shared/authentication/connect",
        format!("app_id={app_id}&sign={good}"),
    )
    .await;
    assert_eq!(v["code"], 200, "{v}");
    // array app_id → refused (HTTP 200 JSON, not a 500)
    let (status, v) = post_raw(
        &app,
        "/shared/authentication/connect",
        format!("app_id[]={app_id}&sign={good}"),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(fail_msg(&v), "商户ID不存在");
    // non-canonical ids never alias a user
    for id in [
        format!("0{app_id}"),
        format!("{app_id}.0"),
        String::new(),
        "abc".into(),
    ] {
        let (_, v) = post_raw(
            &app,
            "/shared/authentication/connect",
            acg_body(&[], &id, &key, false),
        )
        .await;
        assert_eq!(fail_msg(&v), "商户ID不存在", "{id}");
    }
    // JSON bodies are not form bodies
    let req = Request::builder()
        .method("POST")
        .uri("/shared/authentication/connect")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({"app_id": app_id, "sign": good}).to_string(),
        ))
        .unwrap();
    let (_, _, bytes) = app.send(req).await;
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["code"], 0);
}

// PRV-07: credential / key state gates every call; IP allowlist.
#[tokio::test]
async fn prv07_credential_and_key_state() {
    let app = IntApp::new().await;
    let (uid, token, cid, _, _) = app.buyer("state@buyer.example").await;
    let key = issue_key(&app, &token).await["app_key"]
        .as_str()
        .unwrap()
        .to_owned();
    let c = Client {
        app: &app,
        app_id: uid.to_string(),
        app_key: key,
    };
    assert_eq!(
        c.call("/shared/authentication/connect", &[]).await["code"],
        200
    );

    // owner switch
    let res = app
        .call(
            "PUT",
            "/api/v1/api-credential/compat",
            Some(json!({"is_active": false})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&res)["is_active"], false);
    assert_eq!(
        fail_msg(&c.call("/shared/authentication/connect", &[]).await),
        "对接已关闭，请在供货站个人中心开启"
    );
    app.call(
        "PUT",
        "/api/v1/api-credential/compat",
        Some(json!({"is_active": true})),
        Some(&token),
    )
    .await;

    // IP allowlist (the in-process client has no address → refused)
    let res = app
        .call(
            "PUT",
            "/api/v1/api-credential/compat",
            Some(json!({"ip_allowlist": "203.0.113.0/24, 198.51.100.7"})),
            Some(&token),
        )
        .await;
    assert_eq!(data(&res)["ip_allowlist"], "203.0.113.0/24,198.51.100.7");
    assert_eq!(
        fail_msg(&c.call("/shared/authentication/connect", &[]).await),
        "当前IP不在对接白名单内"
    );
    let res = app
        .call(
            "PUT",
            "/api/v1/api-credential/compat",
            Some(json!({"ip_allowlist": "not-an-ip"})),
            Some(&token),
        )
        .await;
    assert_ne!(res["status_code"], 0);
    app.call(
        "PUT",
        "/api/v1/api-credential/compat",
        Some(json!({"ip_allowlist": ""})),
        Some(&token),
    )
    .await;
    assert_eq!(
        c.call("/shared/authentication/connect", &[]).await["code"],
        200
    );

    // credential disabled by the admin
    let res = app
        .admin(
            "PUT",
            &format!("/api/v1/admin/api-credentials/{cid}/status"),
            Some(json!({"is_active": false})),
        )
        .await;
    data(&res);
    assert_eq!(
        fail_msg(&c.call("/shared/authentication/connect", &[]).await),
        "商户ID不存在"
    );
    app.admin(
        "PUT",
        &format!("/api/v1/admin/api-credentials/{cid}/status"),
        Some(json!({"is_active": true})),
    )
    .await;
    assert_eq!(
        c.call("/shared/authentication/connect", &[]).await["code"],
        200
    );

    // user disabled
    let mut user = users::Entity::find_by_id(uid)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    user.status = Set("disabled".into());
    user.update(&app.db).await.unwrap();
    assert_eq!(
        fail_msg(&c.call("/shared/authentication/connect", &[]).await),
        "商户ID不存在"
    );
    let mut user = users::Entity::find_by_id(uid)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    user.status = Set("active".into());
    user.update(&app.db).await.unwrap();

    // re-approval issues a new api_key → the compat key is void until re-issued
    data(
        &app.admin(
            "POST",
            &format!("/api/v1/admin/api-credentials/{cid}/approve"),
            None,
        )
        .await,
    );
    assert_eq!(
        fail_msg(&c.call("/shared/authentication/connect", &[]).await),
        "商户ID不存在"
    );
    let mine = app
        .call("GET", "/api/v1/api-credential/compat", None, Some(&token))
        .await;
    assert_eq!(data(&mine)["app_key"], "");

    // unapproved (pending) users cannot issue a compat key (UPS-18)
    let (_, pending_token) = app.user("pending@buyer.example").await;
    app.call(
        "POST",
        "/api/v1/api-credential/apply",
        None,
        Some(&pending_token),
    )
    .await;
    let res = app
        .call(
            "POST",
            "/api/v1/api-credential/compat/issue",
            None,
            Some(&pending_token),
        )
        .await;
    assert_ne!(res["status_code"], 0, "{res}");
    let res = app
        .call("GET", "/api/v1/api-credential/compat", None, None)
        .await;
    assert_ne!(res["status_code"], 0, "{res}");
}

// PRV-05: a product priced at zero is never handed out.
#[tokio::test]
async fn prv05_zero_price_refused() {
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;
    let (p, skus) = seed_product(&app.db, "free", "auto", &[("F", "0.00", true)]).await;
    add_cards(&app, p, skus[0], &["FREE-1"]).await;
    let mut row = products::Entity::find_by_id(p)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
        .into_active_model();
    row.price_amount = Set(0.into());
    row.update(&app.db).await.unwrap();
    let (uid, token, _, _, _) = app.buyer("free@buyer.example").await;
    fund(&app, uid, "10.00").await;
    let key = issue_key(&app, &token).await["app_key"]
        .as_str()
        .unwrap()
        .to_owned();
    let c = Client {
        app: &app,
        app_id: uid.to_string(),
        app_key: key,
    };
    let v = c
        .call(
            "/shared/commodity/valuation",
            &[("code", &p.to_string()), ("num", "1")],
        )
        .await;
    assert_eq!(fail_msg(&v), "商品价格异常，暂停对接");
    let v = c
        .call(
            "/shared/commodity/trade",
            &[
                ("shared_code", &p.to_string()),
                ("num", "1"),
                ("request_no", "FREE-REQ-1"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "商品价格异常，暂停对接");
    assert_eq!(balance(&app, uid).await, "10.00");
}

// PRV-06: per-`IP|app_id` rate limit answers a JSON failure.
#[tokio::test]
async fn prv06_rate_limited() {
    let app = IntApp::new().await;
    let body = "app_id=424242&sign=x".to_owned();
    let mut limited = false;
    for _ in 0..=zs_domain::integration::provide::COMPAT_REQUESTS_PER_MINUTE {
        let (status, v) = post_raw(&app, "/shared/commodity/draft", body.clone()).await;
        assert_eq!(status, 200);
        if v["msg"] == "请求过于频繁，请稍后再试" {
            limited = true;
            break;
        }
        assert_eq!(v["msg"], "商户ID不存在");
    }
    assert!(limited);
}

// `integration.acg_faka_compat = false`: the paths do not exist.
#[tokio::test]
async fn switch_off_answers_404() {
    let app = IntApp::new().await;
    let mut cfg = integration_common::config();
    cfg.integration.acg_faka_compat = false;
    let router = zs_api::build(zs_api::AppState::new(app.services.clone(), cfg)).router;
    let req = Request::builder()
        .method("POST")
        .uri("/shared/authentication/connect")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from("app_id=1&sign=x"))
        .unwrap();
    use tower::ServiceExt;
    let res = router.oneshot(req).await.unwrap();
    assert_eq!(res.status().as_u16(), 404);
    // mcy stays on
    let (status, _) = post_raw(&app, "/plugin/open-api/connect", String::new()).await;
    assert_eq!(status, 200);
}

// PRV-08 (found live against acg-faka 3.7.9): SKU spec values stored as ONE localized
// value (`{"zh-CN","zh-TW","en-US"}`, what the admin form and seed data write) became
// the race `空月祝福 / 空月祝福 / Welkin`. The race is the zh-CN text, and trading
// with it works.
#[tokio::test]
async fn prv08_localized_spec_value_is_one_race() {
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;
    let (p, skus) = seed_product(
        &app.db,
        "genshin",
        "auto",
        &[("month", "30.00", true), ("bp", "68.00", true)],
    )
    .await;
    set_spec(
        &app,
        skus[0],
        json!({"zh-CN": "空月祝福", "zh-TW": "空月祝福", "en-US": "Welkin"}),
    )
    .await;
    set_spec(
        &app,
        skus[1],
        json!({"zh-CN": "纪行 1.5", "zh-TW": "紀行 1.5", "en-US": "BP 1.5"}),
    )
    .await;
    add_cards(&app, p, skus[0], &["W-1"]).await;
    add_cards(&app, p, skus[1], &["B-1"]).await;
    let (uid, token, _, _, _) = app.buyer("prv08@buyer.example").await;
    fund(&app, uid, "100.00").await;
    let key = issue_key(&app, &token).await["app_key"]
        .as_str()
        .unwrap()
        .to_owned();
    let c = Client {
        app: &app,
        app_id: uid.to_string(),
        app_key: key,
    };
    let code = p.to_string();

    let v = c.call("/shared/commodity/items", &[]).await;
    let row = ok_data(&v)
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|cat| cat["children"].as_array().unwrap().iter())
        .find(|r| r["code"] == code.as_str())
        .unwrap()
        .clone();
    // `.` is an INI level separator on the acg-faka side → `_`
    assert_eq!(
        row["config"],
        "[category]\n空月祝福=30.00\n纪行 1_5=68.00\n"
    );
    let v = c.call("/shared/commodity/item", &[("code", &code)]).await;
    assert_eq!(
        ok_data(&v)["config"]["category_factory"],
        json!({"空月祝福": "30.00", "纪行 1_5": "68.00"})
    );

    let v = c
        .call(
            "/shared/commodity/trade",
            &[
                ("shared_code", &code),
                ("num", "1"),
                ("race", "空月祝福"),
                ("request_no", "PRV08-1"),
            ],
        )
        .await;
    assert_eq!(ok_data(&v)["secret"], "W-1");
    assert_eq!(balance(&app, uid).await, "70.00");
}
