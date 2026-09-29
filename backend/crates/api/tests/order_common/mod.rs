//! Harness of the `order_*` integration tests: in-memory SQLite with
//! `max_connections = 1` (DB-01), the real router, gateways on a scripted transport (epay in
//! redirect mode needs no outbound call), compliance acknowledged, and seeding helpers.

#![allow(
    dead_code,
    reason = "shared by several order test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use sea_orm::prelude::Decimal;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QueryOrder, Set,
};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_app::config::Config;
use zs_domain::payment::form::encode_pairs;
use zs_domain::settings::SettingsExt;
use zs_infra::db::entity::{
    card_secrets, extra::jobs, orders, payment_channels, payments, product_skus, users,
    wallet_accounts, wallet_transactions,
};
use zs_infra::payment::common::{GatewayEnv, OsEntropy};
use zs_infra::payment::epay;
use zs_infra::payment::http::{HttpTransport, MockTransport};

pub const EPAY_KEY: &str = "epay-secret-key";
pub const EPAY_PID: &str = "1001";

/// Compliance statement segments (see `zs_domain::identity::compliance`).
const SEGMENTS: [&str; 3] = [
    "我已阅读并理解上述合规声明提醒",
    "知悉相关法律风险",
    "并确认自行承担部署运营和收费行为产生的法律责任",
];

pub fn dec(v: &str) -> Decimal {
    v.parse().unwrap()
}

pub fn config() -> Config {
    let mut cfg = Config::default();
    cfg.app.secret_key = "test-app-secret-0123456789abcdef".into();
    cfg.jwt.secret = "test-admin-jwt-secret-0123456789".into();
    cfg.user_jwt.secret = "test-user-jwt-secret-0123456789a".into();
    cfg.database.url = "sqlite::memory:".into();
    // DB-01: a single connection exposes any query issued outside a held transaction.
    cfg.database.max_connections = 1;
    cfg.bootstrap.default_admin_username = "admin".into();
    cfg.bootstrap.default_admin_password = "Admin12345".into();
    cfg.security.login_rate_limit.max_attempts = 1000;
    cfg
}

/// Unpadded base64url (`base64.RawURLEncoding`).
pub fn b64url(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..=chunk.len() {
            out.push(char::from(ALPHABET[((n >> (18 - 6 * i)) & 63) as usize]));
        }
    }
    out
}

/// Who sends a request.
#[derive(Debug, Clone)]
pub enum Auth {
    None,
    Bearer(String),
    /// `Authorization: Guest base64url(email "\n" password)` (ORD-02).
    Guest(String, String),
}

pub struct OrderApp {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub ctx: zs_infra::wire::WireCtx,
    pub admin: String,
    pub category: i64,
}

impl OrderApp {
    pub async fn new() -> Self {
        Self::with_transport(Arc::new(MockTransport::new(|_| Err("offline".into())))).await
    }

    /// Same harness with gateways on `transport` (e.g. a slow gateway, PAY-13).
    pub async fn with_transport(transport: Arc<dyn HttpTransport>) -> Self {
        Self::with_converter_policy(transport, false).await
    }

    /// Test harness that allows the configured local card-converter mock.
    pub async fn with_local_converter() -> Self {
        Self::with_converter_policy(
            Arc::new(MockTransport::new(|_| Err("offline".into()))),
            true,
        )
        .await
    }

    async fn with_converter_policy(transport: Arc<dyn HttpTransport>, allow_private: bool) -> Self {
        let mut cfg = config();
        cfg.integration.allow_private_addresses = allow_private;
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        let env = GatewayEnv::new(transport, ctx.clock.clone(), Arc::new(OsEntropy));
        let mut services = zs_infra::wire::services(&ctx);
        services.order = zs_infra::wire::order::build_with_env(&ctx, &env);
        services.payment = zs_infra::wire::payment::build_with(&ctx, env, None);
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let built = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
        let mut app = Self {
            router: built.router,
            db,
            services,
            ctx,
            admin: String::new(),
            category: 0,
        };
        let login = app
            .call(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": "admin", "password": "Admin12345"})),
                &Auth::None,
            )
            .await;
        app.admin = login["data"]["token"].as_str().unwrap().to_owned();
        let ack = app
            .admin_call(
                "POST",
                "/api/v1/admin/compliance/acknowledge",
                Some(json!({"segment1": SEGMENTS[0], "segment2": SEGMENTS[1], "segment3": SEGMENTS[2]})),
            )
            .await;
        assert_eq!(
            ack["status_code"], 0,
            "compliance acknowledge failed: {ack}"
        );
        let cat = app
            .admin_call(
                "POST",
                "/api/v1/admin/categories",
                Some(json!({"name": {"zh-CN": "cat"}, "slug": "cat", "parent_id": 0})),
            )
            .await;
        app.category = cat["data"]["id"].as_i64().unwrap();
        app
    }

    pub async fn send(&self, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, headers, bytes)
    }

    fn request(method: &str, uri: &str, body: Option<Value>, auth: &Auth) -> Request<Body> {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::ACCEPT_LANGUAGE, "en-US")
            .header("x-forwarded-for", "203.0.113.7");
        match auth {
            Auth::None => {}
            Auth::Bearer(t) => req = req.header(header::AUTHORIZATION, format!("Bearer {t}")),
            Auth::Guest(e, p) => {
                req = req.header(
                    header::AUTHORIZATION,
                    format!("Guest {}", b64url(format!("{e}\n{p}").as_bytes())),
                );
            }
        }
        match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string()))
                .unwrap(),
            None => req.body(Body::empty()).unwrap(),
        }
    }

    /// Full response of a request.
    pub async fn raw(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        auth: &Auth,
    ) -> (StatusCode, HeaderMap, Vec<u8>) {
        self.send(Self::request(method, uri, body, auth)).await
    }

    pub async fn call(&self, method: &str, uri: &str, body: Option<Value>, auth: &Auth) -> Value {
        let (_, _, bytes) = self.raw(method, uri, body, auth).await;
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    }

    pub async fn admin_call(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let auth = Auth::Bearer(self.admin.clone());
        self.call(method, uri, body, &auth).await
    }

    pub async fn set_setting(&self, key: &str, value: Value) {
        self.ctx.settings.put(key, &value).await.unwrap();
    }

    /// Creates a product through the admin API; returns `(product_id, default sku_id)`.
    pub async fn product(&self, slug: &str, extra: Value) -> (i64, i64) {
        let mut body = json!({
            "category_id": self.category,
            "slug": slug,
            "title": {"zh-CN": format!("商品 {slug}"), "en-US": format!("Product {slug}")},
            "price_amount": 10,
            "fulfillment_type": "auto",
            "purchase_type": "guest",
            "is_active": true,
        });
        if let (Some(base), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            for (k, v) in extra {
                base.insert(k.clone(), v.clone());
            }
        }
        let res = self
            .admin_call("POST", "/api/v1/admin/products", Some(body))
            .await;
        let id = res["data"]["id"]
            .as_i64()
            .unwrap_or_else(|| panic!("product create failed: {res}"));
        let sku = product_skus::Entity::find()
            .filter(product_skus::Column::ProductId.eq(id))
            .order_by_asc(product_skus::Column::Id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
            .id;
        (id, sku)
    }

    /// Inserts `n` available card secrets.
    pub async fn secrets(&self, product_id: i64, sku_id: i64, n: usize) {
        let now = Utc::now();
        for i in 0..n {
            card_secrets::ActiveModel {
                product_id: Set(product_id),
                sku_id: Set(sku_id),
                batch_id: Set(None),
                secret: Set(format!("CARD-{product_id}-{i}")),
                status: Set("available".into()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&self.db)
            .await
            .unwrap();
        }
    }

    pub async fn secrets_with(&self, product_id: i64, status: &str) -> u64 {
        card_secrets::Entity::find()
            .filter(card_secrets::Column::ProductId.eq(product_id))
            .filter(card_secrets::Column::Status.eq(status))
            .count(&self.db)
            .await
            .unwrap()
    }

    /// An active epay channel in redirect mode (no outbound call on creation).
    pub async fn epay_channel(&self) -> i64 {
        let now = Utc::now();
        payment_channels::ActiveModel {
            name: Set("epay-alipay".into()),
            icon: Set(String::new()),
            provider_type: Set("epay".into()),
            channel_type: Set("alipay".into()),
            interaction_mode: Set("redirect".into()),
            fee_rate: Set(Decimal::ZERO),
            fixed_fee: Set(Decimal::ZERO),
            min_amount: Set(Decimal::ZERO),
            max_amount: Set(Decimal::ZERO),
            hide_amount_out_range: Set(false),
            payment_roles: Set(Some(json!([]))),
            member_levels: Set(Some(json!([]))),
            payment_types: Set(Some(json!([]))),
            config_json: Set(Some(json!({
                "gateway_url": "https://pay.example.com",
                "merchant_id": EPAY_PID,
                "merchant_key": EPAY_KEY,
                "notify_url": "https://shop.example.com/api/v1/payments/callback",
                "return_url": "https://shop.example.com/pay",
            }))),
            is_active: Set(true),
            sort_order: Set(0),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap()
        .id
    }

    /// Sends a signed epay notification for `payment_id`; returns the plain-text answer.
    pub async fn epay_callback(&self, payment_id: i64, money: &str) -> String {
        let row = self.payment(payment_id).await;
        let mut p: BTreeMap<String, String> = [
            ("pid", EPAY_PID),
            ("trade_no", &format!("EP-{payment_id}")),
            ("out_trade_no", &row.gateway_order_no),
            ("type", "alipay"),
            ("name", "order"),
            ("money", money),
            ("trade_status", "TRADE_SUCCESS"),
            ("sign_type", "MD5"),
        ]
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
        let sign = epay::sign_md5(&epay::sign_content(&p), EPAY_KEY);
        p.insert("sign".into(), sign);
        let query = encode_pairs(&p.into_iter().collect::<Vec<_>>());
        let req = Request::builder()
            .method("GET")
            .uri(format!("/api/v1/payments/callback?{query}"))
            .body(Body::empty())
            .unwrap();
        let (_, _, bytes) = self.send(req).await;
        String::from_utf8_lossy(&bytes).into_owned()
    }

    /// Creates an active user; returns `(id, access token)`.
    pub async fn user(&self, email: &str, level: i64) -> (i64, String) {
        let now = Utc::now();
        let id = users::ActiveModel {
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
        .insert(&self.db)
        .await
        .unwrap()
        .id;
        let auth = &self.services.identity.user_auth;
        let user = auth.repo().get(id).await.unwrap().unwrap();
        let (token, _) = auth.issue_token(&user, false).unwrap();
        (id, token)
    }

    /// Sets a user's wallet balance directly.
    pub async fn fund(&self, user_id: i64, amount: &str) {
        let now = Utc::now();
        wallet_accounts::ActiveModel {
            user_id: Set(user_id),
            balance: Set(dec(amount)),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap();
    }

    /// Current balance as a two-decimal string (`"0.00"` without an account).
    pub async fn balance(&self, user_id: i64) -> String {
        let row = wallet_accounts::Entity::find()
            .filter(wallet_accounts::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
            .unwrap();
        format!("{:.2}", row.map(|r| r.balance).unwrap_or_default())
    }

    pub async fn wallet_txns(&self, user_id: i64) -> Vec<wallet_transactions::Model> {
        wallet_transactions::Entity::find()
            .filter(wallet_transactions::Column::UserId.eq(user_id))
            .order_by_asc(wallet_transactions::Column::Id)
            .all(&self.db)
            .await
            .unwrap()
    }

    pub async fn order_by_no(&self, order_no: &str) -> orders::Model {
        orders::Entity::find()
            .filter(orders::Column::OrderNo.eq(order_no))
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }

    pub async fn order(&self, id: i64) -> orders::Model {
        orders::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }

    pub async fn children(&self, parent_id: i64) -> Vec<orders::Model> {
        orders::Entity::find()
            .filter(orders::Column::ParentId.eq(parent_id))
            .order_by_asc(orders::Column::Id)
            .all(&self.db)
            .await
            .unwrap()
    }

    pub async fn payment(&self, id: i64) -> payments::Model {
        payments::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }

    /// Rewrites a payment's amount (a stale payment that no longer covers the order).
    pub async fn set_payment_amount(&self, payment_id: i64, amount: &str) {
        payments::Entity::update_many()
            .col_expr(payments::Column::Amount, Expr::value(dec(amount)))
            .filter(payments::Column::Id.eq(payment_id))
            .exec(&self.db)
            .await
            .unwrap();
    }

    /// Moves an order's payment deadline into the past.
    pub async fn expire(&self, order_id: i64) {
        let past: DateTime<Utc> = Utc::now() - chrono::Duration::minutes(1);
        orders::Entity::update_many()
            .col_expr(orders::Column::ExpiresAt, Expr::value(Some(past)))
            .filter(orders::Column::Id.eq(order_id))
            .exec(&self.db)
            .await
            .unwrap();
    }

    /// Jobs of `kind` currently queued.
    pub async fn jobs(&self, kind: &str) -> Vec<jobs::Model> {
        jobs::Entity::find()
            .filter(jobs::Column::Kind.eq(kind))
            .all(&self.db)
            .await
            .unwrap()
    }
}
