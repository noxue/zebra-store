//! Payment records: model, persistence port and admin projections.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use zs_shared::money::Amount;
use zs_shared::page::PageRequest;

use super::types::PaymentStatus;
use crate::{Id, Result};

/// A payment attempt (JSON shape of the original `domain.Payment`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Payment {
    pub id: Id,
    /// Order id; `0` for wallet recharges (linked through `wallet_recharge_orders.payment_id`).
    pub order_id: Id,
    pub channel_id: Id,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    /// Amount actually requested from the gateway (after fee policy and currency conversion).
    pub amount: Amount,
    pub fee_rate: Amount,
    pub fixed_fee: Amount,
    pub fee_amount: Amount,
    pub fee_policy: String,
    pub currency: String,
    pub status: PaymentStatus,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub exception_code: String,
    pub provider_ref: String,
    pub gateway_order_no: String,
    pub provider_payload: Map<String, Value>,
    pub pay_url: String,
    pub qr_code: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub paid_at: Option<DateTime<Utc>>,
    pub expired_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by_payment_id: Option<Id>,
    pub callback_at: Option<DateTime<Utc>>,
}

impl Payment {
    /// Display channel type: the payload snapshot written at creation (PAY-44).
    pub fn display_channel_type(&self) -> String {
        match self.provider_payload.get("display_channel_type") {
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(Value::Null) | None => String::new(),
            Some(other) => other.to_string(),
        }
    }
}

/// Admin payment list filter (`ListFilter`).
#[derive(Debug, Clone, Default)]
pub struct AdminPaymentFilter {
    pub page: PageRequest,
    pub user_id: Id,
    pub order_id: Id,
    pub channel_id: Id,
    pub provider_type: String,
    pub channel_type: String,
    pub status: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    /// Skip the total count (exports).
    pub skip_count: bool,
}

/// Wallet recharge linked to a payment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RechargeRef {
    pub recharge_no: String,
    pub status: String,
    pub user_id: Id,
}

/// Cross-module references shown next to admin payments.
#[derive(Debug, Clone, Default)]
pub struct PaymentRefs {
    /// order id → order number
    pub order_nos: HashMap<Id, String>,
    /// payment id → recharge
    pub recharges: HashMap<Id, RechargeRef>,
}

/// Persistence port for payments (reads; callback writes go through
/// [`super::settlement::PaymentSettlement`]).
#[async_trait]
pub trait PaymentRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<Payment>>;
    /// Latest (highest id) payment with this gateway order number.
    async fn find_by_gateway_order_no(&self, gateway_order_no: &str) -> Result<Option<Payment>>;
    /// Latest (highest id) payment with this provider reference.
    async fn find_latest_by_provider_ref(&self, provider_ref: &str) -> Result<Option<Payment>>;
    /// Ordered by id desc; the total is `0` when `skip_count` is set.
    async fn list_admin(&self, filter: &AdminPaymentFilter) -> Result<(Vec<Payment>, u64)>;
    /// Order numbers and recharge references of the given payments.
    async fn refs(&self, payments: &[Payment]) -> Result<PaymentRefs>;
    /// Business number the payment belongs to: the order number, or the recharge number
    /// for wallet recharges (`order_id = 0`); `None` when the business record is missing.
    async fn business_no(&self, payment: &Payment) -> Result<Option<String>>;
}

/// An admin payment row (`AdminPaymentItem`): payment with gateway secrets/links removed.
#[derive(Debug, Clone, Serialize)]
pub struct AdminPayment {
    #[serde(flatten)]
    pub payment: Payment,
    pub channel_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_channel_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub order_no: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub recharge_no: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub recharge_status: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub recharge_user_id: Id,
}

fn is_zero(v: &Id) -> bool {
    *v == 0
}

/// Admin projection: provider payload, pay URL and QR code are never exposed (`redactAdminPayment`).
pub fn admin_payment(payment: &Payment, channel_name: &str, refs: &PaymentRefs) -> AdminPayment {
    let recharge = refs.recharges.get(&payment.id).cloned().unwrap_or_default();
    AdminPayment {
        display_channel_type: payment.display_channel_type(),
        payment: Payment {
            provider_payload: Map::new(),
            pay_url: String::new(),
            qr_code: String::new(),
            ..payment.clone()
        },
        channel_name: channel_name.to_owned(),
        order_no: refs
            .order_nos
            .get(&payment.order_id)
            .cloned()
            .unwrap_or_default(),
        recharge_no: recharge.recharge_no,
        recharge_status: recharge.status,
        recharge_user_id: recharge.user_id,
    }
}

/// CSV header of `GET /admin/payments/export` (PAY-44 column order).
pub const EXPORT_HEADER: [&str; 16] = [
    "id",
    "order_id",
    "recharge_no",
    "recharge_status",
    "recharge_user_id",
    "channel_id",
    "provider_type",
    "channel_type",
    "display_channel_type",
    "status",
    "amount",
    "currency",
    "created_at",
    "paid_at",
    "expired_at",
    "provider_ref",
];

fn rfc3339(t: Option<DateTime<Utc>>) -> String {
    t.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

/// One CSV row of the export.
pub fn export_row(payment: &Payment, refs: &PaymentRefs) -> [String; 16] {
    let recharge = refs.recharges.get(&payment.id).cloned().unwrap_or_default();
    [
        payment.id.to_string(),
        payment.order_id.to_string(),
        recharge.recharge_no,
        recharge.status,
        recharge.user_id.to_string(),
        payment.channel_id.to_string(),
        payment.provider_type.clone(),
        payment.channel_type.clone(),
        payment.display_channel_type(),
        payment.status.as_str().to_owned(),
        payment.amount.to_string(),
        payment.currency.clone(),
        rfc3339(Some(payment.created_at)),
        rfc3339(payment.paid_at),
        rfc3339(payment.expired_at),
        payment.provider_ref.clone(),
    ]
}

/// Encodes one CSV record like Go's `encoding/csv` (quotes only when needed).
pub fn csv_record(fields: &[String]) -> String {
    let mut out = String::new();
    for (i, field) in fields.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let needs_quotes = field == r"\."
            || field.contains([',', '"', '\n', '\r'])
            || field.chars().next().is_some_and(char::is_whitespace);
        if needs_quotes {
            out.push('"');
            out.push_str(&field.replace('"', "\"\""));
            out.push('"');
        } else {
            out.push_str(field);
        }
    }
    out.push('\n');
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use serde_json::json;

    pub(crate) fn sample() -> Payment {
        let t = DateTime::parse_from_rfc3339("2026-09-24T10:00:00Z")
            .map(|t| t.with_timezone(&Utc))
            .unwrap_or_default();
        Payment {
            id: 7,
            order_id: 3,
            channel_id: 2,
            provider_type: "bepusdt".into(),
            channel_type: "bepusdt".into(),
            interaction_mode: "redirect".into(),
            amount: Amount::from_cents(1000),
            fee_rate: Amount::ZERO,
            fixed_fee: Amount::ZERO,
            fee_amount: Amount::ZERO,
            fee_policy: "none".into(),
            currency: "CNY".into(),
            status: PaymentStatus::Pending,
            exception_code: String::new(),
            provider_ref: "T1".into(),
            gateway_order_no: "DJP1".into(),
            provider_payload: json!({"display_channel_type": "usdt.arbitrum", "secret": 1})
                .as_object()
                .cloned()
                .unwrap_or_default(),
            pay_url: "https://pay".into(),
            qr_code: "qr".into(),
            created_at: t,
            updated_at: t,
            paid_at: None,
            expired_at: None,
            superseded_at: None,
            superseded_by_payment_id: None,
            callback_at: None,
        }
    }

    /// PAY-45: admin rows never expose provider payload or payment links.
    #[test]
    fn pay_45_admin_payment_redacts() {
        let refs = PaymentRefs::default();
        let item = serde_json::to_value(admin_payment(&sample(), "Ch", &refs)).unwrap_or_default();
        assert_eq!(item["provider_payload"], json!({}));
        assert_eq!(item["pay_url"], "");
        assert_eq!(item["qr_code"], "");
        assert_eq!(item["channel_name"], "Ch");
        assert_eq!(item["display_channel_type"], "usdt.arbitrum");
        assert_eq!(item["amount"], "10.00");
        assert!(item.get("exception_code").is_none());
        assert!(item.get("recharge_no").is_none());
    }

    /// PAY-44: CSV column 9 is the display channel type.
    #[test]
    fn pay_44_export_row() {
        let row = export_row(&sample(), &PaymentRefs::default());
        assert_eq!(row[8], "usdt.arbitrum");
        assert_eq!(row[10], "10.00");
        assert_eq!(row[12], "2026-09-24T10:00:00Z");
        assert_eq!(
            csv_record(&["a,b".into(), "x\"y".into(), String::new()]),
            "\"a,b\",\"x\"\"y\",\n"
        );
    }
}
