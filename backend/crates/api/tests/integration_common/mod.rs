//! Harness of the `integration_*` tests: the real router on in-memory SQLite with the
//! integration adapters pointed at a local mock supplier (loopback allowed only through
//! the explicit test adapter `AddressPolicy::AllowPrivate`), fake order-group ports,
//! a signed client for our own upstream API and seeding helpers.

#![allow(
    dead_code,
    reason = "each integration test binary uses a different subset"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use chrono::Utc;
use http_body_util::BodyExt;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel, NotSet};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_app::config::Config;
use zs_domain::integration::hooks::{ProcurementLifecycle, UpstreamDelivery};
use zs_domain::integration::supplier::{
    PlaceOutcome, PlaceUpstreamOrder, UpstreamOrderError, UpstreamOrderSummary, UpstreamOrderView,
    UpstreamOrdering,
};
use zs_domain::{Error, Id};
use zs_infra::db::entity::{categories, order_items, orders, product_skus, products};
use zs_infra::integration::http::AddressPolicy;
use zs_infra::wire::integration::Adapters;
use zs_shared::sign;

/// Credentials the mock supplier expects from us.
pub const SUP_KEY: &str = "sup-key";
pub const SUP_SECRET: &str = "sup-secret";

/// Smallest valid GIF (1×1) served as a supplier image.
pub const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x01\x00\x00\x00\x00,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

// ---------------------------------------------------------------------------
// Fake order-group ports
// ---------------------------------------------------------------------------

/// Records procurement side effects; `fail_delivery` simulates a fulfillment write error.
#[derive(Default)]
pub struct FakeLifecycle {
    pub calls: Mutex<Vec<String>>,
    pub fail_delivery: Mutex<bool>,
    pub deliveries: Mutex<Vec<(Id, UpstreamDelivery)>>,
}

impl FakeLifecycle {
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

#[async_trait]
impl ProcurementLifecycle for FakeLifecycle {
    async fn mark_fulfilling(&self, order_id: Id) -> zs_domain::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("fulfilling:{order_id}"));
        Ok(())
    }

    async fn deliver_upstream(&self, order_id: Id, d: &UpstreamDelivery) -> zs_domain::Result<()> {
        if *self.fail_delivery.lock().unwrap() {
            self.calls
                .lock()
                .unwrap()
                .push(format!("deliver_failed:{order_id}"));
            return Err(Error::internal_msg("fulfillment write failed"));
        }
        self.calls
            .lock()
            .unwrap()
            .push(format!("deliver:{order_id}"));
        self.deliveries.lock().unwrap().push((order_id, d.clone()));
        Ok(())
    }

    async fn rollback_failed(&self, order_id: Id) -> zs_domain::Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("rollback:{order_id}"));
        Ok(())
    }
}

/// Minimal order-group stand-in for `POST /upstream/orders`: writes an order row and
/// the downstream reference through `insert_ref_in` in one transaction.
pub struct FakeOrdering {
    pub db: sea_orm::DatabaseConnection,
    /// When set, the wallet payment "fails".
    pub fail_payment: Mutex<bool>,
    pub placed: Mutex<u32>,
}

#[async_trait]
impl UpstreamOrdering for FakeOrdering {
    async fn place(&self, req: &PlaceUpstreamOrder) -> Result<PlaceOutcome, UpstreamOrderError> {
        use sea_orm::TransactionTrait;
        let txn = self.db.begin().await.map_err(Error::internal)?;
        let now = Utc::now();
        let order_no = format!("DN{}", now.timestamp_nanos_opt().unwrap_or_default());
        let order = seed_order_row(&txn, &order_no, req.user_id, "paid", "10.00", None).await;
        let r = zs_infra::db::repo::integration::downstream::insert_ref_in(
            &txn,
            &zs_domain::integration::downstream::NewOrderRef {
                order_id: order,
                api_credential_id: req.credential_id,
                downstream_order_no: req.downstream_order_no.clone(),
                callback_url: req.callback_url.clone(),
                trace_id: req.trace_id.clone(),
            },
            now,
        )
        .await;
        if let Err(e) = r {
            if e.key() == zs_infra::db::repo::integration::downstream::DUPLICATE_KEY {
                return Err(UpstreamOrderError::DuplicateDownstreamNo);
            }
            return Err(UpstreamOrderError::Internal(e));
        }
        txn.commit().await.map_err(Error::internal)?;
        *self.placed.lock().unwrap() += 1;
        if *self.fail_payment.lock().unwrap() {
            return Ok(PlaceOutcome::PaymentFailed {
                order_id: order,
                order_no,
                message: "insufficient balance".into(),
            });
        }
        Ok(PlaceOutcome::Placed(UpstreamOrderSummary {
            order_id: order,
            order_no,
            status: "paid".into(),
            amount: "10.00".parse().unwrap(),
            currency: "CNY".into(),
        }))
    }

    async fn get(&self, user_id: Id, order_id: Id) -> zs_domain::Result<Option<UpstreamOrderView>> {
        let row = orders::Entity::find_by_id(order_id)
            .one(&self.db)
            .await
            .map_err(Error::internal)?;
        Ok(row
            .filter(|o| o.user_id == user_id)
            .map(|o| UpstreamOrderView {
                order_id: o.id,
                order_no: o.order_no,
                status: o.status,
                amount: zs_shared::money::Amount::new(o.total_amount),
                refunded_amount: zs_shared::money::Amount::ZERO,
                currency: o.currency,
                refund_records: Vec::new(),
                fulfillment: None,
                items: Vec::new(),
            }))
    }

    async fn cancel(
        &self,
        _user_id: Id,
        _order_id: Id,
    ) -> Result<UpstreamOrderSummary, UpstreamOrderError> {
        Err(UpstreamOrderError::CancelNotAllowed)
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

pub struct IntApp {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub admin_token: String,
    pub lifecycle: Arc<FakeLifecycle>,
    pub ordering: Arc<FakeOrdering>,
}

pub fn config() -> Config {
    let mut cfg = Config::default();
    cfg.app.secret_key = "test-app-secret-0123456789abcdef".into();
    cfg.jwt.secret = "test-admin-jwt-secret-0123456789".into();
    cfg.user_jwt.secret = "test-user-jwt-secret-0123456789a".into();
    cfg.database.url = "sqlite::memory:".into();
    // DB-01: one connection exposes queries issued outside a held transaction.
    cfg.database.max_connections = 1;
    cfg.bootstrap.default_admin_username = "admin".into();
    cfg.bootstrap.default_admin_password = "Admin12345".into();
    cfg.security.login_rate_limit.max_attempts = 1000;
    cfg.upload.dir = std::env::temp_dir()
        .join(format!("zs-integration-uploads-{}", std::process::id()))
        .to_string_lossy()
        .into_owned();
    cfg
}

/// Compliance statement segments (see `zs_domain::identity::compliance`).
const SEGMENTS: [&str; 3] = [
    "我已阅读并理解上述合规声明提醒",
    "知悉相关法律风险",
    "并确认自行承担部署运营和收费行为产生的法律责任",
];

impl IntApp {
    /// App whose outbound traffic may reach loopback (test-only adapter).
    pub async fn new() -> Self {
        Self::with_policy(AddressPolicy::AllowPrivate).await
    }

    pub async fn with_policy(policy: AddressPolicy) -> Self {
        let cfg = config();
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        let mut services = zs_infra::wire::services(&ctx);
        let lifecycle = Arc::new(FakeLifecycle::default());
        let ordering = Arc::new(FakeOrdering {
            db: db.clone(),
            fail_payment: Mutex::new(false),
            placed: Mutex::new(0),
        });
        services.integration = zs_infra::wire::integration::build_with(
            &ctx,
            &Adapters {
                address_policy: policy,
                ordering: Some(ordering.clone()),
                lifecycle: Some(lifecycle.clone()),
                ..Adapters::default()
            },
        );
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
        let mut me = Self {
            router: app.router,
            db,
            services,
            admin_token: String::new(),
            lifecycle,
            ordering,
        };
        let login = me
            .call(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": "admin", "password": "Admin12345"})),
                None,
            )
            .await;
        me.admin_token = login["data"]["token"].as_str().unwrap().to_owned();
        me
    }

    pub async fn acknowledge_compliance(&self) {
        let res = self
            .admin(
                "POST",
                "/api/v1/admin/compliance/acknowledge",
                Some(json!({"segment1": SEGMENTS[0], "segment2": SEGMENTS[1], "segment3": SEGMENTS[2]})),
            )
            .await;
        assert_eq!(
            res["status_code"], 0,
            "compliance acknowledge failed: {res}"
        );
    }

    pub async fn send(&self, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, headers, bytes)
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Value {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let (_, _, bytes) = self.send(req).await;
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    pub async fn admin(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        self.call(method, uri, body, Some(&self.admin_token)).await
    }

    pub async fn set_setting(&self, key: &str, value: Value) {
        use zs_domain::settings::SettingsStore;
        zs_infra::db::repo::settings::SeaSettingsStore::new(self.db.clone())
            .set(key, &value)
            .await
            .unwrap();
    }

    /// Registers a user; returns `(user_id, token)`.
    pub async fn user(&self, email: &str) -> (i64, String) {
        self.set_setting(
            "registration_config",
            json!({"registration_enabled": true, "email_verification_enabled": false}),
        )
        .await;
        let res = self
            .call(
                "POST",
                "/api/v1/auth/register",
                Some(json!({"email": email, "password": "Passw0rdX", "agreement_accepted": true})),
                None,
            )
            .await;
        let token = data(&res)["token"].as_str().unwrap().to_owned();
        use sea_orm::{ColumnTrait, QueryFilter};
        let id = zs_infra::db::entity::users::Entity::find()
            .filter(zs_infra::db::entity::users::Column::Email.eq(email))
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
            .id;
        (id, token)
    }

    /// A user with an approved credential: `(user_id, token, credential_id, api_key, api_secret)`.
    pub async fn buyer(&self, email: &str) -> (i64, String, i64, String, String) {
        let (uid, token) = self.user(email).await;
        let applied = self
            .call("POST", "/api/v1/api-credential/apply", None, Some(&token))
            .await;
        let cid = data(&applied)["id"].as_i64().unwrap();
        let approved = self
            .admin(
                "POST",
                &format!("/api/v1/admin/api-credentials/{cid}/approve"),
                None,
            )
            .await;
        data(&approved);
        let mine = self
            .call("GET", "/api/v1/api-credential", None, Some(&token))
            .await;
        let key = data(&mine)["api_key"].as_str().unwrap().to_owned();
        let regen = self
            .call(
                "POST",
                "/api/v1/api-credential/regenerate",
                None,
                Some(&token),
            )
            .await;
        let secret = data(&regen)["api_secret"].as_str().unwrap().to_owned();
        (uid, token, cid, key, secret)
    }

    /// Signed call to our upstream API; returns `(http status, body)`.
    pub async fn upstream(
        &self,
        method: &str,
        path_and_query: &str,
        body: Option<Value>,
        key: &str,
        secret: &str,
        ts_offset: i64,
    ) -> (u16, Value) {
        let bytes = body.map(|b| b.to_string()).unwrap_or_default();
        let ts = Utc::now().timestamp() + ts_offset;
        let path = path_and_query.split('?').next().unwrap();
        let sig = sign::sign(secret, method, path, ts, bytes.as_bytes());
        let req = Request::builder()
            .method(method)
            .uri(path_and_query)
            .header(sign::HEADER_API_KEY, key)
            .header(sign::HEADER_TIMESTAMP, ts.to_string())
            .header(sign::HEADER_SIGNATURE, sig)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(bytes))
            .unwrap();
        let (status, _, out) = self.send(req).await;
        (
            status.as_u16(),
            serde_json::from_slice(&out).unwrap_or(Value::Null),
        )
    }

    /// Creates a site connection to `base_url` (key/secret of the mock supplier).
    pub async fn connection(&self, base_url: &str, extra: Value) -> i64 {
        let mut body = json!({
            "name": "Supplier",
            "base_url": base_url,
            "api_key": SUP_KEY,
            "api_secret": SUP_SECRET,
            "protocol": "dujiao-next",
            "callback_url": "https://buyer.example.com/api/v1/upstream/callback",
        });
        if let (Some(b), Some(e)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                b.insert(k.clone(), v.clone());
            }
        }
        let res = self
            .admin("POST", "/api/v1/admin/site-connections", Some(body))
            .await;
        data(&res)["id"].as_i64().unwrap()
    }
}

/// Asserts a success envelope and returns `data`.
pub fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    &v["data"]
}

/// Asserts an error envelope.
pub fn err(v: &Value, code: i64, msg: &str) {
    assert_eq!(v["status_code"], code, "{v}");
    assert_eq!(v["msg"], msg, "{v}");
}

// ---------------------------------------------------------------------------
// Seeding
// ---------------------------------------------------------------------------

/// Inserts an order row; returns its id.
pub async fn seed_order_row<C: sea_orm::ConnectionTrait>(
    conn: &C,
    order_no: &str,
    user_id: i64,
    status: &str,
    total: &str,
    parent_id: Option<i64>,
) -> i64 {
    let now = Utc::now();
    let mut m = orders::Model {
        id: 0,
        order_no: order_no.into(),
        parent_id,
        user_id,
        guest_email: String::new(),
        guest_password: String::new(),
        guest_locale: String::new(),
        status: status.into(),
        currency: "CNY".into(),
        original_amount: total.parse().unwrap(),
        discount_amount: 0.into(),
        member_discount_amount: 0.into(),
        promotion_discount_amount: 0.into(),
        wholesale_discount_amount: 0.into(),
        total_amount: total.parse().unwrap(),
        wallet_paid_amount: 0.into(),
        online_paid_amount: 0.into(),
        refunded_amount: 0.into(),
        member_level_id: None,
        coupon_id: None,
        promotion_id: None,
        affiliate_profile_id: None,
        affiliate_code: String::new(),
        reseller_id: None,
        reseller_domain: String::new(),
        reseller_profit_amount: 0.into(),
        client_ip: String::new(),
        risk_ip: String::new(),
        expires_at: None,
        paid_at: Some(now),
        canceled_at: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
    .into_active_model();
    m.id = NotSet;
    m.insert(conn).await.unwrap().id
}

/// Inserts an order item.
pub async fn seed_item(
    db: &sea_orm::DatabaseConnection,
    order_id: i64,
    product_id: i64,
    sku_id: i64,
    qty: i32,
    fulfillment_type: &str,
) {
    let now = Utc::now();
    let mut m = order_items::Model {
        id: 0,
        order_id,
        product_id,
        sku_id,
        title_json: Some(json!({"zh-CN": "商品"})),
        sku_snapshot_json: None,
        tags: None,
        original_unit_price: "10".parse().unwrap(),
        unit_price: "10".parse().unwrap(),
        cost_price: "7".parse().unwrap(),
        quantity: qty,
        original_total_price: "10".parse().unwrap(),
        total_price: "10".parse().unwrap(),
        coupon_discount: 0.into(),
        member_discount: 0.into(),
        promotion_discount: 0.into(),
        wholesale_discount: 0.into(),
        promotion_id: None,
        fulfillment_type: fulfillment_type.into(),
        manual_form_schema_snapshot_json: None,
        manual_form_submission_json: Some(json!({"account": "a@b.c"})),
        instructions_json: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
    .into_active_model();
    m.id = NotSet;
    m.insert(db).await.unwrap();
}

/// Creates an active category + product with SKUs `(code, price, active)`.
pub async fn seed_product(
    db: &sea_orm::DatabaseConnection,
    slug: &str,
    fulfillment: &str,
    skus: &[(&str, &str, bool)],
) -> (i64, Vec<i64>) {
    let now = Utc::now();
    let mut cat = categories::Model {
        id: 0,
        parent_id: 0,
        slug: format!("cat-{slug}"),
        name_json: Some(json!({"zh-CN": "分类"})),
        icon: String::new(),
        sort_order: 0,
        is_active: true,
        created_at: now,
        deleted_at: None,
    }
    .into_active_model();
    cat.id = NotSet;
    let cat = cat.insert(db).await.unwrap();
    let mut p = products::Model {
        id: 0,
        category_id: cat.id,
        slug: slug.into(),
        seo_meta_json: None,
        title_json: Some(json!({"zh-CN": format!("商品 {slug}")})),
        description_json: Some(json!({})),
        content_json: None,
        instructions_json: None,
        price_amount: "10".parse().unwrap(),
        cost_price_amount: "6".parse().unwrap(),
        wholesale_prices: None,
        images: None,
        tags: None,
        purchase_type: "member".into(),
        min_purchase_quantity: 0,
        max_purchase_quantity: 0,
        stock_display_mode: "exact".into(),
        fulfillment_type: fulfillment.into(),
        manual_form_schema_json: None,
        manual_stock_total: 0,
        manual_stock_locked: 0,
        manual_stock_sold: 0,
        payment_channel_ids: String::new(),
        is_affiliate_enabled: false,
        is_mapped: false,
        is_active: true,
        sort_order: 0,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
    .into_active_model();
    p.id = NotSet;
    let p = p.insert(db).await.unwrap();
    let mut ids = Vec::new();
    for (code, price, active) in skus {
        let mut s = product_skus::Model {
            id: 0,
            product_id: p.id,
            sku_code: (*code).into(),
            spec_values_json: Some(json!({})),
            price_amount: price.parse().unwrap(),
            cost_price_amount: "1".parse().unwrap(),
            manual_stock_total: 30,
            manual_stock_locked: 0,
            manual_stock_sold: 0,
            is_active: *active,
            sort_order: 0,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
        .into_active_model();
        s.id = NotSet;
        ids.push(s.insert(db).await.unwrap().id);
    }
    (p.id, ids)
}

// ---------------------------------------------------------------------------
// Mock supplier implementing the legacy compatibility protocol.
// ---------------------------------------------------------------------------

/// One request received by the mock.
#[derive(Debug, Clone)]
pub struct Hit {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: HeaderMap,
    pub body: String,
}

/// Scriptable state of the mock supplier.
#[derive(Default)]
pub struct SupplierState {
    pub hits: Mutex<Vec<Hit>>,
    /// Catalog served by `/products` (JSON products, `id` required).
    pub products: Mutex<Vec<Value>>,
    /// Product ids answered `404 product_deleted`.
    pub deleted: Mutex<Vec<i64>>,
    pub categories: Mutex<Vec<Value>>,
    /// Echo `includes_inactive` when asked (new suppliers do).
    pub echo_inactive: Mutex<bool>,
    /// Pages beyond this answer an empty item list (pagination truncation).
    pub truncate_after_page: Mutex<Option<i64>>,
    /// Fixed `total` override.
    pub total_override: Mutex<Option<i64>>,
    /// `(http status, body)` of `POST /orders`.
    pub order_reply: Mutex<Option<(u16, Value)>>,
    /// Bodies of `GET /orders/:id` by id.
    pub orders: Mutex<HashMap<i64, Value>>,
    /// `(http status, body)` answered by the downstream callback receiver `/cb`.
    pub callback_reply: Mutex<Option<(u16, Value)>>,
}

pub type Supplier = Arc<SupplierState>;

impl SupplierState {
    pub fn hits(&self) -> Vec<Hit> {
        self.hits.lock().unwrap().clone()
    }

    pub fn paths(&self) -> Vec<String> {
        self.hits().into_iter().map(|h| h.path).collect()
    }

    pub fn set_products(&self, list: Vec<Value>) {
        *self.products.lock().unwrap() = list;
    }
}

fn record(
    s: &SupplierState,
    method: &str,
    path: &str,
    query: &str,
    headers: &HeaderMap,
    body: &[u8],
) {
    s.hits.lock().unwrap().push(Hit {
        method: method.into(),
        path: path.into(),
        query: query.into(),
        headers: headers.clone(),
        body: String::from_utf8_lossy(body).into_owned(),
    });
}

/// Verifies our signature like a real supplier.
fn verified(headers: &HeaderMap, method: &str, path: &str, body: &[u8]) -> bool {
    let h = |n: &str| {
        headers
            .get(n)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };
    let ts: i64 = h(sign::HEADER_TIMESTAMP).parse().unwrap_or(0);
    h(sign::HEADER_API_KEY) == SUP_KEY
        && sign::verify(
            SUP_SECRET,
            method,
            path,
            &h(sign::HEADER_SIGNATURE),
            ts,
            body,
        )
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(json!({"ok": false, "error_code": "invalid_signature", "error_message": "bad"})),
    )
        .into_response()
}

async fn sup_ping(State(s): State<Supplier>, headers: HeaderMap, body: Bytes) -> Response {
    record(&s, "POST", "/api/v1/upstream/ping", "", &headers, &body);
    if !verified(&headers, "POST", "/api/v1/upstream/ping", &body) {
        return unauthorized();
    }
    axum::Json(json!({"ok": true, "site_name": "Supplier", "protocol_version": "1.0", "user_id": 7, "balance": "99.00", "currency": "USD"})).into_response()
}

async fn sup_categories(State(s): State<Supplier>, headers: HeaderMap) -> Response {
    record(&s, "GET", "/api/v1/upstream/categories", "", &headers, b"");
    if !verified(&headers, "GET", "/api/v1/upstream/categories", b"") {
        return unauthorized();
    }
    axum::Json(json!({"ok": true, "categories": s.categories.lock().unwrap().clone()}))
        .into_response()
}

async fn sup_products(
    State(s): State<Supplier>,
    Query(q): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let query = q
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    record(
        &s,
        "GET",
        "/api/v1/upstream/products",
        &query,
        &headers,
        b"",
    );
    if !verified(&headers, "GET", "/api/v1/upstream/products", b"") {
        return unauthorized();
    }
    let page: i64 = q.get("page").and_then(|v| v.parse().ok()).unwrap_or(1);
    let size: i64 = q
        .get("page_size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(50);
    let include_inactive = q.get("include_inactive").map(String::as_str) == Some("true");
    let all: Vec<Value> = s
        .products
        .lock()
        .unwrap()
        .iter()
        .filter(|p| include_inactive || p["is_active"] == true)
        .cloned()
        .collect();
    let total = s.total_override.lock().unwrap().unwrap_or(all.len() as i64);
    let truncated = s
        .truncate_after_page
        .lock()
        .unwrap()
        .is_some_and(|t| page > t);
    let items: Vec<Value> = if truncated {
        Vec::new()
    } else {
        all.into_iter()
            .skip(usize::try_from((page - 1) * size).unwrap())
            .take(usize::try_from(size).unwrap())
            .collect()
    };
    let echo = include_inactive && *s.echo_inactive.lock().unwrap();
    axum::Json(json!({"ok": true, "items": items, "total": total, "page": page, "page_size": size, "includes_inactive": echo})).into_response()
}

async fn sup_product(
    State(s): State<Supplier>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    let path = format!("/api/v1/upstream/products/{id}");
    record(&s, "GET", &path, "", &headers, b"");
    if !verified(&headers, "GET", &path, b"") {
        return unauthorized();
    }
    if s.deleted.lock().unwrap().contains(&id) {
        return (
            StatusCode::NOT_FOUND,
            axum::Json(
                json!({"ok": false, "error_code": "product_deleted", "error_message": "gone"}),
            ),
        )
            .into_response();
    }
    let found = s
        .products
        .lock()
        .unwrap()
        .iter()
        .find(|p| p["id"] == id)
        .cloned();
    match found {
        Some(p) => axum::Json(json!({"ok": true, "product": p})).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(
                json!({"ok": false, "error_code": "product_not_found", "error_message": "no"}),
            ),
        )
            .into_response(),
    }
}

async fn sup_create_order(State(s): State<Supplier>, headers: HeaderMap, body: Bytes) -> Response {
    record(&s, "POST", "/api/v1/upstream/orders", "", &headers, &body);
    if !verified(&headers, "POST", "/api/v1/upstream/orders", &body) {
        return unauthorized();
    }
    let (status, reply) = s
        .order_reply
        .lock()
        .unwrap()
        .clone()
        .unwrap_or((200, json!({"ok": true, "order_id": 88, "order_no": "UP88", "status": "paid", "amount": "7.00", "currency": "USD"})));
    (StatusCode::from_u16(status).unwrap(), axum::Json(reply)).into_response()
}

async fn sup_get_order(
    State(s): State<Supplier>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    let path = format!("/api/v1/upstream/orders/{id}");
    record(&s, "GET", &path, "", &headers, b"");
    if !verified(&headers, "GET", &path, b"") {
        return unauthorized();
    }
    match s.orders.lock().unwrap().get(&id).cloned() {
        Some(o) => axum::Json(o).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            axum::Json(json!({"ok": false, "error_code": "order_not_found"})),
        )
            .into_response(),
    }
}

async fn sup_cancel(
    State(s): State<Supplier>,
    Path(id): Path<i64>,
    headers: HeaderMap,
) -> Response {
    let path = format!("/api/v1/upstream/orders/{id}/cancel");
    record(&s, "POST", &path, "", &headers, b"");
    axum::Json(json!({"ok": true})).into_response()
}

async fn sup_image(
    State(s): State<Supplier>,
    Path(name): Path<String>,
    headers: HeaderMap,
) -> Response {
    record(&s, "GET", &format!("/images/{name}"), "", &headers, b"");
    ([(header::CONTENT_TYPE, "image/gif")], GIF).into_response()
}

async fn sup_callback(State(s): State<Supplier>, headers: HeaderMap, body: Bytes) -> Response {
    record(&s, "POST", "/cb", "", &headers, &body);
    let (status, reply) = s
        .callback_reply
        .lock()
        .unwrap()
        .clone()
        .unwrap_or((200, json!({"ok": true})));
    (StatusCode::from_u16(status).unwrap(), axum::Json(reply)).into_response()
}

async fn sup_redirect(State(s): State<Supplier>, headers: HeaderMap, body: Bytes) -> Response {
    record(&s, "POST", "/redirect", "", &headers, &body);
    (
        StatusCode::FOUND,
        [(header::LOCATION, "http://169.254.169.254/")],
    )
        .into_response()
}

/// Starts the mock supplier on 127.0.0.1; returns `(base_url, state)`.
pub async fn start_supplier() -> (String, Supplier) {
    let state: Supplier = Arc::new(SupplierState {
        echo_inactive: Mutex::new(true),
        ..SupplierState::default()
    });
    let app = Router::new()
        .route("/api/v1/upstream/ping", post(sup_ping))
        .route("/api/v1/upstream/categories", get(sup_categories))
        .route("/api/v1/upstream/products", get(sup_products))
        .route("/api/v1/upstream/products/{id}", get(sup_product))
        .route("/api/v1/upstream/orders", post(sup_create_order))
        .route("/api/v1/upstream/orders/{id}", get(sup_get_order))
        .route("/api/v1/upstream/orders/{id}/cancel", post(sup_cancel))
        .route("/images/{name}", get(sup_image))
        .route("/cb", post(sup_callback))
        .route("/redirect", post(sup_redirect))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await });
    (format!("http://{addr}"), state)
}

/// A supplier product JSON.
pub fn remote_product(id: i64, price: &str, skus: Value) -> Value {
    json!({
        "id": id,
        "slug": format!("up-{id}"),
        "title": {"zh-CN": format!("上游商品 {id}")},
        "description": {"zh-CN": "desc"},
        "content": {"zh-CN": "![x](/images/c.gif)"},
        "images": ["/images/a.gif"],
        "tags": ["t"],
        "price_amount": price,
        "fulfillment_type": "auto",
        "manual_form_schema": null,
        "is_active": true,
        "category_id": 3,
        "skus": skus,
        "updated_at": "2026-01-01T00:00:00Z",
    })
}

/// Signs a supplier callback like a supplier would.
pub fn callback_request(key: &str, secret: &str, body: &str, ts_offset: i64) -> Request<Body> {
    let ts = Utc::now().timestamp() + ts_offset;
    let sig = sign::sign(
        secret,
        "POST",
        "/api/v1/upstream/callback",
        ts,
        body.as_bytes(),
    );
    Request::builder()
        .method("POST")
        .uri("/api/v1/upstream/callback")
        .header(sign::HEADER_API_KEY, key)
        .header(sign::HEADER_TIMESTAMP, ts.to_string())
        .header(sign::HEADER_SIGNATURE, sig)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

/// Rows of the job queue of a kind.
pub async fn jobs_of(db: &sea_orm::DatabaseConnection, kind: &str) -> Vec<Value> {
    use sea_orm::{ColumnTrait, QueryFilter};
    zs_infra::db::entity::jobs::Entity::find()
        .filter(zs_infra::db::entity::jobs::Column::Kind.eq(kind))
        .all(db)
        .await
        .unwrap()
        .into_iter()
        .map(|j| serde_json::from_str(&j.payload).unwrap_or(Value::Null))
        .collect()
}
