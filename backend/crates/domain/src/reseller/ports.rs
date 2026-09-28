//! Persistence ports of the reseller module (implemented in `zs-infra`).
//!
//! Multi-row writes are coarse methods that run in one transaction; the business
//! decision is either a pure function of this module called inside that
//! transaction, or a plan closure evaluated on the rows read (and locked) there.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::accounting::WithdrawDraft;
use super::model::{
    BalanceAccount, LedgerEntry, ProductSetting, Profile, ResellerDomain, SiteConfig,
    WithdrawRequest,
};
use super::operations::{CurrentCurrency, OverviewRows, PeriodCurrency};
use super::orders::ResellerOrderRow;
use super::pricing::PricedProduct;
use super::site::SiteConfigDraft;
use crate::{Id, Result};

/// Computes the new state of a profile read inside the update transaction.
pub type ProfilePlan<'a> = dyn Fn(Profile) -> Result<Profile> + Send + Sync + 'a;

/// Computes the changed domain rows from the (locked) target and all domains of its reseller.
pub type DomainPlan<'a> =
    dyn Fn(&ResellerDomain, &[ResellerDomain]) -> Result<Vec<ResellerDomain>> + Send + Sync + 'a;

/// Admin profile list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileFilter {
    pub user_id: Option<Id>,
    pub status: String,
    pub settlement_status: String,
    pub keyword: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Admin domain list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DomainFilter {
    pub reseller_id: Option<Id>,
    pub user_id: Option<Id>,
    pub domain: String,
    pub kind: String,
    pub status: String,
    pub verification_status: String,
    pub keyword: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Profiles and domains.
#[async_trait]
pub trait ProfileRepo: Send + Sync {
    /// Live profile with its user.
    async fn profile_by_id(&self, id: Id) -> Result<Option<Profile>>;
    async fn profile_by_user(&self, user_id: Id) -> Result<Option<Profile>>;
    /// Creates a `pending_review` profile.
    async fn create_profile(
        &self,
        user_id: Id,
        apply_reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Profile>;
    /// Applies `plan` to the profile inside a transaction; `None` when it does not exist.
    /// Also returns every domain host of the reseller (for cache invalidation).
    async fn update_profile(
        &self,
        id: Id,
        plan: &ProfilePlan<'_>,
        now: DateTime<Utc>,
    ) -> Result<Option<(Profile, Vec<String>)>>;
    async fn list_profiles(
        &self,
        filter: &ProfileFilter,
        page: PageRequest,
    ) -> Result<Page<Profile>>;

    /// Live domains of a reseller, primary first then by id.
    async fn domains_of(&self, reseller_id: Id) -> Result<Vec<ResellerDomain>>;
    async fn domain_by_id(&self, id: Id) -> Result<Option<ResellerDomain>>;
    /// Active + verified domain whose profile is active (RSL-03), profile attached.
    async fn live_domain(&self, host: &str) -> Result<Option<ResellerDomain>>;
    /// Adds a pending custom domain, reviving a soft-deleted row of the same name;
    /// a live duplicate fails with `error.reseller_domain_conflict`.
    async fn add_custom_domain(
        &self,
        reseller_id: Id,
        domain: &str,
        now: DateTime<Utc>,
    ) -> Result<ResellerDomain>;
    /// Applies `plan` to a domain and its siblings in one transaction; returns the
    /// reloaded target and every host of the reseller. `None` when not found.
    async fn update_domains(
        &self,
        domain_id: Id,
        plan: &DomainPlan<'_>,
        now: DateTime<Utc>,
    ) -> Result<Option<(ResellerDomain, Vec<String>)>>;
    /// Creates or renames the reseller's system subdomain (`rules::plan_system_domain`)
    /// in one transaction; returns the row and the hosts to invalidate. `None` when the
    /// profile does not exist.
    async fn assign_system_domain(
        &self,
        reseller_id: Id,
        domain: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<(ResellerDomain, Vec<String>)>>;
    async fn list_domains(
        &self,
        filter: &DomainFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerDomain>>;
}

/// Admin site-config list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteConfigFilter {
    pub reseller_id: Option<Id>,
    pub keyword: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Site branding rows.
#[async_trait]
pub trait SiteConfigRepo: Send + Sync {
    /// Live configuration with profile + user.
    async fn site_config(&self, reseller_id: Id) -> Result<Option<SiteConfig>>;
    /// Creates, updates or revives the reseller's single configuration.
    async fn upsert_site_config(
        &self,
        reseller_id: Id,
        draft: &SiteConfigDraft,
        now: DateTime<Utc>,
    ) -> Result<SiteConfig>;
    /// Soft-deletes the configuration.
    async fn delete_site_config(&self, reseller_id: Id, now: DateTime<Utc>) -> Result<()>;
    async fn list_site_configs(
        &self,
        filter: &SiteConfigFilter,
        page: PageRequest,
    ) -> Result<Page<SiteConfig>>;
}

/// Console product list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductListFilter {
    pub reseller_id: Id,
    pub category_id: Option<Id>,
    pub keyword: String,
    /// `configured` / `unconfigured`.
    pub configured: String,
    /// `listed` / `hidden`.
    pub listed: String,
}

/// Admin product-setting list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingFilter {
    pub reseller_id: Option<Id>,
    pub user_id: Option<Id>,
    pub product_id: Option<Id>,
    pub keyword: String,
    pub pricing_mode: String,
    pub listed: String,
}

/// Counters of a reseller's rules.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SettingSummary {
    pub configured_products: i64,
    pub hidden_products: i64,
    pub sku_overrides: i64,
    pub pricing_overrides: i64,
}

/// A product with the reseller's rules for it.
pub type ProductWithSettings = (PricedProduct, Vec<ProductSetting>);

/// Pricing rules and the products they apply to.
#[async_trait]
pub trait ProductSettingRepo: Send + Sync {
    /// Active products of active categories with their active SKUs and rules.
    async fn list_products(
        &self,
        filter: &ProductListFilter,
        page: PageRequest,
    ) -> Result<Page<ProductWithSettings>>;
    /// A live product (any status) with its active SKUs and the reseller's rules.
    async fn product_with_settings(
        &self,
        reseller_id: Id,
        product_id: Id,
    ) -> Result<Option<ProductWithSettings>>;
    /// A live product with every non-deleted SKU (validation before saving).
    async fn product_for_save(&self, product_id: Id) -> Result<Option<PricedProduct>>;
    /// Upserts rules (reviving soft-deleted rows) in one transaction.
    async fn save_settings(
        &self,
        reseller_id: Id,
        product_id: Id,
        rows: &[ProductSetting],
        now: DateTime<Utc>,
    ) -> Result<()>;
    /// Soft-deletes one rule.
    async fn delete_setting(
        &self,
        reseller_id: Id,
        product_id: Id,
        sku_id: Id,
        now: DateTime<Utc>,
    ) -> Result<()>;
    async fn list_settings(
        &self,
        filter: &SettingFilter,
        page: PageRequest,
    ) -> Result<Page<ProductSetting>>;
    async fn summarize(&self, reseller_id: Id) -> Result<SettingSummary>;
}

/// Reads used by the order and catalog groups when pricing reseller sites.
#[async_trait]
pub trait PricingRepo: Send + Sync {
    /// Product-level rules plus the SKU rules of `sku_ids` (`ListProductSettingsForPricing`).
    async fn settings_for_pricing(
        &self,
        reseller_id: Id,
        product_ids: &[Id],
        sku_ids: &[Id],
    ) -> Result<Vec<ProductSetting>>;
    /// Products hidden on the reseller site (product rule hidden, or every active SKU hidden).
    async fn hidden_product_ids(&self, reseller_id: Id) -> Result<Vec<Id>>;
    /// Whether `user_id` is an active related account of the reseller (self-dealing).
    async fn is_related_account(&self, reseller_id: Id, user_id: Id) -> Result<bool>;
}

/// Payment details recorded with a profit entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentMeta {
    pub id: Id,
    pub channel_id: Id,
    pub amount: Amount,
    pub status: String,
}

/// An order became paid (`PostOrderProfit`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderPaid {
    pub order_id: Id,
    pub order_no: String,
    /// `None` / `Some(0)` for main-site orders (ignored).
    pub reseller_id: Option<Id>,
    pub currency: String,
    pub wallet_paid_amount: Amount,
    pub online_paid_amount: Amount,
    pub payment: Option<PaymentMeta>,
}

/// A refund was recorded for an order (`HandleRefundDeduct`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRefunded {
    pub order_id: Id,
    pub order_no: String,
    pub reseller_id: Option<Id>,
    pub order_currency: String,
    pub order_total_amount: Amount,
    pub refund_record_id: Id,
    /// e.g. `wallet` / `manual`.
    pub refund_type: String,
    pub refund_amount: Amount,
    pub refund_currency: String,
    /// Amount refunded before this refund.
    pub refunded_before: Amount,
}

/// Admin decision on a withdrawal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WithdrawAction {
    Reject,
    Pay,
}

/// Ledger / balance / withdraw list filter (`admin` adds relations and admin ordering).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FinanceFilter {
    pub admin: bool,
    pub reseller_id: Option<Id>,
    pub user_id: Option<Id>,
    pub keyword: String,
    pub currency: String,
    pub kind: String,
    pub status: String,
    pub order_id: Option<Id>,
    pub order_no: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Profit ledger, balances and withdrawals. Every write refreshes the affected
/// balance caches in the same transaction (RSL-02, RSL-04).
#[async_trait]
pub trait LedgerRepo: Send + Sync {
    /// Posts `order_profit:{order_id}` as `pending_confirm` (idempotent); `false` when
    /// skipped (no snapshot, not eligible, no profit) or already posted.
    async fn post_order_profit(
        &self,
        event: &OrderPaid,
        now: DateTime<Utc>,
        confirm_days: i64,
    ) -> Result<bool>;
    /// Posts `refund_deduct:{refund_record_id}` (RSL-01, RSL-04); `false` when nothing to deduct.
    async fn deduct_refund(
        &self,
        event: &OrderRefunded,
        now: DateTime<Utc>,
        confirm_days: i64,
    ) -> Result<bool>;
    /// Moves due `pending_confirm` entries to `available`; returns the number moved.
    async fn confirm_due(&self, now: DateTime<Utc>) -> Result<u64>;
    /// Locks available entries for a withdrawal (RSL-02 / RSL-05).
    async fn apply_withdraw(
        &self,
        reseller_id: Id,
        draft: &WithdrawDraft,
        now: DateTime<Utc>,
    ) -> Result<WithdrawRequest>;
    /// Rejects (unlock) or pays (withdrawn) a pending withdrawal; `None` when not found.
    async fn review_withdraw(
        &self,
        id: Id,
        admin_id: Id,
        action: WithdrawAction,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<WithdrawRequest>>;
    async fn list_balances(
        &self,
        filter: &FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<BalanceAccount>>;
    async fn list_ledger(
        &self,
        filter: &FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<LedgerEntry>>;
    async fn list_withdraws(
        &self,
        filter: &FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>>;
}

/// Reseller order list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderFilter {
    pub status: String,
    pub order_no: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    pub paid_from: Option<DateTime<Utc>>,
    pub paid_to: Option<DateTime<Utc>>,
}

/// Order counters of a reseller.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderStats {
    pub total: i64,
    pub by_status: BTreeMap<String, i64>,
    pub by_currency: BTreeMap<String, i64>,
}

/// The reseller's sales orders (parent orders carrying its snapshot).
#[async_trait]
pub trait OrderViewRepo: Send + Sync {
    async fn list_orders(
        &self,
        reseller_id: Id,
        filter: &OrderFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerOrderRow>>;
    async fn order_stats(&self, reseller_id: Id, filter: &OrderFilter) -> Result<OrderStats>;
    async fn order_by_no(
        &self,
        reseller_id: Id,
        order_no: &str,
    ) -> Result<Option<ResellerOrderRow>>;
}

/// Admin operations aggregates.
#[async_trait]
pub trait OperationsRepo: Send + Sync {
    async fn overview(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<OverviewRows>;
    async fn finance(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(Vec<PeriodCurrency>, Vec<CurrentCurrency>)>;
}
