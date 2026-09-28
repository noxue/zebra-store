//! Dashboard facts, the read port and the pure aggregations behind
//! overview / trends / rankings (port of `dashboard/application` and the
//! aggregations of `dashboard/infrastructure/gormstore`).
//!
//! The adapter loads light projections of the rows inside a window; every
//! count, sum and day bucket is computed here, so the results are identical
//! on SQLite, MySQL and Postgres and day buckets honour DST (DB-02, DB-05).

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::Value;

use super::inventory::{InventorySnapshot, StockStats};
use super::report::{Window, money, percent, rate};
use crate::settings::schema::storefront::{DashboardAlert, DashboardSetting};
use crate::{Id, Result};

/// Order statuses counted as paid (original `paidOrderStatuses`).
pub const PAID_ORDER_STATUSES: [&str; 6] = [
    "paid",
    "fulfilling",
    "partially_delivered",
    "partially_refunded",
    "delivered",
    "completed",
];
/// Order statuses counted as processing.
pub const PROCESSING_ORDER_STATUSES: [&str; 4] =
    ["paid", "fulfilling", "partially_delivered", "delivered"];
/// Order status counted as completed.
pub const ORDER_COMPLETED: &str = "completed";
/// Order status counted as awaiting payment.
pub const ORDER_PENDING_PAYMENT: &str = "pending_payment";
/// Extra status included in profit statistics (`profitOrderStatuses`).
pub const ORDER_REFUNDED: &str = "refunded";
/// Successful payment status.
pub const PAYMENT_SUCCESS: &str = "success";
/// Failed payment status.
pub const PAYMENT_FAILED: &str = "failed";
/// Wallet (balance) payments are excluded from online payment statistics (PAY-24).
pub const PROVIDER_WALLET: &str = "wallet";
/// Only merchant-absorbed payment fees reduce the profit.
pub const FEE_POLICY_MERCHANT_ABSORBED: &str = "merchant_absorbed";

/// Tolerance under which the refund-reversed cost is treated as zero.
const COST_EPSILON: Decimal = Decimal::from_parts(1, 0, 0, false, 6);

pub fn is_paid_status(status: &str) -> bool {
    PAID_ORDER_STATUSES.contains(&status)
}

/// Statuses whose items count towards revenue and cost.
pub fn is_profit_status(status: &str) -> bool {
    is_paid_status(status) || status == ORDER_REFUNDED
}

// ---------------------------------------------------------------------------
// Facts (row projections loaded by the adapter)
// ---------------------------------------------------------------------------

/// A top-level order (`parent_id IS NULL`) created inside the window.
#[derive(Debug, Clone)]
pub struct OrderFact {
    pub id: Id,
    pub status: String,
    pub currency: String,
    pub total_amount: Decimal,
    pub created_at: DateTime<Utc>,
}

/// A non-wallet payment created inside the window.
#[derive(Debug, Clone)]
pub struct PaymentFact {
    pub status: String,
    pub amount: Decimal,
    pub fee_amount: Decimal,
    pub fee_policy: String,
    pub channel_id: Id,
    /// `payment_channels.name` (empty when the channel is gone).
    pub channel_name: String,
    pub provider_type: String,
    pub channel_type: String,
    pub created_at: DateTime<Utc>,
}

/// An order item whose order was created inside the window with a profit status.
#[derive(Debug, Clone)]
pub struct ItemFact {
    pub order_id: Id,
    pub order_status: String,
    pub order_created_at: DateTime<Utc>,
    pub product_id: Id,
    pub sku_id: Id,
    /// Localized title (first of zh-CN, zh-TW, en-US present).
    pub title: String,
    /// `product_skus.sku_code` of a live SKU, empty otherwise.
    pub sku_code: String,
    /// `product_skus.spec_values_json` of a live SKU.
    pub sku_spec_values: Option<Value>,
    pub quantity: i64,
    pub total_price: Decimal,
    pub coupon_discount: Decimal,
    pub cost_price: Decimal,
}

/// A refund record created inside the window, with its order's cost basis.
#[derive(Debug, Clone)]
pub struct RefundFact {
    pub order_id: Id,
    pub amount: Decimal,
    pub payment_fee_refunded: Decimal,
    /// `orders.total_amount` of the refunded order (zero when missing).
    pub order_total: Decimal,
    /// Item cost of the order itself plus its direct child orders.
    pub cost_basis: Decimal,
    pub created_at: DateTime<Utc>,
}

/// Read port over orders, payments, refunds, users, products and wallets.
#[async_trait]
pub trait DashboardRepo: Send + Sync {
    async fn orders(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<OrderFact>>;
    /// Currency of the most recent top-level order (fallback for empty windows).
    async fn latest_currency(&self) -> Result<String>;
    async fn payments(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<PaymentFact>>;
    async fn items(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<ItemFact>>;
    async fn refunds(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<RefundFact>>;
    async fn new_users(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<i64>;
    async fn active_products(&self) -> Result<i64>;
    async fn total_user_balance(&self) -> Result<Decimal>;
    async fn inventory(&self) -> Result<InventorySnapshot>;
}

// ---------------------------------------------------------------------------
// Responses (JSON shapes of the original `application/types.go`)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OverviewResponse {
    pub range: String,
    pub from: String,
    pub to: String,
    pub timezone: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub currency: String,
    pub kpi: Kpi,
    pub funnel: Funnel,
    pub alerts: Vec<AlertItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Kpi {
    pub orders_total: i64,
    pub paid_orders: i64,
    pub completed_orders: i64,
    pub pending_payment_orders: i64,
    pub processing_orders: i64,
    pub gmv_paid: String,
    pub total_cost: String,
    pub payment_fee: String,
    pub total_profit: String,
    pub profit_margin: String,
    pub payments_total: i64,
    pub payments_success: i64,
    pub payments_failed: i64,
    pub payment_success_rate: String,
    pub new_users: i64,
    pub active_products: i64,
    pub out_of_stock_products: i64,
    pub low_stock_products: i64,
    pub out_of_stock_skus: i64,
    pub low_stock_skus: i64,
    pub auto_available_secrets: i64,
    pub manual_available_units: i64,
    pub total_user_balance: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Funnel {
    pub orders_created: i64,
    pub payments_created: i64,
    pub payments_success: i64,
    pub orders_paid: i64,
    pub orders_completed: i64,
    pub payment_conversion_rate: String,
    pub completion_rate: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AlertItem {
    #[serde(rename = "type")]
    pub kind: String,
    pub level: String,
    pub value: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrendResponse {
    pub range: String,
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub points: Vec<TrendPoint>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TrendPoint {
    pub date: String,
    pub orders_total: i64,
    pub orders_paid: i64,
    pub payments_success: i64,
    pub payments_failed: i64,
    pub gmv_paid: String,
    pub profit: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RankingsResponse {
    pub range: String,
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub top_products: Vec<ProductRanking>,
    pub top_channels: Vec<ChannelRanking>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ProductRanking {
    pub product_id: Id,
    #[serde(skip_serializing_if = "is_zero")]
    pub sku_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sku_code: String,
    #[serde(skip_serializing_if = "is_empty_json")]
    pub sku_spec_values: Option<Value>,
    pub title: String,
    pub paid_orders: i64,
    pub quantity: i64,
    pub paid_amount: String,
    pub total_cost: String,
    pub profit: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ChannelRanking {
    pub channel_id: Id,
    pub channel_name: String,
    pub provider_type: String,
    pub channel_type: String,
    pub success_count: i64,
    pub failed_count: i64,
    pub success_amount: String,
    pub success_rate: String,
}

fn is_zero(v: &Id) -> bool {
    *v == 0
}

/// Go `omitempty` on a JSON map: nil or empty maps are omitted.
pub(crate) fn is_empty_json(v: &Option<Value>) -> bool {
    match v {
        None | Some(Value::Null) => true,
        Some(Value::Object(m)) => m.is_empty(),
        Some(_) => false,
    }
}

// ---------------------------------------------------------------------------
// Aggregations
// ---------------------------------------------------------------------------

/// Order and payment totals of a window (original `OverviewRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderTotals {
    pub orders_total: i64,
    pub paid_orders: i64,
    pub completed_orders: i64,
    pub pending_payment_orders: i64,
    pub processing_orders: i64,
    pub gmv_paid: Decimal,
    pub payments_total: i64,
    pub payments_success: i64,
    pub payments_failed: i64,
    /// Currency of the newest order with a currency.
    pub currency: String,
}

pub fn order_totals(orders: &[OrderFact], payments: &[PaymentFact]) -> OrderTotals {
    let mut t = OrderTotals::default();
    for o in orders {
        let s = o.status.as_str();
        t.orders_total += 1;
        if is_paid_status(s) {
            t.paid_orders += 1;
            t.gmv_paid += o.total_amount;
        }
        if s == ORDER_COMPLETED {
            t.completed_orders += 1;
        }
        if s == ORDER_PENDING_PAYMENT {
            t.pending_payment_orders += 1;
        }
        if PROCESSING_ORDER_STATUSES.contains(&s) {
            t.processing_orders += 1;
        }
    }
    t.currency = orders
        .iter()
        .filter(|o| !o.currency.is_empty())
        .max_by_key(|o| o.id)
        .map(|o| o.currency.clone())
        .unwrap_or_default();
    for p in payments {
        t.payments_total += 1;
        match p.status.as_str() {
            PAYMENT_SUCCESS => t.payments_success += 1,
            PAYMENT_FAILED => t.payments_failed += 1,
            _ => {}
        }
    }
    t
}

/// Revenue and cost of a window or day (original `ProfitOverviewRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profit {
    pub revenue: Decimal,
    pub cost: Decimal,
    pub refunded_cost: Decimal,
    pub payment_fee: Decimal,
}

impl Profit {
    /// Cost including payment fees, optionally reversed by refunds.
    pub fn effective_cost(&self, refund_reverses_cost: bool) -> Decimal {
        let mut cost = self.cost;
        if refund_reverses_cost {
            cost -= self.refunded_cost;
            if cost.abs() < COST_EPSILON {
                cost = Decimal::ZERO;
            }
        }
        cost + self.payment_fee
    }

    pub fn profit(&self, refund_reverses_cost: bool) -> Decimal {
        self.revenue - self.effective_cost(refund_reverses_cost)
    }
}

/// Refund adjustment: refunded amount, fee refunded and proportionally
/// reversed cost (`cost_basis × amount / order_total`).
fn refund_reversed_cost(r: &RefundFact) -> Decimal {
    if r.order_total > Decimal::ZERO && r.amount > Decimal::ZERO {
        r.cost_basis * r.amount / r.order_total
    } else {
        Decimal::ZERO
    }
}

/// Item revenue (`total_price - coupon_discount`; MISC-03: promotion and member
/// discounts are already inside `total_price`).
fn item_revenue(i: &ItemFact) -> Decimal {
    i.total_price - i.coupon_discount
}

fn item_cost(i: &ItemFact) -> Decimal {
    i.cost_price * Decimal::from(i.quantity)
}

fn fee_counts(p: &PaymentFact) -> bool {
    p.status == PAYMENT_SUCCESS
        && p.provider_type != PROVIDER_WALLET
        && p.fee_policy == FEE_POLICY_MERCHANT_ABSORBED
}

/// Profit per local day (`None` key = whole window).
pub fn profit_by_day(
    w: &Window,
    items: &[ItemFact],
    refunds: &[RefundFact],
    payments: &[PaymentFact],
) -> BTreeMap<String, Profit> {
    let mut by_day: BTreeMap<String, Profit> = BTreeMap::new();
    for i in items.iter().filter(|i| is_profit_status(&i.order_status)) {
        let p = by_day.entry(w.day_of(i.order_created_at)).or_default();
        p.revenue += item_revenue(i);
        p.cost += item_cost(i);
    }
    for r in refunds {
        let p = by_day.entry(w.day_of(r.created_at)).or_default();
        p.revenue -= r.amount;
        p.refunded_cost += refund_reversed_cost(r);
        p.payment_fee -= r.payment_fee_refunded;
    }
    for pay in payments.iter().filter(|p| fee_counts(p)) {
        by_day
            .entry(w.day_of(pay.created_at))
            .or_default()
            .payment_fee += pay.fee_amount;
    }
    by_day
}

/// Profit of the whole window.
pub fn profit_total(
    items: &[ItemFact],
    refunds: &[RefundFact],
    payments: &[PaymentFact],
) -> Profit {
    let mut p = Profit::default();
    for i in items.iter().filter(|i| is_profit_status(&i.order_status)) {
        p.revenue += item_revenue(i);
        p.cost += item_cost(i);
    }
    for r in refunds {
        p.revenue -= r.amount;
        p.refunded_cost += refund_reversed_cost(r);
        p.payment_fee -= r.payment_fee_refunded;
    }
    for pay in payments.iter().filter(|p| fee_counts(p)) {
        p.payment_fee += pay.fee_amount;
    }
    p
}

/// Alerts of the overview (original `buildDashboardAlerts`).
pub fn alerts(totals: &OrderTotals, stock: &StockStats, a: &DashboardAlert) -> Vec<AlertItem> {
    let item = |kind: &str, level: &str, value: i64| AlertItem {
        kind: kind.to_owned(),
        level: level.to_owned(),
        value,
    };
    let mut out = Vec::with_capacity(4);
    if stock.out_of_stock_products >= a.out_of_stock_products_threshold {
        out.push(item(
            "out_of_stock_products",
            "error",
            stock.out_of_stock_products,
        ));
    }
    if stock.low_stock_products > 0 {
        out.push(item(
            "low_stock_products",
            "warning",
            stock.low_stock_products,
        ));
    }
    if totals.pending_payment_orders >= a.pending_payment_orders_threshold {
        out.push(item(
            "pending_payment_orders",
            "warning",
            totals.pending_payment_orders,
        ));
    }
    if totals.payments_failed >= a.payments_failed_threshold {
        out.push(item("payments_failed", "warning", totals.payments_failed));
    }
    out
}

/// Inputs of [`overview`] besides the window and setting.
#[derive(Debug, Clone)]
pub struct OverviewInputs<'a> {
    pub totals: &'a OrderTotals,
    pub profit: &'a Profit,
    pub stock: &'a StockStats,
    pub new_users: i64,
    pub active_products: i64,
    pub total_user_balance: Decimal,
}

/// Builds the overview response.
pub fn overview(
    w: &Window,
    setting: &DashboardSetting,
    i: &OverviewInputs<'_>,
) -> OverviewResponse {
    let t = i.totals;
    let reverses = setting.accounting.refund_reverses_cost;
    let total_cost = i.profit.effective_cost(reverses);
    let total_profit = i.profit.revenue - total_cost;
    let d = Decimal::from;
    OverviewResponse {
        range: w.range.clone(),
        from: w.from_label(),
        to: w.to_label(),
        timezone: w.timezone.clone(),
        currency: t.currency.trim().to_uppercase(),
        kpi: Kpi {
            orders_total: t.orders_total,
            paid_orders: t.paid_orders,
            completed_orders: t.completed_orders,
            pending_payment_orders: t.pending_payment_orders,
            processing_orders: t.processing_orders,
            gmv_paid: money(t.gmv_paid),
            total_cost: money(total_cost),
            payment_fee: money(i.profit.payment_fee),
            total_profit: money(total_profit),
            profit_margin: percent(rate(total_profit, i.profit.revenue)),
            payments_total: t.payments_total,
            payments_success: t.payments_success,
            payments_failed: t.payments_failed,
            payment_success_rate: percent(rate(d(t.payments_success), d(t.payments_total))),
            new_users: i.new_users,
            active_products: i.active_products,
            out_of_stock_products: i.stock.out_of_stock_products,
            low_stock_products: i.stock.low_stock_products,
            out_of_stock_skus: i.stock.out_of_stock_skus,
            low_stock_skus: i.stock.low_stock_skus,
            auto_available_secrets: i.stock.auto_available_secrets,
            manual_available_units: i.stock.manual_available_units,
            total_user_balance: money(i.total_user_balance),
        },
        funnel: Funnel {
            orders_created: t.orders_total,
            payments_created: t.payments_total,
            payments_success: t.payments_success,
            orders_paid: t.paid_orders,
            orders_completed: t.completed_orders,
            payment_conversion_rate: percent(rate(d(t.paid_orders), d(t.orders_total))),
            completion_rate: percent(rate(d(t.completed_orders), d(t.paid_orders))),
        },
        alerts: alerts(t, i.stock, &setting.alert),
    }
}

/// Builds the trend response: one point per local day of the window.
pub fn trends(
    w: &Window,
    setting: &DashboardSetting,
    orders: &[OrderFact],
    payments: &[PaymentFact],
    items: &[ItemFact],
    refunds: &[RefundFact],
) -> TrendResponse {
    #[derive(Default)]
    struct Day {
        orders_total: i64,
        orders_paid: i64,
        payments_success: i64,
        payments_failed: i64,
        gmv_paid: Decimal,
    }
    let mut days: HashMap<String, Day> = HashMap::new();
    for o in orders {
        let d = days.entry(w.day_of(o.created_at)).or_default();
        d.orders_total += 1;
        if is_paid_status(&o.status) {
            d.orders_paid += 1;
        }
    }
    for p in payments {
        let d = days.entry(w.day_of(p.created_at)).or_default();
        match p.status.as_str() {
            PAYMENT_SUCCESS => {
                d.payments_success += 1;
                d.gmv_paid += p.amount;
            }
            PAYMENT_FAILED => d.payments_failed += 1,
            _ => {}
        }
    }
    let profits = profit_by_day(w, items, refunds, payments);
    let reverses = setting.accounting.refund_reverses_cost;
    let empty = Day::default();
    let no_profit = Profit::default();
    let points = w
        .days()
        .into_iter()
        .map(|date| {
            let d = days.get(&date).unwrap_or(&empty);
            let p = profits.get(&date).unwrap_or(&no_profit);
            TrendPoint {
                orders_total: d.orders_total,
                orders_paid: d.orders_paid,
                payments_success: d.payments_success,
                payments_failed: d.payments_failed,
                gmv_paid: money(d.gmv_paid),
                profit: money(p.profit(reverses)),
                date,
            }
        })
        .collect();
    TrendResponse {
        range: w.range.clone(),
        from: w.from_label(),
        to: w.to_label(),
        timezone: w.timezone.clone(),
        points,
    }
}

/// Top products by paid amount then quantity (paid orders only).
pub fn top_products(items: &[ItemFact], limit: usize) -> Vec<ProductRanking> {
    struct Acc {
        product_id: Id,
        sku_id: Id,
        title: String,
        sku_code: String,
        spec: Option<Value>,
        orders: HashSet<Id>,
        quantity: i64,
        paid: Decimal,
        cost: Decimal,
    }
    let mut groups: Vec<Acc> = Vec::new();
    let mut index: HashMap<(Id, Id, String), usize> = HashMap::new();
    for i in items.iter().filter(|i| is_paid_status(&i.order_status)) {
        let key = (i.product_id, i.sku_id, i.title.clone());
        let at = *index.entry(key).or_insert_with(|| {
            groups.push(Acc {
                product_id: i.product_id,
                sku_id: i.sku_id,
                title: i.title.clone(),
                sku_code: i.sku_code.clone(),
                spec: i.sku_spec_values.clone(),
                orders: HashSet::new(),
                quantity: 0,
                paid: Decimal::ZERO,
                cost: Decimal::ZERO,
            });
            groups.len() - 1
        });
        let g = &mut groups[at];
        g.orders.insert(i.order_id);
        g.quantity += i.quantity;
        g.paid += item_revenue(i);
        if i.cost_price > Decimal::ZERO {
            g.cost += item_cost(i);
        }
    }
    groups.sort_by(|a, b| {
        b.paid
            .cmp(&a.paid)
            .then(b.quantity.cmp(&a.quantity))
            .then(a.product_id.cmp(&b.product_id))
            .then(a.sku_id.cmp(&b.sku_id))
    });
    groups
        .into_iter()
        .take(limit)
        .map(|g| {
            let title = g.title.trim();
            ProductRanking {
                product_id: g.product_id,
                sku_id: g.sku_id,
                sku_code: g.sku_code,
                sku_spec_values: g.spec,
                title: if title.is_empty() {
                    "-".to_owned()
                } else {
                    title.to_owned()
                },
                paid_orders: i64::try_from(g.orders.len()).unwrap_or(i64::MAX),
                quantity: g.quantity,
                paid_amount: money(g.paid),
                total_cost: money(g.cost),
                profit: money(g.paid - g.cost),
            }
        })
        .collect()
}

/// Top payment channels by success amount then success count.
pub fn top_channels(payments: &[PaymentFact], limit: usize) -> Vec<ChannelRanking> {
    struct Acc {
        key: (Id, String, String, String),
        success: i64,
        failed: i64,
        amount: Decimal,
    }
    let mut groups: Vec<Acc> = Vec::new();
    let mut index: HashMap<(Id, String, String, String), usize> = HashMap::new();
    for p in payments
        .iter()
        .filter(|p| p.provider_type != PROVIDER_WALLET)
    {
        let key = (
            p.channel_id,
            p.channel_name.clone(),
            p.provider_type.clone(),
            p.channel_type.clone(),
        );
        let at = *index.entry(key.clone()).or_insert_with(|| {
            groups.push(Acc {
                key,
                success: 0,
                failed: 0,
                amount: Decimal::ZERO,
            });
            groups.len() - 1
        });
        let g = &mut groups[at];
        match p.status.as_str() {
            PAYMENT_SUCCESS => {
                g.success += 1;
                g.amount += p.amount;
            }
            PAYMENT_FAILED => g.failed += 1,
            _ => {}
        }
    }
    groups.sort_by(|a, b| {
        b.amount
            .cmp(&a.amount)
            .then(b.success.cmp(&a.success))
            .then(a.key.0.cmp(&b.key.0))
    });
    groups
        .into_iter()
        .take(limit)
        .map(|g| {
            let (channel_id, name, provider, channel_type) = g.key;
            ChannelRanking {
                channel_id,
                channel_name: name.trim().to_owned(),
                provider_type: provider.trim().to_owned(),
                channel_type: channel_type.trim().to_owned(),
                success_count: g.success,
                failed_count: g.failed,
                success_amount: money(g.amount),
                success_rate: percent(rate(
                    Decimal::from(g.success),
                    Decimal::from(g.success + g.failed),
                )),
            }
        })
        .collect()
}

/// Builds the rankings response.
pub fn rankings(
    w: &Window,
    setting: &DashboardSetting,
    items: &[ItemFact],
    payments: &[PaymentFact],
) -> RankingsResponse {
    let limit = |v: i64| usize::try_from(v).ok().filter(|v| *v > 0).unwrap_or(5);
    RankingsResponse {
        range: w.range.clone(),
        from: w.from_label(),
        to: w.to_label(),
        timezone: w.timezone.clone(),
        top_products: top_products(items, limit(setting.ranking.top_products_limit)),
        top_channels: top_channels(payments, limit(setting.ranking.top_channels_limit)),
    }
}

#[cfg(test)]
mod tests {
    use super::super::report::{ReportQuery, resolve};
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn window(tz: &str) -> Window {
        resolve(
            &ReportQuery {
                range: "7d".into(),
                timezone: tz.into(),
                ..ReportQuery::default()
            },
            at("2026-09-24T12:00:00Z"),
        )
        .unwrap()
    }

    fn order(id: Id, status: &str, total: Decimal, at_: &str) -> OrderFact {
        OrderFact {
            id,
            status: status.into(),
            currency: "cny".into(),
            total_amount: total,
            created_at: at(at_),
        }
    }

    fn payment(status: &str, amount: Decimal, provider: &str, at_: &str) -> PaymentFact {
        PaymentFact {
            status: status.into(),
            amount,
            fee_amount: Decimal::ZERO,
            fee_policy: String::new(),
            channel_id: 1,
            channel_name: "Alipay".into(),
            provider_type: provider.into(),
            channel_type: "alipay".into(),
            created_at: at(at_),
        }
    }

    fn item(order_id: Id, status: &str, product_id: Id, qty: i64, total: Decimal) -> ItemFact {
        ItemFact {
            order_id,
            order_status: status.into(),
            order_created_at: at("2026-09-24T01:00:00Z"),
            product_id,
            sku_id: 0,
            title: format!("P{product_id}"),
            sku_code: String::new(),
            sku_spec_values: None,
            quantity: qty,
            total_price: total,
            coupon_discount: Decimal::ZERO,
            cost_price: Decimal::ZERO,
        }
    }

    #[test]
    fn totals_count_statuses_and_currency() {
        let orders = [
            order(1, "pending_payment", d("10"), "2026-09-24T01:00:00Z"),
            order(2, "paid", d("20"), "2026-09-24T01:00:00Z"),
            order(3, "completed", d("30.5"), "2026-09-24T01:00:00Z"),
            order(4, "partially_refunded", d("5"), "2026-09-24T01:00:00Z"),
            order(5, "canceled", d("99"), "2026-09-24T01:00:00Z"),
        ];
        let payments = [
            payment("success", d("20"), "epay", "2026-09-24T01:00:00Z"),
            payment("failed", d("20"), "epay", "2026-09-24T01:00:00Z"),
            payment("pending", d("20"), "epay", "2026-09-24T01:00:00Z"),
        ];
        let t = order_totals(&orders, &payments);
        assert_eq!(t.orders_total, 5);
        assert_eq!(t.paid_orders, 3);
        assert_eq!(t.completed_orders, 1);
        assert_eq!(t.pending_payment_orders, 1);
        assert_eq!(t.processing_orders, 1);
        assert_eq!(t.gmv_paid, d("55.5"));
        assert_eq!(
            (t.payments_total, t.payments_success, t.payments_failed),
            (3, 1, 1)
        );
        assert_eq!(t.currency, "cny");
    }

    // MISC-03 ①: unit price 100, promotion price 95 × 2, coupon 10 → revenue 180.
    #[test]
    fn revenue_does_not_double_count_discounts() {
        let mut i = item(1, "paid", 1, 2, d("190"));
        i.coupon_discount = d("10");
        i.cost_price = d("50");
        let p = profit_total(&[i], &[], &[]);
        assert_eq!(p.revenue, d("180"));
        assert_eq!(p.cost, d("100"));
        assert_eq!(p.profit(false), d("80"));
    }

    // MISC-03 ②: refunds reduce revenue; cost is reversed proportionally only when enabled.
    #[test]
    fn refund_reverses_cost_when_enabled() {
        let mut i = item(7, "partially_refunded", 1, 1, d("100"));
        i.cost_price = d("60");
        let refund = RefundFact {
            order_id: 7,
            amount: d("50"),
            payment_fee_refunded: d("1"),
            order_total: d("100"),
            cost_basis: d("60"),
            created_at: at("2026-09-24T02:00:00Z"),
        };
        let mut fee = payment("success", d("100"), "epay", "2026-09-24T01:00:00Z");
        fee.fee_amount = d("3");
        fee.fee_policy = FEE_POLICY_MERCHANT_ABSORBED.into();
        let mut customer_fee = fee.clone();
        customer_fee.fee_policy = "customer_surcharge".into();
        let p = profit_total(&[i], &[refund], &[fee, customer_fee]);
        assert_eq!(p.revenue, d("50"));
        assert_eq!(p.refunded_cost, d("30"));
        assert_eq!(p.payment_fee, d("2"));
        assert_eq!(p.effective_cost(false), d("62"));
        assert_eq!(p.profit(false), d("-12"));
        assert_eq!(p.effective_cost(true), d("32"));
        assert_eq!(p.profit(true), d("18"));
    }

    #[test]
    fn zero_cost_products_keep_their_revenue() {
        let p = profit_total(&[item(1, "completed", 1, 1, d("12"))], &[], &[]);
        assert_eq!(p.profit(false), d("12"));
    }

    #[test]
    fn unpaid_items_are_ignored() {
        let p = profit_total(&[item(1, "pending_payment", 1, 1, d("12"))], &[], &[]);
        assert_eq!(p, Profit::default());
    }

    // PAY-24: wallet payments are excluded from channel rankings.
    #[test]
    fn channel_ranking_excludes_wallet_and_orders_by_amount() {
        let mut b = payment("success", d("5"), "epusdt", "2026-09-24T01:00:00Z");
        b.channel_id = 2;
        b.channel_name = " USDT ".into();
        let payments = [
            payment("success", d("10"), "epay", "2026-09-24T01:00:00Z"),
            payment("failed", d("10"), "epay", "2026-09-24T01:00:00Z"),
            b,
            payment("success", d("999"), PROVIDER_WALLET, "2026-09-24T01:00:00Z"),
        ];
        let r = top_channels(&payments, 5);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].channel_id, 1);
        assert_eq!(r[0].success_amount, "10.00");
        assert_eq!(r[0].success_rate, "50.00");
        assert_eq!(r[1].channel_name, "USDT");
        assert_eq!(r[1].success_rate, "100.00");
        assert_eq!(top_channels(&payments, 1).len(), 1);
    }

    #[test]
    fn product_ranking_groups_and_sorts() {
        let mut a = item(1, "paid", 1, 1, d("10"));
        a.cost_price = d("4");
        let a2 = item(2, "completed", 1, 2, d("20"));
        let b = item(3, "paid", 2, 5, d("30"));
        let mut refunded = item(4, "refunded", 2, 1, d("100"));
        refunded.title = String::new();
        let r = top_products(&[a, a2, b, refunded], 5);
        assert_eq!(r.len(), 2, "refunded orders are not ranked");
        assert_eq!(r[0].product_id, 2, "ties on amount fall back to quantity");
        assert_eq!(r[0].quantity, 5);
        assert_eq!(r[1].product_id, 1);
        assert_eq!(r[1].paid_orders, 2);
        assert_eq!(r[1].quantity, 3);
        assert_eq!(r[1].paid_amount, "30.00");
        assert_eq!(r[1].total_cost, "4.00");
        assert_eq!(r[1].profit, "26.00");
        let json = serde_json::to_value(&r[0]).unwrap();
        assert!(json.get("sku_id").is_none() && json.get("sku_spec_values").is_none());
    }

    #[test]
    fn trends_fill_every_day_and_bucket_by_zone() {
        let w = window("Asia/Shanghai");
        let orders = [
            // 09-23 17:00Z → 09-24 01:00 in Shanghai.
            order(1, "paid", d("10"), "2026-09-23T17:00:00Z"),
            order(2, "pending_payment", d("10"), "2026-09-23T15:00:00Z"),
        ];
        let payments = [payment("success", d("10"), "epay", "2026-09-23T17:00:00Z")];
        let mut it = item(1, "paid", 1, 1, d("10"));
        it.order_created_at = at("2026-09-23T17:00:00Z");
        it.cost_price = d("3");
        let r = trends(
            &w,
            &DashboardSetting::default(),
            &orders,
            &payments,
            &[it],
            &[],
        );
        assert_eq!(r.points.len(), 7);
        let last = r.points.last().unwrap();
        assert_eq!(last.date, "2026-09-24");
        assert_eq!(
            (last.orders_total, last.orders_paid, last.payments_success),
            (1, 1, 1)
        );
        assert_eq!(last.gmv_paid, "10.00");
        assert_eq!(last.profit, "7.00");
        let prev = &r.points[5];
        assert_eq!(prev.date, "2026-09-23");
        assert_eq!((prev.orders_total, prev.orders_paid), (1, 0));
        assert_eq!(r.points[0].gmv_paid, "0.00");
        assert_eq!(r.points[0].profit, "0.00");
    }

    // MISC-03 (3): two orders, one paid after two payment attempts → conversion 50%.
    #[test]
    fn conversion_rate_is_order_based() {
        let w = window("UTC");
        let orders = [
            order(1, "paid", d("10"), "2026-09-24T01:00:00Z"),
            order(2, "pending_payment", d("10"), "2026-09-24T01:00:00Z"),
        ];
        let payments = [
            payment("failed", d("10"), "epay", "2026-09-24T01:00:00Z"),
            payment("success", d("10"), "epay", "2026-09-24T01:01:00Z"),
        ];
        let totals = order_totals(&orders, &payments);
        let o = overview(
            &w,
            &DashboardSetting::default(),
            &OverviewInputs {
                totals: &totals,
                profit: &Profit::default(),
                stock: &StockStats::default(),
                new_users: 0,
                active_products: 0,
                total_user_balance: Decimal::ZERO,
            },
        );
        assert_eq!(o.funnel.payment_conversion_rate, "50.00");
        assert_eq!(o.kpi.payment_success_rate, "50.00");
    }

    #[test]
    fn alerts_follow_thresholds() {
        let setting = DashboardSetting::default();
        let totals = OrderTotals {
            pending_payment_orders: 20,
            payments_failed: 9,
            ..OrderTotals::default()
        };
        let stock = StockStats {
            out_of_stock_products: 1,
            low_stock_products: 2,
            ..StockStats::default()
        };
        let a = alerts(&totals, &stock, &setting.alert);
        let kinds: Vec<_> = a
            .iter()
            .map(|x| (x.kind.as_str(), x.level.as_str(), x.value))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("out_of_stock_products", "error", 1),
                ("low_stock_products", "warning", 2),
                ("pending_payment_orders", "warning", 20),
            ]
        );
    }

    #[test]
    fn overview_rates_and_shape() {
        let w = window("UTC");
        let totals = OrderTotals {
            orders_total: 4,
            paid_orders: 3,
            completed_orders: 1,
            gmv_paid: d("30"),
            payments_total: 3,
            payments_success: 2,
            currency: " usd ".into(),
            ..OrderTotals::default()
        };
        let profit = Profit {
            revenue: d("30"),
            cost: d("10"),
            refunded_cost: Decimal::ZERO,
            payment_fee: d("0.5"),
        };
        let stock = StockStats::default();
        let o = overview(
            &w,
            &DashboardSetting::default(),
            &OverviewInputs {
                totals: &totals,
                profit: &profit,
                stock: &stock,
                new_users: 2,
                active_products: 3,
                total_user_balance: d("12.3"),
            },
        );
        assert_eq!(o.currency, "USD");
        assert_eq!(o.kpi.total_cost, "10.50");
        assert_eq!(o.kpi.total_profit, "19.50");
        assert_eq!(o.kpi.profit_margin, "65.00");
        assert_eq!(o.kpi.payment_success_rate, "66.67");
        assert_eq!(o.kpi.total_user_balance, "12.30");
        assert_eq!(o.funnel.payment_conversion_rate, "75.00");
        assert_eq!(o.funnel.completion_rate, "33.33");
        let json = serde_json::to_value(&o).unwrap();
        assert_eq!(json["alerts"], serde_json::json!([]));
    }
}
