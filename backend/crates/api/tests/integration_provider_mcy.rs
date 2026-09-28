//! mcy-shop OpenApi plugin provider compat (`/plugin/open-api/*`), called exactly like
//! acg-faka's "萌次元(V4.0)" client (`Shared::mcyRequest`: form body of business fields
//! only, `Api-Id` + `Api-Signature` headers, `trade_no = md5(order no)[0..24]`) and
//! gmshop-edge (re-sends `trade` with the same `trade_no` to reconcile).
//!
//! Lessons: PRV-01 exact signatures, PRV-02 only auto-delivery products, PRV-03 a
//! repeated `trade_no` answers the original contents without a second charge, PRV-07
//! credential / key state.

#![expect(clippy::unwrap_used, reason = "integration tests")]

mod integration_common;

use std::collections::BTreeMap;

use axum::body::Body;
use axum::http::{Request, header};
use chrono::Utc;
use integration_common::{IntApp, data, seed_product};
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use zs_infra::db::entity::wallet_accounts;

/// PHP `urlencode`.
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

/// `Str::generateSignature` over flat business fields (ksort, `''` dropped).
fn mcy_sign(fields: &[(&str, &str)], key: &str) -> String {
    let sorted: BTreeMap<&str, &str> = fields
        .iter()
        .copied()
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let query: Vec<String> = sorted.iter().map(|(k, v)| format!("{k}={v}")).collect();
    zs_shared::sign::md5_hex(format!("{}&key={key}", query.join("&")).as_bytes())
}

fn form(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(k, v)| format!("{}={}", urlencode(k), urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

async fn post(app: &IntApp, path: &str, api_id: &str, signature: &str, body: String) -> Value {
    let req = Request::builder()
        .method("POST")
        .uri(path)
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header("Api-Id", api_id)
        .header("Api-Signature", signature)
        .body(Body::from(body))
        .unwrap();
    let (status, _, bytes) = app.send(req).await;
    assert_eq!(status.as_u16(), 200);
    serde_json::from_slice(&bytes).unwrap()
}

struct Client<'a> {
    app: &'a IntApp,
    api_id: String,
    app_key: String,
}

impl Client<'_> {
    async fn call(&self, endpoint: &str, fields: &[(&str, &str)]) -> Value {
        let sig = mcy_sign(fields, &self.app_key);
        post(
            self.app,
            &format!("/plugin/open-api/{endpoint}"),
            &self.api_id,
            &sig,
            form(fields),
        )
        .await
    }
}

fn ok_data(v: &Value) -> &Value {
    assert_eq!(v["code"], 200, "expected success, got {v}");
    assert_eq!(v["msg"], "success");
    &v["data"]
}

fn fail_msg(v: &Value) -> &str {
    assert_eq!(v["code"], 0, "expected failure, got {v}");
    v["msg"].as_str().unwrap()
}

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

/// `substr(md5($requestNo), 0, 24)` of the acg-faka client.
fn trade_no(order_no: &str) -> String {
    zs_shared::sign::md5_hex(order_no.as_bytes())[..24].to_owned()
}

// mcy-shop.md §2.4 vector.
#[test]
fn client_signing_matches_spec_vector() {
    assert_eq!(trade_no("123456789012345678"), "9efebb3d7d059bff092842bf");
    let fields = [
        ("sku_id", "17"),
        ("quantity", "1"),
        ("trade_no", "9efebb3d7d059bff092842bf"),
        ("account", "a b+c@x"),
    ];
    assert_eq!(
        mcy_sign(&fields, "8F3A2C9D1E7B6A54"),
        "2f3078747abcffb750084230a4d60019"
    );
    assert_eq!(
        form(&fields),
        "sku_id=17&quantity=1&trade_no=9efebb3d7d059bff092842bf&account=a+b%2Bc%40x"
    );
}

#[tokio::test]
async fn mcy_open_api_end_to_end() {
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;
    let (p1, p1_skus) = seed_product(&app.db, "m1", "auto", &[("A", "8.50", true)]).await;
    add_cards(&app, p1, p1_skus[0], &["MC-1", "MC-2"]).await;
    let (p2, p2_skus) = seed_product(&app.db, "m2", "manual", &[("X", "3.00", true)]).await;
    let (p3, p3_skus) = seed_product(&app.db, "m3", "auto", &[("B", "300.00", true)]).await;
    add_cards(&app, p3, p3_skus[0], &["BIG"]).await;
    let (uid, token, _, _, _) = app.buyer("mcy@buyer.example").await;
    fund(&app, uid, "20.00").await;
    let api_id = uid.to_string();

    // no compat key yet
    let pre = Client {
        app: &app,
        api_id: api_id.clone(),
        app_key: "X".into(),
    };
    assert_eq!(
        fail_msg(&pre.call("connect", &[]).await),
        "API-ID不存在或未开通对接"
    );

    let view = app
        .call(
            "POST",
            "/api/v1/api-credential/compat/issue",
            None,
            Some(&token),
        )
        .await;
    let app_key = data(&view)["app_key"].as_str().unwrap().to_owned();
    assert_eq!(data(&view)["protocols"][1]["id"], "mcy-open-api");
    let c = Client {
        app: &app,
        api_id: api_id.clone(),
        app_key: app_key.clone(),
    };

    // connect: signature over no fields = md5("&key=<app_key>")
    let d = ok_data(&c.call("connect", &[]).await).clone();
    assert!(!d["username"].as_str().unwrap().is_empty());
    assert_eq!(d["balance"], "20.00");

    // items: auto products only, stock_price = caller price
    let v = c.call("items", &[]).await;
    let items = ok_data(&v).as_array().unwrap().clone();
    let ids: Vec<i64> = items.iter().map(|i| i["id"].as_i64().unwrap()).collect();
    assert!(ids.contains(&p1) && ids.contains(&p3));
    assert!(
        !ids.contains(&p2),
        "PRV-02: manual products are not offered: {ids:?}"
    );
    let item = items.iter().find(|i| i["id"] == p1).unwrap();
    assert_eq!(item["name"], "商品 m1");
    assert_eq!(item["category"], json!({"name": "分类"}));
    assert_eq!(item["widget"], "[]");
    assert_eq!(
        item["sku"],
        json!([{"id": p1_skus[0], "name": "A", "stock_price": "8.50", "stock": 2}])
    );

    // item / stock / state / amount
    let v = c.call("item", &[("id", &p1.to_string())]).await;
    assert_eq!(ok_data(&v)["sku"][0]["id"], p1_skus[0]);
    let v = c.call("item", &[("id", &p2.to_string())]).await;
    assert_eq!(fail_msg(&v), "商品未开放对接");
    let sku = p1_skus[0].to_string();
    let v = c.call("sku/stock", &[("sku_id", &sku)]).await;
    assert_eq!(ok_data(&v), &json!({"stock": 2}));
    let v = c
        .call("sku/state", &[("sku_id", &sku), ("quantity", "2")])
        .await;
    assert_eq!(ok_data(&v), &json!({"state": true}));
    let v = c
        .call("sku/state", &[("sku_id", &sku), ("quantity", "3")])
        .await;
    assert_eq!(ok_data(&v), &json!({"state": false}));
    let v = c
        .call(
            "sku/state",
            &[("sku_id", &p2_skus[0].to_string()), ("quantity", "1")],
        )
        .await;
    assert_eq!(ok_data(&v), &json!({"state": false}));
    let v = c
        .call("amount", &[("sku_id", &sku), ("quantity", "2")])
        .await;
    assert_eq!(ok_data(&v), &json!({"amount": "17.00"}));

    // trade like acg-faka's 萌次元 client: widget values posted as extra fields
    let tn = trade_no("202609250000000777");
    let fields = [
        ("sku_id", sku.as_str()),
        ("quantity", "1"),
        ("trade_no", tn.as_str()),
        ("account", "a b+c@x"),
    ];
    let v = c.call("trade", &fields).await;
    let d = ok_data(&v).clone();
    assert!(d["contents"].as_str().unwrap().starts_with("MC-"), "{d}");
    assert_eq!(d["amount"], "8.50");
    assert_eq!(d["trade_no"], tn);
    assert_eq!(balance(&app, uid).await, "11.50");

    // PRV-03: gmshop-edge reconciliation re-sends the same trade_no
    let v = c.call("trade", &fields).await;
    assert_eq!(ok_data(&v)["contents"], d["contents"]);
    assert_eq!(ok_data(&v)["order_no"], d["order_no"]);
    assert_eq!(balance(&app, uid).await, "11.50");

    // out of stock, insufficient balance, manual product, bad input
    let v = c
        .call(
            "trade",
            &[("sku_id", &sku), ("quantity", "5"), ("trade_no", "T-OOS")],
        )
        .await;
    assert_eq!(fail_msg(&v), "库存不足");
    let v = c
        .call(
            "trade",
            &[
                ("sku_id", &p3_skus[0].to_string()),
                ("quantity", "1"),
                ("trade_no", "T-BAL"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "余额不足");
    assert_eq!(balance(&app, uid).await, "11.50");
    let v = c
        .call(
            "trade",
            &[
                ("sku_id", &p2_skus[0].to_string()),
                ("quantity", "1"),
                ("trade_no", "T-MAN"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "商品未开放对接");
    let v = c
        .call("trade", &[("sku_id", &sku), ("quantity", "1")])
        .await;
    assert_eq!(fail_msg(&v), "trade_no 不能为空");
    let v = c
        .call(
            "trade",
            &[
                ("sku_id", "999999"),
                ("quantity", "1"),
                ("trade_no", "T-404"),
            ],
        )
        .await;
    assert_eq!(fail_msg(&v), "商品不存在");

    // PRV-01: signatures
    let v = post(&app, "/plugin/open-api/connect", &api_id, "", String::new()).await;
    assert_eq!(fail_msg(&v), "签名错误");
    let v = post(
        &app,
        "/plugin/open-api/connect",
        &api_id,
        "0e462097431906509019562988736854",
        String::new(),
    )
    .await;
    assert_eq!(fail_msg(&v), "签名错误");
    let sig = mcy_sign(&[("id", &p1.to_string())], &app_key);
    let v = post(
        &app,
        "/plugin/open-api/item",
        &api_id,
        &sig,
        format!("id={p3}"),
    )
    .await;
    assert_eq!(fail_msg(&v), "签名错误", "a field changed after signing");
    let v = post(
        &app,
        "/plugin/open-api/connect",
        "",
        &mcy_sign(&[], &app_key),
        String::new(),
    )
    .await;
    assert_eq!(fail_msg(&v), "API-ID不存在或未开通对接");
    // arrays are not signed by mcy (`is_array` → unset)
    let sig = mcy_sign(&[("id", &p1.to_string())], &app_key);
    let v = post(
        &app,
        "/plugin/open-api/item",
        &api_id,
        &sig,
        format!("id={p1}&extra[a]=1"),
    )
    .await;
    assert_eq!(ok_data(&v)["id"], p1);

    // PRV-07: owner switch
    let res = app
        .call(
            "PUT",
            "/api/v1/api-credential/compat",
            Some(json!({"is_active": false})),
            Some(&token),
        )
        .await;
    data(&res);
    assert_eq!(
        fail_msg(&c.call("connect", &[]).await),
        "对接已关闭，请在供货站个人中心开启"
    );
    // re-issuing re-enables with a new key; the old key no longer signs
    let view = app
        .call(
            "POST",
            "/api/v1/api-credential/compat/issue",
            None,
            Some(&token),
        )
        .await;
    assert_eq!(fail_msg(&c.call("connect", &[]).await), "签名错误");
    let fresh = Client {
        app: &app,
        api_id,
        app_key: data(&view)["app_key"].as_str().unwrap().to_owned(),
    };
    assert_eq!(fresh.call("connect", &[]).await["code"], 200);
}
