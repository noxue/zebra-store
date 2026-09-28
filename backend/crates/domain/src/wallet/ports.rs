//! Wallet ports (implemented in `zs-infra`).

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::model::{Account, RechargeFilter, RechargeOrder, Transaction, TransactionFilter, keys};
use crate::identity::user::User;
use crate::marketing::gift_card::GiftCard;
use crate::payment::callback::CallbackInput;
use crate::payment::channel::PaymentChannel;
use crate::payment::eligibility::Payer;
use crate::payment::model::Payment;
use crate::payment::types::FeePolicy;
use crate::{Error, Id, Result};

/// Failure of a ledger movement (original `walletcontract.Err*`).
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// Amount not positive (or a zero adjustment).
    #[error("wallet invalid amount")]
    InvalidAmount,
    /// The balance would become negative.
    #[error("wallet insufficient balance")]
    InsufficientBalance,
    /// Guests (user id 0) have no wallet.
    #[error("wallet not supported for guest")]
    NotSupportedForGuest,
    /// Credits need a non-empty idempotency reference.
    #[error("wallet reference required")]
    ReferenceRequired,
    /// Storage failure (or a concurrent change the caller should retry).
    #[error(transparent)]
    Store(#[from] Error),
}

impl PartialEq for LedgerError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Store(a), Self::Store(b)) => a.key() == b.key(),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}

impl From<LedgerError> for Error {
    fn from(err: LedgerError) -> Self {
        match err {
            LedgerError::InvalidAmount => Error::invalid(),
            LedgerError::InsufficientBalance => Error::bad_request(keys::PAYMENT_AMOUNT_MISMATCH),
            LedgerError::NotSupportedForGuest => Error::bad_request(keys::PAYMENT_INVALID),
            LedgerError::ReferenceRequired => Error::internal_msg("wallet reference required"),
            LedgerError::Store(e) => e,
        }
    }
}

/// An atomic balance change (`changeBalance`): positive deltas credit, negative debit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BalanceChangeRequest {
    pub user_id: Id,
    pub delta: Amount,
    pub kind: String,
    pub reference: String,
    pub remark: String,
    pub currency: String,
    pub operator_admin_id: Option<Id>,
    pub order_id: Option<Id>,
}

/// Minimal user summary shown next to recharges.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserBrief {
    pub id: Id,
    pub email: String,
    pub display_name: String,
}

/// Result of a gift card redemption.
#[derive(Debug, Clone, PartialEq)]
pub struct GiftCardRedemption {
    pub card: GiftCard,
    pub account: Account,
    pub transaction: Transaction,
}

/// Wallet persistence.
#[async_trait]
pub trait WalletRepo: Send + Sync {
    /// The user's account, created with a zero balance when missing (`GetAccount`).
    async fn account(&self, user_id: Id) -> Result<Account>;
    /// Balances of existing accounts.
    async fn balances(&self, user_ids: &[Id]) -> Result<HashMap<Id, Amount>>;
    /// Newest first.
    async fn list_transactions(
        &self,
        filter: &TransactionFilter,
        page: PageRequest,
    ) -> Result<Page<Transaction>>;
    /// Applies a change in one transaction with a conditional balance update.
    async fn change_balance(
        &self,
        change: &BalanceChangeRequest,
    ) -> std::result::Result<(Account, Transaction), LedgerError>;
    /// Newest first.
    async fn list_recharges(
        &self,
        filter: &RechargeFilter,
        page: PageRequest,
    ) -> Result<Page<RechargeOrder>>;
    /// Count per status of a user's recharges.
    async fn recharge_stats(&self, user_id: Id, recharge_no: &str)
    -> Result<BTreeMap<String, i64>>;
    async fn recharge_by_no(&self, user_id: Id, recharge_no: &str)
    -> Result<Option<RechargeOrder>>;
    async fn recharge_by_payment(
        &self,
        payment_id: Id,
        user_id: Id,
    ) -> Result<Option<RechargeOrder>>;
    /// Locks the card, credits the wallet (`gift_card:<id>`) and marks the card redeemed,
    /// all in one transaction.
    async fn redeem_gift_card(
        &self,
        user_id: Id,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<GiftCardRedemption>;
}

/// Read models the admin wallet pages need from other modules.
#[async_trait]
pub trait WalletAdminLookup: Send + Sync {
    async fn user(&self, id: Id) -> Result<Option<User>>;
    async fn user_briefs(&self, ids: &[Id]) -> Result<Vec<UserBrief>>;
    async fn channel_names(&self, ids: &[Id]) -> Result<HashMap<Id, String>>;
    async fn payment_statuses(&self, ids: &[Id]) -> Result<HashMap<Id, String>>;
}

/// Read access to payments, channels and payers used by recharges.
#[async_trait]
pub trait RechargeLookup: Send + Sync {
    async fn payment(&self, id: Id) -> Result<Option<Payment>>;
    async fn channel(&self, id: Id) -> Result<Option<PaymentChannel>>;
    /// Active channels, `sort_order DESC, id ASC`.
    async fn active_channels(&self) -> Result<Vec<PaymentChannel>>;
    /// The payer seen by channel rules; `None` when the user does not exist.
    async fn payer(&self, user_id: Id) -> Result<Option<Payer>>;
}

/// Everything needed to persist a new recharge and its payment.
#[derive(Debug, Clone, PartialEq)]
pub struct RechargeDraft {
    pub recharge_no: String,
    pub user_id: Id,
    pub channel: PaymentChannel,
    pub amount: Amount,
    pub payable_amount: Amount,
    pub fee_rate: Amount,
    pub fixed_fee: Amount,
    pub fee_amount: Amount,
    pub fee_policy: FeePolicy,
    pub currency: String,
    pub remark: String,
    pub now: DateTime<Utc>,
}

/// Outcome of [`RechargeStore::settle`].
#[derive(Debug, Clone, PartialEq)]
pub struct RechargeSettled {
    pub payment: Payment,
    pub recharge: Option<RechargeOrder>,
    /// True only for the single pending→success transition that credited the wallet.
    pub newly_succeeded: bool,
}

/// Recharge state changes; every method is one database transaction with row locks.
#[async_trait]
pub trait RechargeStore: Send + Sync {
    /// Creates the `initiated` payment (`order_id = 0`) and the `pending` recharge.
    async fn create(&self, draft: &RechargeDraft) -> Result<(RechargeOrder, Payment)>;
    /// Saves the gateway creation result (pay URL, QR, provider reference, status).
    async fn save_started(&self, payment: &Payment) -> Result<()>;
    /// Marks the payment and recharge failed; a successful recharge is left untouched.
    async fn mark_failed(&self, payment_id: Id, now: DateTime<Utc>) -> Result<()>;
    /// Timeout job (WAL-01): expires a still-open recharge payment; order payments and
    /// terminal states are untouched. `None` when the payment does not exist.
    async fn expire(&self, payment_id: Id, now: DateTime<Utc>) -> Result<Option<Payment>>;
    /// Applies a verified gateway result under row locks (WAL-01 matrix); the wallet is
    /// credited exactly once, on the recharge's transition to success.
    async fn settle(&self, input: &CallbackInput, now: DateTime<Utc>) -> Result<RechargeSettled>;
}

/// Gateway half of recharges.
#[async_trait]
pub trait RechargeGateway: Send + Sync {
    /// Creates the payment at the gateway and returns the payment with pay URL / QR,
    /// provider reference and status applied (not persisted: the caller saves it).
    async fn start(
        &self,
        channel: &PaymentChannel,
        payment: &Payment,
        recharge: &RechargeOrder,
        client_ip: &str,
    ) -> Result<Payment>;
    /// Actively queries the gateway; `None` when the gateway reports no state change.
    /// Errors with `error.payment_provider_not_supported` when the channel cannot be
    /// queried (WAL-03: callers then fall back to the stored payment).
    async fn query(
        &self,
        channel: &PaymentChannel,
        payment: &Payment,
    ) -> Result<Option<CallbackInput>>;
}

/// Side effects after a recharge's first success (member level, notifications).
#[async_trait]
pub trait RechargeHooks: Send + Sync {
    async fn on_succeeded(&self, recharge: &RechargeOrder, payment: &Payment);
}
