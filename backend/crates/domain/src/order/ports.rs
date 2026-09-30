//! Ports of the order group (implemented in `zs-infra`).
//!
//! Reads go through [`OrderRepo`]; every multi-table write is one coarse method of
//! [`OrderStore`] / [`OrderPaymentStore`] that runs in a single database transaction with
//! row locks and conditional updates (DB-01, ORD-01, ORD-03, DLV-03, PRC-01, PAY-04).

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::model::{Fulfillment, JsonMap, Order, OrderStatus, RefundRecord};
use super::risk::{RiskInput, RiskPrep};
use crate::integration::downstream::NewOrderRef;
use crate::integration::hooks::UpstreamDelivery;
use crate::payment::callback::CallbackInput;
use crate::payment::channel::PaymentChannel;
use crate::payment::model::Payment;
use crate::reseller::pricing::OrderPricingContext;
use crate::wallet::Transaction;
use crate::{Id, Result};

/// Storefront tenant scope of order queries (`TenantScope`): the main shop only sees
/// orders without a reseller, a reseller site only its own orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    #[default]
    Main,
    Reseller(Id),
}

/// Who owns an order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Owner {
    User(Id),
    /// Normalized email and the keyed credential digest (ORD-02).
    Guest {
        email: String,
        credential: String,
    },
}

/// Admin order list filter (`ListFilter`).
#[derive(Debug, Clone, Default)]
pub struct AdminOrderFilter {
    pub page: PageRequest,
    pub user_id: Id,
    pub user_keyword: String,
    pub status: String,
    pub order_no: String,
    pub guest_email: String,
    pub product_keyword: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    /// `created_at` / `updated_at` / `total_amount`; anything else sorts by id.
    pub sort_by: String,
    pub sort_asc: bool,
    /// LQA-I3: only paid orders whose purchase order failed (`procurement_issue=1`).
    pub procurement_issue: bool,
    /// LQA-R8: `main` = main-shop orders, `reseller` = any reseller site, a numeric
    /// reseller id = that site; empty = all orders.
    pub reseller: String,
}

/// Admin refund list filter (`RefundRecordListFilter`).
#[derive(Debug, Clone, Default)]
pub struct RefundFilter {
    pub page: PageRequest,
    pub user_id: Id,
    pub user_keyword: String,
    pub order_no: String,
    pub guest_email: String,
    pub product_keyword: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// A user as shown next to admin rows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UserBrief {
    pub email: String,
    pub display_name: String,
    pub locale: String,
}

/// Order reads. Every order comes with its items, fulfillment and children
/// (soft-deleted rows excluded).
#[async_trait]
pub trait OrderRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<Order>>;
    /// Local auto fulfillments awaiting external card conversion after a retryable failure.
    async fn pending_auto_fulfillment_ids(&self) -> Result<Vec<Id>>;
    /// A parent order by number for its owner within the scope.
    async fn find_parent(
        &self,
        order_no: &str,
        owner: &Owner,
        scope: Scope,
    ) -> Result<Option<Order>>;
    /// A parent order by id for its owner within the scope.
    async fn find_parent_by_id(&self, id: Id, owner: &Owner, scope: Scope)
    -> Result<Option<Order>>;
    /// Parent or child order by number for its owner (downloads, DLV-05).
    async fn find_any(&self, order_no: &str, owner: &Owner, scope: Scope) -> Result<Option<Order>>;
    /// Parent orders of an owner, newest first; `order_no` is a substring filter.
    async fn list_for_owner(
        &self,
        owner: &Owner,
        scope: Scope,
        status: &str,
        order_no: &str,
        page: PageRequest,
    ) -> Result<Page<Order>>;
    /// Parent order counts per status of a user.
    async fn stats_for_user(
        &self,
        user_id: Id,
        scope: Scope,
        order_no: &str,
    ) -> Result<BTreeMap<String, i64>>;
    async fn list_admin(&self, filter: &AdminOrderFilter) -> Result<Page<Order>>;
    /// Unconditional status write used by the lazy refund-status sync.
    async fn set_status(&self, id: Id, status: OrderStatus, now: DateTime<Utc>) -> Result<()>;

    async fn refunds_of(&self, order_ids: &[Id]) -> Result<Vec<RefundRecord>>;
    async fn refund(&self, id: Id) -> Result<Option<RefundRecord>>;
    async fn list_refunds(&self, filter: &RefundFilter) -> Result<Page<RefundRecord>>;

    /// Payments of an order, newest first.
    async fn payments_of(&self, order_id: Id) -> Result<Vec<Payment>>;
    /// Latest reusable pending payment with a pay link (`GetLatestPendingByOrder`).
    async fn latest_pending_payment(
        &self,
        order_id: Id,
        now: DateTime<Utc>,
    ) -> Result<Option<Payment>>;
    async fn payment(&self, id: Id) -> Result<Option<Payment>>;
    async fn channel(&self, id: Id) -> Result<Option<PaymentChannel>>;
    async fn channels(&self, ids: &[Id]) -> Result<Vec<PaymentChannel>>;
    /// Active channels, `sort_order DESC, id ASC`.
    async fn active_channels(&self) -> Result<Vec<PaymentChannel>>;
    /// Raw `payment_channel_ids` of the given products.
    async fn product_channel_ids(&self, product_ids: &[Id]) -> Result<Vec<String>>;

    async fn users(&self, ids: &[Id]) -> Result<HashMap<Id, UserBrief>>;
    /// Telegram id bound to a user (bot notifications).
    async fn telegram_user_id(&self, user_id: Id) -> Result<Option<String>>;
    /// Coupon code / promotion names for the admin detail.
    async fn coupon_code(&self, id: Id) -> Result<Option<String>>;
    async fn promotion_names(&self, ids: &[Id]) -> Result<HashMap<Id, String>>;
    /// A user's current member level (`0` = none).
    async fn member_level_of(&self, user_id: Id) -> Result<Id>;
}

/// Stock reservation of a new child order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reservation {
    None,
    /// Reserve `quantity` available card secrets (`sku_id` 0 = any SKU).
    Secrets {
        product_id: Id,
        sku_id: Id,
        quantity: u64,
    },
    /// Reserve manual SKU stock.
    ManualSku {
        sku_id: Id,
        quantity: i32,
    },
}

/// Risk quota check to run inside the create transaction.
#[derive(Debug, Clone)]
pub struct RiskGate {
    pub prep: RiskPrep,
    pub input: RiskInput,
}

/// Everything persisted when an order is created.
#[derive(Debug, Clone)]
pub struct NewOrder {
    /// Parent order with `children` (each with exactly one item); ids are ignored and the
    /// guest password is the plain text password (hashed by the store, ORD-02).
    pub order: Order,
    /// Aligned with `order.children`.
    pub reservations: Vec<Reservation>,
    /// Coupon usage to claim (PRC-01).
    pub coupon: Option<(Id, Amount)>,
    pub reseller: Option<OrderPricingContext>,
    pub risk: Option<RiskGate>,
    /// API order of a downstream shop: the `downstream_order_refs` row is inserted in the
    /// same transaction (`order_id` is filled by the store); a duplicate
    /// `(credential, downstream_order_no)` fails with `keys::DOWNSTREAM_ORDER_DUPLICATE`
    /// (UPS-10).
    pub downstream_ref: Option<NewOrderRef>,
}

/// Why an order is canceled (affiliate reason strings).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelReason {
    User,
    Admin,
    Expired,
    /// The timeout job could not be scheduled.
    Rollback,
}

impl CancelReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "order_canceled_by_user",
            Self::Admin => "order_canceled_by_admin",
            Self::Expired | Self::Rollback => "order_expired_canceled",
        }
    }
}

/// A refund to apply (`AdminRefundToWallet` / `AdminManualRefund`).
#[derive(Debug, Clone, PartialEq)]
pub struct RefundRequest {
    pub order_id: Id,
    pub amount: Amount,
    pub remark: String,
    /// `true` refunds to the user's wallet; `false` only records a manual refund.
    pub to_wallet: bool,
    /// Manual refunds may also give back the payment fee (RFD-01).
    pub payment_fee_refunded: bool,
    pub max_refund_days: i64,
    pub reseller_confirm_days: i64,
    pub now: DateTime<Utc>,
}

/// Result of a refund.
#[derive(Debug, Clone)]
pub struct RefundDone {
    pub order: Order,
    pub record: RefundRecord,
    pub transaction: Option<Transaction>,
}

/// Result of [`OrderStore::upstream_deliver`].
#[derive(Debug, Clone)]
pub struct UpstreamDelivered {
    pub fulfillment: Fulfillment,
    /// Order whose status email is due (the parent of a child order) and its status.
    pub notify: Option<(Id, OrderStatus)>,
}

/// Order state changes, each one database transaction.
#[async_trait]
pub trait OrderStore: Send + Sync {
    /// Creates the parent, children, items, reservations, coupon usage and reseller
    /// snapshot; the risk quota check runs first under its lock keys. Returns the order.
    async fn create(&self, new: &NewOrder, now: DateTime<Utc>) -> Result<Order>;

    /// Cancels a pending order and its children after re-reading it under lock
    /// (ORD-01): releases secrets, manual stock, coupon usages and wallet money and
    /// expires open payments (ORD-04). `error.order_cancel_not_allowed` when it is no
    /// longer pending. Returns the canceled order.
    async fn cancel(&self, order_id: Id, now: DateTime<Utc>) -> Result<Order>;

    /// Admin status change of a single (child or childless) order: plain transitions,
    /// with the parent status recomputed.
    async fn set_status(&self, order_id: Id, target: OrderStatus, now: DateTime<Utc>)
    -> Result<()>;

    /// Admin status change of a parent order to `completed` / `partially_refunded` /
    /// `refunded`, cascading to the children with transition checks.
    async fn set_parent_status(
        &self,
        order_id: Id,
        target: OrderStatus,
        now: DateTime<Utc>,
    ) -> Result<()>;

    /// Recomputes a parent's status from its children; returns the resulting status.
    async fn sync_parent(&self, parent_id: Id, now: DateTime<Utc>) -> Result<Option<OrderStatus>>;

    /// Auto delivery of a paid child order (DLV-09): reserved (else available) secrets
    /// become used, the fulfillment row is written and the order becomes `completed`.
    async fn auto_fulfill(&self, order_id: Id, now: DateTime<Utc>) -> Result<Fulfillment>;

    /// Reserves and marks the cards used, but leaves the order `fulfilling` with an
    /// empty pending fulfillment. Returns the raw content only to the internal caller.
    async fn prepare_auto_fulfill(&self, order_id: Id, now: DateTime<Utc>) -> Result<String>;

    /// Publishes a fully converted payload and transitions `fulfilling → completed`.
    async fn finalize_auto_fulfill(
        &self,
        order_id: Id,
        payload: &str,
        now: DateTime<Utc>,
    ) -> Result<Fulfillment>;

    /// Manual delivery of a paid/fulfilling order: fulfillment row + `delivered`.
    async fn manual_fulfill(
        &self,
        order_id: Id,
        admin_id: Id,
        payload: &str,
        delivery_data: &JsonMap,
        delivered_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Fulfillment>;

    /// Procurement accepted: `paid → fulfilling` (conditional) with the parent recomputed.
    /// Returns whether the order changed.
    async fn upstream_fulfilling(&self, order_id: Id, now: DateTime<Utc>) -> Result<bool>;

    /// Supplier delivery: one `upstream` fulfillment per order (idempotent), order
    /// `paid|fulfilling → delivered` (conditional), parent recomputed. `None` when the
    /// fulfillment already existed.
    async fn upstream_deliver(
        &self,
        order_id: Id,
        delivery: &UpstreamDelivery,
        now: DateTime<Utc>,
    ) -> Result<Option<UpstreamDelivered>>;

    /// Procurement failed: `fulfilling → paid` (conditional) with the parent recomputed.
    async fn upstream_rollback(&self, order_id: Id, now: DateTime<Utc>) -> Result<bool>;

    /// Records a refund (RFD-01 … RFD-04), with affiliate and reseller claw-backs.
    async fn refund(&self, request: &RefundRequest) -> Result<RefundDone>;

    /// Switches the fee-refunded flag of a manual refund record (RFD-01).
    async fn set_refund_fee_flag(
        &self,
        record_id: Id,
        refunded: bool,
        now: DateTime<Utc>,
    ) -> Result<RefundRecord>;
}

/// Inputs of [`OrderPaymentStore::begin`].
#[derive(Debug, Clone)]
pub struct BeginPayment {
    pub order_id: Id,
    /// `0` = wallet balance only.
    pub channel_id: Id,
    /// Selected method within a multi-method channel; empty keeps legacy channel behavior.
    pub channel_type: String,
    pub use_balance: bool,
    pub wallet_only: bool,
    pub customer_fee_enabled: bool,
    pub reuse_legacy_fee_payment: bool,
    pub reseller_confirm_days: i64,
    pub now: DateTime<Utc>,
}

/// Outcome of [`OrderPaymentStore::begin`].
#[derive(Debug, Clone)]
pub enum BeginOutcome {
    /// An open payment of the same channel with a pay link is reused.
    Reused {
        payment: Payment,
        channel: PaymentChannel,
        order: Order,
    },
    /// The wallet covered everything: the order is paid (with its children, after the
    /// status changes) by a `wallet` payment (PAY-24).
    PaidByWallet { payment: Payment, order: Order },
    /// A new `initiated` payment waits for the gateway.
    Created {
        payment: Payment,
        channel: PaymentChannel,
        order: Order,
    },
}

/// A settled order payment.
#[derive(Debug, Clone)]
pub struct Settled {
    pub payment: Payment,
    /// The order after settlement (with children and items).
    pub order: Option<Order>,
    /// True only for the transition that paid the order (first success).
    pub order_paid: bool,
    /// Success that did not cover the online requirement (PAY-02).
    pub underpaid: bool,
    /// Wallet recharge that was credited by this call.
    pub recharge_credited: Option<RechargeCredited>,
}

/// A recharge credited by a settlement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RechargeCredited {
    pub recharge_id: Id,
    pub recharge_no: String,
    pub user_id: Id,
    pub amount: Amount,
    pub currency: String,
    pub provider_type: String,
    pub channel_type: String,
}

/// Payment writes of orders and wallet recharges.
#[async_trait]
pub trait OrderPaymentStore: Send + Sync {
    /// Locks the order, applies/releases wallet balance, and either pays the order with the
    /// wallet, reuses an open payment, or creates an `initiated` payment (PAY-02/03/05/11/12/23).
    async fn begin(&self, request: &BeginPayment) -> Result<BeginOutcome>;
    /// Saves the gateway creation result (payment becomes `pending`).
    async fn save_started(&self, payment: &Payment) -> Result<()>;
    /// Gateway creation failed: payment `failed`, wallet part returned.
    async fn fail_started(&self, payment_id: Id, order_id: Id, now: DateTime<Utc>) -> Result<()>;
    /// Other open payments of the order become `expired` + superseded (PAY-03).
    async fn supersede_others(
        &self,
        order_id: Id,
        keep_payment_id: Id,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    /// Applies a verified callback (orders and wallet recharges) under row locks (PAY-04).
    async fn settle(
        &self,
        input: &CallbackInput,
        reseller_confirm_days: i64,
        now: DateTime<Utc>,
    ) -> Result<Settled>;
}

/// Cart rows (`cart_items`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CartRow {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub fulfillment_type: String,
}

/// Cart persistence (rows are hard-deleted, ORD-11).
#[async_trait]
pub trait CartRepo: Send + Sync {
    async fn list(&self, user_id: Id) -> Result<Vec<CartRow>>;
    async fn upsert(&self, user_id: Id, row: &CartRow, now: DateTime<Utc>) -> Result<()>;
    async fn delete(&self, user_id: Id, product_id: Id, sku_id: Id) -> Result<()>;
}

/// Wallet reads the order group needs before its transactions.
#[async_trait]
pub trait OrderWallet: Send + Sync {
    /// Current balance (`0` without an account).
    async fn balance(&self, user_id: Id) -> Result<Amount>;
}

/// Affiliate reactions to the order lifecycle (implemented against the affiliate tables
/// until the affiliate group exposes its service).
#[async_trait]
pub trait AffiliateHooks: Send + Sync {
    /// `(affiliate_profile_id, affiliate_code)` snapshot for a new order; self-referral
    /// and unknown codes yield `(None, "")`.
    async fn resolve_snapshot(
        &self,
        user_id: Id,
        code: &str,
        visitor_key: &str,
    ) -> Result<(Option<Id>, String)>;
    /// Creates the order's commission (idempotent).
    async fn order_paid(&self, order_id: Id) -> Result<()>;
    /// Rejects the order's open commissions.
    async fn order_canceled(&self, order_id: Id, reason: &str) -> Result<()>;
}
