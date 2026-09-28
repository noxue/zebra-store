//! Helpers shared by the `catalog_*` and `marketing_*` integration tests.

#![allow(dead_code, reason = "each test binary uses a different subset")]
#![expect(clippy::unwrap_used, reason = "test harness")]

use std::str::FromStr;

use axum::body::Body;
use axum::http::{HeaderMap, Request, header};
use chrono::Utc;
use http_body_util::BodyExt;
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, Set};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_infra::db::entity::{card_secrets, order_items, payment_channels, users};

use crate::common::{TestApp, data};

pub fn dec(v: &str) -> Decimal {
    Decimal::from_str(v).unwrap()
}

/// Raw response: status, headers and body text.
pub struct Raw {
    pub headers: HeaderMap,
    pub body: String,
}

impl Raw {
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap_or(Value::Null)
    }
}

/// Sends a JSON request and returns the raw response (for downloads).
pub async fn raw_json(app: &TestApp, method: &str, uri: &str, body: Value) -> Raw {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", app.admin_token.as_deref().unwrap()),
        )
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    send(app, req).await
}

/// Sends a GET with the admin token and returns the raw response.
pub async fn raw_get(app: &TestApp, uri: &str) -> Raw {
    let req = Request::builder()
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", app.admin_token.as_deref().unwrap()),
        )
        .body(Body::empty())
        .unwrap();
    send(app, req).await
}

async fn send(app: &TestApp, req: Request<Body>) -> Raw {
    let res = app.router.clone().oneshot(req).await.unwrap();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Raw {
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// Multipart POST: text fields plus an optional `file` part.
pub async fn multipart(
    app: &TestApp,
    uri: &str,
    fields: &[(&str, &str)],
    file: Option<&str>,
) -> Value {
    let boundary = "zsboundary7MA4YWxkTrZu0gW";
    let mut body = String::new();
    for (k, v) in fields {
        body.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n"
        ));
    }
    if let Some(content) = file {
        body.push_str(&format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"secrets.csv\"\r\nContent-Type: text/csv\r\n\r\n{content}\r\n"
        ));
    }
    body.push_str(&format!("--{boundary}--\r\n"));
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", app.admin_token.as_deref().unwrap()),
        )
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    send(app, req).await.json()
}

/// Creates a category through the API; returns its id.
pub async fn category(app: &TestApp, slug: &str, parent_id: i64) -> i64 {
    let res = app
        .post(
            "/api/v1/admin/categories",
            json!({"name": {"zh-CN": slug}, "slug": slug, "parent_id": parent_id}),
        )
        .await;
    data(&res)["id"].as_i64().unwrap()
}

/// Creates a product through the API from a JSON body merged over sensible defaults.
pub async fn product(app: &TestApp, category_id: i64, slug: &str, extra: Value) -> Value {
    let mut body = json!({
        "category_id": category_id,
        "slug": slug,
        "title": {"zh-CN": format!("商品 {slug}"), "en-US": format!("Product {slug}")},
        "price_amount": 10,
        "fulfillment_type": "manual",
        "manual_stock_total": 5,
        "is_active": true,
    });
    if let (Some(base), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
        for (k, v) in extra {
            base.insert(k.clone(), v.clone());
        }
    }
    let res = app.post("/api/v1/admin/products", body).await;
    data(&res).clone()
}

pub fn id_of(v: &Value) -> i64 {
    v["id"].as_i64().unwrap()
}

/// Inserts card secrets directly.
pub async fn insert_secrets(app: &TestApp, product_id: i64, sku_id: i64, status: &str, n: usize) {
    let now = Utc::now();
    for i in 0..n {
        card_secrets::ActiveModel {
            product_id: Set(product_id),
            sku_id: Set(sku_id),
            batch_id: Set(None),
            secret: Set(format!(
                "S-{product_id}-{sku_id}-{status}-{i}-{}",
                uuid::Uuid::new_v4()
            )),
            status: Set(status.to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&app.db)
        .await
        .unwrap();
    }
}

pub async fn count_secrets(app: &TestApp, product_id: i64, status: &str) -> u64 {
    card_secrets::Entity::find()
        .filter(card_secrets::Column::ProductId.eq(product_id))
        .filter(card_secrets::Column::Status.eq(status))
        .filter(card_secrets::Column::DeletedAt.is_null())
        .count(&app.db)
        .await
        .unwrap()
}

/// Inserts a payment channel row.
pub async fn payment_channel(app: &TestApp, active: bool) -> i64 {
    let now = Utc::now();
    payment_channels::ActiveModel {
        name: Set("ch".into()),
        icon: Set(String::new()),
        provider_type: Set("epay".into()),
        channel_type: Set("alipay".into()),
        interaction_mode: Set("redirect".into()),
        fee_rate: Set(dec("0")),
        fixed_fee: Set(dec("0")),
        min_amount: Set(dec("0")),
        max_amount: Set(dec("0")),
        hide_amount_out_range: Set(false),
        is_active: Set(active),
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

/// Inserts an order item referencing a product.
pub async fn order_item(app: &TestApp, product_id: i64) {
    let now = Utc::now();
    order_items::ActiveModel {
        order_id: Set(1),
        product_id: Set(product_id),
        sku_id: Set(0),
        original_unit_price: Set(dec("1")),
        unit_price: Set(dec("1")),
        cost_price: Set(dec("0")),
        quantity: Set(1),
        original_total_price: Set(dec("1")),
        total_price: Set(dec("1")),
        coupon_discount: Set(dec("0")),
        member_discount: Set(dec("0")),
        promotion_discount: Set(dec("0")),
        wholesale_discount: Set(dec("0")),
        fulfillment_type: Set("manual".into()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap();
}

/// Inserts a user; returns its id.
pub async fn user(app: &TestApp, email: &str, level: i64) -> i64 {
    let now = Utc::now();
    users::ActiveModel {
        email: Set(email.into()),
        password_hash: Set(String::new()),
        password_setup_required: Set(false),
        display_name: Set(email.split('@').next().unwrap_or_default().into()),
        locale: Set("zh-CN".into()),
        status: Set("active".into()),
        member_level_id: Set(level),
        total_recharged: Set(dec("0")),
        total_spent: Set(dec("0")),
        admin_note: Set(String::new()),
        token_version: Set(0),
        totp_secret: Set(String::new()),
        totp_pending_secret: Set(String::new()),
        recovery_codes: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(&app.db)
    .await
    .unwrap()
    .id
}

pub async fn user_row(app: &TestApp, id: i64) -> users::Model {
    users::Entity::find_by_id(id)
        .one(&app.db)
        .await
        .unwrap()
        .unwrap()
}
