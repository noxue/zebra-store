//! Admin refunds (`order/application/refund`): refund to wallet, manual refund records,
//! fee-refunded corrections and the refund record lists.

use std::collections::HashMap;

use chrono::{FixedOffset, Offset, Utc};
use serde::Serialize;
use serde_json::Map;
use zs_domain::order::model::{Order, OrderItem, RefundRecord, keys};
use zs_domain::order::ports::{RefundDone, RefundFilter, RefundRequest};
use zs_domain::order::refund::{parse_refund_amount, plan_refund};
use zs_domain::payment::gateway::{GatewayRefundInput, GatewayRefundResult};
use zs_domain::payment::model::AdminPaymentFilter;
use zs_domain::payment::refund::{
    GatewayRefundAttempt, GatewayRefundAttemptUpdate, NewGatewayRefundAttempt,
    status as gateway_refund_status,
};
use zs_domain::payment::types::PaymentStatus;
use zs_domain::{Error, Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::OrderService;

/// Refund row of the admin lists (`AdminOrderRefundItem`).
#[derive(Debug, Clone, Serialize)]
pub struct AdminRefundItem {
    #[serde(flatten)]
    pub record: RefundRecord,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub order_no: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guest_locale: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<OrderItem>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user_display_name: String,
    pub refund_type_label: String,
}

#[derive(Debug, Clone)]
pub struct OriginalRefundResult {
    pub gateway: GatewayRefundResult,
    pub attempt: GatewayRefundAttempt,
    pub completed: Option<RefundDone>,
}

impl OrderService {
    /// Requests an original-route refund through the successful payment's provider. The local
    /// refund is applied only when the provider already reports a final success.
    pub async fn original_refund(
        &self,
        order_id: Id,
        raw_amount: &str,
        remark: &str,
        fee_refunded: bool,
        client_ip: &str,
    ) -> Result<OriginalRefundResult> {
        let amount = parse_refund_amount(raw_amount)?;
        let (payments, _) = self
            .deps
            .payment_records
            .list_admin(&AdminPaymentFilter {
                page: PageRequest {
                    page: 1,
                    page_size: 100,
                },
                order_id,
                status: PaymentStatus::Success.as_str().into(),
                skip_count: true,
                ..AdminPaymentFilter::default()
            })
            .await?;
        let payment = payments
            .into_iter()
            .find(|payment| payment.status == PaymentStatus::Success)
            .ok_or_else(|| Error::bad_request("error.payment_not_found"))?;
        let channel = self
            .deps
            .payment_channels
            .get(payment.channel_id)
            .await?
            .ok_or_else(|| Error::bad_request("error.payment_channel_not_found"))?;
        let gateway = self
            .deps
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .ok_or_else(|| Error::bad_request("error.payment_provider_not_supported"))?;
        let previous_attempts = self.deps.store.gateway_refund_attempts(order_id).await?;
        let previous_attempt = previous_attempts
            .into_iter()
            .find(|attempt| attempt.payment_id == payment.id);
        if let Some(attempt) = previous_attempt.as_ref()
            && attempt.status == gateway_refund_status::PENDING
            && (attempt.amount != amount
                || attempt.payment_fee_refunded != fee_refunded
                || attempt.remark != remark)
        {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if let Some(attempt) = previous_attempt.as_ref()
            && attempt.status == gateway_refund_status::SUCCEEDED
            && attempt.amount == amount
            && attempt.payment_fee_refunded == fee_refunded
            && attempt.remark == remark
        {
            let result = GatewayRefundResult {
                provider_ref: attempt.provider_ref.clone(),
                status: Some(PaymentStatus::Success),
                payload: attempt.payload.clone(),
            };
            let completed = Some(self.apply_gateway_refund(attempt, &result).await?);
            return Ok(OriginalRefundResult {
                gateway: result,
                attempt: attempt.clone(),
                completed,
            });
        }
        if previous_attempt
            .as_ref()
            .is_none_or(|attempt| attempt.status != gateway_refund_status::PENDING)
        {
            self.preflight_gateway_refund(order_id, amount).await?;
        }
        let now = self.deps.clock.now();
        let request_date = now
            .with_timezone(&FixedOffset::east_opt(8 * 3600).unwrap_or_else(|| Utc.fix()))
            .format("%Y%m%d")
            .to_string();
        let refund_no = format!("R{}{}", order_id, now.timestamp_millis());
        let reservation = self
            .deps
            .store
            .reserve_gateway_refund(&NewGatewayRefundAttempt {
                order_id,
                payment_id: payment.id,
                channel_id: payment.channel_id,
                request_date: request_date.clone(),
                refund_no: refund_no.clone(),
                provider_ref: format!("{request_date}:{refund_no}"),
                amount,
                remark: remark.to_owned(),
                payment_fee_refunded: fee_refunded,
                now,
            })
            .await?;
        let attempt = reservation.attempt;

        if attempt.status == gateway_refund_status::SUCCEEDED {
            let result = GatewayRefundResult {
                provider_ref: attempt.provider_ref.clone(),
                status: Some(PaymentStatus::Success),
                payload: attempt.payload.clone(),
            };
            let completed = Some(self.apply_gateway_refund(&attempt, &result).await?);
            return Ok(OriginalRefundResult {
                gateway: result,
                attempt,
                completed,
            });
        }

        let gateway_result = if reservation.created {
            gateway
                .refund_payment(
                    &channel.config_json,
                    &GatewayRefundInput {
                        provider_ref: payment.provider_ref,
                        request_date: attempt.request_date.clone(),
                        refund_no: attempt.refund_no.clone(),
                        amount: attempt.amount,
                        notify_url: String::new(),
                        remark: attempt.remark.clone(),
                        client_ip: client_ip.to_owned(),
                    },
                )
                .await
                .map_err(Error::from)?
        } else {
            if attempt.amount != amount
                || attempt.payment_fee_refunded != fee_refunded
                || attempt.remark != remark
            {
                return Err(Error::bad_request("error.gateway_refund_pending"));
            }
            gateway
                .query_refund(&channel.config_json, &attempt.provider_ref)
                .await
                .map_err(Error::from)?
        };

        let gateway_status = gateway_result.status.unwrap_or(PaymentStatus::Pending);
        let completed = if gateway_status == PaymentStatus::Success {
            Some(self.apply_gateway_refund(&attempt, &gateway_result).await?)
        } else {
            let status = if gateway_status == PaymentStatus::Failed {
                gateway_refund_status::FAILED
            } else {
                gateway_refund_status::PENDING
            };
            let updated = self
                .deps
                .store
                .update_gateway_refund_attempt(
                    attempt.id,
                    &GatewayRefundAttemptUpdate {
                        status: status.to_owned(),
                        payload: gateway_result.payload.clone(),
                        error: String::new(),
                        now: self.deps.clock.now(),
                    },
                )
                .await?;
            return Ok(OriginalRefundResult {
                gateway: gateway_result,
                attempt: updated,
                completed: None,
            });
        };
        let attempt = self
            .deps
            .store
            .gateway_refund_attempt(attempt.id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        Ok(OriginalRefundResult {
            gateway: gateway_result,
            attempt,
            completed,
        })
    }

    async fn preflight_gateway_refund(
        &self,
        order_id: Id,
        amount: zs_shared::money::Amount,
    ) -> Result<()> {
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        let setting = self.order_setting().await;
        plan_refund(
            &order,
            amount,
            setting.max_refund_days,
            self.deps.clock.now(),
        )?;
        if let Some(parent_id) = order.parent_id
            && let Some(parent) = self.deps.repo.get(parent_id).await?
            && amount > parent.total_amount - parent.refunded_amount
        {
            return Err(zs_domain::order::refund::refund_exceeded());
        }
        Ok(())
    }

    async fn apply_gateway_refund(
        &self,
        attempt: &GatewayRefundAttempt,
        result: &GatewayRefundResult,
    ) -> Result<RefundDone> {
        self.apply_refund(
            attempt.order_id,
            &attempt.amount.to_string(),
            &attempt.remark,
            false,
            attempt.payment_fee_refunded,
            Some((
                attempt.id,
                result.provider_ref.clone(),
                result.payload.clone(),
            )),
        )
        .await
    }

    async fn apply_refund(
        &self,
        order_id: Id,
        raw_amount: &str,
        remark: &str,
        to_wallet: bool,
        fee: bool,
        gateway_attempt: Option<(Id, String, Map<String, serde_json::Value>)>,
    ) -> Result<RefundDone> {
        let amount = parse_refund_amount(raw_amount)?;
        if order_id <= 0 {
            return Err(Error::not_found(keys::ORDER_NOT_FOUND));
        }
        let setting = self.order_setting().await;
        let done = self
            .deps
            .store
            .refund(&RefundRequest {
                order_id,
                amount,
                remark: remark.to_owned(),
                to_wallet,
                payment_fee_refunded: fee,
                gateway_refund_attempt_id: gateway_attempt.as_ref().map(|attempt| attempt.0),
                gateway_refund_provider_ref: gateway_attempt
                    .as_ref()
                    .map_or_else(String::new, |attempt| attempt.1.clone()),
                gateway_refund_payload: gateway_attempt.map_or_else(Map::new, |attempt| attempt.2),
                max_refund_days: setting.max_refund_days,
                reseller_confirm_days: self.deps.reseller_confirm_days,
                now: self.deps.clock.now(),
            })
            .await
            .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))?;
        self.after_refund(&done).await;
        Ok(done)
    }

    /// Refund status email on the parent order (`enqueueOrderRefundStatusEmail`) and the
    /// downstream buyer notification.
    pub(super) async fn after_refund(&self, done: &RefundDone) {
        let target = match done.order.parent_id {
            Some(parent) => self
                .deps
                .repo
                .get(parent)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| done.order.clone()),
            None => done.order.clone(),
        };
        self.enqueue_status_email(target.id, target.status, Some(done.record.id))
            .await;
        if let Some(integration) = &self.deps.integration
            && let Err(error) = integration.order_status_changed(done.order.id).await
        {
            tracing::warn!(%error, order_id = done.order.id, "refund_downstream_callback_failed");
        }
    }

    /// `POST /admin/orders/:id/refund-to-wallet` (RFD-02/03/04).
    pub async fn refund_to_wallet(
        &self,
        order_id: Id,
        amount: &str,
        remark: &str,
    ) -> Result<RefundDone> {
        self.apply_refund(order_id, amount, remark, true, false, None)
            .await
    }

    /// `POST /admin/orders/:id/manual-refund` (RFD-01/02/04).
    pub async fn manual_refund(
        &self,
        order_id: Id,
        amount: &str,
        remark: &str,
        fee_refunded: bool,
    ) -> Result<RefundDone> {
        self.apply_refund(order_id, amount, remark, false, fee_refunded, None)
            .await
    }

    /// `PATCH /admin/order-refunds/:id/payment-fee` (RFD-01): returns the full admin row.
    pub async fn set_refund_fee(&self, record_id: Id, refunded: bool) -> Result<AdminRefundItem> {
        if record_id <= 0 {
            return Err(Error::not_found(keys::ORDER_NOT_FOUND));
        }
        self.deps
            .store
            .set_refund_fee_flag(record_id, refunded, self.deps.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::ORDER_UPDATE_FAILED))?;
        self.admin_refund(record_id).await
    }

    async fn refund_items(&self, records: Vec<RefundRecord>) -> Result<Vec<AdminRefundItem>> {
        let mut orders: HashMap<Id, Option<Order>> = HashMap::new();
        for r in &records {
            if let std::collections::hash_map::Entry::Vacant(slot) = orders.entry(r.order_id) {
                slot.insert(self.deps.repo.get(r.order_id).await?);
            }
        }
        let guest_of = |r: &RefundRecord, o: Option<&Order>| {
            if !r.guest_email.trim().is_empty() {
                r.guest_email.trim().to_owned()
            } else {
                o.map(|o| o.guest_email.trim().to_owned())
                    .unwrap_or_default()
            }
        };
        let user_ids: Vec<Id> = records
            .iter()
            .filter_map(|r| {
                let o = orders.get(&r.order_id).and_then(Option::as_ref);
                if !guest_of(r, o).is_empty() {
                    return None;
                }
                let id = if r.user_id > 0 {
                    r.user_id
                } else {
                    o.map_or(0, |o| o.user_id)
                };
                (id > 0).then_some(id)
            })
            .collect();
        let users = self.deps.repo.users(&user_ids).await?;
        Ok(records
            .into_iter()
            .map(|mut record| {
                let order = orders.get(&record.order_id).and_then(Option::as_ref);
                let guest = guest_of(&record, order);
                record.guest_email.clone_from(&guest);
                let user_id = if record.user_id > 0 {
                    record.user_id
                } else {
                    order.map_or(0, |o| o.user_id)
                };
                let user = if guest.is_empty() {
                    users.get(&user_id)
                } else {
                    None
                };
                let items = order
                    .map(|o| o.all_items().into_iter().cloned().collect())
                    .unwrap_or_default();
                AdminRefundItem {
                    order_no: order
                        .map(|o| o.order_no.trim().to_owned())
                        .unwrap_or_default(),
                    guest_locale: order
                        .map(|o| o.guest_locale.trim().to_owned())
                        .unwrap_or_default(),
                    items,
                    user_email: user.map(|u| u.email.trim().to_owned()).unwrap_or_default(),
                    user_display_name: user
                        .map(|u| u.display_name.trim().to_owned())
                        .unwrap_or_default(),
                    refund_type_label: record.kind.clone(),
                    record,
                }
            })
            .collect())
    }

    /// `GET /admin/order-refunds`.
    pub async fn admin_refunds(&self, filter: &RefundFilter) -> Result<Page<AdminRefundItem>> {
        let page = self
            .deps
            .repo
            .list_refunds(filter)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?;
        Ok(Page {
            items: self.refund_items(page.items).await?,
            total: page.total,
        })
    }

    /// `GET /admin/order-refunds/:id`.
    pub async fn admin_refund(&self, id: Id) -> Result<AdminRefundItem> {
        let record = self
            .deps
            .repo
            .refund(id)
            .await
            .map_err(|e| e.or_internal(keys::ORDER_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        self.refund_items(vec![record])
            .await?
            .pop()
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))
    }
}
