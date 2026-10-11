//! Human confirmation of merchant-owned collection-code payments.

use serde::Serialize;
use serde_json::{Map, Value};
use subtle::ConstantTimeEq;
use zs_domain::order::model::{OrderStatus, keys as order_keys};
use zs_domain::payment::errors::keys as payment_keys;
use zs_domain::payment::gateway::ManualIssueNotice;
use zs_domain::payment::model::Payment;
use zs_domain::payment::types::{PaymentStatus, provider};
use zs_domain::{Error, Id, Result};
use zs_shared::crypto::sha256_hex;
use zs_shared::money::Amount;

use super::OrderService;

#[derive(Debug, Clone, Serialize)]
pub struct ManualPaymentItem {
    pub title: Map<String, Value>,
    pub quantity: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManualPaymentReview {
    pub payment_id: Id,
    pub order_no: String,
    pub channel_name: String,
    pub channel_type: String,
    pub amount: Amount,
    pub currency: String,
    pub payment_status: PaymentStatus,
    pub order_status: OrderStatus,
    pub items: Vec<ManualPaymentItem>,
    pub can_confirm: bool,
    pub requires_force: bool,
    pub buyer_report: Option<Value>,
}

impl OrderService {
    /// A buyer may ask the channel's reviewers to reconcile an unresolved payment.
    /// This only sends a notice; it never changes payment or order status.
    pub async fn report_manual_payment_issue(
        &self,
        payment: Payment,
        payer_name: &str,
        issue: &str,
        note: &str,
        review_origin: &str,
    ) -> Result<()> {
        if payment.provider_type != provider::MANUAL_QR || payment.order_id <= 0 {
            return Err(Error::bad_request(payment_keys::PAYMENT_INVALID));
        }
        let name = payer_name.split_whitespace().collect::<Vec<_>>().join(" ");
        let note = note.split_whitespace().collect::<Vec<_>>().join(" ");
        if name.chars().count() < 2
            || name.chars().count() > 80
            || note.chars().count() > 500
            || name.chars().any(char::is_control)
            || note.chars().any(char::is_control)
        {
            return Err(Error::bad_request("error.invalid_params"));
        }
        let issue_label = match issue {
            "not_received" => "付款后长时间未确认到账",
            "wrong_amount" => "支付金额与订单不符",
            _ => return Err(Error::bad_request("error.invalid_params")),
        };
        let order = self
            .deps
            .repo
            .get(payment.order_id)
            .await?
            .ok_or_else(|| Error::not_found(order_keys::ORDER_NOT_FOUND))?;
        if payment.status == PaymentStatus::Success
            || !matches!(
                order.status,
                OrderStatus::PendingPayment | OrderStatus::Canceled
            )
        {
            return Err(Error::bad_request(order_keys::ORDER_STATUS_INVALID));
        }
        let channel = self
            .deps
            .repo
            .archived_channel(payment.channel_id)
            .await?
            .ok_or_else(|| Error::not_found(payment_keys::CHANNEL_NOT_FOUND))?;
        let token = payment
            .provider_payload
            .get("manual_link_token")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("mql_"))
            .ok_or_else(|| Error::bad_request(payment_keys::PAYMENT_INVALID))?;
        let gateway = self
            .deps
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .ok_or_else(|| Error::bad_request(payment_keys::PROVIDER_NOT_SUPPORTED))?;
        let fallback_base = format!("{}/admin", review_origin.trim_end_matches('/'));
        let base = channel
            .config_json
            .get("review_base_url")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .unwrap_or(&fallback_base);
        let review_url = format!(
            "{}/manual-confirm/{}#key={token}",
            base.trim_end_matches('/'),
            payment.id
        );
        let rate_key = payment.id.to_string();
        self.manual_issue_limiter
            .hit(&rate_key)
            .map_err(|wait| Error::too_many("error.rate_limited").arg(wait))?;
        if let Err(error) = self
            .deps
            .payments
            .record_manual_issue(payment.id, &name, issue, &note, self.deps.clock.now())
            .await
        {
            self.manual_issue_limiter.reset(&rate_key);
            return Err(error);
        }
        if let Err(error) = gateway
            .notify_manual_issue(
                &channel.config_json,
                &ManualIssueNotice {
                    order_no: &order.order_no,
                    amount: payment.amount,
                    currency: &payment.currency,
                    payer_name: &name,
                    issue: issue_label,
                    note: if note.is_empty() { "未填写" } else { &note },
                    review_url: &review_url,
                },
            )
            .await
        {
            self.manual_issue_limiter.reset(&rate_key);
            tracing::error!(%error, payment_id = payment.id, "manual_payment_issue_notification_failed");
            return Err(Error::from(error).or_internal(payment_keys::PAYMENT_CALLBACK_FAILED));
        }
        Ok(())
    }

    /// A channel key authorizes both this review and the confirmation call.
    pub async fn manual_payment_review(
        &self,
        payment_id: Id,
        key: &str,
    ) -> Result<ManualPaymentReview> {
        let payment = self
            .deps
            .repo
            .payment(payment_id)
            .await?
            .ok_or_else(|| Error::not_found(payment_keys::PAYMENT_NOT_FOUND))?;
        if payment.provider_type != provider::MANUAL_QR || payment.order_id <= 0 {
            return Err(Error::bad_request(payment_keys::PAYMENT_INVALID));
        }
        let channel = self
            .deps
            .repo
            .archived_channel(payment.channel_id)
            .await?
            .ok_or_else(|| Error::not_found(payment_keys::CHANNEL_NOT_FOUND))?;
        let stored = channel
            .config_json
            .get("confirmation_api_key_hash")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let actual = sha256_hex(key.as_bytes());
        let channel_key_ok = key.len() >= 32
            && key.len() <= 128
            && stored.len() == actual.len()
            && bool::from(stored.as_bytes().ct_eq(actual.as_bytes()));
        let link = payment
            .provider_payload
            .get("manual_link_token")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let link_ok = key.starts_with("mql_")
            && key.len() == 68
            && link.len() == key.len()
            && bool::from(link.as_bytes().ct_eq(key.as_bytes()));
        if !channel_key_ok && !link_ok {
            return Err(Error::unauthorized("error.unauthorized"));
        }
        let order = self
            .deps
            .repo
            .get(payment.order_id)
            .await?
            .ok_or_else(|| Error::not_found(order_keys::ORDER_NOT_FOUND))?;
        let can_confirm = payment.status != PaymentStatus::Success
            && matches!(
                order.status,
                OrderStatus::PendingPayment | OrderStatus::Canceled
            );
        Ok(ManualPaymentReview {
            payment_id,
            order_no: order.order_no.clone(),
            channel_name: channel.name,
            channel_type: payment.channel_type,
            amount: payment.amount,
            currency: payment.currency,
            payment_status: payment.status,
            order_status: order.status,
            items: order
                .all_items()
                .iter()
                .map(|item| ManualPaymentItem {
                    title: item.title.clone(),
                    quantity: item.quantity,
                })
                .collect(),
            can_confirm,
            requires_force: order.status == OrderStatus::Canceled,
            buyer_report: payment.provider_payload.get("manual_buyer_report").cloned(),
        })
    }

    /// The received amount is always supplied by the human or monitoring program;
    /// it must equal the snapshotted payment amount exactly.
    pub async fn manual_payment_confirm(
        &self,
        payment_id: Id,
        key: &str,
        received: Amount,
        force: bool,
        note: &str,
    ) -> Result<ManualPaymentReview> {
        let review = self.manual_payment_review(payment_id, key).await?;
        if !review.can_confirm {
            return Err(Error::bad_request(order_keys::ORDER_STATUS_INVALID));
        }
        let settled = self
            .deps
            .payments
            .manual_confirm(
                payment_id,
                received,
                force,
                note,
                self.deps.reseller_confirm_days,
                self.deps.clock.now(),
            )
            .await?;
        if settled.order_paid
            && let Some(order) = &settled.order
        {
            self.after_order_paid(order, Some(&settled.payment)).await;
        }
        self.manual_payment_review(payment_id, key).await
    }
}
