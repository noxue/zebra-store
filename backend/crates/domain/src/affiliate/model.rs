//! Affiliate models in the JSON shape of the original `affiliatedomain` structs.

use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::money::Amount;

use crate::Id;
use crate::identity::user::User;

/// Status and type values (original `constants.Affiliate*`).
pub mod status {
    pub const PROFILE_ACTIVE: &str = "active";
    pub const PROFILE_DISABLED: &str = "disabled";
    pub const COMMISSION_PENDING_CONFIRM: &str = "pending_confirm";
    pub const COMMISSION_AVAILABLE: &str = "available";
    pub const COMMISSION_REJECTED: &str = "rejected";
    pub const COMMISSION_WITHDRAWN: &str = "withdrawn";
    pub const WITHDRAW_PENDING_REVIEW: &str = "pending_review";
    pub const WITHDRAW_REJECTED: &str = "rejected";
    pub const WITHDRAW_PAID: &str = "paid";
    pub const COMMISSION_TYPE_ORDER: &str = "order";
    pub const ACTION_REJECT: &str = "reject";
    pub const ACTION_PAY: &str = "pay";
}

/// i18n keys used by the affiliate endpoints (the original handlers' keys).
pub mod keys {
    pub const SAVE_FAILED: &str = "error.save_failed";
    pub const USER_FETCH_FAILED: &str = "error.user_fetch_failed";
    pub const USER_NOT_FOUND: &str = "error.user_not_found";
    pub const FORBIDDEN: &str = "error.forbidden";
}

/// An affiliate (推广用户) profile.
#[derive(Debug, Clone, Serialize)]
pub struct Profile {
    pub id: Id,
    pub user_id: Id,
    #[serde(rename = "code")]
    pub affiliate_code: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<User>,
}

/// Order projection preloaded on commissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderRef {
    pub id: Id,
    pub order_no: String,
}

/// Administrator who processed a withdrawal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Processor {
    pub id: Id,
    pub username: String,
}

/// A commission earned on an order.
#[derive(Debug, Clone, Serialize)]
pub struct Commission {
    pub id: Id,
    pub affiliate_profile_id: Id,
    pub order_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_item_id: Option<Id>,
    pub commission_type: String,
    pub base_amount: Amount,
    pub rate_percent: Amount,
    pub commission_amount: Amount,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdraw_request_id: Option<Id>,
    pub invalid_reason: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affiliate_profile: Option<Profile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<OrderRef>,
}

/// A withdrawal request.
#[derive(Debug, Clone, Serialize)]
pub struct WithdrawRequest {
    pub id: Id,
    pub affiliate_profile_id: Id,
    pub amount: Amount,
    pub channel: String,
    pub account: String,
    pub status: String,
    pub reject_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_by: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affiliate_profile: Option<Profile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor: Option<Processor>,
}

/// Raw per-profile aggregates (`ProfileStatsAggregate`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ProfileStats {
    pub click_count: i64,
    pub valid_order_count: i64,
    pub pending: Amount,
    pub available: Amount,
    pub withdrawn: Amount,
}

/// Admin statistics; the original struct has no JSON tags, so keys are PascalCase.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct Stats {
    pub click_count: i64,
    pub valid_order_count: i64,
    pub conversion_rate: f64,
    pub pending_commission: Amount,
    pub available_commission: Amount,
    pub withdrawn_commission: Amount,
}

/// Row of the admin affiliate user list.
#[derive(Debug, Clone, Serialize)]
pub struct AdminUserItem {
    pub profile: Profile,
    pub stats: Stats,
}

/// The user's affiliate center (`Dashboard`).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Dashboard {
    pub opened: bool,
    pub affiliate_code: String,
    pub promotion_path: String,
    pub click_count: i64,
    pub valid_order_count: i64,
    pub conversion_rate: f64,
    pub pending_commission: Amount,
    pub available_commission: Amount,
    pub withdrawn_commission: Amount,
}

/// Admin profile list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileFilter {
    pub user_id: Id,
    pub status: String,
    pub code: String,
    pub keyword: String,
}

/// Commission list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommissionFilter {
    pub profile_id: Id,
    pub order_id: Id,
    pub order_no: String,
    pub status: String,
    pub keyword: String,
}

/// Withdrawal list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WithdrawFilter {
    pub profile_id: Id,
    pub status: String,
    pub keyword: String,
}
