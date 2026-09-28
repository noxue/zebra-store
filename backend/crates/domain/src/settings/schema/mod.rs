//! Typed setting schemas: defaults, normalization, validation, masking and
//! patching for every key in [`super::keys`] (port of `modules/settings/schema`
//! and `application/default_registry.go`).

pub mod captcha;
pub mod integration;
pub mod login;
pub mod notification;
pub mod order_email;
pub mod risk;
pub mod site;
pub mod smtp;
pub mod storefront;
pub mod telegram_bot;
pub mod value;

use serde_json::{Value, json};

use super::keys;

/// A side effect a successful write must trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Drop the cached `/public/config` payload.
    InvalidatePublicConfig,
    /// Drop the cached custom callback routes.
    InvalidateCallbackRoutes,
}

/// Every key stored in the `settings` table (the generic endpoint refuses the rest).
pub const KNOWN_KEYS: &[&str] = &[
    keys::SITE_CONFIG,
    keys::ORDER_CONFIG,
    keys::SMTP_CONFIG,
    keys::CAPTCHA_CONFIG,
    keys::TELEGRAM_AUTH_CONFIG,
    keys::GOOGLE_AUTH_CONFIG,
    keys::DASHBOARD_CONFIG,
    keys::NOTIFICATION_CENTER_CONFIG,
    keys::AFFILIATE_CONFIG,
    keys::TELEGRAM_BOT_CONFIG,
    keys::TELEGRAM_BOT_RUNTIME_STATUS,
    keys::ORDER_EMAIL_TEMPLATE_CONFIG,
    keys::NAV_CONFIG,
    keys::WALLET_CONFIG,
    keys::PAYMENT_CONFIG,
    keys::REGISTRATION_CONFIG,
    keys::ORDER_RISK_CONTROL_CONFIG,
    keys::UPSTREAM_SYNC_CONFIG,
    keys::CALLBACK_ROUTES_CONFIG,
    keys::HOME_ANNOUNCEMENT,
    keys::COMPLIANCE_ACK,
    crate::identity::compliance::SETTING_KEY,
];

/// Whether `key` is a setting this application knows (live QA I-1).
pub fn is_known(key: &str) -> bool {
    KNOWN_KEYS.contains(&key)
}

/// Whether `PUT /admin/settings` may write `key`: known keys except the
/// runtime/compliance records that only the server itself writes.
pub fn is_writable(key: &str) -> bool {
    is_known(key)
        && !matches!(
            key,
            keys::TELEGRAM_BOT_RUNTIME_STATUS | keys::COMPLIANCE_ACK
        )
        && key != crate::identity::compliance::SETTING_KEY
}

/// Effects declared for `key` (unknown keys have none).
///
/// Every key read by `/public/config` must invalidate its cache (live QA I-4:
/// `affiliate_config` did not).
pub fn effects(key: &str) -> &'static [Effect] {
    match key {
        keys::SITE_CONFIG
        | keys::GOOGLE_AUTH_CONFIG
        | keys::TELEGRAM_AUTH_CONFIG
        | keys::CAPTCHA_CONFIG
        | keys::SMTP_CONFIG
        | keys::AFFILIATE_CONFIG
        | keys::PAYMENT_CONFIG
        | keys::NAV_CONFIG
        | keys::REGISTRATION_CONFIG
        | keys::HOME_ANNOUNCEMENT
        | keys::WALLET_CONFIG => &[Effect::InvalidatePublicConfig],
        keys::CALLBACK_ROUTES_CONFIG => &[Effect::InvalidateCallbackRoutes],
        _ => &[],
    }
}

/// Validates a raw value written through the generic `PUT /admin/settings`
/// before it is normalized (keys without rules accept anything).
pub fn validate(key: &str, value: &Value) -> crate::Result<()> {
    match key {
        keys::CALLBACK_ROUTES_CONFIG => integration::validate_callback_routes(value),
        keys::HOME_ANNOUNCEMENT => storefront::validate_announcement(value),
        _ => Ok(()),
    }
}

/// Normalizes a raw value written through the generic `PUT /admin/settings`.
///
/// Registered keys are rewritten into their canonical shape; unknown keys are
/// stored as-is (the original pass-through behaviour).
pub fn normalize(key: &str, value: &Value) -> Value {
    let raw = Some(value);
    match key {
        keys::DASHBOARD_CONFIG => {
            storefront::DashboardSetting::decode(raw, storefront::DashboardSetting::default())
                .encode()
        }
        keys::ORDER_CONFIG => json!(site::OrderSetting::decode(
            raw,
            site::OrderSetting::default()
        )),
        keys::SITE_CONFIG => site::normalize_site(value),
        keys::TELEGRAM_AUTH_CONFIG => {
            let fallback = login::TelegramAuthSetting::default().normalized();
            login::TelegramAuthSetting::decode(raw, fallback).encode()
        }
        keys::GOOGLE_AUTH_CONFIG => {
            login::GoogleAuthSetting::decode(raw, login::GoogleAuthSetting::default()).encode()
        }
        keys::NOTIFICATION_CENTER_CONFIG => notification::NotificationCenterSetting::decode(
            raw,
            notification::NotificationCenterSetting::default(),
        )
        .encode(),
        keys::AFFILIATE_CONFIG => integration::AffiliateSetting::decode(raw).encode(),
        keys::TELEGRAM_BOT_CONFIG => telegram_bot::TelegramBotSetting::decode(
            raw,
            telegram_bot::TelegramBotSetting::defaults(),
        )
        .normalized()
        .encode(),
        keys::NAV_CONFIG => site::normalize_nav(value),
        keys::REGISTRATION_CONFIG => site::normalize_registration(value),
        keys::ORDER_RISK_CONTROL_CONFIG => {
            risk::OrderRiskSetting::decode(raw, risk::OrderRiskSetting::default()).encode()
        }
        keys::UPSTREAM_SYNC_CONFIG => integration::UpstreamSyncSetting::decode(
            raw,
            integration::UpstreamSyncSetting::default(),
        )
        .encode(),
        keys::CALLBACK_ROUTES_CONFIG => integration::CallbackRoutes::decode(raw)
            .deduplicated()
            .encode(),
        keys::HOME_ANNOUNCEMENT => storefront::normalize_announcement(value),
        keys::PAYMENT_CONFIG => json!(site::PaymentFeeSetting::decode(raw)),
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registered_keys_are_normalized_and_unknown_pass_through() {
        let v = normalize(keys::ORDER_CONFIG, &json!({"payment_expire_minutes": "30"}));
        assert_eq!(
            v,
            json!({"payment_expire_minutes": 30, "max_refund_days": 30})
        );
        let v = normalize("custom_key", &json!({"a": [1, 2]}));
        assert_eq!(v, json!({"a": [1, 2]}));
        let v = normalize(
            keys::PAYMENT_CONFIG,
            &json!({"customer_fee_enabled": "yes", "x": 1}),
        );
        assert_eq!(
            v,
            json!({"customer_fee_enabled": true, "reuse_legacy_order_fee_payment": false})
        );
        // wallet_config is effect-only
        let v = normalize(keys::WALLET_CONFIG, &json!({"wallet_only_payment": true}));
        assert_eq!(v, json!({"wallet_only_payment": true}));
    }

    // SET-04: numeric and enum settings are clamped / whitelisted on write.
    #[test]
    fn set_04_clamps_sync_and_template() {
        let v = normalize(keys::UPSTREAM_SYNC_CONFIG, &json!({"sync_page_size": 0}));
        assert_eq!(v["sync_page_size"], 50);
        let v = normalize(
            keys::UPSTREAM_SYNC_CONFIG,
            &json!({"sync_page_size": 100_000}),
        );
        assert_eq!(v["sync_page_size"], 200);
        let v = normalize(keys::SITE_CONFIG, &json!({"storefront_template": "evil"}));
        assert_eq!(v["storefront_template"], "classic");
    }

    // SET-01 ①②③: unsafe callback paths are dropped, normalized and de-duplicated.
    #[test]
    fn set_01_callback_routes_normalized_on_write() {
        let v = normalize(
            keys::CALLBACK_ROUTES_CONFIG,
            &json!({
                "payment_callback": "/api/v1/admin/x",
                "paypal_webhook": "/api/pay/cb/?a=1#x",
                "stripe_webhook": "/api/cb",
                "upstream_callback": "/api/cb",
            }),
        );
        assert_eq!(v["payment_callback"], "");
        assert_eq!(v["paypal_webhook"], "/api/pay/cb");
        assert_eq!(v["stripe_webhook"], "/api/cb");
        assert_eq!(v["upstream_callback"], "");
        assert_eq!(
            effects(keys::CALLBACK_ROUTES_CONFIG),
            &[Effect::InvalidateCallbackRoutes]
        );
    }

    #[test]
    fn effects_declared() {
        assert_eq!(
            effects(keys::SITE_CONFIG),
            &[Effect::InvalidatePublicConfig]
        );
        assert!(effects(keys::ORDER_CONFIG).is_empty());
    }

    // QA-A04: every key that feeds `/public/config` invalidates its cache.
    #[test]
    fn qa_a04_public_config_keys_invalidate_cache() {
        for key in [
            keys::SITE_CONFIG,
            keys::AFFILIATE_CONFIG,
            keys::PAYMENT_CONFIG,
            keys::SMTP_CONFIG,
            keys::CAPTCHA_CONFIG,
            keys::TELEGRAM_AUTH_CONFIG,
            keys::GOOGLE_AUTH_CONFIG,
            keys::WALLET_CONFIG,
            keys::REGISTRATION_CONFIG,
            keys::NAV_CONFIG,
            keys::HOME_ANNOUNCEMENT,
        ] {
            assert_eq!(effects(key), &[Effect::InvalidatePublicConfig], "{key}");
        }
    }

    // QA-A20: an announcement ending before it starts is refused.
    #[test]
    fn qa_a20_announcement_window() {
        let v = |s: &str, e: &str| json!({"enabled": true, "start_at": s, "end_at": e});
        assert!(
            validate(
                keys::HOME_ANNOUNCEMENT,
                &v("2026-09-02T00:00:00Z", "2026-09-01T00:00:00Z")
            )
            .is_err()
        );
        assert!(
            validate(
                keys::HOME_ANNOUNCEMENT,
                &v("2026-09-01T00:00:00Z", "2026-09-02T00:00:00Z")
            )
            .is_ok()
        );
        assert!(validate(keys::HOME_ANNOUNCEMENT, &v("", "2026-09-01T00:00:00Z")).is_ok());
    }

    // QA-A01: unknown and server-owned keys are not writable.
    #[test]
    fn qa_a01_key_whitelist() {
        assert!(is_writable(keys::SITE_CONFIG));
        assert!(is_known(keys::TELEGRAM_BOT_RUNTIME_STATUS));
        assert!(!is_writable(keys::TELEGRAM_BOT_RUNTIME_STATUS));
        assert!(!is_writable(crate::identity::compliance::SETTING_KEY));
        assert!(!is_known("foo_bar_unknown"));
        assert!(!is_writable("foo_bar_unknown"));
    }
}
