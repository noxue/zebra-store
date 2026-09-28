//! Ports between the order group and the integration group.
//!
//! - [`ProcurementLifecycle`] is **implemented by the order group** and called by
//!   procurement when a supplier accepts, delivers or fails a purchase order.
//! - [`IntegrationOrderEvents`] is **implemented by this group** (see
//!   `zs_infra::wire::integration::order_events`) and called by the order group.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::catalog::product::JsonMap;
use crate::{Id, Result};

/// A supplier delivery to copy into the local order's fulfillment.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct UpstreamDelivery {
    /// Supplier fulfillment type (`auto` / `manual`).
    pub kind: String,
    pub status: String,
    /// Delivered content (card secrets / text).
    pub payload: String,
    /// Logistics / structured delivery data (`fulfillments.logistics_json`).
    pub delivery_data: JsonMap,
    pub delivered_at: Option<DateTime<Utc>>,
}

/// Order-side effects of the procurement state machine (implemented by the order group).
#[async_trait]
pub trait ProcurementLifecycle: Send + Sync {
    /// The supplier accepted the purchase order: local order `paid → fulfilling`
    /// (conditional update; other statuses are left alone).
    async fn mark_fulfilling(&self, order_id: Id) -> Result<()>;

    /// Records the supplier delivery on the local order **idempotently** (one
    /// fulfillment per order), marks the order delivered, recomputes the parent order
    /// status and enqueues the customer status email. Must return an error when the
    /// fulfillment could not be written: the purchase order then stays non-terminal so
    /// the supplier / poller retries (UPS-02).
    async fn deliver_upstream(&self, order_id: Id, delivery: &UpstreamDelivery) -> Result<()>;

    /// Terminal procurement failure (rejected / canceled / retries exhausted): local
    /// order `fulfilling → paid` (conditional) and parent status recomputed (UPS-07).
    async fn rollback_failed(&self, order_id: Id) -> Result<()>;

    /// An admin gave up on the purchase (LQA-I3): refunds what is left of the local
    /// order — to the buyer's wallet, or as a manual refund record for a guest (the
    /// admin returns the money through the original payment method). No refund
    /// window applies: the buyer never got the goods.
    async fn refund_failed(&self, order_id: Id, reason: &str) -> Result<FailureRefund> {
        let _ = (order_id, reason);
        Ok(FailureRefund::None)
    }
}

/// How a given-up purchase was refunded to the buyer (LQA-I3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureRefund {
    /// Nothing left to refund (already refunded / not paid).
    None,
    /// Credited to the buyer's wallet.
    Wallet(zs_shared::money::Amount),
    /// Recorded as a manual refund (guest order): the admin pays it back.
    Manual(zs_shared::money::Amount),
}

/// Integration reactions to order events (implemented by this group, called by the
/// order group from its payment / status-change workflows).
#[async_trait]
pub trait IntegrationOrderEvents: Send + Sync {
    /// A paid order (parent with children or single): creates one purchase order per
    /// order with upstream items (idempotent) and notifies the downstream buyer.
    async fn order_paid(&self, order_id: Id) -> Result<()>;

    /// Any later status change (delivered / canceled / refunded): notifies the
    /// downstream buyer; child orders resolve their parent's reference (UPS-12).
    async fn order_status_changed(&self, order_id: Id) -> Result<()>;

    /// Pre-order guard for upstream SKUs (UPS-06): passes for non-mapped SKUs,
    /// unlimited or sufficient cached stock, and on any lookup/sync failure
    /// (fail-open); otherwise re-syncs the product and fails with
    /// `error.upstream_stock_insufficient` when still short.
    async fn ensure_upstream_stock(&self, sku_id: Id, quantity: i32) -> Result<()>;

    /// Purchase problems needing an admin (LQA-I3): status (`rejected`,
    /// `manual_review`, `canceled`; `failed` is still being retried) of the purchase
    /// order of each of `local_order_ids` that has one in such a state.
    async fn procurement_issues(
        &self,
        local_order_ids: &[Id],
    ) -> Result<std::collections::HashMap<Id, String>> {
        let _ = local_order_ids;
        Ok(std::collections::HashMap::new())
    }
}
