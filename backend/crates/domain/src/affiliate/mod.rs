//! `affiliate` domain: profiles, clicks, commissions, withdrawals, rules and ports.
//!
//! Ports for other groups: the order group calls
//! `AffiliateServices.service.{resolve_order_snapshot, handle_order_paid, handle_order_canceled}`
//! and, inside its refund transaction,
//! `zs_infra::db::repo::affiliate::clawback_on_refund(&txn, …)`.

pub mod model;
pub mod ports;
pub mod rules;

pub use model::{
    AdminUserItem, Commission, CommissionFilter, Dashboard, OrderRef, Processor, Profile,
    ProfileFilter, ProfileStats, Stats, WithdrawFilter, WithdrawRequest, keys, status,
};
pub use ports::{AffiliateRepo, CommissionItem, CommissionOrder, NewClick, NewCommission};
