//! Wiring of the `payment` group.

use std::sync::Arc;

use zs_app::identity::rate_limit::RateLimiter;
use zs_app::payment::callback::CallbackDeps;
use zs_app::payment::{
    CALLBACK_RATE_RULE, CallbackService, ChannelService, PaymentAdminService, PaymentServices,
};
use zs_domain::payment::settlement::PaymentSettlement;

use super::WireCtx;
use crate::db::repo::payment::QueuedPaymentAlerts;
use crate::db::repo::payment::channel::SeaChannelRepo;
use crate::db::repo::payment::records::SeaPaymentRepo;
use crate::payment::common::{GatewayEnv, OsEntropy};
use crate::payment::http::ReqwestTransport;
use crate::payment::{build_registry, http::HttpTransport};
use crate::queue::JobRegistry;

/// Builds the `payment` services with real HTTP gateways.
pub fn build(ctx: &WireCtx) -> PaymentServices {
    let env = GatewayEnv::new(
        Arc::new(ReqwestTransport::new()),
        ctx.clock.clone(),
        Arc::new(OsEntropy),
    );
    build_with(ctx, env, None)
}

/// Builds the services with a custom gateway transport (integration tests use a mock).
pub fn build_with_transport(ctx: &WireCtx, http: Arc<dyn HttpTransport>) -> PaymentServices {
    build_with(
        ctx,
        GatewayEnv::new(http, ctx.clock.clone(), Arc::new(OsEntropy)),
        None,
    )
}

/// Builds the services; `settlement` overrides the payment-row settlement (the order group
/// passes its order-aware implementation here).
pub fn build_with(
    ctx: &WireCtx,
    env: GatewayEnv,
    settlement: Option<Arc<dyn PaymentSettlement>>,
) -> PaymentServices {
    let registry = Arc::new(build_registry(&env));
    let channels = Arc::new(SeaChannelRepo::new(ctx.db.clone()));
    let payments = Arc::new(SeaPaymentRepo::new(ctx.db.clone()));
    // The order group's order-aware settlement (recharges delegated to the wallet group).
    let settlement = settlement.unwrap_or_else(|| super::order::settlement(ctx));
    PaymentServices {
        channels: Arc::new(ChannelService::new(channels.clone(), registry.clone())),
        payments: Arc::new(PaymentAdminService::new(payments.clone(), channels.clone())),
        callbacks: Arc::new(CallbackService::new(CallbackDeps {
            payments,
            channels,
            registry,
            settlement,
            alerts: Arc::new(QueuedPaymentAlerts::new(
                ctx.queue.clone(),
                ctx.clock.clone(),
            )),
            settings: ctx.settings.clone(),
            clock: ctx.clock.clone(),
        })),
        callback_limiter: RateLimiter::new(CALLBACK_RATE_RULE, ctx.clock.clone()),
    }
}

/// Registers the `payment` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, _services: &zs_app::Services, _registry: &mut JobRegistry) {}

/// Clears the cached `/public/config` (which embeds the payment channels) on channel changes.
pub async fn bootstrap(_ctx: &WireCtx, services: &zs_app::Services) -> zs_domain::Result<()> {
    let settings = services.content.settings.clone();
    services
        .payment
        .channels
        .on_change(Arc::new(move || settings.invalidate_public_config()));
    Ok(())
}
