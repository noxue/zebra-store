//! Wiring of the `reseller` group.
//!
//! Ports for other groups:
//! - order: `services.reseller.ledger` (`on_order_paid` / `on_order_refunded`), or inside
//!   an existing transaction `db::repo::reseller::ledger::{post_order_profit_in,
//!   deduct_refund_in, create_order_snapshot_in}`; pricing via `services.reseller.pricing`
//!   plus `zs_domain::reseller::pricing::{price_order_line, OrderPricingContext}`.
//! - catalog: `services.reseller.pricing` + `zs_domain::reseller::pricing::display_prices`.
//! - content: the `/public/config` overlay is installed in [`bootstrap`].

use std::sync::Arc;
use std::time::Duration;

use zs_app::reseller::finance::FinanceService;
use zs_app::reseller::ledger::{ConfirmLedgerJob, LedgerService};
use zs_app::reseller::management::ManagementService;
use zs_app::reseller::operations::OperationsService;
use zs_app::reseller::orders::OrderQueryService;
use zs_app::reseller::product_setting::ProductSettingService;
use zs_app::reseller::site_config::SiteConfigService;
use zs_app::reseller::tenant::{TenantCache, TenantResolver};
use zs_app::reseller::{PricingReads, ResellerServices};
use zs_domain::queue::kinds;

use super::WireCtx;
use crate::db::repo::reseller::SeaResellerStore;
use crate::queue::JobRegistry;

/// Interval of `reseller:confirm_ledger` (original: `@every 1m`).
const CONFIRM_LEDGER_EVERY: Duration = Duration::from_secs(60);

/// Builds the `reseller` services.
pub fn build(ctx: &WireCtx) -> ResellerServices {
    let store = Arc::new(SeaResellerStore::new(ctx.db.clone()));
    let cfg = &ctx.cfg.reseller;
    let cache = Arc::new(TenantCache::default());
    ResellerServices {
        tenant: TenantResolver::new(store.clone(), cfg, cache.clone(), ctx.clock.clone()),
        management: ManagementService::new(store.clone(), cfg, cache, ctx.clock.clone()),
        site_config: SiteConfigService::new(store.clone(), store.clone(), ctx.clock.clone()),
        product_settings: ProductSettingService::new(
            store.clone(),
            store.clone(),
            ctx.clock.clone(),
        ),
        orders: OrderQueryService::new(store.clone(), store.clone()),
        finance: FinanceService::new(store.clone(), store.clone(), ctx.clock.clone()),
        ledger: LedgerService::new(
            store.clone(),
            cfg.settlement_confirm_days,
            ctx.clock.clone(),
        ),
        operations: OperationsService::new(store.clone(), ctx.clock.clone()),
        pricing: PricingReads {
            profiles: store.clone(),
            repo: store,
        },
    }
}

/// Registers `reseller:confirm_ledger` (every minute).
pub fn jobs(_ctx: &WireCtx, services: &zs_app::Services, registry: &mut JobRegistry) {
    registry
        .handle(
            kinds::RESELLER_CONFIRM_LEDGER,
            Arc::new(ConfirmLedgerJob(services.reseller.ledger.clone())),
        )
        .every(kinds::RESELLER_CONFIRM_LEDGER, CONFIRM_LEDGER_EVERY);
}

/// Installs the `/public/config` overlay and its cache invalidation (content group).
pub async fn bootstrap(_ctx: &WireCtx, services: &zs_app::Services) -> zs_domain::Result<()> {
    let site = services.reseller.site_config.clone();
    services.content.public_config.set_overlay(Arc::new(site));
    let settings = services.content.settings.clone();
    services
        .reseller
        .site_config
        .set_invalidator(Arc::new(move || settings.invalidate_public_config()));
    Ok(())
}
