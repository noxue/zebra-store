//! Orders, order items, fulfillments and refund records (JSON shape of the original
//! `orderdomain.Order` / `OrderItem`, `fulfillmentdomain.Fulfillment`, `OrderRefundRecord`).

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use zs_shared::money::Amount;

use crate::Id;

/// Free-form JSON object (`jsonmap.JSON`).
pub type JsonMap = Map<String, Value>;

/// Error keys of the order module (identical to the original handlers).
pub mod keys {
    pub const ORDER_NOT_FOUND: &str = "error.order_not_found";
    pub const ORDER_FETCH_FAILED: &str = "error.order_fetch_failed";
    pub const ORDER_CREATE_FAILED: &str = "error.order_create_failed";
    pub const ORDER_UPDATE_FAILED: &str = "error.order_update_failed";
    pub const ORDER_STATUS_INVALID: &str = "error.order_status_invalid";
    /// Refunded states are reachable only through the refund endpoints (live QA I-2).
    pub const ORDER_STATUS_REFUND_REQUIRED: &str = "error.order_status_refund_required";
    pub const ORDER_ITEM_INVALID: &str = "error.order_item_invalid";
    pub const ORDER_AMOUNT_INVALID: &str = "error.order_amount_invalid";
    pub const ORDER_CANCEL_NOT_ALLOWED: &str = "error.order_cancel_not_allowed";
    pub const ORDER_REFUND_EXPIRED: &str = "error.order_refund_expired";
    pub const ORDER_CURRENCY_MISMATCH: &str = "error.order_currency_mismatch";
    pub const GUEST_ORDER_NOT_FOUND: &str = "error.guest_order_not_found";
    pub const GUEST_EMAIL_REQUIRED: &str = "error.guest_email_required";
    pub const GUEST_PASSWORD_REQUIRED: &str = "error.guest_password_required";
    pub const GUEST_PASSWORD_TOO_SHORT: &str = "error.guest_password_too_short";
    pub const EMAIL_INVALID: &str = "error.email_invalid";
    pub const PRODUCT_NOT_AVAILABLE: &str = "error.product_not_available";
    pub const PRODUCT_PURCHASE_NOT_ALLOWED: &str = "error.product_purchase_not_allowed";
    pub const PRODUCT_PRICE_INVALID: &str = "error.product_price_invalid";
    pub const FULFILLMENT_INVALID: &str = "error.fulfillment_invalid";
    pub const FULFILLMENT_EXISTS: &str = "error.fulfillment_exists";
    pub const FULFILLMENT_CREATE_FAILED: &str = "error.fulfillment_create_failed";
    pub const FULFILLMENT_NOT_FOUND: &str = "error.fulfillment_not_found";
    pub const CARD_SECRET_INSUFFICIENT: &str = "error.card_secret_insufficient";
    pub const MANUAL_STOCK_INSUFFICIENT: &str = "error.manual_stock_insufficient";
    pub const QUEUE_UNAVAILABLE: &str = "error.queue_unavailable";
    pub const RESELLER_COUPON_NOT_ALLOWED: &str = "error.reseller_coupon_not_allowed";
    pub const WALLET_ONLY_PAYMENT_REQUIRED: &str = "error.wallet_only_payment_required";
    pub const PAYMENT_CREATE_FAILED: &str = "error.payment_create_failed";
    pub const RISK_IP_BLACKLISTED: &str = "error.risk_ip_blacklisted";
    pub const RISK_CLIENT_IP_UNAVAILABLE: &str = "error.risk_client_ip_unavailable";
    pub const RISK_TOO_MANY_PENDING: &str = "error.risk_too_many_pending_orders";
    pub const RISK_PRODUCT_QUANTITY_LIMIT: &str = "error.risk_product_quantity_limit";
    pub const RISK_PENDING_PRODUCT_LIMIT: &str = "error.risk_pending_product_quantity_limit";
    pub const RISK_ORDER_RATE_LIMITED: &str = "error.risk_order_rate_limited";
    /// Unique `(api_credential_id, downstream_order_no)` violated (UPS-10).
    pub const DOWNSTREAM_ORDER_DUPLICATE: &str = "error.duplicate_downstream_order";
}

/// Order status values (`constants.OrderStatus*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    PendingPayment,
    Paid,
    Fulfilling,
    PartiallyDelivered,
    Delivered,
    Completed,
    Canceled,
    Refunded,
    PartiallyRefunded,
}

impl OrderStatus {
    pub const ALL: [Self; 9] = [
        Self::PendingPayment,
        Self::Paid,
        Self::Fulfilling,
        Self::PartiallyDelivered,
        Self::Delivered,
        Self::Completed,
        Self::Canceled,
        Self::Refunded,
        Self::PartiallyRefunded,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PendingPayment => "pending_payment",
            Self::Paid => "paid",
            Self::Fulfilling => "fulfilling",
            Self::PartiallyDelivered => "partially_delivered",
            Self::Delivered => "delivered",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
            Self::Refunded => "refunded",
            Self::PartiallyRefunded => "partially_refunded",
        }
    }

    /// Parses a (trimmed, case-insensitive) status; unknown values yield `None`.
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|s| s.as_str() == raw)
    }
}

impl fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Fulfillment types stored on order items (`constants.FulfillmentType*`).
pub mod fulfillment_type {
    pub const AUTO: &str = "auto";
    pub const MANUAL: &str = "manual";
    pub const UPSTREAM: &str = "upstream";

    /// Blank means manual (`NormalizeFulfillmentType`).
    pub fn normalize(raw: &str) -> &'static str {
        match raw.trim().to_ascii_lowercase().as_str() {
            "auto" => AUTO,
            "upstream" => UPSTREAM,
            _ => MANUAL,
        }
    }
}

/// Fulfillment row status values.
pub mod fulfillment_status {
    pub const PENDING: &str = "pending";
    pub const DELIVERED: &str = "delivered";
}

/// Refund record types (`constants.OrderRefundType*`).
pub mod refund_type {
    pub const MANUAL: &str = "manual";
    pub const WALLET: &str = "wallet";
}

/// Delivered content lines shown in API responses (`PayloadMaxPreviewLines`).
pub const PAYLOAD_MAX_PREVIEW_LINES: usize = 100;
/// Delivered content lines embedded in an email before it becomes an attachment
/// (`PayloadMaxEmailLines`, NTF-10).
pub const PAYLOAD_MAX_EMAIL_LINES: usize = 20;

fn is_zero(v: &Id) -> bool {
    *v == 0
}

/// An order item (`order_items`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderItem {
    pub id: Id,
    pub order_id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub title: JsonMap,
    pub sku_snapshot: JsonMap,
    pub tags: Vec<String>,
    pub original_unit_price: Amount,
    pub unit_price: Amount,
    pub cost_price: Amount,
    pub quantity: i32,
    pub original_total_price: Amount,
    pub total_price: Amount,
    #[serde(rename = "coupon_discount_amount")]
    pub coupon_discount: Amount,
    #[serde(rename = "member_discount_amount")]
    pub member_discount: Amount,
    #[serde(rename = "promotion_discount_amount")]
    pub promotion_discount: Amount,
    #[serde(rename = "wholesale_discount_amount")]
    pub wholesale_discount: Amount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<Id>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub promotion_name: String,
    pub fulfillment_type: String,
    #[serde(rename = "manual_form_schema_snapshot")]
    pub manual_form_schema: JsonMap,
    #[serde(rename = "manual_form_submission")]
    pub manual_form_submission: JsonMap,
    pub instructions: JsonMap,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Delivered content of an order (`fulfillments`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Fulfillment {
    pub id: Id,
    pub order_id: Id,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub payload: String,
    /// Not persisted; filled when the payload is truncated for a response.
    pub payload_line_count: usize,
    #[serde(rename = "delivery_data")]
    pub logistics: JsonMap,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_by: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Fulfillment {
    /// Counts lines and keeps the first `max_lines` (`TruncatePayload`, DLV-05).
    pub fn truncate_payload(&mut self, max_lines: usize) {
        if self.payload.is_empty() {
            return;
        }
        let lines: Vec<&str> = self.payload.split('\n').collect();
        self.payload_line_count = lines.len();
        if lines.len() > max_lines {
            self.payload = lines[..max_lines].join("\n");
        }
    }
}

/// An order (parent or child) with its items, fulfillment and children.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Order {
    pub id: Id,
    pub order_no: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Id>,
    #[serde(skip_serializing_if = "is_zero")]
    pub user_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guest_email: String,
    /// Keyed HMAC digest of the guest credentials (never serialized, ORD-02).
    #[serde(skip)]
    pub guest_password: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guest_locale: String,
    pub status: OrderStatus,
    pub currency: String,
    pub original_amount: Amount,
    pub discount_amount: Amount,
    pub member_discount_amount: Amount,
    pub promotion_discount_amount: Amount,
    pub wholesale_discount_amount: Amount,
    pub total_amount: Amount,
    pub wallet_paid_amount: Amount,
    pub online_paid_amount: Amount,
    pub refunded_amount: Amount,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_level_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coupon_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub promotion_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affiliate_profile_id: Option<Id>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub affiliate_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reseller_id: Option<Id>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reseller_domain: String,
    pub reseller_profit_amount: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub client_ip: String,
    #[serde(skip)]
    pub risk_ip: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<OrderItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfillment: Option<Fulfillment>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Order>,
}

impl Order {
    /// True for a parent order (no `parent_id`).
    pub fn is_parent(&self) -> bool {
        self.parent_id.is_none()
    }

    /// Items of the order itself or of its children (`FillOrderItemsFromChildren` source).
    pub fn all_items(&self) -> Vec<&OrderItem> {
        let mut items: Vec<&OrderItem> = self.items.iter().collect();
        for child in &self.children {
            items.extend(child.items.iter());
        }
        items
    }

    /// Copies child items onto an item-less parent for response compatibility
    /// (`FillOrderItemsFromChildren`).
    pub fn fill_items_from_children(&mut self) {
        if !self.items.is_empty() || self.children.is_empty() {
            return;
        }
        let id = self.id;
        self.items = self
            .children
            .iter()
            .flat_map(|c| c.items.iter().cloned())
            .map(|mut item| {
                item.order_id = id;
                item
            })
            .collect();
    }

    /// Ids of the order and its children.
    pub fn family_ids(&self) -> Vec<Id> {
        let mut ids = vec![self.id];
        ids.extend(self.children.iter().map(|c| c.id));
        ids
    }

    /// Truncates the delivered content of the order and its children (DLV-05).
    pub fn truncate_fulfillment_payload(&mut self) {
        if let Some(f) = self.fulfillment.as_mut() {
            f.truncate_payload(PAYLOAD_MAX_PREVIEW_LINES);
        }
        for child in &mut self.children {
            child.truncate_fulfillment_payload();
        }
    }

    /// Full delivered content: the order's own payload, else its children's joined by `\n`
    /// (`collectFulfillmentPayload`).
    pub fn collect_fulfillment_payload(&self) -> String {
        if let Some(f) = self.fulfillment.as_ref().filter(|f| !f.payload.is_empty()) {
            return f.payload.clone();
        }
        self.children
            .iter()
            .filter_map(|c| c.fulfillment.as_ref())
            .filter(|f| !f.payload.is_empty())
            .map(|f| f.payload.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A refund record (`order_refund_records`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefundRecord {
    pub id: Id,
    pub user_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guest_email: String,
    pub order_id: Id,
    #[serde(rename = "type")]
    pub kind: String,
    pub amount: Amount,
    pub payment_fee_refunded: bool,
    pub payment_fee_refunded_amount: Amount,
    pub currency: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub remark: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Builds the child order number `{parent}-{nn}` (`buildChildOrderNo`).
pub fn child_order_no(parent: &str, seq: usize) -> String {
    if seq == 0 {
        parent.to_owned()
    } else {
        format!("{parent}-{seq:02}")
    }
}

/// Prefix of order numbers (`serial.Generate("DJ")`).
pub const ORDER_NO_PREFIX: &str = "DJ";

/// Generates an order number such as `DJ20260924173502588608`.
pub fn new_order_no(now: DateTime<Utc>) -> String {
    zs_shared::serial::generate(ORDER_NO_PREFIX, now)
}

/// `"{product_id}:{sku_id}"` (`ItemKey`).
pub fn item_key(product_id: Id, sku_id: Id) -> String {
    format!("{product_id}:{sku_id}")
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn t0() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-24T10:00:00Z")
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default()
    }

    pub fn item(product_id: Id, ft: &str, qty: i32) -> OrderItem {
        OrderItem {
            id: product_id,
            order_id: 0,
            product_id,
            sku_id: 0,
            title: JsonMap::new(),
            sku_snapshot: JsonMap::new(),
            tags: vec![],
            original_unit_price: Amount::from(10),
            unit_price: Amount::from(10),
            cost_price: Amount::ZERO,
            quantity: qty,
            original_total_price: Amount::from(10),
            total_price: Amount::from(10),
            coupon_discount: Amount::ZERO,
            member_discount: Amount::ZERO,
            promotion_discount: Amount::ZERO,
            wholesale_discount: Amount::ZERO,
            promotion_id: None,
            promotion_name: String::new(),
            fulfillment_type: ft.into(),
            manual_form_schema: JsonMap::new(),
            manual_form_submission: JsonMap::new(),
            instructions: JsonMap::new(),
            created_at: t0(),
            updated_at: t0(),
        }
    }

    pub fn order(id: Id, status: OrderStatus) -> Order {
        Order {
            id,
            order_no: format!("DJ{id}"),
            parent_id: None,
            user_id: 0,
            guest_email: String::new(),
            guest_password: String::new(),
            guest_locale: String::new(),
            status,
            currency: "CNY".into(),
            original_amount: Amount::from(100),
            discount_amount: Amount::ZERO,
            member_discount_amount: Amount::ZERO,
            promotion_discount_amount: Amount::ZERO,
            wholesale_discount_amount: Amount::ZERO,
            total_amount: Amount::from(100),
            wallet_paid_amount: Amount::ZERO,
            online_paid_amount: Amount::from(100),
            refunded_amount: Amount::ZERO,
            member_level_id: None,
            coupon_id: None,
            promotion_id: None,
            affiliate_profile_id: None,
            affiliate_code: String::new(),
            reseller_id: None,
            reseller_domain: String::new(),
            reseller_profit_amount: Amount::ZERO,
            client_ip: String::new(),
            risk_ip: String::new(),
            expires_at: None,
            paid_at: None,
            canceled_at: None,
            created_at: t0(),
            updated_at: t0(),
            items: vec![],
            fulfillment: None,
            children: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::*;
    use super::*;

    #[test]
    fn status_round_trip() {
        for s in OrderStatus::ALL {
            assert_eq!(OrderStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(OrderStatus::parse(" PAID "), Some(OrderStatus::Paid));
        assert_eq!(OrderStatus::parse("bogus"), None);
    }

    #[test]
    fn child_numbers_are_two_digit() {
        assert_eq!(child_order_no("DJ1", 1), "DJ1-01");
        assert_eq!(child_order_no("DJ1", 12), "DJ1-12");
        assert_eq!(child_order_no("DJ1", 0), "DJ1");
    }

    /// DLV-05: responses keep 100 lines and report the real count.
    #[test]
    fn dlv_05_truncates_payload() {
        let payload = (0..3000)
            .map(|i| format!("L{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut f = Fulfillment {
            id: 1,
            order_id: 1,
            kind: "auto".into(),
            status: "delivered".into(),
            payload,
            payload_line_count: 0,
            logistics: JsonMap::new(),
            delivered_by: None,
            delivered_at: None,
            created_at: t0(),
            updated_at: t0(),
        };
        f.truncate_payload(PAYLOAD_MAX_PREVIEW_LINES);
        assert_eq!(f.payload_line_count, 3000);
        assert_eq!(f.payload.split('\n').count(), 100);
        assert!(f.payload.ends_with("L99"));
    }

    /// DLV-05: a parent's download joins its children's payloads.
    #[test]
    fn dlv_05_parent_payload_joins_children() {
        let mut parent = order(1, OrderStatus::Completed);
        for (i, text) in ["A\nB", "C"].iter().enumerate() {
            let mut child = order(10 + i as Id, OrderStatus::Completed);
            child.fulfillment = Some(Fulfillment {
                id: 1,
                order_id: child.id,
                kind: "auto".into(),
                status: "delivered".into(),
                payload: (*text).into(),
                payload_line_count: 0,
                logistics: JsonMap::new(),
                delivered_by: None,
                delivered_at: None,
                created_at: t0(),
                updated_at: t0(),
            });
            parent.children.push(child);
        }
        assert_eq!(parent.collect_fulfillment_payload(), "A\nB\nC");
    }

    #[test]
    fn fills_items_from_children() {
        let mut parent = order(1, OrderStatus::Paid);
        let mut child = order(2, OrderStatus::Paid);
        child.items.push(item(5, "auto", 1));
        parent.children.push(child);
        parent.fill_items_from_children();
        assert_eq!(parent.items.len(), 1);
        assert_eq!(parent.items[0].order_id, 1);
        let json = serde_json::to_value(&parent).unwrap_or_default();
        assert!(json.get("guest_password").is_none());
        assert!(json.get("risk_ip").is_none());
        assert!(json.get("user_id").is_none());
        assert_eq!(json["status"], "paid");
        assert_eq!(json["items"][0]["coupon_discount_amount"], "0.00");
    }
}
