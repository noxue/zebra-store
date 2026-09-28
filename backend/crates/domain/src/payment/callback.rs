//! Callback fact validation and payment-row state transitions (PAY-04, PAY-09, PAY-10, PAY-44).
//!
//! These pure functions are applied twice: once before settlement and again on the
//! row-locked payment inside the settlement transaction.

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use zs_shared::money::Amount;

use super::model::Payment;
use super::types::{PAYLOAD_FIAT_CURRENCY_SENT, PaymentStatus, provider};
use crate::Id;

/// A verified gateway notification, ready to be applied to one payment (`PaymentCallbackInput`).
#[derive(Debug, Clone, PartialEq)]
pub struct CallbackInput {
    pub payment_id: Id,
    /// Merchant order number echoed by the gateway.
    pub order_no: String,
    /// Channel whose credentials verified the notification.
    pub channel_id: Id,
    pub status: PaymentStatus,
    pub provider_ref: String,
    pub amount: Amount,
    /// Upper-cased currency; empty when the gateway sent none.
    pub currency: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub payload: Map<String, Value>,
    /// Set only by the verified DujiaoPay webhook entry for pre-upgrade payments (PAY-04).
    pub verified_legacy_currency: String,
}

/// Why callback facts do not match the stored payment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactError {
    /// `error.payment_invalid` (channel or order number mismatch)
    Invalid,
    /// `error.payment_currency_mismatch`
    CurrencyMismatch,
    /// `error.payment_amount_mismatch`
    AmountMismatch,
}

/// `matchesBusinessOrderNo`: empty, the business number, or this payment's gateway order number.
pub fn matches_business_order_no(
    callback_order_no: &str,
    business_no: &str,
    payment: &Payment,
) -> bool {
    let callback_order_no = callback_order_no.trim();
    callback_order_no.is_empty()
        || callback_order_no == business_no.trim()
        || callback_order_no == payment.gateway_order_no.trim()
}

/// `validateCallbackPaymentFacts`: channel, order number, currency and exact amount checks.
/// Success callbacks must carry a currency and a positive amount.
pub fn validate_callback_facts(
    payment: &Payment,
    business_no: &str,
    input: &CallbackInput,
) -> Result<(), FactError> {
    if input.channel_id != 0 && input.channel_id != payment.channel_id {
        return Err(FactError::Invalid);
    }
    if !matches_business_order_no(&input.order_no, business_no, payment) {
        return Err(FactError::Invalid);
    }
    let currency = input.currency.trim();
    if input.status == PaymentStatus::Success {
        if currency.is_empty() {
            return Err(FactError::CurrencyMismatch);
        }
        if !input.amount.is_positive() {
            return Err(FactError::AmountMismatch);
        }
    }
    if !currency.is_empty()
        && !currency.eq_ignore_ascii_case(payment.currency.trim())
        && !can_adopt_legacy_currency(payment, input)
    {
        return Err(FactError::CurrencyMismatch);
    }
    if !input.amount.is_zero() && input.amount != payment.amount {
        return Err(FactError::AmountMismatch);
    }
    Ok(())
}

/// `canAdoptVerifiedLegacyDujiaoPayCurrency`: pre-upgrade DujiaoPay payments (no fiat snapshot)
/// may adopt the signed webhook currency on their first success.
pub fn can_adopt_legacy_currency(payment: &Payment, input: &CallbackInput) -> bool {
    if payment.provider_type != provider::DUJIAOPAY
        || payment.status == PaymentStatus::Success
        || input.status != PaymentStatus::Success
    {
        return false;
    }
    let verified = input.verified_legacy_currency.trim().to_ascii_uppercase();
    if verified.is_empty() || verified != input.currency.trim().to_ascii_uppercase() {
        return false;
    }
    if !input.amount.is_positive() || input.amount != payment.amount {
        return false;
    }
    !payment
        .provider_payload
        .contains_key(PAYLOAD_FIAT_CURRENCY_SENT)
}

fn adopt_legacy_currency(payment: &mut Payment, input: &CallbackInput) -> bool {
    if !can_adopt_legacy_currency(payment, input) {
        return false;
    }
    let currency = input.verified_legacy_currency.trim().to_ascii_uppercase();
    payment.currency.clone_from(&currency);
    payment.provider_payload.insert(
        PAYLOAD_FIAT_CURRENCY_SENT.to_owned(),
        Value::String(currency),
    );
    true
}

/// Shallow merge: incoming keys overwrite, other stored keys (e.g. `display_channel_type`) stay (PAY-44).
pub fn merge_provider_payload(
    existing: &Map<String, Value>,
    incoming: &Map<String, Value>,
) -> Map<String, Value> {
    let mut merged = existing.clone();
    for (k, v) in incoming {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// What [`apply_callback`] did to the payment row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallbackOutcome {
    /// Already successful or already in the target status: only metadata changed.
    Idempotent,
    /// The status changed; `first_success` is true when this is the first success
    /// (the only time orders may be fulfilled / wallets credited).
    Transitioned {
        previous: PaymentStatus,
        first_success: bool,
    },
}

/// Applies a validated callback to a (row-locked) payment, mirroring `updateCallbackMeta`
/// and the payment part of `applyPaymentUpdate`. Order-level effects are the caller's job.
pub fn apply_callback(
    payment: &mut Payment,
    input: &CallbackInput,
    now: DateTime<Utc>,
) -> CallbackOutcome {
    adopt_legacy_currency(payment, input);
    if payment.status == PaymentStatus::Success || payment.status == input.status {
        if !input.provider_ref.is_empty() && payment.provider_ref.is_empty() {
            payment.provider_ref.clone_from(&input.provider_ref);
        }
        // The original always passes a (possibly empty) payload map, so metadata is always touched.
        payment.provider_payload =
            merge_provider_payload(&payment.provider_payload, &input.payload);
        if payment.status == PaymentStatus::Success
            && payment.paid_at.is_none()
            && input.paid_at.is_some()
        {
            payment.paid_at = input.paid_at;
        }
        payment.callback_at = Some(now);
        payment.updated_at = now;
        return CallbackOutcome::Idempotent;
    }
    let previous = payment.status;
    match input.status {
        PaymentStatus::Success => payment.paid_at = Some(input.paid_at.unwrap_or(now)),
        PaymentStatus::Expired => payment.expired_at = Some(now),
        _ => {}
    }
    payment.status = input.status;
    payment.callback_at = Some(now);
    payment.updated_at = now;
    if !input.provider_ref.is_empty() {
        payment.provider_ref.clone_from(&input.provider_ref);
    }
    if !input.payload.is_empty() {
        payment.provider_payload =
            merge_provider_payload(&payment.provider_payload, &input.payload);
    }
    CallbackOutcome::Transitioned {
        previous,
        first_success: input.status == PaymentStatus::Success,
    }
}

/// Order state seen by a late/duplicate success (`applyPaymentUpdate` exception codes, PAY-02/PAY-03).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct OrderPaymentState {
    /// Order still `pending_payment` and unpaid.
    pub open: bool,
    /// Order already has `paid_at`.
    pub paid: bool,
    /// Payment covers less than the order's current online requirement.
    pub underpaid: bool,
}

/// Exception code recorded on a successful payment that must not fulfil the order.
pub fn success_exception_code(payment: &Payment, order: OrderPaymentState) -> Option<&'static str> {
    use super::types::exception;
    if order.underpaid {
        Some(exception::UNDERPAID_SUCCEEDED)
    } else if payment.superseded_at.is_some() {
        Some(exception::SUPERSEDED_SUCCEEDED)
    } else if !order.open {
        Some(if order.paid {
            exception::DUPLICATE_SUCCEEDED
        } else {
            exception::CLOSED_ORDER_SUCCEEDED
        })
    } else {
        None
    }
}

/// Applies a successful gateway creation to the payment (`applyProviderPayment`, PAY-33):
/// the provider reference falls back to the gateway order number, never the business number;
/// converted amounts/currencies are persisted as the callback reference (PAY-10/PAY-40).
pub fn apply_create_result(
    payment: &mut Payment,
    result: &super::gateway::GatewayCreateResult,
    provider_order_no: &str,
    now: DateTime<Utc>,
) {
    payment.pay_url = result.redirect_url.trim().to_owned();
    payment.qr_code = result.qr_code_url.trim().to_owned();
    if !result.provider_ref.is_empty() {
        payment.provider_ref.clone_from(&result.provider_ref);
    }
    if payment.provider_ref.is_empty() {
        payment.provider_ref = provider_order_no.to_owned();
    }
    if !result.payload.is_empty() {
        payment.provider_payload = result.payload.clone();
    }
    let display = result.display_channel_type.trim();
    if !display.is_empty() {
        payment.provider_payload.insert(
            "display_channel_type".to_owned(),
            Value::String(display.to_owned()),
        );
    }
    if !result.currency_sent.trim().is_empty() {
        payment.currency = result.currency_sent.trim().to_owned();
    }
    if let Ok(amount) = result.amount_sent.trim().parse::<Amount>()
        && !result.amount_sent.trim().is_empty()
    {
        payment.amount = amount;
    }
    payment.status = PaymentStatus::Pending;
    payment.updated_at = now;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payment::gateway::GatewayCreateResult;
    use crate::payment::model::tests::sample;
    use serde_json::json;

    fn input(status: PaymentStatus, amount: i64, currency: &str) -> CallbackInput {
        CallbackInput {
            payment_id: 7,
            order_no: "DJP1".into(),
            channel_id: 2,
            status,
            provider_ref: "T1".into(),
            amount: Amount::from_cents(amount),
            currency: currency.into(),
            paid_at: None,
            payload: Map::new(),
            verified_legacy_currency: String::new(),
        }
    }

    /// PAY-04: strict fact validation.
    #[test]
    fn pay_04_fact_validation() {
        let p = sample();
        assert_eq!(
            validate_callback_facts(&p, "DJ1", &input(PaymentStatus::Success, 1000, "cny")),
            Ok(())
        );
        assert_eq!(
            validate_callback_facts(&p, "DJ1", &input(PaymentStatus::Success, 1000, "")),
            Err(FactError::CurrencyMismatch)
        );
        assert_eq!(
            validate_callback_facts(&p, "DJ1", &input(PaymentStatus::Success, 0, "CNY")),
            Err(FactError::AmountMismatch)
        );
        let mut other_channel = input(PaymentStatus::Success, 1000, "CNY");
        other_channel.channel_id = 3;
        assert_eq!(
            validate_callback_facts(&p, "DJ1", &other_channel),
            Err(FactError::Invalid)
        );
        // Pending callbacks may omit amount and currency.
        assert_eq!(
            validate_callback_facts(&p, "DJ1", &input(PaymentStatus::Pending, 0, "")),
            Ok(())
        );
    }

    /// PAY-09: order number matching accepts business and gateway numbers only.
    #[test]
    fn pay_09_order_no_matching() {
        let p = sample();
        let mut i = input(PaymentStatus::Success, 1000, "CNY");
        i.order_no = "DJ1".into();
        assert_eq!(validate_callback_facts(&p, "DJ1", &i), Ok(()));
        i.order_no = String::new();
        assert_eq!(validate_callback_facts(&p, "DJ1", &i), Ok(()));
        i.order_no = "DJP999".into();
        assert_eq!(
            validate_callback_facts(&p, "DJ1", &i),
            Err(FactError::Invalid)
        );
    }

    /// PAY-10: converted payments are checked against the converted snapshot.
    #[test]
    fn pay_10_converted_amounts() {
        let mut p = sample();
        p.amount = Amount::from(72);
        p.currency = "CNY".into();
        assert_eq!(
            validate_callback_facts(&p, "", &input(PaymentStatus::Success, 7200, "CNY")),
            Ok(())
        );
        assert_eq!(
            validate_callback_facts(&p, "", &input(PaymentStatus::Success, 1000, "CNY")),
            Err(FactError::AmountMismatch)
        );
        assert_eq!(
            validate_callback_facts(&p, "", &input(PaymentStatus::Success, 7200, "USD")),
            Err(FactError::CurrencyMismatch)
        );
    }

    /// PAY-04 (legacy DujiaoPay currency adoption only for verified pre-upgrade payments).
    #[test]
    fn pay_04_legacy_dujiaopay_currency() {
        let mut p = sample();
        p.provider_type = "dujiaopay".into();
        p.provider_payload.clear();
        let mut i = input(PaymentStatus::Success, 1000, "USD");
        assert_eq!(
            validate_callback_facts(&p, "", &i),
            Err(FactError::CurrencyMismatch)
        );
        i.verified_legacy_currency = "usd".into();
        assert_eq!(validate_callback_facts(&p, "", &i), Ok(()));
        let outcome = apply_callback(&mut p, &i, Utc::now());
        assert!(matches!(
            outcome,
            CallbackOutcome::Transitioned {
                first_success: true,
                ..
            }
        ));
        assert_eq!(p.currency, "USD");
        assert_eq!(p.provider_payload[PAYLOAD_FIAT_CURRENCY_SENT], "USD");
        let mut snap = sample();
        snap.provider_type = "dujiaopay".into();
        snap.provider_payload
            .insert(PAYLOAD_FIAT_CURRENCY_SENT.into(), json!("CNY"));
        assert_eq!(
            validate_callback_facts(&snap, "", &i),
            Err(FactError::CurrencyMismatch)
        );
    }

    /// PAY-04 / PAY-44: idempotent success keeps state; payloads merge.
    #[test]
    fn pay_44_apply_callback_merges_payload() {
        let mut p = sample();
        let mut i = input(PaymentStatus::Success, 1000, "CNY");
        i.payload = json!({"trade_id": "T1"})
            .as_object()
            .cloned()
            .unwrap_or_default();
        let now = Utc::now();
        assert!(matches!(
            apply_callback(&mut p, &i, now),
            CallbackOutcome::Transitioned {
                previous: PaymentStatus::Pending,
                first_success: true
            }
        ));
        assert_eq!(p.provider_payload["display_channel_type"], "usdt.arbitrum");
        assert_eq!(p.provider_payload["trade_id"], "T1");
        assert_eq!(p.paid_at, Some(now));
        // A later failed callback never downgrades a success.
        let failed = input(PaymentStatus::Failed, 0, "");
        assert_eq!(
            apply_callback(&mut p, &failed, now),
            CallbackOutcome::Idempotent
        );
        assert_eq!(p.status, PaymentStatus::Success);
        let expired = input(PaymentStatus::Expired, 0, "");
        let mut q = sample();
        apply_callback(&mut q, &expired, now);
        assert_eq!(q.expired_at, Some(now));
    }

    /// PAY-02 / PAY-03: exception codes for late or duplicate successes.
    #[test]
    fn pay_03_exception_codes() {
        let p = sample();
        let open = OrderPaymentState {
            open: true,
            ..OrderPaymentState::default()
        };
        assert_eq!(success_exception_code(&p, open), None);
        assert_eq!(
            success_exception_code(
                &p,
                OrderPaymentState {
                    underpaid: true,
                    ..open
                }
            ),
            Some("underpaid_payment_succeeded")
        );
        let mut superseded = sample();
        superseded.superseded_at = Some(Utc::now());
        assert_eq!(
            success_exception_code(&superseded, open),
            Some("superseded_payment_succeeded")
        );
        assert_eq!(
            success_exception_code(
                &p,
                OrderPaymentState {
                    open: false,
                    paid: true,
                    underpaid: false
                }
            ),
            Some("duplicate_payment_succeeded")
        );
        assert_eq!(
            success_exception_code(&p, OrderPaymentState::default()),
            Some("closed_order_payment_succeeded")
        );
    }

    /// PAY-33 / PAY-08: provider ref falls back to the gateway order number; sent amounts persist.
    #[test]
    fn pay_33_apply_create_result() {
        let mut p = sample();
        p.provider_ref = String::new();
        let result = GatewayCreateResult {
            redirect_url: " https://pay ".into(),
            amount_sent: "616.00000000".into(),
            currency_sent: "USDT".into(),
            display_channel_type: "usdt.trc20".into(),
            ..GatewayCreateResult::default()
        };
        apply_create_result(&mut p, &result, "DJP20260924", Utc::now());
        assert_eq!(p.provider_ref, "DJP20260924");
        assert_eq!(p.pay_url, "https://pay");
        assert_eq!(p.amount, Amount::from(616));
        assert_eq!(p.currency, "USDT");
        assert_eq!(p.provider_payload["display_channel_type"], "usdt.trc20");
        assert_eq!(p.status, PaymentStatus::Pending);
    }
}
