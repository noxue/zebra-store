//! Harness shared by the `wallet_*` and `affiliate_*` integration tests: in-memory SQLite
//! with `max_connections = 1` (DB-01), the real router, compliance acknowledged, and a stub
//! recharge gateway.

#![allow(
    dead_code,
    reason = "shared by several test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::{DateTime, Utc};
use http_body_util::BodyExt;
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_app::config::Config;
use zs_domain::Result;
use zs_domain::payment::callback::CallbackInput;
use zs_domain::payment::channel::PaymentChannel;
use zs_domain::payment::errors::keys as pay_keys;
use zs_domain::payment::model::Payment;
use zs_domain::payment::types::PaymentStatus;
use zs_domain::settings::SettingsExt;
use zs_domain::wallet::{RechargeGateway, RechargeOrder};
use zs_infra::db::entity::{
    extra::jobs, gift_cards, order_items, orders, payment_channels, payments, products, users,
    wallet_accounts,
};

/// Compliance statement segments (see `zs_domain::identity::compliance`).
const SEGMENTS: [&str; 3] = [
    "我已阅读并理解上述合规声明提醒",
    "知悉相关法律风险",
    "并确认自行承担部署运营和收费行为产生的法律责任",
];

pub fn dec(v: &str) -> Decimal {
    v.parse().unwrap()
}

/// How the stub gateway answers active queries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryMode {
    /// `error.payment_provider_not_supported` (WAL-03 fallback).
    Unsupported,
    /// Reports the payment as paid in full.
    Paid,
}

/// Recharge gateway stub: creation returns a pay URL, queries follow [`QueryMode`].
#[derive(Debug)]
pub struct StubGateway {
    pub query: Mutex<QueryMode>,
    pub fail_start: Mutex<bool>,
}

#[async_trait]
impl RechargeGateway for StubGateway {
    async fn start(
        &self,
        _: &PaymentChannel,
        payment: &Payment,
        recharge: &RechargeOrder,
        _: &str,
    ) -> Result<Payment> {
        if *self.fail_start.lock().unwrap() {
            return Err(zs_domain::Error::bad_request(
                pay_keys::GATEWAY_REQUEST_FAILED,
            ));
        }
        let mut p = payment.clone();
        p.status = PaymentStatus::Pending;
        p.gateway_order_no = format!("DJPTEST{}", payment.id);
        p.provider_ref = p.gateway_order_no.clone();
        p.pay_url = format!("https://pay.test/{}", recharge.recharge_no);
        Ok(p)
    }

    async fn query(
        &self,
        channel: &PaymentChannel,
        payment: &Payment,
    ) -> Result<Option<CallbackInput>> {
        match *self.query.lock().unwrap() {
            QueryMode::Unsupported => Err(zs_domain::Error::bad_request(
                pay_keys::PROVIDER_NOT_SUPPORTED,
            )),
            QueryMode::Paid => Ok(Some(paid_input(
                payment.id,
                channel.id,
                &payment.amount.to_string(),
                &payment.currency,
            ))),
        }
    }
}

/// A verified success notification for `payment_id`.
pub fn paid_input(payment_id: i64, channel_id: i64, amount: &str, currency: &str) -> CallbackInput {
    input(
        payment_id,
        channel_id,
        PaymentStatus::Success,
        amount,
        currency,
    )
}

/// A verified notification with an arbitrary status.
pub fn input(
    payment_id: i64,
    channel_id: i64,
    status: PaymentStatus,
    amount: &str,
    currency: &str,
) -> CallbackInput {
    CallbackInput {
        payment_id,
        order_no: String::new(),
        channel_id,
        status,
        provider_ref: format!("REF{payment_id}"),
        amount: amount.parse().unwrap(),
        currency: currency.to_owned(),
        paid_at: None,
        payload: serde_json::Map::new(),
        verified_legacy_currency: String::new(),
    }
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

pub struct App {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub ctx: zs_infra::wire::WireCtx,
    pub admin: String,
    pub gateway: Arc<StubGateway>,
}

impl App {
    /// App with the super admin logged in and compliance acknowledged.
    pub async fn new() -> Self {
        let app = Self::without_ack().await;
        let res = app
            .admin_call(
                "POST",
                "/api/v1/admin/compliance/acknowledge",
                Some(json!({"segment1": SEGMENTS[0], "segment2": SEGMENTS[1], "segment3": SEGMENTS[2]})),
            )
            .await;
        assert_eq!(
            res["status_code"], 0,
            "compliance acknowledge failed: {res}"
        );
        app
    }

    pub async fn without_ack() -> Self {
        let cfg = config();
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        let gateway = Arc::new(StubGateway {
            query: Mutex::new(QueryMode::Unsupported),
            fail_start: Mutex::new(false),
        });
        let mut services = zs_infra::wire::services(&ctx);
        services.wallet = zs_infra::wire::wallet::build_with_gateway(&ctx, Some(gateway.clone()));
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let built = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
        let mut app = Self {
            router: built.router,
            db,
            services,
            ctx,
            admin: String::new(),
            gateway,
        };
        let login = app
            .call(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": "admin", "password": "Admin12345"})),
                None,
            )
            .await;
        app.admin = login["data"]["token"].as_str().unwrap().to_owned();
        app
    }

    /// Sends a request (English messages); returns the HTTP status and JSON body.
    pub async fn raw(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::ACCEPT_LANGUAGE, "en-US");
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
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Value {
        self.raw(method, uri, body, token).await.1
    }

    pub async fn admin_call(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        self.call(method, uri, body, Some(&self.admin)).await
    }

    /// Creates an active user; returns `(id, access token)`.
    pub async fn user(&self, email: &str) -> (i64, String) {
        let now = Utc::now();
        let id = users::ActiveModel {
            email: Set(email.into()),
            password_hash: Set(String::new()),
            password_setup_required: Set(false),
            display_name: Set(email.split('@').next().unwrap_or_default().into()),
            locale: Set("zh-CN".into()),
            status: Set("active".into()),
            member_level_id: Set(0),
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

    pub async fn user_row(&self, id: i64) -> users::Model {
        users::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }

    /// Current balance as a two-decimal string (`"0.00"` without an account).
    pub async fn balance(&self, user_id: i64) -> String {
        let row = wallet_accounts::Entity::find()
            .filter(wallet_accounts::Column::UserId.eq(user_id))
            .one(&self.db)
            .await
            .unwrap();
        let v = row.map(|r| r.balance).unwrap_or_default();
        format!("{:.2}", v)
    }

    pub async fn set_setting(&self, key: &str, value: Value) {
        self.ctx.settings.put(key, &value).await.unwrap();
    }

    /// Inserts an active channel (`payment_types` given as JSON).
    pub async fn channel(
        &self,
        provider: &str,
        channel: &str,
        payment_types: Value,
        fee_rate: &str,
    ) -> i64 {
        let now = Utc::now();
        payment_channels::ActiveModel {
            name: Set(format!("{provider}-{channel}")),
            icon: Set(String::new()),
            provider_type: Set(provider.into()),
            channel_type: Set(channel.into()),
            interaction_mode: Set("redirect".into()),
            fee_rate: Set(dec(fee_rate)),
            fixed_fee: Set(Decimal::ZERO),
            min_amount: Set(Decimal::ZERO),
            max_amount: Set(Decimal::ZERO),
            hide_amount_out_range: Set(false),
            payment_roles: Set(Some(json!([]))),
            member_levels: Set(Some(json!([]))),
            payment_types: Set(Some(payment_types)),
            config_json: Set(Some(json!({}))),
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

    pub async fn payment_row(&self, id: i64) -> payments::Model {
        payments::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }

    /// Jobs of `kind` currently queued.
    pub async fn jobs(&self, kind: &str) -> Vec<jobs::Model> {
        jobs::Entity::find()
            .filter(jobs::Column::Kind.eq(kind))
            .all(&self.db)
            .await
            .unwrap()
    }

    /// Inserts a gift card.
    pub async fn gift_card(
        &self,
        code: &str,
        amount: &str,
        status: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> i64 {
        let now = Utc::now();
        gift_cards::ActiveModel {
            batch_id: Set(None),
            name: Set("card".into()),
            code: Set(code.into()),
            amount: Set(dec(amount)),
            currency: Set("CNY".into()),
            status: Set(status.into()),
            expires_at: Set(expires_at),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap()
        .id
    }

    pub async fn gift_card_row(&self, id: i64) -> gift_cards::Model {
        gift_cards::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }

    /// Inserts a product (`is_affiliate_enabled` as given).
    pub async fn product(&self, slug: &str, affiliate: bool) -> i64 {
        let now = Utc::now();
        products::ActiveModel {
            category_id: Set(1),
            slug: Set(slug.into()),
            seo_meta_json: Set(Some(json!({}))),
            title_json: Set(Some(json!({"zh-CN": slug}))),
            description_json: Set(Some(json!({}))),
            content_json: Set(Some(json!({}))),
            instructions_json: Set(Some(json!({}))),
            price_amount: Set(dec("10")),
            cost_price_amount: Set(Decimal::ZERO),
            wholesale_prices: Set(Some(json!([]))),
            images: Set(Some(json!([]))),
            tags: Set(Some(json!([]))),
            purchase_type: Set("member".into()),
            min_purchase_quantity: Set(0),
            max_purchase_quantity: Set(0),
            stock_display_mode: Set("exact".into()),
            fulfillment_type: Set("manual".into()),
            manual_form_schema_json: Set(Some(json!({}))),
            manual_stock_total: Set(-1),
            manual_stock_locked: Set(0),
            manual_stock_sold: Set(0),
            payment_channel_ids: Set(String::new()),
            is_affiliate_enabled: Set(affiliate),
            is_mapped: Set(false),
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

    /// Inserts a paid order (optionally a child of `parent`) with affiliate snapshot.
    pub async fn order(
        &self,
        order_no: &str,
        user_id: i64,
        total: &str,
        parent: Option<i64>,
        affiliate_profile_id: Option<i64>,
    ) -> i64 {
        let now = Utc::now();
        let z = Decimal::ZERO;
        orders::ActiveModel {
            order_no: Set(order_no.into()),
            parent_id: Set(parent),
            user_id: Set(user_id),
            guest_email: Set(String::new()),
            guest_password: Set(String::new()),
            guest_locale: Set(String::new()),
            status: Set("paid".into()),
            currency: Set("CNY".into()),
            original_amount: Set(dec(total)),
            discount_amount: Set(z),
            member_discount_amount: Set(z),
            promotion_discount_amount: Set(z),
            wholesale_discount_amount: Set(z),
            total_amount: Set(dec(total)),
            wallet_paid_amount: Set(z),
            online_paid_amount: Set(dec(total)),
            refunded_amount: Set(z),
            member_level_id: Set(None),
            coupon_id: Set(None),
            promotion_id: Set(None),
            affiliate_profile_id: Set(affiliate_profile_id),
            affiliate_code: Set(String::new()),
            reseller_id: Set(None),
            reseller_domain: Set(String::new()),
            reseller_profit_amount: Set(z),
            client_ip: Set(String::new()),
            risk_ip: Set(String::new()),
            expires_at: Set(None),
            paid_at: Set(Some(now)),
            canceled_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap()
        .id
    }

    /// Inserts an order item.
    pub async fn order_item(&self, order_id: i64, product_id: i64, total: &str, coupon: &str) {
        let now = Utc::now();
        let z = Decimal::ZERO;
        order_items::ActiveModel {
            order_id: Set(order_id),
            product_id: Set(product_id),
            sku_id: Set(0),
            title_json: Set(Some(json!({}))),
            sku_snapshot_json: Set(Some(json!({}))),
            tags: Set(Some(json!([]))),
            original_unit_price: Set(dec(total)),
            unit_price: Set(dec(total)),
            cost_price: Set(z),
            quantity: Set(1),
            original_total_price: Set(dec(total)),
            total_price: Set(dec(total)),
            coupon_discount: Set(dec(coupon)),
            member_discount: Set(z),
            promotion_discount: Set(z),
            wholesale_discount: Set(z),
            promotion_id: Set(None),
            fulfillment_type: Set("manual".into()),
            manual_form_schema_snapshot_json: Set(None),
            manual_form_submission_json: Set(None),
            instructions_json: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap();
    }
}

/// Asserts a success envelope and returns `data`.
pub fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    assert_eq!(v["msg"], "success");
    &v["data"]
}

/// Asserts an error envelope with `code` and English message `msg`.
pub fn err(v: &Value, code: i64, msg: &str) {
    assert_eq!(v["status_code"], code, "unexpected envelope {v}");
    assert_eq!(v["msg"], msg, "unexpected envelope {v}");
}

/// Keys of a JSON object, sorted.
pub fn keys(v: &Value) -> Vec<String> {
    let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    k.sort();
    k
}
