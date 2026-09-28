//! Wiring of the provider-compat facades of the `integration` group (acg-faka
//! `/shared/*`, mcy OpenApi `/plugin/open-api/*`). Kept apart from
//! `wire/integration.rs` so supplier adapters and provider facades evolve
//! independently; both use the same repositories and the order group's
//! [`UpstreamOrdering`](zs_domain::integration::supplier::UpstreamOrdering).

use std::sync::Arc;

use zs_app::identity::rate_limit::{RateLimiter, RateRule};
use zs_app::integration::credential::CredentialService;
use zs_app::integration::provide::ProvideServices;
use zs_app::integration::provide::access::CompatAccess;
use zs_app::integration::provide::desk::SupplyDesk;
use zs_domain::integration::provide::COMPAT_REQUESTS_PER_MINUTE;
use zs_domain::integration::supplier::UpstreamOrdering;

use super::WireCtx;
use crate::db::repo::integration::credential::SeaCredentialRepo;
use crate::db::repo::integration::downstream::SeaOrderRefRepo;
use crate::db::repo::integration::provide::SeaCompatKeyRepo;
use crate::db::repo::integration::supplier::SeaSupplierCatalog;
use crate::db::repo::integration::zs::SeaZsStore;

/// Compat endpoints: [`COMPAT_REQUESTS_PER_MINUTE`] per `protocol|IP|app_id`, 30 s block.
const COMPAT_RATE_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: COMPAT_REQUESTS_PER_MINUTE,
    block_seconds: 30,
};

/// Builds the facades with the order group's ordering port.
pub fn build(ctx: &WireCtx) -> ProvideServices {
    build_with(ctx, super::order::integration_ports(ctx))
}

/// Builds the facades on an explicit ordering port (tests).
pub fn build_with(ctx: &WireCtx, ordering: Arc<dyn UpstreamOrdering>) -> ProvideServices {
    let db = &ctx.db;
    let desk = SupplyDesk::new(
        Arc::new(SeaSupplierCatalog::new(db.clone(), ctx.settings.clone())),
        ordering,
        Arc::new(SeaOrderRefRepo::new(db.clone())),
    );
    let credentials = CredentialService::new(
        Arc::new(SeaCredentialRepo::new(db.clone())),
        ctx.cipher.clone(),
        ctx.clock.clone(),
    )
    .with_security(Arc::new(SeaZsStore::new(db.clone())));
    let access = CompatAccess::new(
        Arc::new(SeaCompatKeyRepo::new(db.clone())),
        credentials,
        ctx.cipher.clone(),
        ctx.clock.clone(),
    );
    ProvideServices::new(
        desk,
        access,
        RateLimiter::new(COMPAT_RATE_RULE, ctx.clock.clone()),
    )
}
