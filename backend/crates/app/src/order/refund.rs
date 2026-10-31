//! Admin refunds (`order/application/refund`): refund to wallet, manual refund records,
//! fee-refunded corrections and the refund record lists.

use std::collections::HashMap;

use chrono::{FixedOffset, Offset, Utc};
use serde::Serialize;
use serde_json::Map;
use zs_domain::order::model::{Order, OrderItem, RefundRecord, keys};
use zs_domain::order::ports::{RefundDone, RefundFilter, RefundRequest};
use zs_domain::order::refund::{parse_refund_amount, plan_refund};
use zs_domain::payment::gateway::{
    GatewayRefundInput, GatewayRefundResult, supports_refund_amount,
};
use zs_domain::payment::model::AdminPaymentFilter;
use zs_domain::payment::refund::{
    AdminRefundOptions, GatewayRefundAttempt, GatewayRefundAttemptUpdate, NewGatewayRefundAttempt,
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

/// Twice the payment transport's 15-second timeout; an in-flight submit must not
/// be interpreted as a failed refund by a concurrent query.
const REFUND_SUBMISSION_GRACE_SECONDS: i64 = 30;

impl OrderService {
    pub async fn latest_refund_attempt(
        &self,
        order_id: Id,
    ) -> Result<Option<GatewayRefundAttempt>> {
        Ok(self
            .deps
            .store
            .gateway_refund_attempts(order_id)
            .await?
            .into_iter()
            .next())
    }
    pub async fn public_refund_state(&self, order_id: Id) -> Result<&'static str> {
        let attempt = self
            .deps
            .store
            .gateway_refund_attempts(order_id)
            .await?
            .into_iter()
            .next();
        Ok(
            match attempt.as_ref().map(|attempt| attempt.status.as_str()) {
                Some(gateway_refund_status::PREPARING) => "checking",
                Some(gateway_refund_status::PENDING) => "processing",
                Some(gateway_refund_status::MANUAL_PREPARING) => "checking",
                Some(gateway_refund_status::MANUAL_LOCKING) => "checking",
                Some(gateway_refund_status::MANUAL_AWAITING) => "awaiting_support",
                Some(gateway_refund_status::MANUAL_CANCELING) => "checking",
                Some(gateway_refund_status::MANUAL_CANCELED) => "none",
                Some(gateway_refund_status::SUCCEEDED)
                    if attempt.as_ref().is_some_and(|a| a.card_confirmed) =>
                {
                    "succeeded"
                }
                Some(gateway_refund_status::SUCCEEDED) => "syncing",
                Some(gateway_refund_status::FAILED) => "needs_review",
                _ => "none",
            },
        )
    }

    /// Reconciles durable provider attempts after process restarts or transient API failures.
    pub async fn reconcile_gateway_refunds(&self) -> Result<()> {
        for attempt in self.deps.store.unfinished_gateway_refund_attempts().await? {
            let result = if matches!(
                attempt.status.as_str(),
                gateway_refund_status::MANUAL_PREPARING | gateway_refund_status::MANUAL_LOCKING
            ) {
                self.prepare_manual_refund(
                    attempt.order_id,
                    &attempt.amount.to_string(),
                    &attempt.remark,
                    attempt.payment_fee_refunded,
                )
                .await
                .map(|_| ())
            } else if attempt.status == gateway_refund_status::MANUAL_CANCELING {
                self.cancel_manual_refund(attempt.order_id, attempt.id)
                    .await
                    .map(|_| ())
            } else if attempt.status == gateway_refund_status::SUCCEEDED {
                let order = self
                    .deps
                    .repo
                    .get(attempt.order_id)
                    .await?
                    .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
                self.confirm_and_mark(&order, &attempt).await
            } else {
                self.original_refund_with_options(
                    attempt.order_id,
                    &attempt.amount.to_string(),
                    &attempt.remark,
                    attempt.payment_fee_refunded,
                    "",
                    attempt.admin_options,
                )
                .await
                .map(|_| ())
            };
            if let Err(error) = result {
                tracing::warn!(%error, attempt_id = attempt.id, "gateway_refund_reconciliation_failed");
            }
        }
        Ok(())
    }

    async fn ensure_non_gateway_refund_safe(&self, order_id: Id) -> Result<()> {
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        let targets: Vec<&Order> = if order.children.is_empty() {
            vec![&order]
        } else {
            order.children.iter().collect()
        };
        for target in targets {
            if target
                .fulfillment
                .as_ref()
                .is_some_and(|fulfillment| fulfillment.logistics.contains_key("card_converter_id"))
            {
                return Err(Error::bad_request(
                    "error.card_converter_refund_review_required",
                ));
            }
            if let Some(integration) = &self.deps.integration
                && integration.requires_converter(target).await?
            {
                return Err(Error::bad_request(
                    "error.card_converter_refund_review_required",
                ));
            }
        }
        Ok(())
    }

    async fn ensure_no_online_payment(&self, order: &Order) -> Result<()> {
        // Payment is attached to the parent. Child payment split fields are the
        // checkout snapshot and are not updated by a wallet-only payment.
        let paid_order = if let Some(parent_id) = order.parent_id {
            self.deps
                .repo
                .get(parent_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?
        } else {
            order.clone()
        };
        if paid_order.online_paid_amount.is_positive() {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        Ok(())
    }

    pub async fn original_refund_supported(&self, order: &Order) -> Result<bool> {
        if order.paid_at.is_none() || order.total_amount <= order.refunded_amount {
            return Ok(false);
        }
        let (payments, _) = self
            .deps
            .payment_records
            .list_admin(&AdminPaymentFilter {
                page: PageRequest {
                    page: 1,
                    page_size: 100,
                },
                order_id: order.id,
                status: PaymentStatus::Success.as_str().into(),
                skip_count: true,
                ..Default::default()
            })
            .await?;
        for payment in payments {
            let Some(channel) = self.deps.payment_channels.get(payment.channel_id).await? else {
                continue;
            };
            if self
                .deps
                .registry
                .lookup(&channel.provider_type, &channel.channel_type)
                .is_some_and(|gateway| gateway.capabilities().refund)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Buyer-facing availability for a full refund to the successful payment method.
    pub async fn buyer_refund_mode(&self, order: &Order) -> Result<&'static str> {
        if order.paid_at.is_none() || order.total_amount <= order.refunded_amount {
            return Ok("none");
        }
        // A terminal provider failure needs staff review before another gateway
        // submission. The admin flow can retry after the merchant fixes the cause.
        if self
            .latest_refund_attempt(order.id)
            .await?
            .is_some_and(|attempt| attempt.status == gateway_refund_status::FAILED)
        {
            return Ok("contact_support");
        }
        if order.refunded_amount != zs_shared::money::Amount::ZERO
            || order.wallet_paid_amount != zs_shared::money::Amount::ZERO
            || order.online_paid_amount != order.total_amount
            || plan_refund(
                order,
                order.total_amount,
                self.order_setting().await.max_refund_days,
                self.deps.clock.now(),
            )
            .is_err()
        {
            return Ok("contact_support");
        }
        let targets: Vec<&Order> = if order.children.is_empty() {
            vec![order]
        } else {
            order.children.iter().collect()
        };
        for target in targets {
            if let Some(fulfillment) = &target.fulfillment {
                if self.deps.integration.is_none() {
                    return Ok("contact_support");
                }
                let has_binding = if let Some(integration) = &self.deps.integration {
                    integration.requires_converter(target).await?
                } else {
                    false
                };
                if fulfillment.status != "delivered"
                    || (!fulfillment.logistics.contains_key("card_converter_id") && !has_binding)
                {
                    return Ok("contact_support");
                }
            }
        }
        let (payments, _) = self
            .deps
            .payment_records
            .list_admin(&AdminPaymentFilter {
                page: PageRequest {
                    page: 1,
                    page_size: 100,
                },
                order_id: order.id,
                status: PaymentStatus::Success.as_str().into(),
                skip_count: true,
                ..AdminPaymentFilter::default()
            })
            .await?;
        let Some(payment) = payments.into_iter().find(|payment| {
            payment.status == PaymentStatus::Success && payment.amount == order.total_amount
        }) else {
            return Ok("contact_support");
        };
        let Some(channel) = self.deps.payment_channels.get(payment.channel_id).await? else {
            return Ok("contact_support");
        };
        Ok(
            if self
                .deps
                .registry
                .lookup(&channel.provider_type, &channel.channel_type)
                .is_some_and(|gateway| gateway.capabilities().refund)
            {
                "automatic"
            } else {
                "contact_support"
            },
        )
    }

    pub async fn buyer_original_refund(&self, order: &Order) -> Result<OriginalRefundResult> {
        if self.buyer_refund_mode(order).await? != "automatic" {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        self.original_refund(
            order.id,
            &order.total_amount.to_string(),
            "buyer_requested_refund",
            false,
            "",
        )
        .await
    }

    async fn prepare_card_refund(&self, order: &Order, refund_no: &str) -> Result<()> {
        let Some(integration) = &self.deps.integration else {
            return Err(Error::bad_request(
                "error.card_converter_refund_review_required",
            ));
        };
        if order.children.is_empty() {
            integration.prepare_refund(order, refund_no).await?;
        } else {
            for child in &order.children {
                integration.prepare_refund(child, refund_no).await?;
            }
        }
        Ok(())
    }

    async fn confirm_card_refund(&self, order: &Order, refund_no: &str) -> Result<()> {
        let Some(integration) = &self.deps.integration else {
            return Err(Error::bad_request(
                "error.card_converter_refund_review_required",
            ));
        };
        if order.children.is_empty() {
            integration.confirm_refund(order, refund_no).await?;
        } else {
            for child in &order.children {
                integration.confirm_refund(child, refund_no).await?;
            }
        }
        Ok(())
    }

    async fn confirm_and_mark(&self, order: &Order, attempt: &GatewayRefundAttempt) -> Result<()> {
        if !attempt.admin_options.force_refund {
            self.confirm_card_refund(order, &attempt.refund_no).await?;
        }
        self.deps
            .store
            .mark_gateway_refund_card_confirmed(attempt.id)
            .await
    }

    async fn cancel_card_refund(&self, order: &Order, refund_no: &str) -> Result<()> {
        let Some(integration) = &self.deps.integration else {
            return Err(Error::bad_request(
                "error.card_converter_refund_review_required",
            ));
        };
        if order.children.is_empty() {
            integration.cancel_refund(order, refund_no).await?;
        } else {
            for child in &order.children {
                integration.cancel_refund(child, refund_no).await?;
            }
        }
        Ok(())
    }

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
        self.original_refund_with_options(
            order_id,
            raw_amount,
            remark,
            fee_refunded,
            client_ip,
            AdminRefundOptions::default(),
        )
        .await
    }

    pub async fn original_refund_with_options(
        &self,
        order_id: Id,
        raw_amount: &str,
        remark: &str,
        fee_refunded: bool,
        client_ip: &str,
        admin_options: AdminRefundOptions,
    ) -> Result<OriginalRefundResult> {
        if (admin_options.force_refund && admin_options.admin_id.is_none_or(|id| id <= 0))
            || (admin_options.restock_cards && !admin_options.force_refund)
        {
            return Err(Error::invalid());
        }
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
        let current_order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        if payment.order_id != order_id || payment.currency != current_order.currency {
            return Err(Error::bad_request("error.payment_not_found"));
        }
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
        let capabilities = gateway.capabilities();
        if !supports_refund_amount(capabilities, payment.amount, amount) {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        let previous_attempts = self.deps.store.gateway_refund_attempts(order_id).await?;
        let previous_attempt = previous_attempts
            .into_iter()
            .find(|attempt| attempt.payment_id == payment.id);
        if previous_attempt.as_ref().is_some_and(|attempt| {
            matches!(
                attempt.status.as_str(),
                gateway_refund_status::MANUAL_PREPARING
                    | gateway_refund_status::MANUAL_LOCKING
                    | gateway_refund_status::MANUAL_AWAITING
                    | gateway_refund_status::MANUAL_CANCELING
            )
        }) {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if let Some(attempt) = previous_attempt.as_ref()
            && matches!(
                attempt.status.as_str(),
                gateway_refund_status::PREPARING | gateway_refund_status::PENDING
            )
            && (attempt.amount != amount
                || attempt.payment_fee_refunded != fee_refunded
                || attempt.remark != remark
                || attempt.admin_options != admin_options)
        {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if let Some(attempt) = previous_attempt.as_ref()
            && attempt.status == gateway_refund_status::SUCCEEDED
            && attempt.amount == amount
            && attempt.payment_fee_refunded == fee_refunded
            && attempt.remark == remark
            && attempt.admin_options == admin_options
        {
            let result = GatewayRefundResult {
                provider_ref: attempt.provider_ref.clone(),
                status: Some(PaymentStatus::Success),
                payload: attempt.payload.clone(),
            };
            let completed = Some(self.apply_gateway_refund(attempt, &result).await?);
            let order = self
                .deps
                .repo
                .get(order_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            self.confirm_and_mark(&order, attempt).await?;
            return Ok(OriginalRefundResult {
                gateway: result,
                attempt: attempt.clone(),
                completed,
            });
        }
        if previous_attempt.as_ref().is_none_or(|attempt| {
            !matches!(
                attempt.status.as_str(),
                gateway_refund_status::PREPARING | gateway_refund_status::PENDING
            )
        }) {
            self.preflight_gateway_refund_with_options(order_id, amount, admin_options)
                .await?;
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
                initial_status: gateway_refund_status::PREPARING.to_owned(),
                admin_options,
            })
            .await?;
        let attempt = reservation.attempt;
        if attempt.admin_options != admin_options {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }

        if attempt.status == gateway_refund_status::SUCCEEDED {
            let result = GatewayRefundResult {
                provider_ref: attempt.provider_ref.clone(),
                status: Some(PaymentStatus::Success),
                payload: attempt.payload.clone(),
            };
            let completed = Some(self.apply_gateway_refund(&attempt, &result).await?);
            let order = self
                .deps
                .repo
                .get(order_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            self.confirm_and_mark(&order, &attempt).await?;
            return Ok(OriginalRefundResult {
                gateway: result,
                attempt,
                completed,
            });
        }

        let should_submit = if attempt.status == gateway_refund_status::PREPARING {
            let order = self
                .deps
                .repo
                .get(order_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            if let Err(error) = if admin_options.force_refund {
                Ok(())
            } else {
                self.prepare_card_refund(&order, &attempt.refund_no).await
            } {
                if self
                    .cancel_card_refund(&order, &attempt.refund_no)
                    .await
                    .is_ok()
                {
                    self.deps
                        .store
                        .update_gateway_refund_attempt(
                            attempt.id,
                            &GatewayRefundAttemptUpdate {
                                status: gateway_refund_status::FAILED.to_owned(),
                                payload: Map::new(),
                                error: error.to_string(),
                                now: self.deps.clock.now(),
                            },
                        )
                        .await?;
                }
                return Err(error);
            }
            self.deps
                .store
                .claim_gateway_refund_submission(attempt.id, self.deps.clock.now())
                .await?
        } else {
            false
        };
        let gateway_result = if should_submit {
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
            let current = self
                .deps
                .store
                .gateway_refund_attempt(attempt.id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            if matches!(
                current.status.as_str(),
                gateway_refund_status::PREPARING | gateway_refund_status::PENDING
            ) && self.deps.clock.now()
                < current.updated_at + chrono::Duration::seconds(REFUND_SUBMISSION_GRACE_SECONDS)
            {
                return Ok(OriginalRefundResult {
                    gateway: GatewayRefundResult {
                        provider_ref: current.provider_ref.clone(),
                        status: Some(PaymentStatus::Pending),
                        payload: current.payload.clone(),
                    },
                    attempt: current,
                    completed: None,
                });
            }
            gateway
                .query_refund(&channel.config_json, &attempt.provider_ref)
                .await
                .map_err(Error::from)?
        };

        let gateway_status = gateway_result.status.unwrap_or(PaymentStatus::Pending);
        let completed = if gateway_status == PaymentStatus::Success {
            let completed = self.apply_gateway_refund(&attempt, &gateway_result).await?;
            let order = self
                .deps
                .repo
                .get(order_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            self.confirm_and_mark(&order, &attempt).await?;
            Some(completed)
        } else {
            let status = if gateway_status == PaymentStatus::Failed {
                gateway_refund_status::FAILED
            } else {
                gateway_refund_status::PENDING
            };
            if gateway_status == PaymentStatus::Failed {
                let order = self
                    .deps
                    .repo
                    .get(order_id)
                    .await?
                    .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
                if !attempt.admin_options.force_refund {
                    self.cancel_card_refund(&order, &attempt.refund_no).await?;
                }
            }
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
        self.preflight_gateway_refund_with_options(order_id, amount, AdminRefundOptions::default())
            .await
    }

    async fn preflight_gateway_refund_with_options(
        &self,
        order_id: Id,
        amount: zs_shared::money::Amount,
        admin_options: AdminRefundOptions,
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
            if admin_options.force_refund {
                0
            } else {
                setting.max_refund_days
            },
            self.deps.clock.now(),
        )?;
        let delivered = order.fulfillment.is_some()
            || order
                .children
                .iter()
                .any(|child| child.fulfillment.is_some());
        if delivered
            && !admin_options.force_refund
            && (amount != order.total_amount
                || order.refunded_amount != zs_shared::money::Amount::ZERO)
        {
            return Err(Error::bad_request(
                "error.card_converter_refund_review_required",
            ));
        }
        if !admin_options.force_refund {
            let targets: Vec<&Order> = if order.children.is_empty() {
                vec![&order]
            } else {
                order.children.iter().collect()
            };
            for target in targets {
                if let Some(f) = &target.fulfillment
                    && f.kind != zs_domain::order::model::fulfillment_type::MANUAL
                    && !f.logistics.contains_key("card_converter_id")
                {
                    let bound = if let Some(integration) = &self.deps.integration {
                        integration.requires_converter(target).await?
                    } else {
                        false
                    };
                    if !bound {
                        return Err(Error::bad_request(
                            "error.card_converter_refund_review_required",
                        ));
                    }
                }
            }
        }
        if admin_options.restock_cards {
            zs_domain::order::refund::restock_targets(&order, amount)?;
        }
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
        self.ensure_non_gateway_refund_safe(order_id).await?;
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        self.ensure_no_online_payment(&order).await?;
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
        self.ensure_non_gateway_refund_safe(order_id).await?;
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        self.ensure_no_online_payment(&order).await?;
        self.apply_refund(order_id, amount, remark, false, fee_refunded, None)
            .await
    }

    /// Lock delivered cards before support transfers money outside the payment gateway.
    /// This records no completed refund until `confirm_manual_refund` is called.
    pub async fn prepare_manual_refund(
        &self,
        order_id: Id,
        raw_amount: &str,
        remark: &str,
        fee_refunded: bool,
    ) -> Result<GatewayRefundAttempt> {
        let amount = parse_refund_amount(raw_amount)?;
        self.preflight_gateway_refund(order_id, amount).await?;
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
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
        if payment.order_id != order_id || payment.currency != order.currency {
            return Err(Error::bad_request("error.payment_not_found"));
        }
        if amount > payment.amount || order.wallet_paid_amount != zs_shared::money::Amount::ZERO {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        let channel = self
            .deps
            .payment_channels
            .get(payment.channel_id)
            .await?
            .ok_or_else(|| Error::bad_request("error.payment_channel_not_found"))?;
        if self
            .deps
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .is_some_and(|gateway| {
                supports_refund_amount(gateway.capabilities(), payment.amount, amount)
            })
        {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        let now = self.deps.clock.now();
        let refund_no = format!("MR{}{}", order_id, now.timestamp_millis());
        let reservation = self
            .deps
            .store
            .reserve_gateway_refund(&NewGatewayRefundAttempt {
                order_id,
                payment_id: payment.id,
                channel_id: payment.channel_id,
                request_date: String::new(),
                refund_no: refund_no.clone(),
                provider_ref: String::new(),
                amount,
                remark: remark.to_owned(),
                payment_fee_refunded: fee_refunded,
                now,
                initial_status: gateway_refund_status::MANUAL_PREPARING.to_owned(),
                admin_options: AdminRefundOptions::default(),
            })
            .await?;
        let attempt = reservation.attempt;
        if attempt.amount != amount
            || attempt.remark != remark
            || attempt.payment_fee_refunded != fee_refunded
        {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if attempt.status == gateway_refund_status::MANUAL_AWAITING {
            return Ok(attempt);
        }
        if !matches!(
            attempt.status.as_str(),
            gateway_refund_status::MANUAL_PREPARING | gateway_refund_status::MANUAL_LOCKING
        ) {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if !self
            .deps
            .store
            .claim_manual_refund_preparation(attempt.id, self.deps.clock.now())
            .await?
        {
            return Ok(attempt);
        }
        if let Err(error) = self.prepare_card_refund(&order, &attempt.refund_no).await {
            if self
                .cancel_card_refund(&order, &attempt.refund_no)
                .await
                .is_ok()
            {
                self.deps
                    .store
                    .update_gateway_refund_attempt(
                        attempt.id,
                        &GatewayRefundAttemptUpdate {
                            status: gateway_refund_status::FAILED.to_owned(),
                            payload: Map::new(),
                            error: error.to_string(),
                            now: self.deps.clock.now(),
                        },
                    )
                    .await?;
            }
            return Err(error);
        }
        self.deps
            .store
            .update_gateway_refund_attempt(
                attempt.id,
                &GatewayRefundAttemptUpdate {
                    status: gateway_refund_status::MANUAL_AWAITING.to_owned(),
                    payload: Map::new(),
                    error: String::new(),
                    now: self.deps.clock.now(),
                },
            )
            .await
    }

    /// Staff supplies the external original-route transfer reference after confirming payment.
    pub async fn confirm_manual_refund(
        &self,
        order_id: Id,
        attempt_id: Id,
        transfer_ref: &str,
    ) -> Result<RefundDone> {
        let transfer_ref = transfer_ref.trim();
        if transfer_ref.is_empty() {
            return Err(Error::invalid());
        }
        let attempt = self
            .deps
            .store
            .gateway_refund_attempt(attempt_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        if attempt.order_id != order_id
            || !matches!(
                attempt.status.as_str(),
                gateway_refund_status::MANUAL_AWAITING | gateway_refund_status::SUCCEEDED
            )
        {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if attempt.status == gateway_refund_status::SUCCEEDED
            && attempt.provider_ref != transfer_ref
        {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        let mut payload = Map::new();
        payload.insert("manual_transfer_ref".into(), transfer_ref.into());
        let done = self
            .apply_refund(
                order_id,
                &attempt.amount.to_string(),
                &attempt.remark,
                false,
                attempt.payment_fee_refunded,
                Some((attempt.id, transfer_ref.to_owned(), payload)),
            )
            .await?;
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        self.confirm_and_mark(&order, &attempt).await?;
        Ok(done)
    }

    /// Releases a prepared manual refund only when no transfer has been confirmed.
    pub async fn cancel_manual_refund(
        &self,
        order_id: Id,
        attempt_id: Id,
    ) -> Result<GatewayRefundAttempt> {
        let attempt = self
            .deps
            .store
            .gateway_refund_attempt(attempt_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        if attempt.order_id != order_id || !attempt.refund_no.starts_with("MR") {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if attempt.status == gateway_refund_status::MANUAL_CANCELED {
            return Ok(attempt);
        }
        if !matches!(
            attempt.status.as_str(),
            gateway_refund_status::MANUAL_AWAITING | gateway_refund_status::MANUAL_CANCELING
        ) {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        if !self
            .deps
            .store
            .claim_manual_refund_cancellation(attempt.id, self.deps.clock.now())
            .await?
        {
            let current = self
                .deps
                .store
                .gateway_refund_attempt(attempt_id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            return if matches!(
                current.status.as_str(),
                gateway_refund_status::MANUAL_CANCELING | gateway_refund_status::MANUAL_CANCELED
            ) {
                Ok(current)
            } else {
                Err(Error::bad_request("error.gateway_refund_pending"))
            };
        }
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        self.cancel_card_refund(&order, &attempt.refund_no).await?;
        self.deps
            .store
            .update_gateway_refund_attempt(
                attempt.id,
                &GatewayRefundAttemptUpdate {
                    status: gateway_refund_status::MANUAL_CANCELED.to_owned(),
                    payload: Map::new(),
                    error: "manual refund canceled before transfer".to_owned(),
                    now: self.deps.clock.now(),
                },
            )
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
