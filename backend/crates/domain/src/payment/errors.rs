//! Error keys of the payment module and conversions into [`crate::Error`].

use super::callback::FactError;
use super::channel::ChannelRuleError;
use super::eligibility::EligibilityError;
use super::gateway::GatewayError;
use crate::Error;

/// i18n keys (identical to the original `error.*` keys).
pub mod keys {
    pub const PAYMENT_INVALID: &str = "error.payment_invalid";
    pub const PAYMENT_NOT_FOUND: &str = "error.payment_not_found";
    pub const PAYMENT_FETCH_FAILED: &str = "error.payment_fetch_failed";
    pub const PAYMENT_UPDATE_FAILED: &str = "error.payment_update_failed";
    pub const PAYMENT_STATUS_INVALID: &str = "error.payment_status_invalid";
    pub const PAYMENT_AMOUNT_MISMATCH: &str = "error.payment_amount_mismatch";
    pub const PAYMENT_CURRENCY_MISMATCH: &str = "error.payment_currency_mismatch";
    pub const PAYMENT_CALLBACK_FAILED: &str = "error.payment_callback_failed";
    pub const PAYMENT_EXPORT_FAILED: &str = "error.payment_export_failed";
    pub const PROVIDER_NOT_SUPPORTED: &str = "error.payment_provider_not_supported";
    pub const GATEWAY_REQUEST_FAILED: &str = "error.payment_gateway_request_failed";
    pub const GATEWAY_RESPONSE_INVALID: &str = "error.payment_gateway_response_invalid";
    pub const PROVIDER_PERMISSION_MISSING: &str = "error.payment_provider_permission_missing";
    pub const CHANNEL_INVALID: &str = "error.payment_channel_invalid";
    pub const CHANNEL_CONFIG_INVALID: &str = "error.payment_channel_config_invalid";
    pub const CHANNEL_NOT_FOUND: &str = "error.payment_channel_not_found";
    pub const CHANNEL_INACTIVE: &str = "error.payment_channel_inactive";
    pub const CHANNEL_CREATE_FAILED: &str = "error.payment_channel_create_failed";
    pub const CHANNEL_UPDATE_FAILED: &str = "error.payment_channel_update_failed";
    pub const CHANNEL_DELETE_FAILED: &str = "error.payment_channel_delete_failed";
    pub const CHANNEL_FETCH_FAILED: &str = "error.payment_channel_fetch_failed";
    pub const CHANNEL_NOT_ALLOWED_FOR_PRODUCT: &str =
        "error.payment_channel_not_allowed_for_product";
    pub const CHANNEL_NOT_ALLOWED_FOR_RECHARGE: &str =
        "error.payment_channel_not_allowed_for_recharge";
    pub const WECHAT_KEY_TEST_UNSUPPORTED: &str = "error.wechatpay_key_test_unsupported";
    pub const WECHAT_KEY_TEST_CONFIG_INVALID: &str = "error.wechatpay_key_test_config_invalid";
    pub const WECHAT_KEY_TEST_REQUEST_FAILED: &str = "error.wechatpay_key_test_request_failed";
    pub const WECHAT_KEY_TEST_RESPONSE_INVALID: &str = "error.wechatpay_key_test_response_invalid";
    pub const WECHAT_KEY_TEST_FAILED: &str = "error.wechatpay_key_test_failed";
}

/// `mapProviderErrorToService` followed by the handler's key rules (all business errors, 400).
impl From<GatewayError> for Error {
    fn from(err: GatewayError) -> Self {
        let key = match &err {
            GatewayError::ConfigInvalid(_) => keys::CHANNEL_CONFIG_INVALID,
            GatewayError::RequestFailed(_) | GatewayError::AuthFailed(_) => {
                keys::GATEWAY_REQUEST_FAILED
            }
            GatewayError::ProviderPermissionMissing(_) => keys::PROVIDER_PERMISSION_MISSING,
            GatewayError::ResponseInvalid(_) | GatewayError::SignatureInvalid(_) => {
                keys::GATEWAY_RESPONSE_INVALID
            }
            GatewayError::UnsupportedChannel(_)
            | GatewayError::ProviderNotFound
            | GatewayError::Unsupported => keys::PROVIDER_NOT_SUPPORTED,
        };
        Error::bad_request(key)
    }
}

impl From<FactError> for Error {
    fn from(err: FactError) -> Self {
        Error::bad_request(match err {
            FactError::Invalid => keys::PAYMENT_INVALID,
            FactError::CurrencyMismatch => keys::PAYMENT_CURRENCY_MISMATCH,
            FactError::AmountMismatch => keys::PAYMENT_AMOUNT_MISMATCH,
        })
    }
}

impl From<ChannelRuleError> for Error {
    fn from(_: ChannelRuleError) -> Self {
        Error::bad_request(keys::CHANNEL_CONFIG_INVALID)
    }
}

impl From<EligibilityError> for Error {
    fn from(err: EligibilityError) -> Self {
        Error::bad_request(match err {
            EligibilityError::NotAllowedForProduct => keys::CHANNEL_NOT_ALLOWED_FOR_PRODUCT,
            EligibilityError::NotAllowedForRecharge => keys::CHANNEL_NOT_ALLOWED_FOR_RECHARGE,
            EligibilityError::CurrencyMismatch => keys::PAYMENT_CURRENCY_MISMATCH,
            EligibilityError::Invalid
            | EligibilityError::AmountTooSmall
            | EligibilityError::AmountTooLarge => keys::PAYMENT_INVALID,
        })
    }
}
