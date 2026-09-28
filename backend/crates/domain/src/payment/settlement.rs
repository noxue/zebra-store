//! Settlement port: applies a verified callback to the payment (and later the order).

use async_trait::async_trait;

use super::callback::CallbackInput;
use super::model::Payment;
use crate::Result;

/// Applies a verified, fact-checked gateway notification atomically.
///
/// Implementations must, inside one transaction: lock the payment row, re-run
/// [`super::callback::validate_callback_facts`] against the locked row and its business
/// number, apply [`super::callback::apply_callback`], and only on the first success
/// perform side effects (order paid, wallet credit, fulfilment jobs). Repeated or
/// concurrent notifications must never settle twice (PAY-04).
///
/// The payment group ships a payment-only implementation; the order group replaces it
/// with one that also settles orders and wallet recharges.
#[async_trait]
pub trait PaymentSettlement: Send + Sync {
    async fn settle(&self, input: CallbackInput) -> Result<Payment>;
}
