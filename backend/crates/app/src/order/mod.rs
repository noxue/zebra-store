//! `order` use cases: cart, pricing preview and checkout, order queries and cancellation,
//! payments and settlement, delivery, refunds, admin order management and status emails.
//!
//! One [`OrderService`] carries the dependencies; its methods are split by concern across
//! the submodules. Multi-table writes go through the transactional ports of
//! `zs_domain::order::ports`; side effects after commit go through the job queue.

pub mod admin;
pub mod affiliate;
pub mod cart;
pub mod checkout;
pub mod effects;
pub mod email;
pub mod fulfillment;
pub mod jobs;
pub mod payment;
pub mod query;
pub mod refund;
pub mod risk;
pub mod settlement;
pub mod upstream;

use std::sync::Arc;

use zs_domain::Result;
use zs_domain::catalog::ordering::CatalogOrdering;
use zs_domain::integration::hooks::IntegrationOrderEvents;
use zs_domain::marketing::coupon::CouponRepo;
use zs_domain::marketing::member_level::MemberLevelRepo;
use zs_domain::marketing::promotion::PromotionRepo;
use zs_domain::notify::ports::Notifier;
use zs_domain::order::ports::{
    AffiliateHooks, CartRepo, OrderPaymentStore, OrderRepo, OrderStore, OrderWallet,
};
use zs_domain::payment::channel::ChannelRepo;
use zs_domain::payment::gateway::GatewayRegistry;
use zs_domain::payment::model::PaymentRepo;
use zs_domain::queue::JobQueue;
use zs_domain::settings::schema::risk::OrderRiskSetting;
use zs_domain::settings::schema::site::{
    OrderSetting, PaymentFeeSetting, SiteBrand, WalletSetting, normalize_currency,
};
use zs_domain::settings::schema::smtp::SmtpSetting;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_shared::clock::Clock;

use crate::identity::rate_limit::{RateLimiter, RateRule};
use crate::marketing::member_level::MemberLevelService;
use crate::reseller::PricingReads;

pub use email::{OrderMail, OrderMailer};

/// Guest read limit per IP (original `guestReadRule`: 120 / 60 s, 60 s block).
pub const GUEST_READ_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: 120,
    block_seconds: 60,
};

/// Guest write limit per IP (original `guestWriteRule`: 20 / 60 s, 300 s block).
pub const GUEST_WRITE_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: 20,
    block_seconds: 300,
};

/// Captcha scene of guest order creation.
pub const CAPTCHA_SCENE_GUEST_CREATE_ORDER: &str = "guest_create_order";

/// Everything the order use cases depend on.
#[derive(Clone)]
pub struct OrderDeps {
    pub repo: Arc<dyn OrderRepo>,
    pub store: Arc<dyn OrderStore>,
    pub payments: Arc<dyn OrderPaymentStore>,
    pub payment_records: Arc<dyn PaymentRepo>,
    pub payment_channels: Arc<dyn ChannelRepo>,
    pub wallet: Arc<dyn OrderWallet>,
    pub cart: Arc<dyn CartRepo>,
    pub catalog: Arc<dyn CatalogOrdering>,
    pub promotions: Arc<dyn PromotionRepo>,
    pub levels: Arc<dyn MemberLevelRepo>,
    pub coupons: Arc<dyn CouponRepo>,
    pub member_levels: MemberLevelService,
    pub reseller_pricing: PricingReads,
    pub reseller_confirm_days: i64,
    pub affiliate: Arc<dyn AffiliateHooks>,
    /// Procurement / downstream callbacks of the integration group.
    pub integration: Option<Arc<dyn IntegrationOrderEvents>>,
    pub notifier: Arc<dyn Notifier>,
    pub registry: Arc<GatewayRegistry>,
    pub mailer: Arc<dyn OrderMailer>,
    /// Brand of status mails (main shop / reseller site, NTF-02).
    pub mail_brands: Arc<dyn zs_domain::identity::mailer::MailBrands>,
    pub queue: Arc<dyn JobQueue>,
    pub settings: Arc<dyn SettingsStore>,
    pub clock: Arc<dyn Clock>,
    /// `order` section of the configuration file (fallback of `order_config`).
    pub order_fallback: OrderSetting,
    /// Guest credential key (`app.secret_key`, ORD-02).
    pub guest_secret: String,
    /// Fallback SMTP settings (`email` section of the configuration file).
    pub smtp_fallback: SmtpSetting,
}

impl std::fmt::Debug for OrderDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OrderDeps")
    }
}

/// The order use cases.
#[derive(Debug, Clone)]
pub struct OrderService {
    deps: Arc<OrderDeps>,
    risk_limits: risk::RiskLimiters,
}

impl OrderService {
    pub fn new(deps: OrderDeps) -> Self {
        let clock = deps.clock.clone();
        Self {
            deps: Arc::new(deps),
            risk_limits: risk::RiskLimiters::new(clock),
        }
    }

    pub fn deps(&self) -> &OrderDeps {
        &self.deps
    }

    async fn raw_setting(&self, key: &str) -> Option<serde_json::Value> {
        match self.deps.settings.get(key).await {
            Ok(v) => v,
            Err(error) => {
                tracing::warn!(%error, key, "order_setting_read_failed");
                None
            }
        }
    }

    /// `site_config.currency` (default `CNY`).
    pub async fn site_currency(&self) -> String {
        let site = self.raw_setting(setting_keys::SITE_CONFIG).await;
        normalize_currency(site.as_ref().and_then(|s| s.get("currency")))
    }

    /// `site_config.brand`.
    pub async fn site_brand(&self) -> SiteBrand {
        SiteBrand::decode(self.raw_setting(setting_keys::SITE_CONFIG).await.as_ref())
    }

    /// `order_config` over the configuration file.
    pub async fn order_setting(&self) -> OrderSetting {
        OrderSetting::decode(
            self.raw_setting(setting_keys::ORDER_CONFIG).await.as_ref(),
            self.deps.order_fallback,
        )
    }

    pub async fn wallet_setting(&self) -> WalletSetting {
        WalletSetting::decode(self.raw_setting(setting_keys::WALLET_CONFIG).await.as_ref())
    }

    pub async fn fee_setting(&self) -> PaymentFeeSetting {
        PaymentFeeSetting::decode(
            self.raw_setting(setting_keys::PAYMENT_CONFIG)
                .await
                .as_ref(),
        )
    }

    /// `order_risk_control_config`; read errors propagate so callers can fail closed.
    pub async fn risk_setting(&self) -> Result<OrderRiskSetting> {
        let raw = self
            .deps
            .settings
            .get(setting_keys::ORDER_RISK_CONTROL_CONFIG)
            .await?;
        Ok(OrderRiskSetting::decode(
            raw.as_ref(),
            OrderRiskSetting::default(),
        ))
    }

    /// `smtp_config` over the configuration file.
    pub async fn smtp_setting(&self) -> Result<SmtpSetting> {
        let raw = self.deps.settings.get(setting_keys::SMTP_CONFIG).await?;
        Ok(SmtpSetting::decode(
            raw.as_ref(),
            self.deps.smtp_fallback.clone(),
        ))
    }
}

/// Services of the `order` group.
#[derive(Clone)]
pub struct OrderServices {
    pub service: OrderService,
    /// Order-aware payment settlement (callbacks and captures).
    pub settlement: Arc<dyn zs_domain::payment::settlement::PaymentSettlement>,
    pub guest_read_limiter: RateLimiter,
    pub guest_write_limiter: RateLimiter,
}

impl std::fmt::Debug for OrderServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OrderServices").finish_non_exhaustive()
    }
}
