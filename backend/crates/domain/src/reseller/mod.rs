//! Resellers (white-label shops): profiles, domains and tenant resolution, site
//! branding, pricing rules, profit ledger, balances and withdrawals.
//!
//! Port of `internal/modules/reseller` of the original project. Pure rules live in
//! the sub-modules; persistence is reached through the traits in [`ports`].

pub mod accounting;
pub mod model;
pub mod operations;
pub mod orders;
pub mod ports;
pub mod pricing;
pub mod rules;
pub mod site;
pub mod tenant;

pub use model::*;

/// Message keys of reseller errors (identical to the original `error.*` keys).
///
/// The HTTP layer remaps some of them per surface exactly like the original
/// handlers do (e.g. the console answers `error.forbidden` for an inactive profile).
pub mod keys {
    pub const BAD_REQUEST: &str = "error.bad_request";
    pub const FORBIDDEN: &str = "error.forbidden";
    pub const NOT_FOUND: &str = "error.not_found";
    /// Sentinel of `ErrProfileInactive`; finance endpoints show it verbatim.
    pub const PROFILE_INACTIVE: &str = "error.reseller_profile_inactive";
    pub const SETTLEMENT_UNAVAILABLE: &str = "error.reseller_settlement_unavailable";
    pub const DOMAIN_INVALID: &str = "error.reseller_domain_invalid";
    pub const DOMAIN_CONFLICT: &str = "error.reseller_domain_conflict";
    pub const DOMAIN_MAIN_HOST: &str = "error.reseller_domain_main_host_not_allowed";
    pub const SUBDOMAIN_BASE_MISSING: &str = "error.reseller_subdomain_base_missing";
    pub const SITE_CONFIG_INVALID: &str = "error.reseller_site_config_invalid";
    pub const SUPPORT_TELEGRAM_INVALID: &str = "error.reseller_support_telegram_invalid";
    pub const SUPPORT_WHATSAPP_INVALID: &str = "error.reseller_support_whatsapp_invalid";
    pub const SUPPORT_EMAIL_INVALID: &str = "error.reseller_support_email_invalid";
    pub const SUPPORT_URL_INVALID: &str = "error.reseller_support_url_invalid";
    pub const IMAGE_INVALID: &str = "error.reseller_image_invalid";
    pub const LINK_INVALID: &str = "error.reseller_link_invalid";
    pub const PRICE_INVALID: &str = "error.reseller_price_invalid";
    pub const MARKUP_EXCEEDED: &str = "error.reseller_markup_exceeded";
    pub const PRODUCT_NOT_LISTED: &str = "error.reseller_product_not_listed";
    pub const COUPON_NOT_ALLOWED: &str = "error.reseller_coupon_not_allowed";
    pub const SKU_INVALID: &str = "error.order_item_invalid";
    pub const WITHDRAW_AMOUNT_INVALID: &str = "error.reseller_withdraw_amount_invalid";
    pub const WITHDRAW_INSUFFICIENT: &str = "error.reseller_withdraw_insufficient";
    pub const WITHDRAW_CURRENCY_UNAVAILABLE: &str = "error.reseller_withdraw_currency_unavailable";
    pub const BALANCE_FROZEN: &str = "error.reseller_balance_frozen";
    pub const ORDER_NOT_FOUND: &str = "error.order_not_found";

    /// Keys the original admin handlers collapse into `error.bad_request`.
    pub const ADMIN_BAD_REQUEST: [&str; 11] = [
        DOMAIN_INVALID,
        DOMAIN_CONFLICT,
        DOMAIN_MAIN_HOST,
        SITE_CONFIG_INVALID,
        SUPPORT_TELEGRAM_INVALID,
        SUPPORT_WHATSAPP_INVALID,
        SUPPORT_EMAIL_INVALID,
        SUPPORT_URL_INVALID,
        IMAGE_INVALID,
        LINK_INVALID,
        PROFILE_INACTIVE,
    ];
}

use crate::Error;

/// `ErrNotOpened`: the user has no reseller profile.
pub fn not_opened() -> Error {
    Error::bad_request(keys::BAD_REQUEST)
}

/// `ErrProfileInactive`.
pub fn profile_inactive() -> Error {
    Error::bad_request(keys::PROFILE_INACTIVE)
}

/// Generic not-found (`productcontract.ErrNotFound`).
pub fn not_found() -> Error {
    Error::not_found(keys::NOT_FOUND)
}
