//! `notify` use cases: notification center, channel clients and the channel
//! API identity/bot endpoints, Telegram broadcasts and bot callbacks.

pub mod bot;
pub mod broadcast;
pub mod center;
pub mod channel;
pub mod clients;
pub mod guard;

use std::sync::Arc;

use zs_domain::notify::ports::Notifier;

use crate::identity::rate_limit::{RateLimiter, RateRule};

/// Channel API limit: 600 requests per minute per IP + channel key, 30 s block
/// (original `channelAPIRule`).
pub const CHANNEL_RATE_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: 600,
    block_seconds: 30,
};

/// Services of the `notify` group.
#[derive(Clone)]
pub struct NotifyServices {
    pub center: center::NotificationService,
    /// Queue-backed [`Notifier`] other modules can share.
    pub notifier: Arc<dyn Notifier>,
    pub clients: clients::ChannelClientService,
    pub channel: channel::ChannelService,
    pub broadcasts: broadcast::BroadcastService,
    pub bot: bot::BotNotifyService,
    /// Rate limiter of `/api/v1/channel/*`.
    pub channel_limiter: RateLimiter,
}

impl std::fmt::Debug for NotifyServices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NotifyServices").finish_non_exhaustive()
    }
}
