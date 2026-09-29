//! Wiring of the `order` group.
//!
//! Ports for other groups:
//! - [`settlement`] is the order-aware `PaymentSettlement` (wallet recharges delegated to
//!   the wallet group); `wire::payment::build` uses it for gateway callbacks.
//! - [`integration_ports`] is the order group's `UpstreamOrdering` and
//!   `ProcurementLifecycle` (default of `wire::integration::build_with`).

use std::sync::Arc;

use zs_app::identity::rate_limit::RateLimiter;
use zs_app::marketing::member_level::MemberLevelService;
use zs_app::notify::center::QueueNotifier;
use zs_app::order::affiliate::ServiceAffiliateHooks;
use zs_app::order::jobs::{AutoFulfillJob, StatusEmailJob, TimeoutCancelJob};
use zs_app::order::settlement::OrderSettlement;
use zs_app::order::upstream::OrderIntegrationPorts;
use zs_app::order::{GUEST_READ_RULE, GUEST_WRITE_RULE, OrderDeps, OrderService, OrderServices};
use zs_app::reseller::PricingReads;
use zs_domain::integration::hooks::IntegrationOrderEvents;
use zs_domain::payment::settlement::PaymentSettlement;
use zs_domain::queue::kinds;
use zs_domain::settings::schema::site::OrderSetting;
use zs_domain::settings::schema::smtp::{SmtpSetting, SmtpVerifyCode};

use super::{WireCtx, integration};
use crate::db::repo::catalog::ordering::SeaCatalogOrdering;
use crate::db::repo::marketing::coupon::SeaCouponRepo;
use crate::db::repo::marketing::member_level::SeaMemberLevelRepo;
use crate::db::repo::marketing::promotion::SeaPromotionRepo;
use crate::db::repo::order::SeaOrderStore;
use crate::db::repo::order::cart::SeaCartRepo;
use crate::db::repo::order::read::SeaOrderRepo;
use crate::db::repo::payment::channel::SeaChannelRepo;
use crate::db::repo::payment::records::SeaPaymentRepo;
use crate::db::repo::reseller::SeaResellerStore;
use crate::identity::mail::SmtpMailer;
use crate::order::mail::SmtpOrderMailer;
use crate::payment::build_registry;
use crate::payment::common::{GatewayEnv, OsEntropy};
use crate::payment::http::ReqwestTransport;
use crate::queue::JobRegistry;

fn smtp_fallback(ctx: &WireCtx) -> SmtpSetting {
    let e = &ctx.cfg.email;
    SmtpSetting {
        enabled: e.enabled,
        host: e.host.clone(),
        port: i64::from(e.port),
        username: e.username.clone(),
        password: e.password.clone(),
        from: e.from.clone(),
        from_name: e.from_name.clone(),
        use_tls: e.use_tls,
        use_ssl: e.use_ssl,
        order_notification_enabled: true,
        verify_code: SmtpVerifyCode {
            expire_minutes: e.verify_code.expire_minutes,
            send_interval_seconds: e.verify_code.send_interval_seconds,
            max_attempts: i64::from(e.verify_code.max_attempts),
            length: i64::try_from(e.verify_code.length).unwrap_or(6),
        },
    }
    .normalized()
}

fn default_env(ctx: &WireCtx) -> GatewayEnv {
    GatewayEnv::new(
        Arc::new(ReqwestTransport::new()),
        ctx.clock.clone(),
        Arc::new(OsEntropy),
    )
}

/// The order group's implementation of the integration ports (`UpstreamOrdering`,
/// `ProcurementLifecycle`), used by `wire::integration::build_with` by default.
pub fn integration_ports(ctx: &WireCtx) -> Arc<OrderIntegrationPorts> {
    ports(ctx, &default_env(ctx))
}

fn ports(ctx: &WireCtx, env: &GatewayEnv) -> Arc<OrderIntegrationPorts> {
    Arc::new(OrderIntegrationPorts(base_service(ctx, env, None), None))
}

/// Integration port with card conversion enabled. The full event graph cannot be
/// injected here because this port is itself built while the integration graph wires.
pub fn integration_ports_with_converter(
    ctx: &WireCtx,
    converters: zs_app::integration::card_converter::CardConverterService,
) -> Arc<OrderIntegrationPorts> {
    Arc::new(OrderIntegrationPorts(
        base_service(ctx, &default_env(ctx), None),
        Some(converters),
    ))
}

/// The order use cases with gateways on `env` (tests pass a mock transport), wired to
/// the integration group's order events (procurement, downstream callbacks).
pub fn service_with_env(ctx: &WireCtx, env: &GatewayEnv) -> OrderService {
    let ports = ports(ctx, env);
    let events = integration::build_with(
        ctx,
        &integration::Adapters {
            ordering: Some(ports.clone()),
            lifecycle: Some(ports),
            ..integration::Adapters::from_config(&ctx.cfg)
        },
    )
    .order_events;
    base_service(ctx, env, Some(events))
}

fn base_service(
    ctx: &WireCtx,
    env: &GatewayEnv,
    integration: Option<Arc<dyn IntegrationOrderEvents>>,
) -> OrderService {
    let db = &ctx.db;
    let store = Arc::new(SeaOrderStore::new(db.clone(), &ctx.cfg.app.secret_key));
    let levels = Arc::new(SeaMemberLevelRepo::new(db.clone()));
    let reseller = Arc::new(SeaResellerStore::new(db.clone()));
    OrderService::new(OrderDeps {
        repo: Arc::new(SeaOrderRepo::new(db.clone())),
        store: store.clone(),
        payments: store.clone(),
        payment_records: Arc::new(SeaPaymentRepo::new(db.clone())),
        payment_channels: Arc::new(SeaChannelRepo::new(db.clone())),
        wallet: store,
        cart: Arc::new(SeaCartRepo::new(db.clone())),
        catalog: Arc::new(SeaCatalogOrdering::new(db.clone())),
        promotions: Arc::new(SeaPromotionRepo::new(db.clone())),
        levels: levels.clone(),
        coupons: Arc::new(SeaCouponRepo::new(db.clone())),
        member_levels: MemberLevelService::new(levels.clone(), levels, ctx.clock.clone()),
        reseller_pricing: PricingReads {
            profiles: reseller.clone(),
            repo: reseller,
        },
        reseller_confirm_days: ctx.cfg.reseller.settlement_confirm_days,
        affiliate: Arc::new(ServiceAffiliateHooks(super::affiliate::build(ctx).service)),
        integration,
        notifier: Arc::new(QueueNotifier::new(ctx.queue.clone())),
        registry: Arc::new(build_registry(env)),
        mailer: Arc::new(SmtpOrderMailer::new(SmtpMailer::new(
            ctx.settings.clone(),
            ctx.cfg.email.clone(),
            ctx.cipher.clone(),
        ))),
        mail_brands: super::identity::mail_brands(ctx),
        queue: ctx.queue.clone(),
        settings: ctx.settings.clone(),
        clock: ctx.clock.clone(),
        order_fallback: OrderSetting::from_config(
            ctx.cfg.order.payment_expire_minutes,
            ctx.cfg.order.max_refund_days,
        ),
        guest_secret: ctx.cfg.app.secret_key.clone(),
        smtp_fallback: smtp_fallback(ctx),
    })
}

/// The order-aware payment settlement; wallet recharge payments (`order_id = 0`) are
/// settled by the wallet group.
pub fn settlement(ctx: &WireCtx) -> Arc<dyn PaymentSettlement> {
    let orders: Arc<dyn PaymentSettlement> =
        Arc::new(OrderSettlement(service_with_env(ctx, &default_env(ctx))));
    let wallet = super::wallet::build(ctx);
    super::wallet::recharge_settlement(ctx, &wallet, orders)
}

/// Builds the `order` services with gateways on `env`.
pub fn build_with_env(ctx: &WireCtx, env: &GatewayEnv) -> OrderServices {
    let service = service_with_env(ctx, env);
    let orders: Arc<dyn PaymentSettlement> = Arc::new(OrderSettlement(service.clone()));
    let wallet = super::wallet::build(ctx);
    OrderServices {
        settlement: super::wallet::recharge_settlement(ctx, &wallet, orders),
        service,
        guest_read_limiter: RateLimiter::new(GUEST_READ_RULE, ctx.clock.clone()),
        guest_write_limiter: RateLimiter::new(GUEST_WRITE_RULE, ctx.clock.clone()),
    }
}

/// Builds the `order` services.
pub fn build(ctx: &WireCtx) -> OrderServices {
    build_with_env(ctx, &default_env(ctx))
}

/// Registers the `order` job handlers.
pub fn jobs(_ctx: &WireCtx, services: &zs_app::Services, registry: &mut JobRegistry) {
    let svc = &services.order.service;
    registry
        .handle(
            kinds::ORDER_AUTO_FULFILL,
            Arc::new(AutoFulfillJob(svc.clone())),
        )
        .handle(
            kinds::ORDER_TIMEOUT_CANCEL,
            Arc::new(TimeoutCancelJob(svc.clone())),
        )
        .handle(
            kinds::ORDER_STATUS_EMAIL,
            Arc::new(StatusEmailJob(svc.clone())),
        );
}
