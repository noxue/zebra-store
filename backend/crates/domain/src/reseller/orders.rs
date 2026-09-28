//! Reseller view of its sales orders (`application/order_query.go`): masked buyer
//! labels (RSL-10), neutral profit status and per-item pricing from the snapshot.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde_json::Value;
use zs_shared::money::Amount;

use super::model::{LedgerEntry, LedgerStatus, LedgerType, OrderSnapshot};
use crate::Id;

/// `profit_status` values.
pub const PROFIT_CREDITED: &str = "credited";
pub const PROFIT_PENDING: &str = "pending";
pub const PROFIT_UNAVAILABLE: &str = "unavailable";

/// Order statuses the profit view cares about (same strings as the order module).
const STATUS_CANCELED: &str = "canceled";
const STATUS_REFUNDED: &str = "refunded";
const STATUS_PARTIALLY_REFUNDED: &str = "partially_refunded";
const STATUS_PENDING_PAYMENT: &str = "pending_payment";

/// Order statuses counted as paid in reseller operations.
pub const PAID_STATUSES: [&str; 6] = [
    "paid",
    "fulfilling",
    "partially_delivered",
    "partially_refunded",
    "delivered",
    "completed",
];

/// Parent-order fields shown to the reseller.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderHead {
    pub id: Id,
    pub order_no: String,
    pub status: String,
    pub user_id: Id,
    pub guest_email: String,
    pub total_amount: Amount,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}

/// An order item (of the parent, or of its children when the parent has none).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderItemView {
    pub id: Id,
    pub title: Value,
    pub sku_snapshot: Value,
    pub quantity: i32,
    pub unit_price: Amount,
    pub total_price: Amount,
}

/// Everything loaded for one reseller order.
#[derive(Debug, Clone, PartialEq)]
pub struct ResellerOrderRow {
    pub snapshot: OrderSnapshot,
    pub order: OrderHead,
    pub items: Vec<OrderItemView>,
    pub ledger: Vec<LedgerEntry>,
    pub buyer_email: String,
}

/// Order list item (`OrderListItem`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResellerOrderItem {
    pub order_no: String,
    pub status: String,
    pub currency: String,
    pub total_amount: Amount,
    pub base_amount: Amount,
    pub profit_amount: Amount,
    pub profit_status: &'static str,
    pub domain: String,
    pub buyer_label: String,
    pub items_count: usize,
    pub created_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
}

/// Item pricing taken from `pricing_snapshot_json.items[order_item_id]`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemPricing {
    pub base_unit_amount: String,
    pub reseller_unit_amount: String,
    pub base_total_amount: String,
    pub reseller_total_amount: String,
    pub profit_amount: String,
}

/// Order detail line (`OrderItemDetail`).
#[derive(Debug, Clone, PartialEq)]
pub struct ResellerOrderLine {
    pub item: OrderItemView,
    pub pricing: ItemPricing,
}

/// `b***@example.test`; empty when not an address.
pub fn mask_email(email: &str) -> String {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else {
        return String::new();
    };
    let first: String = local.chars().take(1).collect();
    format!("{first}***@{domain}")
}

/// Buyer label shown to the reseller (RSL-10): masked e-mail, `user#{id}` or `guest`.
pub fn buyer_label(user_id: Id, buyer_email: &str, guest_email: &str) -> String {
    if user_id > 0 {
        let label = mask_email(buyer_email);
        return if label.is_empty() {
            format!("user#{user_id}")
        } else {
            label
        };
    }
    let label = mask_email(guest_email);
    if label.is_empty() {
        "guest".to_owned()
    } else {
        label
    }
}

/// Neutral profit status (`neutralProfitStatus`).
pub fn profit_status(
    snapshot: &OrderSnapshot,
    order: &OrderHead,
    ledger: &[LedgerEntry],
) -> &'static str {
    if !snapshot.profit_eligible || !snapshot.profit_amount.is_positive() {
        return PROFIT_UNAVAILABLE;
    }
    if matches!(
        order.status.as_str(),
        STATUS_CANCELED | STATUS_REFUNDED | STATUS_PARTIALLY_REFUNDED
    ) {
        return PROFIT_UNAVAILABLE;
    }
    if order.paid_at.is_none() || order.status == STATUS_PENDING_PAYMENT {
        return PROFIT_PENDING;
    }
    match ledger
        .iter()
        .find(|e| e.kind == LedgerType::OrderProfit)
        .map(|e| e.status)
    {
        Some(LedgerStatus::Available | LedgerStatus::Locked | LedgerStatus::Withdrawn) => {
            PROFIT_CREDITED
        }
        Some(LedgerStatus::Canceled) => PROFIT_UNAVAILABLE,
        Some(LedgerStatus::PendingConfirm) | None => PROFIT_PENDING,
    }
}

/// Builds the list item of a loaded row.
pub fn list_item(row: &ResellerOrderRow) -> ResellerOrderItem {
    ResellerOrderItem {
        order_no: row.order.order_no.clone(),
        status: row.order.status.clone(),
        currency: row.snapshot.currency.clone(),
        total_amount: row.order.total_amount,
        base_amount: row.snapshot.base_amount,
        profit_amount: row.snapshot.profit_amount,
        profit_status: profit_status(&row.snapshot, &row.order, &row.ledger),
        domain: row.snapshot.domain.clone(),
        buyer_label: buyer_label(row.order.user_id, &row.buyer_email, &row.order.guest_email),
        items_count: row.items.len(),
        created_at: row.order.created_at,
        paid_at: row.order.paid_at,
    }
}

fn snapshot_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => {
            super::accounting::fixed(n.to_string().parse().unwrap_or_default(), 2)
        }
        _ => String::new(),
    }
}

fn snapshot_id(v: Option<&Value>) -> Id {
    match v {
        Some(Value::Number(n)) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f as Id))
            .unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<f64>().map(|f| f as Id).unwrap_or(0),
        _ => 0,
    }
}

/// Detail lines with pricing matched by `order_item_id`.
pub fn detail_lines(row: &ResellerOrderRow) -> Vec<ResellerOrderLine> {
    let mut by_item: HashMap<Id, ItemPricing> = HashMap::new();
    if let Some(items) = row
        .snapshot
        .pricing_snapshot_json
        .get("items")
        .and_then(Value::as_array)
    {
        for item in items {
            let id = snapshot_id(item.get("order_item_id"));
            if id <= 0 {
                continue;
            }
            by_item.insert(
                id,
                ItemPricing {
                    base_unit_amount: snapshot_string(item.get("base_unit_amount")),
                    reseller_unit_amount: snapshot_string(item.get("reseller_unit_amount")),
                    base_total_amount: snapshot_string(item.get("base_total_amount")),
                    reseller_total_amount: snapshot_string(item.get("reseller_total_amount")),
                    profit_amount: snapshot_string(item.get("profit_amount")),
                },
            );
        }
    }
    row.items
        .iter()
        .map(|item| ResellerOrderLine {
            item: item.clone(),
            pricing: by_item.get(&item.id).cloned().unwrap_or_default(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::rules::fixtures::{amt, at};
    use serde_json::json;

    fn snapshot(eligible: bool, profit: &str) -> OrderSnapshot {
        OrderSnapshot {
            id: 1,
            order_id: 10,
            reseller_id: 1,
            domain: "r.test".into(),
            currency: "CNY".into(),
            reseller_user_id: 9,
            buyer_user_id: 0,
            base_amount: amt("100"),
            reseller_amount: amt("130"),
            profit_amount: amt(profit),
            profit_eligible: eligible,
            profit_block_reason: String::new(),
            pricing_snapshot_json: json!({"items": [{"order_item_id": 5, "profit_amount": "30.00", "base_unit_amount": 100}]}),
            risk_snapshot_json: json!({}),
            created_at: at(0),
            updated_at: at(0),
        }
    }

    fn head(status: &str, paid: bool) -> OrderHead {
        OrderHead {
            id: 10,
            order_no: "DJ1".into(),
            status: status.into(),
            user_id: 0,
            guest_email: String::new(),
            total_amount: amt("130"),
            created_at: at(0),
            paid_at: paid.then(|| at(1)),
        }
    }

    // RSL-10: masked buyer labels.
    #[test]
    fn rsl10_buyer_labels() {
        assert_eq!(
            buyer_label(3, "buyer-label@example.test", ""),
            "b***@example.test"
        );
        assert_eq!(buyer_label(3, "", ""), "user#3");
        assert_eq!(buyer_label(0, "", "guest@x.test"), "g***@x.test");
        assert_eq!(buyer_label(0, "", ""), "guest");
        assert_eq!(mask_email("noat"), "");
    }

    #[test]
    fn profit_statuses() {
        assert_eq!(
            profit_status(&snapshot(false, "30"), &head("paid", true), &[]),
            PROFIT_UNAVAILABLE
        );
        assert_eq!(
            profit_status(&snapshot(true, "0"), &head("paid", true), &[]),
            PROFIT_UNAVAILABLE
        );
        assert_eq!(
            profit_status(&snapshot(true, "30"), &head("refunded", true), &[]),
            PROFIT_UNAVAILABLE
        );
        assert_eq!(
            profit_status(&snapshot(true, "30"), &head("pending_payment", false), &[]),
            PROFIT_PENDING
        );
        assert_eq!(
            profit_status(&snapshot(true, "30"), &head("paid", true), &[]),
            PROFIT_PENDING
        );
    }

    #[test]
    fn detail_lines_match_snapshot_items() {
        let row = ResellerOrderRow {
            snapshot: snapshot(true, "30"),
            order: head("paid", true),
            items: vec![OrderItemView {
                id: 5,
                title: json!({"zh-CN": "x"}),
                sku_snapshot: json!({}),
                quantity: 1,
                unit_price: amt("130"),
                total_price: amt("130"),
            }],
            ledger: vec![],
            buyer_email: String::new(),
        };
        let lines = detail_lines(&row);
        assert_eq!(lines[0].pricing.profit_amount, "30.00");
        assert_eq!(lines[0].pricing.base_unit_amount, "100.00");
        assert_eq!(list_item(&row).items_count, 1);
    }
}
