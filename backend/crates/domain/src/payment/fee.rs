//! Fee policy and amount-coverage rules (PAY-02, PAY-03, PAY-12, PAY-26).

use rust_decimal::Decimal;
use zs_shared::money::Amount;

use super::model::Payment;
use super::types::FeePolicy;

/// Result of [`calculate_payment_amounts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaymentAmounts {
    /// Amount the customer pays online.
    pub payable: Amount,
    pub fee: Amount,
    pub policy: FeePolicy,
}

/// `calculatePaymentAmounts`: fee = round2(fixed + base × rate / 100); the customer only pays
/// it when `customer_fee_enabled` (otherwise the merchant absorbs it).
pub fn calculate_payment_amounts(
    base: Amount,
    fee_rate: Decimal,
    fixed_fee: Decimal,
    customer_fee_enabled: bool,
) -> PaymentAmounts {
    let mut fee = Amount::new(fixed_fee).decimal();
    if fee_rate > Decimal::ZERO {
        fee += base.decimal() * fee_rate / Decimal::ONE_HUNDRED;
    }
    let fee = Amount::new(fee);
    if fee.is_zero() {
        return PaymentAmounts {
            payable: base,
            fee,
            policy: FeePolicy::None,
        };
    }
    if customer_fee_enabled {
        PaymentAmounts {
            payable: base + fee,
            fee,
            policy: FeePolicy::CustomerSurcharge,
        }
    } else {
        PaymentAmounts {
            payable: base,
            fee,
            policy: FeePolicy::MerchantAbsorbed,
        }
    }
}

fn payload_decimal(payment: &Payment, key: &str) -> Option<Decimal> {
    let raw = match payment.provider_payload.get(key)? {
        serde_json::Value::String(s) => s.trim().to_owned(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => return None,
    };
    raw.parse::<Decimal>().ok()
}

/// Exchange-rate snapshot taken at creation (`paymentExchangeRate`); only positive rates count.
pub fn payment_exchange_rate(payment: &Payment) -> Option<Decimal> {
    payload_decimal(payment, "exchange_rate").filter(|r| *r > Decimal::ZERO)
}

/// Order-currency amount a payment covers (`paymentCoveredOrderAmount`, PAY-02/PAY-26):
/// converted payments are divided back by the snapshotted rate, customer surcharges removed.
pub fn covered_order_amount(payment: &Payment, callback_amount: Amount) -> Amount {
    let mut covered = payment.amount.decimal();
    if let Some(rate) = payment_exchange_rate(payment) {
        let source = if callback_amount.is_zero() {
            payment.amount.decimal()
        } else {
            callback_amount.decimal()
        };
        covered = source / rate;
    } else if let Some(original) = payload_decimal(payment, "original_amount") {
        covered = original;
    }
    let fee = payment.fee_amount.decimal();
    match FeePolicy::parse(&payment.fee_policy) {
        Some(FeePolicy::CustomerSurcharge | FeePolicy::LegacyCustomerSurcharge) => covered -= fee,
        None if payment.fee_policy.trim().is_empty() && fee > Decimal::ZERO => covered -= fee,
        _ => {}
    }
    Amount::new(covered).non_negative()
}

/// Online amount an order still requires (`total − wallet_paid`, clamped at zero).
pub fn required_online_amount(total: Amount, wallet_paid: Amount) -> Amount {
    (total - wallet_paid).non_negative()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::model::tests::sample;
    use serde_json::json;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap_or_default()
    }

    /// PAY-03: fee policy snapshot.
    #[test]
    fn pay_03_fee_policy() {
        let base = Amount::from(100);
        let absorbed = calculate_payment_amounts(base, dec("3"), Decimal::ZERO, false);
        assert_eq!(absorbed.payable, Amount::from(100));
        assert_eq!(absorbed.fee, Amount::from(3));
        assert_eq!(absorbed.policy, FeePolicy::MerchantAbsorbed);
        let surcharge = calculate_payment_amounts(base, dec("3"), Decimal::ZERO, true);
        assert_eq!(surcharge.payable, Amount::from(103));
        assert_eq!(surcharge.policy, FeePolicy::CustomerSurcharge);
        let none = calculate_payment_amounts(base, Decimal::ZERO, Decimal::ZERO, true);
        assert_eq!(none.policy, FeePolicy::None);
        assert_eq!(none.payable, base);
    }

    /// PAY-12: fixed fee + rate fee, rounded to two decimals.
    #[test]
    fn pay_12_fixed_fee() {
        let r = calculate_payment_amounts(Amount::from(100), dec("2.5"), dec("1"), true);
        assert_eq!(r.fee.to_string(), "3.50");
        assert_eq!(r.payable.to_string(), "103.50");
        // Wallet part excluded by the caller: 100 total − 60 wallet = 40 online.
        let partial = calculate_payment_amounts(
            required_online_amount(Amount::from(100), Amount::from(60)),
            dec("2.5"),
            Decimal::ZERO,
            true,
        );
        assert_eq!(partial.fee.to_string(), "1.00");
        // 0.333 rounds half away from zero.
        let r = calculate_payment_amounts(Amount::from_cents(1), dec("50"), Decimal::ZERO, false);
        assert_eq!(r.fee.to_string(), "0.01");
    }

    /// PAY-26: converted payments are compared in order currency.
    #[test]
    fn pay_26_covered_amount_uses_rate_snapshot() {
        let mut p = sample();
        p.amount = Amount::from_cents(11);
        p.currency = "GBP".into();
        p.provider_payload =
            json!({"exchange_rate": "0.11", "original_amount": "1", "original_currency": "CNY"})
                .as_object()
                .cloned()
                .unwrap_or_default();
        assert_eq!(
            covered_order_amount(&p, Amount::from_cents(11)),
            Amount::from(1)
        );
        assert_eq!(covered_order_amount(&p, Amount::ZERO), Amount::from(1));
        let mut plain = sample();
        plain.fee_amount = Amount::from(3);
        plain.fee_policy = "customer_surcharge".into();
        plain.amount = Amount::from(103);
        plain.provider_payload.clear();
        assert_eq!(
            covered_order_amount(&plain, Amount::ZERO),
            Amount::from(100)
        );
        plain.fee_policy = "merchant_absorbed".into();
        assert_eq!(
            covered_order_amount(&plain, Amount::ZERO),
            Amount::from(103)
        );
        plain.fee_policy = String::new();
        assert_eq!(
            covered_order_amount(&plain, Amount::ZERO),
            Amount::from(100)
        );
    }
}
