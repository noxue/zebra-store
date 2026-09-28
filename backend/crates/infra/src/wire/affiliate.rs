//! Wiring of the `affiliate` group.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use zs_app::affiliate::{AffiliateService, AffiliateServices};
use zs_domain::queue::{JobHandler, kinds};

use super::WireCtx;
use crate::db::repo::affiliate::SeaAffiliateRepo;
use crate::queue::JobRegistry;

/// Interval of `affiliate:confirm_commissions` (original `@every 1m`, AFF-01).
const CONFIRM_INTERVAL: Duration = Duration::from_secs(60);

/// Builds the `affiliate` services.
pub fn build(ctx: &WireCtx) -> AffiliateServices {
    AffiliateServices {
        service: Arc::new(AffiliateService::new(
            Arc::new(SeaAffiliateRepo::new(ctx.db.clone())),
            ctx.settings.clone(),
            ctx.clock.clone(),
        )),
    }
}

/// `affiliate:confirm_commissions` handler (idempotent conditional update).
struct ConfirmCommissions(Arc<AffiliateService>);

#[async_trait]
impl JobHandler for ConfirmCommissions {
    async fn handle(&self, _payload: serde_json::Value) -> zs_domain::Result<()> {
        let confirmed = self.0.confirm_due().await?;
        if confirmed > 0 {
            tracing::info!(confirmed, "affiliate_commissions_confirmed");
        }
        Ok(())
    }
}

/// Registers the `affiliate` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, services: &zs_app::Services, registry: &mut JobRegistry) {
    registry
        .handle(
            kinds::AFFILIATE_CONFIRM,
            Arc::new(ConfirmCommissions(services.affiliate.service.clone())),
        )
        .every(kinds::AFFILIATE_CONFIRM, CONFIRM_INTERVAL);
}
