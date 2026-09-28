//! Wallet models in the JSON shape of the original `walletdomain` structs.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zs_shared::money::Amount;

use crate::Id;

/// Default currency of accounts and transactions (original `defaultCurrency`).
pub const DEFAULT_CURRENCY: &str = "CNY";

/// Transaction types (original `constants.WalletTxnType*`).
pub mod txn_type {
    pub const RECHARGE: &str = "recharge";
    pub const ORDER_PAY: &str = "order_pay";
    pub const ORDER_REFUND: &str = "order_refund";
    pub const ADMIN_ADJUST: &str = "admin_adjust";
    pub const ADMIN_REFUND: &str = "admin_refund";
    pub const GIFT_CARD_REDEEM: &str = "gift_card_redeem";
    pub const ORDER_UNDERPAID_CREDIT: &str = "order_underpaid_credit";
}

/// Transaction directions.
pub mod direction {
    pub const IN: &str = "in";
    pub const OUT: &str = "out";
}

/// i18n keys used by the wallet endpoints (identical to the original keys).
pub mod keys {
    pub const ADJUST_REMARK_REQUIRED: &str = "error.wallet_adjust_remark_required";
    pub const USER_ID_INVALID: &str = "error.user_id_invalid";
    pub const USER_NOT_FOUND: &str = "error.user_not_found";
    pub const USER_FETCH_FAILED: &str = "error.user_fetch_failed";
    pub const USER_UPDATE_FAILED: &str = "error.user_update_failed";
    pub const PAYMENT_FETCH_FAILED: &str = "error.payment_fetch_failed";
    pub const PAYMENT_NOT_FOUND: &str = "error.payment_not_found";
    pub const PAYMENT_INVALID: &str = "error.payment_invalid";
    pub const PAYMENT_CREATE_FAILED: &str = "error.payment_create_failed";
    pub const PAYMENT_UPDATE_FAILED: &str = "error.payment_update_failed";
    pub const PAYMENT_CALLBACK_FAILED: &str = "error.payment_callback_failed";
    /// The original maps an insufficient balance on admin adjustments to this key.
    pub const PAYMENT_AMOUNT_MISMATCH: &str = "error.payment_amount_mismatch";
    /// Admin deduction larger than the balance (live QA I-12).
    pub const WALLET_INSUFFICIENT_BALANCE: &str = "error.wallet_insufficient_balance";
    pub const QUEUE_UNAVAILABLE: &str = "error.queue_unavailable";
    pub const GIFT_CARD_REDEEM_FAILED: &str = "error.gift_card_redeem_failed";
}

/// A user's wallet account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Account {
    pub id: Id,
    pub user_id: Id,
    pub balance: Amount,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// One balance movement; `reference` is the unique idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Transaction {
    pub id: Id,
    pub user_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_admin_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<Id>,
    #[serde(rename = "type")]
    pub kind: String,
    pub direction: String,
    pub amount: Amount,
    pub balance_before: Amount,
    pub balance_after: Amount,
    pub currency: String,
    pub reference: String,
    pub remark: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Status of a recharge order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RechargeStatus {
    Pending,
    Success,
    Failed,
    Expired,
}

impl RechargeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Success => "success",
            Self::Failed => "failed",
            Self::Expired => "expired",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "pending" => Some(Self::Pending),
            "success" => Some(Self::Success),
            "failed" => Some(Self::Failed),
            "expired" => Some(Self::Expired),
            _ => None,
        }
    }

    /// Success, failed and expired are terminal.
    pub fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

/// A wallet top-up paid through a payment channel (`wallet_recharge_orders`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RechargeOrder {
    pub id: Id,
    pub recharge_no: String,
    pub user_id: Id,
    pub payment_id: Id,
    pub channel_id: Id,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    pub amount: Amount,
    pub payable_amount: Amount,
    pub fee_rate: Amount,
    pub fee_amount: Amount,
    pub currency: String,
    pub status: String,
    pub remark: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Transaction list filter (`TransactionListFilter`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransactionFilter {
    pub user_id: Id,
    pub order_id: Id,
    pub kind: String,
    pub direction: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Recharge list filter (`RechargeListFilter`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RechargeFilter {
    pub recharge_no: String,
    pub user_id: Id,
    pub user_keyword: String,
    pub payment_id: Id,
    pub channel_id: Id,
    pub provider_type: String,
    pub channel_type: String,
    pub status: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    pub paid_from: Option<DateTime<Utc>>,
    pub paid_to: Option<DateTime<Utc>>,
}
