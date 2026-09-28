//! The order-aware [`PaymentSettlement`] (replaces the payment group's row-only stub).

use async_trait::async_trait;
use zs_domain::Result;
use zs_domain::payment::callback::CallbackInput;
use zs_domain::payment::model::Payment;
use zs_domain::payment::settlement::PaymentSettlement;

use super::OrderService;

impl OrderService {
    /// Applies a verified gateway result (callback, webhook or capture): the store locks the
    /// payment and the order, re-validates the facts and settles atomically (PAY-02/03/04);
    /// the paid-order side effects run once, after commit, only for the first success.
    pub async fn settle_payment(&self, input: CallbackInput) -> Result<Payment> {
        let now = self.deps.clock.now();
        let settled = self
            .deps
            .payments
            .settle(&input, self.deps.reseller_confirm_days, now)
            .await?;
        tracing::info!(
            payment_id = settled.payment.id,
            order_id = settled.payment.order_id,
            status = %settled.payment.status,
            order_paid = settled.order_paid,
            underpaid = settled.underpaid,
            exception_code = %settled.payment.exception_code,
            "payment_callback_processed"
        );
        if settled.order_paid
            && let Some(order) = &settled.order
        {
            self.after_order_paid(order, Some(&settled.payment)).await;
        }
        Ok(settled.payment)
    }
}

/// [`PaymentSettlement`] for order payments.
#[derive(Debug, Clone)]
pub struct OrderSettlement(pub OrderService);

#[async_trait]
impl PaymentSettlement for OrderSettlement {
    async fn settle(&self, input: CallbackInput) -> Result<Payment> {
        self.0.settle_payment(input).await
    }
}
