//! Wiring of the `wallet` group.
//!
//! Ports for other groups:
//! - `db::repo::wallet::ledger::{change_balance, credit, debit, apply_order_balance,
//!   release_order_balance, count_order_transactions}` move money inside the caller's
//!   transaction (order payment/refund, underpaid credits — PAY-02);
//! - [`recharge_settlement`] wraps a [`PaymentSettlement`] so gateway callbacks of wallet
//!   recharges (`order_id = 0`) credit the wallet (WAL-01) — the order group should wrap
//!   its order-aware settlement with it when wiring the payment callbacks;
//! - `WalletServices.recharge.settle(..)` is the recharge completion function.

use std::sync::Arc;

use async_trait::async_trait;
use zs_app::identity::rate_limit::RateLimiter;
use zs_app::marketing::member_level::MemberLevelService;
use zs_app::wallet::{
    ExpirePayload, GIFT_CARD_REDEEM_RULE, RechargeDeps, RechargeService, WalletService,
    WalletServices,
};
use zs_domain::payment::settlement::PaymentSettlement;
use zs_domain::queue::{JobHandler, kinds};
use zs_domain::settings::schema::site::OrderSetting;
use zs_domain::wallet::RechargeGateway;

use super::WireCtx;
use crate::db::repo::marketing::member_level::SeaMemberLevelRepo;
use crate::db::repo::wallet::SeaWalletRepo;
use crate::db::repo::wallet::recharge::{
    QueuedRechargeHooks, RechargeAwareSettlement, RegistryRechargeGateway, SeaRechargeStore,
};
use crate::payment::build_registry;
use crate::payment::common::{GatewayEnv, OsEntropy};
use crate::payment::http::ReqwestTransport;
use crate::queue::JobRegistry;

/// Builds the `wallet` services with the real gateway registry.
pub fn build(ctx: &WireCtx) -> WalletServices {
    build_with_gateway(ctx, None)
}

/// Builds the services; `gateway` replaces the registry-backed recharge gateway (tests).
pub fn build_with_gateway(
    ctx: &WireCtx,
    gateway: Option<Arc<dyn RechargeGateway>>,
) -> WalletServices {
    let wallets = Arc::new(SeaWalletRepo::new(ctx.db.clone()));
    let store = Arc::new(SeaRechargeStore::new(ctx.db.clone()));
    let gateway = gateway.unwrap_or_else(|| {
        let env = GatewayEnv::new(
            Arc::new(ReqwestTransport::new()),
            ctx.clock.clone(),
            Arc::new(OsEntropy),
        );
        Arc::new(RegistryRechargeGateway::new(
            Arc::new(build_registry(&env)),
            ctx.clock.clone(),
        ))
    });
    let levels = Arc::new(SeaMemberLevelRepo::new(ctx.db.clone()));
    let member_levels = MemberLevelService::new(levels.clone(), levels, ctx.clock.clone());
    let hooks = Arc::new(QueuedRechargeHooks::new(
        ctx.db.clone(),
        ctx.queue.clone(),
        member_levels,
    ));
    let recharge = RechargeService::new(RechargeDeps {
        wallets: wallets.clone(),
        store: store.clone(),
        lookup: store,
        gateway,
        hooks,
        queue: ctx.queue.clone(),
        settings: ctx.settings.clone(),
        clock: ctx.clock.clone(),
        order_fallback: OrderSetting::from_config(
            ctx.cfg.order.payment_expire_minutes,
            ctx.cfg.order.max_refund_days,
        ),
    });
    WalletServices {
        wallet: Arc::new(WalletService::new(
            wallets.clone(),
            wallets,
            ctx.settings.clone(),
            ctx.clock.clone(),
        )),
        recharge: Arc::new(recharge),
        redeem_limiter: RateLimiter::new(GIFT_CARD_REDEEM_RULE, ctx.clock.clone()),
    }
}

/// Wraps `inner` so wallet recharge payments are settled by the wallet group.
pub fn recharge_settlement(
    ctx: &WireCtx,
    services: &WalletServices,
    inner: Arc<dyn PaymentSettlement>,
) -> Arc<dyn PaymentSettlement> {
    Arc::new(RechargeAwareSettlement::new(
        services.recharge.clone(),
        inner,
        ctx.db.clone(),
    ))
}

/// `wallet_recharge:timeout_expire` handler.
struct ExpireRecharge(Arc<RechargeService>);

#[async_trait]
impl JobHandler for ExpireRecharge {
    async fn handle(&self, payload: serde_json::Value) -> zs_domain::Result<()> {
        let Ok(payload) = serde_json::from_value::<ExpirePayload>(payload) else {
            tracing::warn!("worker_wallet_recharge_expire_unmarshal_failed");
            return Ok(());
        };
        self.0.expire(payload.payment_id).await
    }
}

/// Registers the `wallet` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, services: &zs_app::Services, registry: &mut JobRegistry) {
    registry.handle(
        kinds::WALLET_RECHARGE_EXPIRE,
        Arc::new(ExpireRecharge(services.wallet.recharge.clone())),
    );
}
