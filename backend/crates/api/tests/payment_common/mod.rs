//! Harness of the payment integration tests: in-memory SQLite, the real router, gateways on a
//! scripted HTTP transport, compliance acknowledged, and seeding helpers.

#![allow(
    dead_code,
    reason = "shared by several payment test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use std::sync::Arc;

use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode, header};
use chrono::Utc;
use http_body_util::BodyExt;
use sea_orm::prelude::Decimal;
use sea_orm::{ActiveModelTrait, ActiveValue::NotSet, EntityTrait, IntoActiveModel, Set};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_app::config::Config;
use zs_infra::db::entity::{orders, payment_channels, payments, wallet_recharge_orders};
use zs_infra::payment::http::{HttpRequest, HttpResponse, MockTransport};

pub struct PayApp {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub token: String,
}

pub fn config() -> Config {
    let mut cfg = Config::default();
    cfg.app.secret_key = "test-app-secret-0123456789abcdef".into();
    cfg.jwt.secret = "test-admin-jwt-secret-0123456789".into();
    cfg.user_jwt.secret = "test-user-jwt-secret-0123456789a".into();
    cfg.database.url = "sqlite::memory:".into();
    // DB-01: one connection exposes any query issued outside a held transaction.
    cfg.database.max_connections = 1;
    cfg.bootstrap.default_admin_username = "admin".into();
    cfg.bootstrap.default_admin_password = "Admin12345".into();
    cfg
}

/// Compliance statement segments (see `zs_domain::identity::compliance`).
const SEGMENTS: [&str; 3] = [
    "我已阅读并理解上述合规声明提醒",
    "知悉相关法律风险",
    "并确认自行承担部署运营和收费行为产生的法律责任",
];

impl PayApp {
    /// App with compliance acknowledged and gateways answered by `responder`.
    pub async fn new(
        responder: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
    ) -> Self {
        let app = Self::without_ack(responder).await;
        let res = app
            .call(
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

    /// App with gateways failing every outbound request.
    pub async fn offline() -> Self {
        Self::new(|_| Err("offline".into())).await
    }

    pub async fn without_ack(
        responder: impl Fn(&HttpRequest) -> Result<HttpResponse, String> + Send + Sync + 'static,
    ) -> Self {
        let cfg = config();
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        let mut services = zs_infra::wire::services(&ctx);
        services.payment = zs_infra::wire::payment::build_with_transport(
            &ctx,
            Arc::new(MockTransport::new(responder)),
        );
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg));
        let mut me = Self {
            router: app.router,
            db,
            services,
            token: String::new(),
        };
        let login = me
            .call_as(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": "admin", "password": "Admin12345"})),
                None,
            )
            .await;
        me.token = login["data"]["token"].as_str().unwrap().to_owned();
        me
    }

    pub async fn call_as(
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

    /// Admin call with the logged-in super admin.
    pub async fn call(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        let token = self.token.clone();
        self.call_as(method, uri, body, Some(&token)).await
    }

    pub async fn send(&self, req: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let headers = res.headers().clone();
        let bytes = res.into_body().collect().await.unwrap().to_bytes().to_vec();
        (status, headers, bytes)
    }

    /// Raw callback request; returns status, content type and body text.
    pub async fn callback(
        &self,
        method: &str,
        uri: &str,
        content_type: &str,
        headers: &[(&str, &str)],
        body: impl Into<Vec<u8>>,
    ) -> (StatusCode, String, String) {
        let mut req = Request::builder().method(method).uri(uri);
        if !content_type.is_empty() {
            req = req.header(header::CONTENT_TYPE, content_type);
        }
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let (status, h, bytes) = self.send(req.body(Body::from(body.into())).unwrap()).await;
        let ct = h
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        (status, ct, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Inserts a channel directly (bypasses admin validation).
    pub async fn seed_channel(
        &self,
        provider: &str,
        channel: &str,
        mode: &str,
        config: Value,
    ) -> i64 {
        let now = Utc::now();
        payment_channels::ActiveModel {
            name: Set(format!("{provider}-{channel}")),
            icon: Set(String::new()),
            provider_type: Set(provider.into()),
            channel_type: Set(channel.into()),
            interaction_mode: Set(mode.into()),
            fee_rate: Set(Decimal::ZERO),
            fixed_fee: Set(Decimal::ZERO),
            min_amount: Set(Decimal::ZERO),
            max_amount: Set(Decimal::ZERO),
            hide_amount_out_range: Set(false),
            payment_roles: Set(Some(json!([]))),
            member_levels: Set(Some(json!([]))),
            payment_types: Set(Some(json!([]))),
            config_json: Set(Some(config)),
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

    /// Inserts a pending order.
    pub async fn seed_order(&self, order_no: &str, total: &str) -> i64 {
        let now = Utc::now();
        let total: Decimal = total.parse().unwrap();
        let am = orders::Model {
            id: 0,
            order_no: order_no.into(),
            parent_id: None,
            user_id: 0,
            guest_email: "g@example.com".into(),
            guest_password: String::new(),
            guest_locale: "zh-CN".into(),
            status: "pending_payment".into(),
            currency: "CNY".into(),
            original_amount: total,
            discount_amount: Decimal::ZERO,
            member_discount_amount: Decimal::ZERO,
            promotion_discount_amount: Decimal::ZERO,
            wholesale_discount_amount: Decimal::ZERO,
            total_amount: total,
            wallet_paid_amount: Decimal::ZERO,
            online_paid_amount: Decimal::ZERO,
            refunded_amount: Decimal::ZERO,
            member_level_id: None,
            coupon_id: None,
            promotion_id: None,
            affiliate_profile_id: None,
            affiliate_code: String::new(),
            reseller_id: None,
            reseller_domain: String::new(),
            reseller_profit_amount: Decimal::ZERO,
            client_ip: String::new(),
            risk_ip: String::new(),
            expires_at: None,
            paid_at: None,
            canceled_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
        .into_active_model();
        let mut am = am.reset_all();
        am.id = NotSet;
        am.insert(&self.db).await.unwrap().id
    }

    /// Inserts a pending payment.
    pub async fn seed_payment(
        &self,
        order_id: i64,
        channel_id: i64,
        gateway_order_no: &str,
        amount: &str,
        currency: &str,
    ) -> i64 {
        let now = Utc::now();
        let channel = payment_channels::Entity::find_by_id(channel_id)
            .one(&self.db)
            .await
            .unwrap();
        let (provider, channel_type, mode) = channel
            .map(|c| (c.provider_type, c.channel_type, c.interaction_mode))
            .unwrap_or_default();
        payments::ActiveModel {
            order_id: Set(order_id),
            channel_id: Set(channel_id),
            provider_type: Set(provider),
            channel_type: Set(channel_type),
            interaction_mode: Set(mode),
            amount: Set(amount.parse().unwrap()),
            fee_rate: Set(Decimal::ZERO),
            fixed_fee: Set(Decimal::ZERO),
            fee_amount: Set(Decimal::ZERO),
            fee_policy: Set("none".into()),
            currency: Set(currency.into()),
            status: Set("pending".into()),
            exception_code: Set(String::new()),
            provider_ref: Set(format!("REF-{gateway_order_no}")),
            gateway_order_no: Set(gateway_order_no.into()),
            provider_payload: Set(Some(
                json!({"display_channel_type": "demo.type", "secret": "x"}),
            )),
            pay_url: Set("https://pay.example.com/x".into()),
            qr_code: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap()
        .id
    }

    /// Inserts a wallet recharge order linked to a payment.
    pub async fn seed_recharge(&self, payment_id: i64, recharge_no: &str, user_id: i64) {
        let now = Utc::now();
        wallet_recharge_orders::ActiveModel {
            recharge_no: Set(recharge_no.into()),
            user_id: Set(user_id),
            payment_id: Set(payment_id),
            channel_id: Set(0),
            provider_type: Set(String::new()),
            channel_type: Set(String::new()),
            interaction_mode: Set(String::new()),
            amount: Set(Decimal::ONE),
            payable_amount: Set(Decimal::ONE),
            fee_rate: Set(Decimal::ZERO),
            fee_amount: Set(Decimal::ZERO),
            currency: Set("CNY".into()),
            status: Set("pending".into()),
            remark: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap();
    }

    pub async fn payment(&self, id: i64) -> payments::Model {
        payments::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
    }
}

/// Test RSA key pair shared with the Go vector generator (PKCS#8 / PKIX PEM).
pub const PRIV_PEM: &str = include_str!("keys/priv.pem");
pub const PUB_PEM: &str = include_str!("keys/pub.pem");

/// PKCS#1 v1.5 SHA-256 signature (base64).
pub fn rsa_sha256(content: &str) -> String {
    let key = zs_infra::payment::common::parse_rsa_private_key(PRIV_PEM).unwrap();
    zs_infra::payment::common::rsa_sign(
        &key,
        zs_infra::payment::common::RsaHash::Sha256,
        content.as_bytes(),
    )
    .unwrap()
}
