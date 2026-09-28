//! `payment` use cases: admin channel management, admin payment records and gateway
//! callbacks/webhooks (settlement is delegated to the `PaymentSettlement` port).

pub mod admin;
pub mod callback;
pub mod channel;

use std::sync::Arc;

pub use admin::PaymentAdminService;
pub use callback::CallbackService;
pub use channel::ChannelService;

use crate::identity::rate_limit::RateRule;

/// Callback rate limit per client IP (original `callbackRule`: 120 requests / 60 s, 60 s block).
pub const CALLBACK_RATE_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: 120,
    block_seconds: 60,
};

/// Services of the `payment` group.
#[derive(Debug, Clone)]
pub struct PaymentServices {
    pub channels: Arc<ChannelService>,
    pub payments: Arc<PaymentAdminService>,
    pub callbacks: Arc<CallbackService>,
    /// Limits the default callback/webhook paths per client IP.
    pub callback_limiter: crate::identity::rate_limit::RateLimiter,
}
