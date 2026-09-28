//! Storefront DTOs (`transport/presenter/order.go`): white-listed fields only — no cost
//! price, internal ids, guest credential or risk IP (MISC-02); upstream delivery is shown as
//! manual (UPS-19); delivery instructions only after payment (DLV-02).

use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_shared::money::Amount;

use super::model::{
    Fulfillment, JsonMap, Order, OrderItem, OrderStatus, RefundRecord, fulfillment_type,
};
use super::pricing::PricingResult;
use crate::Id;

fn mask_upstream(ft: &str) -> String {
    if ft == fulfillment_type::UPSTREAM {
        fulfillment_type::MANUAL.to_owned()
    } else {
        ft.to_owned()
    }
}

fn non_empty(map: &JsonMap) -> Option<JsonMap> {
    (!map.is_empty()).then(|| map.clone())
}

/// An order item as seen by the buyer (`OrderItemResp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderItemView {
    pub title: JsonMap,
    pub sku_snapshot: JsonMap,
    pub tags: Vec<String>,
    pub quantity: i32,
    pub original_unit_price: Amount,
    pub unit_price: Amount,
    pub original_total_price: Amount,
    pub total_price: Amount,
    pub coupon_discount_amount: Amount,
    pub member_discount_amount: Amount,
    pub promotion_discount_amount: Amount,
    pub wholesale_discount_amount: Amount,
    pub fulfillment_type: String,
    pub manual_form_schema_snapshot: JsonMap,
    pub manual_form_submission: JsonMap,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instructions: Option<JsonMap>,
}

impl OrderItemView {
    pub fn new(item: &OrderItem, show_instructions: bool) -> Self {
        Self {
            title: item.title.clone(),
            sku_snapshot: item.sku_snapshot.clone(),
            tags: item.tags.clone(),
            quantity: item.quantity,
            original_unit_price: item.original_unit_price,
            unit_price: item.unit_price,
            original_total_price: item.original_total_price,
            total_price: item.total_price,
            coupon_discount_amount: item.coupon_discount,
            member_discount_amount: item.member_discount,
            promotion_discount_amount: item.promotion_discount,
            wholesale_discount_amount: item.wholesale_discount,
            fulfillment_type: mask_upstream(&item.fulfillment_type),
            manual_form_schema_snapshot: item.manual_form_schema.clone(),
            manual_form_submission: item.manual_form_submission.clone(),
            instructions: if show_instructions {
                non_empty(&item.instructions)
            } else {
                None
            },
        }
    }
}

/// Delivered content (`FulfillmentResp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FulfillmentView {
    #[serde(rename = "type")]
    pub kind: String,
    pub status: String,
    pub payload: String,
    pub payload_line_count: usize,
    pub delivery_data: JsonMap,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivered_at: Option<DateTime<Utc>>,
}

impl From<&Fulfillment> for FulfillmentView {
    fn from(f: &Fulfillment) -> Self {
        Self {
            kind: mask_upstream(&f.kind),
            status: f.status.clone(),
            payload: f.payload.clone(),
            payload_line_count: f.payload_line_count,
            delivery_data: f.logistics.clone(),
            delivered_at: f.delivered_at,
        }
    }
}

/// A refund as seen by the buyer (`OrderRefundResp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RefundView {
    #[serde(rename = "type")]
    pub kind: String,
    pub amount: Amount,
    pub currency: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub remark: String,
    pub created_at: DateTime<Utc>,
}

impl From<&RefundRecord> for RefundView {
    fn from(r: &RefundRecord) -> Self {
        Self {
            kind: r.kind.trim().to_owned(),
            amount: r.amount,
            currency: r.currency.trim().to_owned(),
            remark: r.remark.trim().to_owned(),
            created_at: r.created_at,
        }
    }
}

/// List row (`OrderSummary`): never carries instructions.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderSummary {
    pub order_no: String,
    pub status: OrderStatus,
    pub currency: String,
    pub discount_amount: Amount,
    pub member_discount_amount: Amount,
    pub promotion_discount_amount: Amount,
    pub wholesale_discount_amount: Amount,
    pub total_amount: Amount,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<OrderItemView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OrderSummary>,
}

impl From<&Order> for OrderSummary {
    fn from(o: &Order) -> Self {
        Self {
            order_no: o.order_no.clone(),
            status: o.status,
            currency: o.currency.clone(),
            discount_amount: o.discount_amount,
            member_discount_amount: o.member_discount_amount,
            promotion_discount_amount: o.promotion_discount_amount,
            wholesale_discount_amount: o.wholesale_discount_amount,
            total_amount: o.total_amount,
            created_at: o.created_at,
            items: o
                .items
                .iter()
                .map(|i| OrderItemView::new(i, false))
                .collect(),
            children: o.children.iter().map(Self::from).collect(),
        }
    }
}

/// Detail view (`OrderDetail`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderDetail {
    pub order_no: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guest_email: String,
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
    pub expires_at: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub canceled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// `None` = unrestricted; `Some([])` = no online channel can pay the order (PAY-11).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_payment_channel_ids: Option<Vec<Id>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub refund_records: Vec<RefundView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<OrderItemView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfillment: Option<FulfillmentView>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<OrderDetail>,
}

impl OrderDetail {
    /// Full detail; with `truncate` the delivered content keeps 100 lines (DLV-05).
    pub fn new(o: &Order, truncate: bool) -> Self {
        let paid = o.paid_at.is_some();
        let fulfillment = o.fulfillment.as_ref().map(|f| {
            let mut f = f.clone();
            if truncate {
                f.truncate_payload(super::model::PAYLOAD_MAX_PREVIEW_LINES);
            }
            FulfillmentView::from(&f)
        });
        Self {
            order_no: o.order_no.clone(),
            guest_email: o.guest_email.clone(),
            guest_locale: o.guest_locale.clone(),
            status: o.status,
            currency: o.currency.clone(),
            original_amount: o.original_amount,
            discount_amount: o.discount_amount,
            member_discount_amount: o.member_discount_amount,
            promotion_discount_amount: o.promotion_discount_amount,
            wholesale_discount_amount: o.wholesale_discount_amount,
            total_amount: o.total_amount,
            wallet_paid_amount: o.wallet_paid_amount,
            online_paid_amount: o.online_paid_amount,
            refunded_amount: o.refunded_amount,
            expires_at: o.expires_at,
            paid_at: o.paid_at,
            canceled_at: o.canceled_at,
            created_at: o.created_at,
            allowed_payment_channel_ids: None,
            refund_records: Vec::new(),
            items: o
                .items
                .iter()
                .map(|i| OrderItemView::new(i, paid))
                .collect(),
            fulfillment,
            children: o.children.iter().map(|c| Self::new(c, truncate)).collect(),
        }
    }
}

/// Price preview line (`OrderPreviewItem`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreviewItem {
    pub product_id: Id,
    pub sku_id: Id,
    pub title: JsonMap,
    pub sku_snapshot: JsonMap,
    pub tags: Vec<String>,
    pub original_unit_price: Amount,
    pub unit_price: Amount,
    pub quantity: i32,
    pub original_total_price: Amount,
    pub total_price: Amount,
    pub member_discount_amount: Amount,
    pub coupon_discount_amount: Amount,
    pub promotion_discount_amount: Amount,
    pub wholesale_discount_amount: Amount,
    pub fulfillment_type: String,
}

/// Price preview (`OrderPreview`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Preview {
    pub currency: String,
    pub original_amount: Amount,
    pub member_discount_amount: Amount,
    pub discount_amount: Amount,
    pub promotion_discount_amount: Amount,
    pub wholesale_discount_amount: Amount,
    pub total_amount: Amount,
    pub items: Vec<PreviewItem>,
}

/// Display data of a priced line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LineDisplay {
    pub title: JsonMap,
    pub sku_snapshot: JsonMap,
    pub tags: Vec<String>,
    pub fulfillment_type: String,
}

impl Preview {
    /// Builds the preview; `display` is aligned with `result.lines`.
    pub fn new(currency: &str, result: &PricingResult, display: &[LineDisplay]) -> Self {
        let items = result
            .lines
            .iter()
            .zip(display)
            .map(|(l, d)| PreviewItem {
                product_id: l.product_id,
                sku_id: l.sku_id,
                title: d.title.clone(),
                sku_snapshot: d.sku_snapshot.clone(),
                tags: d.tags.clone(),
                original_unit_price: l.original_unit_price,
                unit_price: l.unit_price,
                quantity: l.quantity,
                original_total_price: l.original_total_price,
                total_price: l.total_price,
                member_discount_amount: l.member_discount,
                coupon_discount_amount: l.coupon_discount,
                promotion_discount_amount: l.promotion_discount,
                wholesale_discount_amount: l.wholesale_discount,
                fulfillment_type: mask_upstream(&d.fulfillment_type),
            })
            .collect();
        Self {
            currency: currency.to_owned(),
            original_amount: result.original_amount,
            member_discount_amount: result.member_discount_amount,
            discount_amount: result.discount_amount,
            promotion_discount_amount: result.promotion_discount_amount,
            wholesale_discount_amount: result.wholesale_discount_amount,
            total_amount: result.total_amount,
            items,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::testkit::{item, order};
    use super::*;
    use serde_json::json;

    /// DLV-02: instructions only after payment and never in lists; MISC-02: no cost price.
    #[test]
    fn dlv_02_instructions_visibility() {
        let mut o = order(1, OrderStatus::PendingPayment);
        let mut it = item(3, "upstream", 1);
        it.instructions = json!({"zh-CN": "use it"})
            .as_object()
            .cloned()
            .unwrap_or_default();
        it.cost_price = Amount::from(7);
        o.items.push(it);
        let unpaid = serde_json::to_value(OrderDetail::new(&o, true)).unwrap_or_default();
        assert!(unpaid["items"][0].get("instructions").is_none());
        assert!(unpaid["items"][0].get("cost_price").is_none());
        assert_eq!(unpaid["items"][0]["fulfillment_type"], "manual");
        o.paid_at = Some(o.created_at);
        let paid = serde_json::to_value(OrderDetail::new(&o, true)).unwrap_or_default();
        assert_eq!(paid["items"][0]["instructions"]["zh-CN"], "use it");
        let list = serde_json::to_value(OrderSummary::from(&o)).unwrap_or_default();
        assert!(list["items"][0].get("instructions").is_none());
        assert!(list.get("guest_password").is_none());
    }

    #[test]
    fn detail_shape() {
        let mut d = OrderDetail::new(&order(1, OrderStatus::Paid), false);
        let v = serde_json::to_value(&d).unwrap_or_default();
        assert!(v.get("allowed_payment_channel_ids").is_none());
        assert_eq!(v["total_amount"], "100.00");
        assert!(v["paid_at"].is_null());
        d.allowed_payment_channel_ids = Some(vec![]);
        let v = serde_json::to_value(&d).unwrap_or_default();
        assert_eq!(v["allowed_payment_channel_ids"], json!([]));
    }
}
