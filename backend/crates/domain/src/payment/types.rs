//! Payment enums and string constants (identical to the original `constants` package).

use std::fmt;

use serde::{Deserialize, Serialize};

/// `payment_channels.provider_type` values.
pub mod provider {
    pub const OFFICIAL: &str = "official";
    pub const EPAY: &str = "epay";
    pub const EPUSDT: &str = "epusdt";
    pub const BEPUSDT: &str = "bepusdt";
    pub const DUJIAOPAY: &str = "dujiaopay";
    pub const OKPAY: &str = "okpay";
    pub const TOKENPAY: &str = "tokenpay";
    pub const WALLET: &str = "wallet";
}

/// Well-known `payment_channels.channel_type` values (crypto gateways also accept token ids).
pub mod channel_type {
    pub const WECHAT: &str = "wechat";
    pub const WXPAY: &str = "wxpay";
    pub const ALIPAY: &str = "alipay";
    pub const PAYPAL: &str = "paypal";
    pub const STRIPE: &str = "stripe";
    pub const QQPAY: &str = "qqpay";
    pub const USDT: &str = "usdt";
    pub const USDT_TRC20: &str = "usdt-trc20";
    pub const USDC_TRC20: &str = "usdc-trc20";
    pub const TRX: &str = "trx";
    pub const BALANCE: &str = "balance";
}

/// Crypto gateway order modes (`config_json.order_mode`).
pub mod order_mode {
    pub const TRANSACTION: &str = "transaction";
    pub const CASHIER: &str = "cashier";
}

/// `payments.exception_code` values: late/duplicate successes that need manual attention.
pub mod exception {
    pub const SUPERSEDED_SUCCEEDED: &str = "superseded_payment_succeeded";
    pub const DUPLICATE_SUCCEEDED: &str = "duplicate_payment_succeeded";
    pub const CLOSED_ORDER_SUCCEEDED: &str = "closed_order_payment_succeeded";
    pub const UNDERPAID_SUCCEEDED: &str = "underpaid_payment_succeeded";
}

/// Channel `payment_roles` values.
pub mod role {
    pub const GUEST: &str = "guest";
    pub const MEMBER: &str = "member";
}

/// Channel `payment_types` values.
pub mod payment_type {
    pub const ORDER: &str = "order";
    pub const WALLET: &str = "wallet";
}

/// Default site currency (`constants.SiteCurrencyDefault`).
pub const SITE_CURRENCY_DEFAULT: &str = "CNY";

/// Payload key recording the fiat currency actually sent to the gateway at creation
/// (`contract.GatewayPayloadFiatCurrencySent`).
pub const PAYLOAD_FIAT_CURRENCY_SENT: &str = "_dujiao_next_fiat_currency_sent";

/// How the customer interacts with a channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InteractionMode {
    Qr,
    Redirect,
    Wap,
    Page,
    Balance,
}

impl InteractionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Qr => "qr",
            Self::Redirect => "redirect",
            Self::Wap => "wap",
            Self::Page => "page",
            Self::Balance => "balance",
        }
    }

    /// Parses a (trimmed, case-insensitive) mode string.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "qr" => Some(Self::Qr),
            "redirect" => Some(Self::Redirect),
            "wap" => Some(Self::Wap),
            "page" => Some(Self::Page),
            "balance" => Some(Self::Balance),
            _ => None,
        }
    }
}

impl fmt::Display for InteractionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle of a payment row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentStatus {
    Initiated,
    Pending,
    Success,
    Failed,
    Expired,
}

impl PaymentStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initiated => "initiated",
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    /// Parses a (trimmed, case-insensitive) status; unknown values yield `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "initiated" => Some(Self::Initiated),
            "pending" => Some(Self::Pending),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// True for statuses a new payment link can still be paid in.
    pub fn is_open(self) -> bool {
        matches!(self, Self::Initiated | Self::Pending)
    }
}

impl fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Who bears the channel fee, snapshotted on every payment (PAY-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeePolicy {
    None,
    MerchantAbsorbed,
    CustomerSurcharge,
    LegacyCustomerSurcharge,
}

impl FeePolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::MerchantAbsorbed => "merchant_absorbed",
            Self::CustomerSurcharge => "customer_surcharge",
            Self::LegacyCustomerSurcharge => "legacy_customer_surcharge",
        }
    }

    /// Parses a stored policy; empty or unknown values yield `None` (legacy rows).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "none" => Some(Self::None),
            "merchant_absorbed" => Some(Self::MerchantAbsorbed),
            "customer_surcharge" => Some(Self::CustomerSurcharge),
            "legacy_customer_surcharge" => Some(Self::LegacyCustomerSurcharge),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_statuses_case_insensitively() {
        assert_eq!(
            PaymentStatus::parse(" SUCCESS "),
            Some(PaymentStatus::Success)
        );
        assert_eq!(PaymentStatus::parse("paid"), None);
        assert_eq!(
            InteractionMode::parse("Redirect"),
            Some(InteractionMode::Redirect)
        );
        assert_eq!(FeePolicy::parse(""), None);
        assert_eq!(
            serde_json::to_string(&FeePolicy::LegacyCustomerSurcharge)
                .ok()
                .as_deref(),
            Some("\"legacy_customer_surcharge\"")
        );
    }
}
