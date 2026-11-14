//! Wallet-only converter refunds use the existing durable ledger, never a payment gateway.
use super::*;
use zs_domain::payment::{
    model::Payment,
    types::{channel_type, provider},
};
use zs_shared::money::Amount;

pub(super) struct WalletRefundOutcome {
    pub attempt: GatewayRefundAttempt,
    pub completed: Option<RefundDone>,
}

impl OrderService {
    pub(super) async fn wallet_payment(&self, order: &Order) -> Result<Payment> {
        if order.user_id <= 0
            || order.parent_id.is_some()
            || order.paid_at.is_none()
            || !order.total_amount.is_positive()
            || order.wallet_paid_amount != order.total_amount
            || order.online_paid_amount != Amount::ZERO
        {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        let (payments, _) = self
            .deps
            .payment_records
            .list_admin(&AdminPaymentFilter {
                order_id: order.id,
                status: PaymentStatus::Success.as_str().into(),
                page: PageRequest {
                    page: 1,
                    page_size: 100,
                },
                skip_count: true,
                ..Default::default()
            })
            .await?;
        let mut matches = payments
            .into_iter()
            .filter(|p| p.status == PaymentStatus::Success);
        let payment = matches
            .next()
            .ok_or_else(|| Error::bad_request("error.payment_not_found"))?;
        if matches.next().is_some()
            || payment.order_id != order.id
            || payment.channel_id != 0
            || payment.provider_type != provider::WALLET
            || payment.channel_type != channel_type::BALANCE
            || payment.amount != order.total_amount
            || payment.currency != order.currency
            || payment.paid_at.is_none()
            || payment.fee_amount != Amount::ZERO
        {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        Ok(payment)
    }

    pub(super) async fn wallet_converter_refund(
        &self,
        order_id: Id,
        raw_amount: &str,
        remark: &str,
    ) -> Result<WalletRefundOutcome> {
        let amount = parse_refund_amount(raw_amount)?;
        let order = self
            .deps
            .repo
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        let payment = self.wallet_payment(&order).await?;
        let remark = remark.trim();
        let previous = self.deps.store.gateway_refund_attempts(order_id).await?;
        for attempt in previous {
            if matches!(
                attempt.status.as_str(),
                gateway_refund_status::PREPARING
                    | gateway_refund_status::PENDING
                    | gateway_refund_status::WALLET_CANCELING
            ) || (attempt.status == gateway_refund_status::SUCCEEDED
                && attempt.amount == amount
                && attempt.remark == remark)
            {
                if !attempt.is_wallet()
                    || attempt.payment_id != payment.id
                    || attempt.amount != amount
                    || attempt.remark != remark
                    || attempt.payment_fee_refunded
                    || attempt.admin_options != AdminRefundOptions::default()
                {
                    return Err(Error::bad_request("error.gateway_refund_pending"));
                }
                return self.resume_wallet_refund(&attempt).await;
            }
        }
        // A converter freezes an entire delivered batch. A partial credit must not void it.
        if amount != order.total_amount || order.refunded_amount != Amount::ZERO {
            return Err(Error::bad_request(
                "error.card_converter_refund_review_required",
            ));
        }
        self.preflight_gateway_refund(order_id, amount).await?;
        let now = self.deps.clock.now();
        let reserved = self
            .deps
            .store
            .reserve_gateway_refund(&NewGatewayRefundAttempt {
                order_id,
                payment_id: payment.id,
                channel_id: 0,
                request_date: String::new(),
                refund_no: format!("WR{}", uuid::Uuid::new_v4().simple()),
                provider_ref: String::new(),
                amount,
                remark: remark.to_owned(),
                payment_fee_refunded: false,
                now,
                initial_status: gateway_refund_status::PREPARING.to_owned(),
                admin_options: AdminRefundOptions::default(),
            })
            .await?;
        let attempt = reserved.attempt;
        if !attempt.is_wallet()
            || attempt.amount != amount
            || attempt.remark != remark
            || attempt.payment_id != payment.id
            || attempt.payment_fee_refunded
            || attempt.admin_options != AdminRefundOptions::default()
        {
            return Err(Error::bad_request("error.gateway_refund_pending"));
        }
        self.resume_wallet_refund(&attempt).await
    }

    pub(super) async fn resume_wallet_refund(
        &self,
        original: &GatewayRefundAttempt,
    ) -> Result<WalletRefundOutcome> {
        let mut attempt = self
            .deps
            .store
            .gateway_refund_attempt(original.id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        if !attempt.is_wallet() {
            return Err(Error::invalid());
        }
        let order = self
            .deps
            .repo
            .get(attempt.order_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        let payment = self.wallet_payment(&order).await?;
        if payment.id != attempt.payment_id
            || attempt.amount != order.total_amount
            || attempt.payment_fee_refunded
            || attempt.admin_options != AdminRefundOptions::default()
        {
            return Err(Error::bad_request("error.payment_provider_not_supported"));
        }
        if attempt.status == gateway_refund_status::PREPARING {
            if let Err(error) = self.prepare_card_refund(&order, &attempt.refund_no).await {
                // Cancellation wins only before another worker has committed the prepared
                // phase. Its durable tombstone rejects any late prepare from this attempt.
                if self
                    .deps
                    .store
                    .claim_wallet_refund_cancellation(attempt.id, self.deps.clock.now())
                    .await?
                {
                    self.finish_wallet_cancellation(&order, &attempt, error.key())
                        .await?;
                }
                return Err(error);
            }
            self.deps
                .store
                .claim_gateway_refund_submission(attempt.id, self.deps.clock.now())
                .await?;
            attempt = self
                .deps
                .store
                .gateway_refund_attempt(attempt.id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        }
        if attempt.status == gateway_refund_status::WALLET_CANCELING {
            if self
                .deps
                .store
                .claim_wallet_refund_cancellation(attempt.id, self.deps.clock.now())
                .await?
            {
                self.finish_wallet_cancellation(
                    &order,
                    &attempt,
                    "wallet_refund_preparation_failed",
                )
                .await?;
                attempt = self
                    .deps
                    .store
                    .gateway_refund_attempt(attempt.id)
                    .await?
                    .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            }
            return Ok(WalletRefundOutcome {
                attempt,
                completed: None,
            });
        }
        let completed = if attempt.status == gateway_refund_status::PENDING {
            // RefundRequest persists the wallet credit, refund record, order/family state,
            // profit reversals and succeeded attempt together. A lost response is replayed.
            Some(self.credit_accepted_wallet_refund(&attempt).await?)
        } else if attempt.status == gateway_refund_status::SUCCEEDED {
            let id = attempt.refund_record_id.ok_or_else(Error::invalid)?;
            let record = self
                .deps
                .repo
                .refund(id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
            if record.order_id != order.id
                || record.amount != attempt.amount
                || record.user_id != order.user_id
                || record.currency != order.currency
                || record.kind != zs_domain::order::model::refund_type::WALLET
            {
                return Err(Error::invalid());
            }
            Some(RefundDone {
                order: order.clone(),
                record,
                transaction: None,
            })
        } else {
            None
        };
        attempt = self
            .deps
            .store
            .gateway_refund_attempt(attempt.id)
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        if completed.is_some() && !attempt.card_confirmed {
            if let Err(error) = self
                .confirm_and_mark(
                    &completed.as_ref().ok_or_else(Error::invalid)?.order,
                    &attempt,
                )
                .await
            {
                tracing::warn!(
                    attempt_id = attempt.id,
                    category = error.key(),
                    "wallet_refund_card_confirmation_pending"
                );
            }
            attempt = self
                .deps
                .store
                .gateway_refund_attempt(attempt.id)
                .await?
                .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        }
        Ok(WalletRefundOutcome { attempt, completed })
    }

    async fn credit_accepted_wallet_refund(
        &self,
        attempt: &GatewayRefundAttempt,
    ) -> Result<RefundDone> {
        let done = self
            .deps
            .store
            .refund(&RefundRequest {
                order_id: attempt.order_id,
                amount: attempt.amount,
                remark: attempt.remark.clone(),
                to_wallet: true,
                payment_fee_refunded: false,
                gateway_refund_attempt_id: Some(attempt.id),
                gateway_refund_provider_ref: format!("wallet:{}", attempt.refund_no),
                gateway_refund_payload: Map::new(),
                // This immutable WR intent was accepted within the refund window before
                // prepare. Recovery must finish that accepted refund even after an outage;
                // ledger timestamps remain the actual credit time, not the authorization time.
                max_refund_days: 0,
                reseller_confirm_days: self.deps.reseller_confirm_days,
                now: self.deps.clock.now(),
            })
            .await?;
        if done.transaction.is_some() {
            self.after_refund(&done).await;
        }
        Ok(done)
    }

    async fn finish_wallet_cancellation(
        &self,
        order: &Order,
        attempt: &GatewayRefundAttempt,
        category: &str,
    ) -> Result<()> {
        self.cancel_card_refund(order, &attempt.refund_no).await?;
        self.deps
            .store
            .update_gateway_refund_attempt(
                attempt.id,
                &GatewayRefundAttemptUpdate {
                    status: gateway_refund_status::FAILED.to_owned(),
                    payload: Map::new(),
                    error: category.to_owned(),
                    now: self.deps.clock.now(),
                },
            )
            .await?;
        Ok(())
    }
}
