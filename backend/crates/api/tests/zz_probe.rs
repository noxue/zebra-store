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

#![allow(clippy::unwrap_used, dead_code, unused_imports)]

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

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn zz_probe_connect_timing() {
    let app = IntApp::new().await;
    app.acknowledge_compliance().await;
    let (uid, token, _cid, _key, _secret) = app.buyer("acg@buyer.example").await;
    let key = issue_key(&app, &token).await;
    let app_key = key["app_key"].as_str().unwrap().to_owned();
    let c = Client {
        app: &app,
        app_id: uid.to_string(),
        app_key,
    };
    for _ in 0..20 {
        let v = c.call("/shared/authentication/connect", &[]).await;
        assert_eq!(v["code"], 200, "{v}");
    }
    for _ in 0..20 {
        let _ = app.call("GET", "/api/v1/public/config", None, None).await;
    }
}
