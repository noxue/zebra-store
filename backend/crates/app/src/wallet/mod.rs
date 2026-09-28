//! `wallet` use cases: accounts, transactions, admin adjustments, recharges and gift
//! card redemption.

pub mod account;
pub mod recharge;

use std::sync::Arc;

pub use account::{AdminRechargeItem, WalletService};
pub use recharge::{ExpirePayload, RechargeDeps, RechargeRequest, RechargeService, RechargeView};

use crate::identity::rate_limit::{RateLimiter, RateRule};

/// Gift card redemption limit per user + IP (original `giftCardRedeemRule`, RISK-02:
/// 10 requests / 60 s, blocked for 300 s).
pub const GIFT_CARD_REDEEM_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: 10,
    block_seconds: 300,
};

/// Captcha scene of gift card redemption.
pub const CAPTCHA_SCENE_GIFT_CARD_REDEEM: &str = "gift_card_redeem";

/// Services of the `wallet` group.
#[derive(Debug, Clone)]
pub struct WalletServices {
    pub wallet: Arc<WalletService>,
    pub recharge: Arc<RechargeService>,
    /// Limits `POST /gift-cards/redeem` by `user_id|ip`.
    pub redeem_limiter: RateLimiter,
}
