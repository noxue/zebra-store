//! `integration` use cases: API credentials, the supplier APIs we serve
//! (legacy compatibility on `/upstream`, `zebra-store` on `/zs`), site connections through
//! protocol adapters, product mappings + sync, procurement, inbound supplier events,
//! downstream callbacks and reconciliation.

pub mod connection;
pub mod credential;
pub mod downstream;
pub mod inbound;
pub mod mapping;
pub mod procurement;
pub mod provide;
pub mod reconciliation;
pub mod supplier;
pub mod zs_supplier;

use std::sync::Arc;

use async_trait::async_trait;
use zs_domain::integration::hooks::IntegrationOrderEvents;
use zs_domain::{Id, Result};

use crate::identity::rate_limit::RateLimiter;

/// Services of the `integration` group.
#[derive(Clone)]
pub struct IntegrationServices {
    pub credentials: credential::CredentialService,
    pub connections: connection::ConnectionService,
    pub mappings: mapping::MappingService,
    pub procurement: procurement::ProcurementService,
    pub downstream: downstream::DownstreamService,
    pub reconciliation: reconciliation::ReconciliationService,
    pub supplier: supplier::SupplierService,
    /// Protocol-neutral inbound supplier callbacks / events.
    pub inbound: inbound::InboundService,
    /// The `zebra-store` protocol we serve.
    pub zs: zs_supplier::ZsSupplier,
    /// Port the order group calls on payment / status changes.
    pub order_events: Arc<dyn IntegrationOrderEvents>,
    /// `/upstream/*` limiter keyed by `IP|API key` (60 / min).
    pub api_limiter: RateLimiter,
    /// `/upstream/callback` and `/zs/events` limiter keyed by IP.
    pub callback_limiter: RateLimiter,
    /// `/zs/*` limiter keyed by API key (120 / min).
    pub zs_limiter: RateLimiter,
}

impl std::fmt::Debug for IntegrationServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IntegrationServices")
    }
}

/// [`IntegrationOrderEvents`] backed by the procurement, downstream, mapping and
/// zebra-store event services.
#[derive(Debug, Clone)]
pub struct OrderEvents {
    pub procurement: procurement::ProcurementService,
    pub downstream: downstream::DownstreamService,
    pub mappings: mapping::MappingService,
    pub zs: zs_supplier::ZsSupplier,
}

impl OrderEvents {
    async fn zs_event(&self, order_id: Id) {
        if let Err(error) = self.zs.order_changed(order_id).await {
            tracing::warn!(%error, order_id, "zs order event failed");
        }
    }
}

#[async_trait]
impl IntegrationOrderEvents for OrderEvents {
    async fn order_paid(&self, order_id: Id) -> Result<()> {
        let created = self.procurement.create_for_order(order_id).await;
        self.downstream.enqueue_for_order(order_id).await;
        self.zs_event(order_id).await;
        created.map(|_| ())
    }

    async fn order_status_changed(&self, order_id: Id) -> Result<()> {
        self.downstream.enqueue_for_order(order_id).await;
        self.zs_event(order_id).await;
        Ok(())
    }

    async fn ensure_upstream_stock(&self, sku_id: Id, quantity: i32) -> Result<()> {
        self.mappings.ensure_upstream_stock(sku_id, quantity).await
    }

    async fn procurement_issues(
        &self,
        local_order_ids: &[Id],
    ) -> Result<std::collections::HashMap<Id, String>> {
        self.procurement.issues(local_order_ids).await
    }
}
