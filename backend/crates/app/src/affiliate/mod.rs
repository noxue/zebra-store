//! `affiliate` use cases.

pub mod service;

use std::sync::Arc;

pub use service::{AffiliateService, ClickInput};

/// Services of the `affiliate` group.
///
/// The order group uses `service.resolve_order_snapshot` (order creation),
/// `service.handle_order_paid` (after payment) and `service.handle_order_canceled`
/// (cancel/timeout); refunds call `zs_infra::db::repo::affiliate::clawback_on_refund`
/// inside the refund transaction.
#[derive(Debug, Clone)]
pub struct AffiliateServices {
    pub service: Arc<AffiliateService>,
}
