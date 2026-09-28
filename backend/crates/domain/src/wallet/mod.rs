//! `wallet` domain: accounts, transactions, recharge orders, balance rules and ports.
//!
//! Other groups move money through the ledger functions in
//! `zs_infra::db::repo::wallet::ledger` (they run inside the caller's transaction and use
//! conditional balance updates); the rules and types they need live here.

pub mod model;
pub mod ports;
pub mod rules;

pub use model::{
    Account, RechargeFilter, RechargeOrder, RechargeStatus, Transaction, TransactionFilter,
    direction, keys, txn_type,
};
pub use ports::{
    BalanceChangeRequest, GiftCardRedemption, LedgerError, RechargeDraft, RechargeGateway,
    RechargeHooks, RechargeLookup, RechargeSettled, RechargeStore, UserBrief, WalletAdminLookup,
    WalletRepo,
};
