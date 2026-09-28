//! Refund rules (RFD-01 … RFD-04).

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use zs_shared::money::Amount;

use super::model::{Order, OrderStatus, keys, refund_type};
use super::status::refund_target;
use crate::payment::model::Payment;
use crate::payment::types::{FeePolicy, PaymentStatus, provider};
use crate::{Error, Result};

/// Refunds above what is refundable (`walletcontract.ErrRefundExceeded`, rendered as
/// `error.bad_request` like the original handler).
pub fn refund_exceeded() -> Error {
    Error::invalid()
}

/// Parses a refund amount string (`ParseRefundAmount`): positive after rounding.
pub fn parse_refund_amount(raw: &str) -> Result<Amount> {
    let value: Decimal = raw.trim().parse().map_err(|_| Error::invalid())?;
    let amount = Amount::new(value);
    if !amount.is_positive() {
        return Err(Error::invalid());
    }
    Ok(amount)
}

/// `isOrderRefundWindowExpired` (RFD-04): based on `paid_at` (else `created_at`); `0` days
/// means unlimited.
pub fn refund_window_expired(
    created_at: DateTime<Utc>,
    paid_at: Option<DateTime<Utc>>,
    max_days: i64,
    now: DateTime<Utc>,
) -> bool {
    if max_days <= 0 {
        return false;
    }
    let base = paid_at.unwrap_or(created_at);
    now > base + Duration::days(max_days)
}

/// A validated refund against a locked order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RefundPlan {
    pub amount: Amount,
    pub refunded_before: Amount,
    pub new_refunded: Amount,
    pub target: OrderStatus,
}

/// Validates a refund on a locked order (RFD-02, RFD-03, RFD-04) in the original order:
/// paid → window → positive total → not exceeding what is left.
pub fn plan_refund(
    order: &Order,
    amount: Amount,
    max_refund_days: i64,
    now: DateTime<Utc>,
) -> Result<RefundPlan> {
    if !amount.is_positive() {
        return Err(Error::invalid());
    }
    if order.paid_at.is_none() {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    if refund_window_expired(order.created_at, order.paid_at, max_refund_days, now) {
        return Err(Error::bad_request(keys::ORDER_REFUND_EXPIRED));
    }
    if !order.total_amount.is_positive() {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    let refunded_before = order.refunded_amount;
    let refundable = order.total_amount - refunded_before;
    if amount > refundable {
        return Err(refund_exceeded());
    }
    let new_refunded = refunded_before + amount;
    Ok(RefundPlan {
        amount,
        refunded_before,
        new_refunded,
        target: refund_target(new_refunded, order.total_amount),
    })
}

/// Allocates the payment fee to one refund cumulatively so the last refund absorbs rounding
/// and the total never exceeds the fee (`CalculatePaymentFeeRefundAmount`, RFD-01).
pub fn payment_fee_refund_amount(
    payment_amount: Amount,
    payment_fee: Amount,
    refunded_principal_before: Amount,
    refunded_fee_before: Amount,
    refund_amount: Amount,
) -> Amount {
    if !payment_amount.is_positive() || !payment_fee.is_positive() || !refund_amount.is_positive() {
        return Amount::ZERO;
    }
    let principal_before = refunded_principal_before.non_negative().min(payment_amount);
    let fee_before = refunded_fee_before.non_negative().min(payment_fee);
    let cumulative = (principal_before + refund_amount).min(payment_amount);
    let target = if cumulative >= payment_amount {
        payment_fee
    } else {
        Amount::new(payment_fee.decimal() * cumulative.decimal() / payment_amount.decimal())
    };
    let current = target - fee_before;
    if current.is_negative() {
        return Amount::ZERO;
    }
    current.min(payment_fee - fee_before)
}

/// Payment amount and fee a manual refund may give back (`refundablePaymentFeeSnapshot`):
/// successful, non-wallet, merchant-absorbed payments without an exception code, in the
/// order currency.
pub fn refundable_fee_snapshot(payments: &[Payment], currency: &str) -> (Amount, Amount) {
    let mut amount = Amount::ZERO;
    let mut fee = Amount::ZERO;
    for p in payments {
        if p.status != PaymentStatus::Success
            || p.provider_type == provider::WALLET
            || FeePolicy::parse(&p.fee_policy) != Some(FeePolicy::MerchantAbsorbed)
            || !p.exception_code.trim().is_empty()
        {
            continue;
        }
        if !currency.is_empty()
            && !p.currency.is_empty()
            && !currency.eq_ignore_ascii_case(&p.currency)
        {
            continue;
        }
        if !p.amount.is_positive() || !p.fee_amount.is_positive() {
            continue;
        }
        amount += p.amount;
        fee += p.fee_amount;
    }
    (amount, fee)
}

/// Whether the fee-refunded flag may be changed on a record (manual refunds only).
pub fn fee_flag_editable(kind: &str) -> bool {
    kind == refund_type::MANUAL
}

#[cfg(test)]
mod tests {
    use super::super::model::testkit::order;
    use super::*;
    use crate::payment::model::tests::sample;

    fn a(s: &str) -> Amount {
        s.parse().unwrap_or_default()
    }

    /// RFD-01: cumulative allocation absorbs rounding and caps at the fee.
    #[test]
    fn rfd_01_fee_allocation() {
        let first =
            payment_fee_refund_amount(a("100"), a("3.01"), Amount::ZERO, Amount::ZERO, a("33.33"));
        assert_eq!(first, a("1.00"));
        let second = payment_fee_refund_amount(a("100"), a("3.01"), a("33.33"), first, a("66.67"));
        assert_eq!(second, a("2.01"));
        assert_eq!(
            payment_fee_refund_amount(a("80"), a("2.40"), a("60"), a("1.80"), a("50")),
            a("0.60")
        );
        assert_eq!(
            payment_fee_refund_amount(a("100"), a("3"), Amount::ZERO, Amount::ZERO, a("40")),
            a("1.20")
        );
        assert_eq!(
            payment_fee_refund_amount(a("100"), Amount::ZERO, Amount::ZERO, Amount::ZERO, a("40")),
            Amount::ZERO
        );
    }

    /// RFD-02 / RFD-03: guards and status targets.
    #[test]
    fn rfd_02_plan() {
        let now = chrono::Utc::now();
        let mut o = order(1, OrderStatus::Completed);
        o.created_at = now;
        let unpaid = plan_refund(&o, a("15"), 30, now)
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(unpaid.as_deref(), Some("error.order_status_invalid"));
        o.paid_at = Some(now);
        let p = plan_refund(&o, a("30"), 30, now).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.target, OrderStatus::PartiallyRefunded);
        o.refunded_amount = a("30");
        assert_eq!(
            plan_refund(&o, a("80"), 30, now)
                .err()
                .map(|e| e.key().to_owned())
                .as_deref(),
            Some("error.bad_request")
        );
        let p = plan_refund(&o, a("70"), 30, now).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.target, OrderStatus::Refunded);
        assert_eq!(p.new_refunded, a("100"));
        assert!(plan_refund(&o, Amount::ZERO, 30, now).is_err());
        assert_eq!(
            parse_refund_amount("-1")
                .err()
                .map(|e| e.key().to_owned())
                .as_deref(),
            Some("error.bad_request")
        );
        assert_eq!(parse_refund_amount("12.345").ok(), Some(a("12.35")));
    }

    /// RFD-04: refund window.
    #[test]
    fn rfd_04_window() {
        let now = chrono::Utc::now();
        let paid = |days| Some(now - Duration::days(days));
        assert!(refund_window_expired(now, paid(31), 30, now));
        assert!(!refund_window_expired(now, paid(29), 30, now));
        assert!(!refund_window_expired(now, paid(365), 0, now));
    }

    #[test]
    fn fee_snapshot_filters_payments() {
        let mut ok = sample();
        ok.status = PaymentStatus::Success;
        ok.fee_policy = "merchant_absorbed".into();
        ok.amount = a("100");
        ok.fee_amount = a("3");
        let mut surcharge = ok.clone();
        surcharge.fee_policy = "customer_surcharge".into();
        let mut excepted = ok.clone();
        excepted.exception_code = "duplicate_payment_succeeded".into();
        assert_eq!(
            refundable_fee_snapshot(&[ok, surcharge, excepted], "CNY"),
            (a("100"), a("3"))
        );
    }
}
