//! Harness of the reseller integration tests: in-memory SQLite, real router, a
//! controllable clock, Host headers and seeding helpers for products / orders.

#![allow(
    dead_code,
    reason = "shared by several reseller test binaries that use different helpers"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test harness: failures should abort the test"
)]

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::{DateTime, Duration, TimeZone, Utc};
use http_body_util::BodyExt;
use sea_orm::{ActiveModelTrait, EntityTrait, IntoActiveModel};
use serde_json::{Value, json};
use tower::ServiceExt;
use zs_app::config::Config;
use zs_domain::reseller::pricing::{
    OrderLine, OrderPricingContext, SettingIndex, price_order_line,
};
use zs_domain::settings::SettingsStore;
use zs_shared::clock::Clock;
use zs_shared::money::Amount;

/// Compliance statement segments (see `zs_domain::identity::compliance`).
const SEGMENTS: [&str; 3] = [
    "我已阅读并理解上述合规声明提醒",
    "知悉相关法律风险",
    "并确认自行承担部署运营和收费行为产生的法律责任",
];

pub const BASE: &str = "shop.example.com";
pub const MAIN: &str = "main.example.com";

/// A clock the tests move forward.
#[derive(Debug)]
pub struct TestClock(pub Mutex<DateTime<Utc>>);

impl Clock for TestClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

/// Real "now" (JWTs are validated against the wall clock), truncated to seconds.
pub fn start() -> DateTime<Utc> {
    let now = Utc::now();
    Utc.timestamp_opt(now.timestamp(), 0).single().unwrap()
}

pub fn config() -> Config {
    let mut cfg = Config::default();
    cfg.app.secret_key = "test-app-secret-0123456789abcdef".into();
    cfg.jwt.secret = "test-admin-jwt-secret-0123456789".into();
    cfg.user_jwt.secret = "test-user-jwt-secret-0123456789a".into();
    cfg.database.url = "sqlite::memory:".into();
    cfg.database.max_connections = 1;
    cfg.bootstrap.default_admin_username = "admin".into();
    cfg.bootstrap.default_admin_password = "Admin12345".into();
    cfg.security.login_rate_limit.max_attempts = 1000;
    cfg.reseller.enabled = true;
    cfg.reseller.subdomain_base = BASE.into();
    cfg.reseller.main_hosts = vec!["localhost".into(), "127.0.0.1".into(), MAIN.into()];
    cfg.reseller.settlement_confirm_days = 7;
    cfg
}

pub struct App {
    pub router: axum::Router,
    pub db: sea_orm::DatabaseConnection,
    pub services: zs_app::Services,
    pub cfg: Config,
    pub clock: Arc<TestClock>,
    pub admin: String,
}

impl App {
    pub async fn new() -> Self {
        Self::with(config()).await
    }

    /// App with `cfg`; logs the admin in and acknowledges the compliance statement.
    pub async fn with(cfg: Config) -> Self {
        let me = Self::without_ack(cfg).await;
        let ack = me
            .admin_call(
                "POST",
                "/api/v1/admin/compliance/acknowledge",
                Some(json!({"segment1": SEGMENTS[0], "segment2": SEGMENTS[1], "segment3": SEGMENTS[2]})),
            )
            .await;
        data(&ack);
        me
    }

    /// App with the admin logged in but the compliance statement not acknowledged.
    pub async fn without_ack(cfg: Config) -> Self {
        let db = zs_infra::db::connect(&cfg.database).await.unwrap();
        zs_infra::db::sync_schema(&db).await.unwrap();
        let clock = Arc::new(TestClock(Mutex::new(start())));
        let mut ctx = zs_infra::wire::WireCtx::new(&db, &cfg);
        ctx.clock = clock.clone();
        let services = zs_infra::wire::services(&ctx);
        zs_infra::wire::bootstrap(&ctx, &services).await.unwrap();
        let app = zs_api::build(zs_api::AppState::new(services.clone(), cfg.clone()));
        let mut me = Self {
            router: app.router,
            db,
            services,
            cfg,
            clock,
            admin: String::new(),
        };
        let res = me
            .call(
                "POST",
                "/api/v1/admin/login",
                Some(json!({"username": "admin", "password": "Admin12345"})),
                None,
            )
            .await;
        me.admin = data(&res)["token"].as_str().unwrap().to_owned();
        me
    }

    pub fn advance(&self, by: Duration) {
        let mut now = self.clock.0.lock().unwrap();
        *now += by;
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Raw request with an optional Host and extra headers.
    pub async fn raw(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
        headers: &[(&str, &str)],
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        if let Some(t) = token {
            req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
        }
        for (k, v) in headers {
            req = req.header(*k, *v);
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

    /// Raw GET returning the body as text (sitemap).
    pub async fn text(&self, uri: &str, headers: &[(&str, &str)]) -> (StatusCode, String) {
        let mut req = Request::builder().method("GET").uri(uri);
        for (k, v) in headers {
            req = req.header(*k, *v);
        }
        let res = self
            .router
            .clone()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    pub async fn call(
        &self,
        method: &str,
        uri: &str,
        body: Option<Value>,
        token: Option<&str>,
    ) -> Value {
        self.raw(method, uri, body, token, &[]).await.1
    }

    pub async fn admin_call(&self, method: &str, uri: &str, body: Option<Value>) -> Value {
        self.call(method, uri, body, Some(&self.admin)).await
    }

    pub async fn set_setting(&self, key: &str, value: Value) {
        zs_infra::db::repo::settings::SeaSettingsStore::new(self.db.clone())
            .set(key, &value)
            .await
            .unwrap();
    }

    /// Registers a user and returns `(user_id, token)`.
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
        use zs_infra::db::entity::users;
        let id = users::Entity::find()
            .filter(users::Column::Email.eq(email))
            .one(&self.db)
            .await
            .unwrap()
            .unwrap()
            .id;
        (id, token)
    }

    /// A user with an approved (active) reseller profile: `(user_id, token, profile_id)`.
    pub async fn reseller(&self, email: &str, max_markup: &str) -> (i64, String, i64) {
        let (uid, token) = self.user(email).await;
        let applied = self
            .call(
                "POST",
                "/api/v1/reseller/apply",
                Some(json!({"reason": "hi"})),
                Some(&token),
            )
            .await;
        let pid = data(&applied)["id"].as_i64().unwrap();
        let approved = self
            .admin_call(
                "POST",
                &format!("/api/v1/admin/resellers/profiles/{pid}/approve"),
                Some(json!({"default_markup_percent": "0", "max_markup_percent": max_markup})),
            )
            .await;
        data(&approved);
        (uid, token, pid)
    }

    /// Assigns `label.shop.example.com` to a reseller and returns the host.
    pub async fn system_domain(&self, pid: i64, label: &str) -> String {
        let res = self
            .admin_call(
                "PUT",
                &format!("/api/v1/admin/resellers/profiles/{pid}/system-domain"),
                Some(json!({"subdomain": label})),
            )
            .await;
        data(&res)["domain"].as_str().unwrap().to_owned()
    }

    /// Creates an active category + product with SKUs `(price, cost)`; returns `(product_id, sku_ids)`.
    pub async fn product(&self, slug: &str, price: &str, skus: &[(&str, &str)]) -> (i64, Vec<i64>) {
        use zs_infra::db::entity::{categories, product_skus, products};
        let now = self.now();
        let cat = categories::Model {
            id: 0,
            parent_id: 0,
            slug: format!("cat-{slug}"),
            name_json: Some(json!({"zh-CN": "分类"})),
            icon: String::new(),
            sort_order: 0,
            is_active: true,
            created_at: now,
            deleted_at: None,
        };
        let mut cat = cat.into_active_model();
        cat.id = sea_orm::NotSet;
        let cat = cat.insert(&self.db).await.unwrap();
        let p = products::Model {
            id: 0,
            category_id: cat.id,
            slug: slug.into(),
            seo_meta_json: None,
            title_json: Some(json!({"zh-CN": format!("商品 {slug}")})),
            description_json: Some(json!({})),
            content_json: None,
            instructions_json: None,
            price_amount: price.parse().unwrap(),
            cost_price_amount: 0.into(),
            wholesale_prices: None,
            images: None,
            tags: None,
            purchase_type: "member".into(),
            min_purchase_quantity: 0,
            max_purchase_quantity: 0,
            stock_display_mode: "exact".into(),
            fulfillment_type: "manual".into(),
            manual_form_schema_json: None,
            manual_stock_total: -1,
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
        };
        let mut p = p.into_active_model();
        p.id = sea_orm::NotSet;
        let p = p.insert(&self.db).await.unwrap();
        let mut ids = Vec::new();
        for (i, (sku_price, cost)) in skus.iter().enumerate() {
            let s = product_skus::Model {
                id: 0,
                product_id: p.id,
                sku_code: format!("SKU{i}"),
                spec_values_json: Some(json!({"zh-CN": format!("规格{i}")})),
                price_amount: sku_price.parse().unwrap(),
                cost_price_amount: cost.parse().unwrap(),
                manual_stock_total: -1,
                manual_stock_locked: 0,
                manual_stock_sold: 0,
                is_active: true,
                sort_order: 0,
                created_at: now,
                updated_at: now,
                deleted_at: None,
            };
            let mut s = s.into_active_model();
            s.id = sea_orm::NotSet;
            ids.push(s.insert(&self.db).await.unwrap().id);
        }
        (p.id, ids)
    }

    /// Seeds a paid reseller order for `sku` (base price `base`, reseller unit `unit`)
    /// with its snapshot through the order-group port; returns `(order_id, order_no)`.
    #[expect(clippy::too_many_arguments, reason = "test fixture")]
    pub async fn order(
        &self,
        pid: i64,
        reseller_user: i64,
        buyer: i64,
        product_id: i64,
        sku_id: i64,
        base: &str,
        unit: &str,
        status: &str,
    ) -> (i64, String) {
        use zs_infra::db::entity::{order_items, orders};
        let now = self.now();
        let profile = self
            .services
            .reseller
            .pricing
            .active_profile(pid)
            .await
            .unwrap();
        let settings = self
            .services
            .reseller
            .pricing
            .repo
            .settings_for_pricing(pid, &[product_id], &[sku_id])
            .await
            .unwrap();
        let mut ctx =
            OrderPricingContext::new(pid, "r.shop.example.com", "CNY", reseller_user, buyer);
        let line = OrderLine {
            product_id,
            sku_id,
            quantity: 1,
            base_unit: base.parse().unwrap(),
            cost: Amount::ZERO,
        };
        let mut item = price_order_line(&profile, &SettingIndex::new(&settings), &line).unwrap();
        // tests may force a unit price different from the rules
        let unit: Amount = unit.parse().unwrap();
        item.reseller_unit_amount = unit.decimal();
        item.reseller_total_amount = unit.decimal();
        item.profit_amount = unit.decimal() - item.base_total_amount;
        ctx.push(item);
        let related = self
            .services
            .reseller
            .pricing
            .repo
            .is_related_account(pid, buyer)
            .await
            .unwrap();
        ctx.apply_self_dealing_risk(profile.user_id, related);
        let no = format!("DJ{}{}", pid, rand_suffix(&self.db).await);
        let o = orders::Model {
            id: 0,
            order_no: no.clone(),
            parent_id: None,
            user_id: buyer,
            guest_email: String::new(),
            guest_password: String::new(),
            guest_locale: String::new(),
            status: status.into(),
            currency: "CNY".into(),
            original_amount: unit.decimal(),
            discount_amount: 0.into(),
            member_discount_amount: 0.into(),
            promotion_discount_amount: 0.into(),
            wholesale_discount_amount: 0.into(),
            total_amount: unit.decimal(),
            wallet_paid_amount: 0.into(),
            online_paid_amount: unit.decimal(),
            refunded_amount: 0.into(),
            member_level_id: None,
            coupon_id: None,
            promotion_id: None,
            affiliate_profile_id: None,
            affiliate_code: String::new(),
            reseller_id: Some(pid),
            reseller_domain: "r.shop.example.com".into(),
            reseller_profit_amount: ctx.effective_profit,
            client_ip: String::new(),
            risk_ip: String::new(),
            expires_at: None,
            paid_at: (status != "pending_payment").then_some(now),
            canceled_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        let mut o = o.into_active_model();
        o.id = sea_orm::NotSet;
        let o = o.insert(&self.db).await.unwrap();
        let it = order_items::Model {
            id: 0,
            order_id: o.id,
            product_id,
            sku_id,
            title_json: Some(json!({"zh-CN": "商品"})),
            sku_snapshot_json: Some(json!({})),
            tags: None,
            original_unit_price: unit.decimal(),
            unit_price: unit.decimal(),
            cost_price: 0.into(),
            quantity: 1,
            original_total_price: unit.decimal(),
            total_price: unit.decimal(),
            coupon_discount: 0.into(),
            member_discount: 0.into(),
            promotion_discount: 0.into(),
            wholesale_discount: 0.into(),
            promotion_id: None,
            fulfillment_type: "manual".into(),
            manual_form_schema_snapshot_json: None,
            manual_form_submission_json: None,
            instructions_json: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        };
        let mut it = it.into_active_model();
        it.id = sea_orm::NotSet;
        let it = it.insert(&self.db).await.unwrap();
        ctx.bind_created_order_item(0, o.id, it.id);
        zs_infra::db::repo::reseller::ledger::create_order_snapshot_in(&self.db, o.id, &ctx, now)
            .await
            .unwrap();
        (o.id, no)
    }
}

async fn rand_suffix(db: &sea_orm::DatabaseConnection) -> u64 {
    use sea_orm::PaginatorTrait;
    zs_infra::db::entity::orders::Entity::find()
        .count(db)
        .await
        .unwrap()
        + 1
}

/// Asserts a success envelope and returns `data`.
pub fn data(v: &Value) -> &Value {
    assert_eq!(v["status_code"], 0, "expected success, got {v}");
    assert_eq!(v["msg"], "success");
    &v["data"]
}

/// Asserts an error envelope with `code` and zh-CN message `msg`.
pub fn err(v: &Value, code: i64, msg: &str) {
    assert_eq!(v["status_code"], code, "unexpected envelope {v}");
    assert_eq!(v["msg"], msg, "unexpected envelope {v}");
}
