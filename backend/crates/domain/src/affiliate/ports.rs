//! Affiliate ports (implemented in `zs-infra`).

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::model::{
    Commission, CommissionFilter, Profile, ProfileFilter, ProfileStats, WithdrawFilter,
    WithdrawRequest,
};
use crate::{Id, Result};

/// A click to record.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NewClick {
    pub profile_id: Id,
    pub visitor_key: String,
    pub landing_path: String,
    pub referrer: String,
    pub client_ip: String,
    pub user_agent: String,
}

/// An order item as seen by commission calculation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommissionItem {
    pub product_id: Id,
    pub total_price: Amount,
    pub coupon_discount: Amount,
    /// `products.is_affiliate_enabled` of the item's product.
    pub affiliate_enabled: bool,
}

/// The order snapshot commission calculation needs (`OrderReader`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommissionOrder {
    pub id: Id,
    pub user_id: Id,
    pub paid_at: Option<DateTime<Utc>>,
    pub affiliate_profile_id: Option<Id>,
    pub affiliate_code: String,
    /// Items of the child orders, or of the order itself when it has no children.
    pub items: Vec<CommissionItem>,
}

/// A commission to create.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewCommission {
    pub profile_id: Id,
    pub order_id: Id,
    pub commission_type: String,
    pub base_amount: Amount,
    pub rate_percent: Amount,
    pub commission_amount: Amount,
    pub status: String,
    pub confirm_at: Option<DateTime<Utc>>,
    pub available_at: Option<DateTime<Utc>>,
    pub now: DateTime<Utc>,
}

/// Affiliate persistence. Soft-deleted rows are never returned; preloaded relations
/// (profile user, commission order, withdrawal processor) skip soft-deleted rows too.
#[async_trait]
pub trait AffiliateRepo: Send + Sync {
    /// `users.status` of a live user (`None` when missing).
    async fn user_status(&self, user_id: Id) -> Result<Option<String>>;
    async fn profile(&self, id: Id) -> Result<Option<Profile>>;
    async fn profile_by_user(&self, user_id: Id) -> Result<Option<Profile>>;
    /// Case-insensitive (codes are stored upper-case).
    async fn profile_by_code(&self, code: &str) -> Result<Option<Profile>>;
    /// Inserts an active profile; `Ok(None)` when the code (or the user) is already taken.
    async fn create_profile(
        &self,
        user_id: Id,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<Profile>>;
    async fn set_profile_status(&self, id: Id, status: &str, now: DateTime<Utc>) -> Result<()>;
    async fn batch_profile_status(
        &self,
        ids: &[Id],
        status: &str,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    /// Newest first.
    async fn list_profiles(
        &self,
        filter: &ProfileFilter,
        page: PageRequest,
    ) -> Result<Page<Profile>>;
    async fn profile_stats(&self, ids: &[Id]) -> Result<HashMap<Id, ProfileStats>>;

    async fn has_recent_click(
        &self,
        profile_id: Id,
        visitor_key: &str,
        landing_path: &str,
        since: DateTime<Utc>,
    ) -> Result<bool>;
    async fn create_click(&self, click: &NewClick, now: DateTime<Utc>) -> Result<()>;
    /// Active profile of the visitor's most recent click since `since`.
    async fn latest_profile_by_visitor(
        &self,
        visitor_key: &str,
        since: DateTime<Utc>,
    ) -> Result<Option<Profile>>;

    async fn list_commissions(
        &self,
        filter: &CommissionFilter,
        page: PageRequest,
    ) -> Result<Page<Commission>>;
    async fn list_withdraws(
        &self,
        filter: &WithdrawFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>>;
    async fn withdraw(&self, id: Id) -> Result<Option<WithdrawRequest>>;

    /// `pending_confirm → available` for due, unbound commissions with one conditional
    /// update (idempotent, safe for concurrent workers — AFF-01). Returns rows changed.
    async fn confirm_due(&self, now: DateTime<Utc>) -> Result<u64>;
    /// In one transaction: locks the profile's available unbound commissions, allocates
    /// `amount` (splitting the last row), creates the request and binds the commissions.
    /// Errors: `error.bad_request` when not opened/inactive or the balance is insufficient.
    async fn apply_withdraw(
        &self,
        user_id: Id,
        amount: Amount,
        channel: &str,
        account: &str,
        now: DateTime<Utc>,
    ) -> Result<WithdrawRequest>;
    /// In one transaction: locks a `pending_review` request, then pays (commissions become
    /// `withdrawn`) or rejects it (commissions are released).
    async fn review_withdraw(
        &self,
        id: Id,
        admin_id: Id,
        pay: bool,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<WithdrawRequest>;

    /// The order (with child items) for commission calculation.
    async fn commission_order(&self, order_id: Id) -> Result<Option<CommissionOrder>>;
    /// Inserts a commission; `false` when one already exists for (profile, order, type).
    async fn create_commission(&self, commission: &NewCommission) -> Result<bool>;
    /// Rejects the order's pending/available commissions not yet bound to a withdrawal.
    async fn reject_order_commissions(
        &self,
        order_id: Id,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<u64>;
}
