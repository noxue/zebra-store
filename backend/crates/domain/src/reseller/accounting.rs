//! Profit ledger arithmetic: refund claw-back (RSL-01/04/05), balance caches
//! (RSL-02/04) and withdrawal allocation (RSL-02/05).
//!
//! Every function is pure; `zs-infra` runs them inside one transaction together
//! with the rows they read.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::Value;
use zs_shared::money::Amount;

use super::keys;
use super::model::{BalanceStatus, LedgerStatus};
use crate::{Error, Id, Result};

/// Upper bound of `reseller.settlement_confirm_days` (RSL-04).
pub const MAX_CONFIRM_DAYS: i64 = 3650;

/// Clamps the configured confirmation window to `0..=3650` days.
pub fn clamp_confirm_days(days: i64) -> i64 {
    days.clamp(0, MAX_CONFIRM_DAYS)
}

/// When a profit posted at `now` becomes available.
pub fn available_at(now: DateTime<Utc>, confirm_days: i64) -> DateTime<Utc> {
    now + Duration::days(clamp_confirm_days(confirm_days))
}

/// Idempotency key of an order's profit entry.
pub fn profit_key(order_id: Id) -> String {
    format!("order_profit:{order_id}")
}

/// Idempotency key of a refund claw-back entry.
pub fn refund_key(refund_record_id: Id) -> String {
    format!("refund_deduct:{refund_record_id}")
}

/// Idempotency key of the remainder row created when a withdrawal splits an entry.
pub fn split_key(row_id: Id, nanos: i64) -> String {
    format!("split:{row_id}:{nanos}")
}

fn round2(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Inputs of a refund claw-back.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefundDeductInput {
    /// Snapshot profit of the order.
    pub profit: Decimal,
    /// Order amount the ratio is taken against (snapshot reseller amount, else order total).
    pub order_amount: Decimal,
    /// This refund.
    pub refund_amount: Decimal,
    /// Refunded before this refund.
    pub refunded_before: Decimal,
    /// Sum of earlier `refund_deduct` entries of the order (negative or zero).
    pub deducted_so_far: Decimal,
}

/// Result of [`refund_deduction`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefundDeduction {
    /// Positive amount to claw back (stored negated).
    pub deduct: Decimal,
    /// Refund amount after clamping to the remaining refundable amount.
    pub refund_amount: Decimal,
    /// `refund / order_amount`.
    pub ratio: Decimal,
    /// `deduct / profit`, used to split the deduction over the snapshot items.
    pub allocation_ratio: Decimal,
}

/// Claw-back of one refund (RSL-01): the ratio always uses the full order amount as
/// denominator, the cumulative deduction is capped at the profit, and a refund that
/// completes the order takes exactly the remaining profit. `None` = nothing to deduct.
pub fn refund_deduction(input: RefundDeductInput) -> Result<Option<RefundDeduction>> {
    let profit = round2(input.profit);
    let mut refund = round2(input.refund_amount);
    if refund <= Decimal::ZERO || profit <= Decimal::ZERO {
        return Ok(None);
    }
    let order_amount = round2(input.order_amount);
    if order_amount <= Decimal::ZERO {
        return Err(Error::internal_msg("reseller ledger invalid snapshot"));
    }
    let refunded_before = round2(input.refunded_before);
    let remaining_before = round2(order_amount - refunded_before);
    if remaining_before <= Decimal::ZERO {
        return Ok(None);
    }
    if refund > remaining_before {
        refund = remaining_before;
    }
    let remaining_profit = round2(profit - round2(input.deducted_so_far.abs()));
    if remaining_profit <= Decimal::ZERO {
        return Ok(None);
    }
    let ratio = refund / order_amount;
    let mut deduct = round2(profit * ratio);
    let fully_refunded = refunded_before + refund >= order_amount;
    if fully_refunded || deduct > remaining_profit {
        deduct = remaining_profit;
    }
    if deduct <= Decimal::ZERO {
        return Ok(None);
    }
    Ok(Some(RefundDeduction {
        deduct,
        refund_amount: refund,
        ratio,
        allocation_ratio: deduct / profit,
    }))
}

/// Splits a deduction over the pricing-snapshot items (`refund_allocation_json.items`).
pub fn allocate_refund(pricing_snapshot: &Value, deduction: &RefundDeduction) -> Vec<Value> {
    let Some(items) = pricing_snapshot.get("items").and_then(Value::as_array) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let item_profit = decimal_of(item.get("profit_amount"));
            let item_deduct = round2(item_profit * deduction.allocation_ratio);
            if item_deduct <= Decimal::ZERO {
                return None;
            }
            Some(serde_json::json!({
                "order_item_id": id_string(item.get("order_item_id")),
                "refund_ratio": fixed(deduction.ratio, 8),
                "original_profit_amount": fixed(item_profit, 2),
                "deduct_amount": fixed(item_deduct, 2),
            }))
        })
        .collect()
}

/// `value.StringFixed(places)`.
pub fn fixed(v: Decimal, places: u32) -> String {
    let mut r = v.round_dp_with_strategy(places, RoundingStrategy::MidpointAwayFromZero);
    r.rescale(places);
    r.to_string()
}

/// Reads a snapshot amount stored as string or number.
pub fn decimal_of(v: Option<&Value>) -> Decimal {
    match v {
        Some(Value::String(s)) => s.trim().parse().map(round2).unwrap_or_default(),
        Some(Value::Number(n)) => n.to_string().parse().map(round2).unwrap_or_default(),
        _ => Decimal::ZERO,
    }
}

fn id_string(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Number(n)) => n.as_i64().map_or_else(|| n.to_string(), |i| i.to_string()),
        _ => String::new(),
    }
}

/// Status of a claw-back entry mirrors the order's profit entry (RSL-04): a refund
/// inside the confirmation window is `pending_confirm` with the profit's
/// `available_at`, otherwise it is immediately `available`.
pub fn deduct_status(
    profit_status: Option<LedgerStatus>,
    profit_available_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    confirm_days: i64,
) -> (LedgerStatus, Option<DateTime<Utc>>) {
    if profit_status == Some(LedgerStatus::PendingConfirm) {
        let at = profit_available_at.unwrap_or_else(|| available_at(now, confirm_days));
        (LedgerStatus::PendingConfirm, Some(at))
    } else {
        (LedgerStatus::Available, None)
    }
}

/// Balance cache derived from the ledger (`RefreshBalanceAccount`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalanceCache {
    pub available: Amount,
    pub locked: Amount,
    pub negative: Amount,
    pub status: BalanceStatus,
}

/// Recomputes the caches from the sums of `available` and `locked` entries (RSL-02:
/// withdrawn entries are not subtracted again). A negative net marks the account
/// `negative_balance`; recovering clears only that status (frozen / disabled stay).
pub fn balance_cache(
    current: BalanceStatus,
    available_sum: Decimal,
    locked_sum: Decimal,
) -> BalanceCache {
    let net = round2(available_sum);
    let (negative, status) = if net < Decimal::ZERO {
        (round2(net.abs()), BalanceStatus::NegativeBalance)
    } else if current == BalanceStatus::NegativeBalance {
        (Decimal::ZERO, BalanceStatus::Normal)
    } else {
        (Decimal::ZERO, current)
    };
    BalanceCache {
        available: Amount::new(net),
        locked: Amount::new(locked_sum),
        negative: Amount::new(negative),
        status,
    }
}

/// Accounts in these states cannot withdraw.
pub fn balance_blocks_withdraw(status: BalanceStatus) -> bool {
    matches!(
        status,
        BalanceStatus::NegativeBalance | BalanceStatus::FrozenReview | BalanceStatus::Disabled
    )
}

/// A validated withdrawal request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawDraft {
    pub amount: Amount,
    pub currency: String,
    pub channel: String,
    pub account: String,
}

/// Validates withdrawal input (amount > 0, currency, channel and account required).
pub fn withdraw_draft(
    amount: Decimal,
    currency: &str,
    channel: &str,
    account: &str,
) -> Result<WithdrawDraft> {
    let amount = round2(amount);
    if amount <= Decimal::ZERO {
        return Err(Error::bad_request(keys::WITHDRAW_AMOUNT_INVALID));
    }
    let currency = currency.trim();
    if currency.is_empty() {
        return Err(Error::bad_request(keys::WITHDRAW_CURRENCY_UNAVAILABLE));
    }
    let (channel, account) = (channel.trim(), account.trim());
    if channel.is_empty() || account.is_empty() {
        return Err(Error::bad_request(keys::WITHDRAW_AMOUNT_INVALID));
    }
    Ok(WithdrawDraft {
        amount: Amount::new(amount),
        currency: currency.to_owned(),
        channel: channel.to_owned(),
        account: account.to_owned(),
    })
}

/// Which available entries a withdrawal locks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WithdrawPlan {
    /// Entries locked whole.
    pub lock_ids: Vec<Id>,
    /// Entry split: `(id, locked part, remainder kept available)`.
    pub split: Option<(Id, Amount, Amount)>,
}

/// Allocates `amount` over positive available entries (oldest first). The net
/// available sum — which includes negative claw-backs — is checked first (RSL-02),
/// so positive rows alone can never cover more than the real balance.
pub fn plan_withdraw(
    amount: Amount,
    net_available: Decimal,
    rows: &[(Id, Amount)],
) -> Result<WithdrawPlan> {
    let insufficient = || Error::bad_request(keys::WITHDRAW_INSUFFICIENT);
    if amount.decimal() > round2(net_available) {
        return Err(insufficient());
    }
    let mut remaining = amount.decimal();
    let mut plan = WithdrawPlan::default();
    for &(id, row_amount) in rows {
        if remaining <= Decimal::ZERO {
            break;
        }
        let row = row_amount.decimal();
        if row <= Decimal::ZERO {
            continue;
        }
        if row <= remaining {
            plan.lock_ids.push(id);
            remaining = round2(remaining - row);
            continue;
        }
        plan.split = Some((id, Amount::new(remaining), Amount::new(row - remaining)));
        plan.lock_ids.push(id);
        remaining = Decimal::ZERO;
        break;
    }
    if remaining > Decimal::ZERO {
        return Err(insufficient());
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::rules::fixtures::{amt, at, d};

    fn deduct(
        profit: &str,
        order: &str,
        refund: &str,
        before: &str,
        so_far: &str,
    ) -> Option<Decimal> {
        refund_deduction(RefundDeductInput {
            profit: d(profit),
            order_amount: d(order),
            refund_amount: d(refund),
            refunded_before: d(before),
            deducted_so_far: d(so_far),
        })
        .unwrap_or_else(|e| panic!("{e}"))
        .map(|r| r.deduct)
    }

    /// Runs refunds in sequence and returns each deduction.
    fn sequence(profit: &str, order: &str, refunds: &[&str]) -> Vec<Decimal> {
        let (mut before, mut so_far) = (Decimal::ZERO, Decimal::ZERO);
        refunds
            .iter()
            .map(|r| {
                let got = deduct(profit, order, r, &before.to_string(), &so_far.to_string())
                    .unwrap_or_default();
                before += d(r);
                so_far -= got;
                got
            })
            .collect()
    }

    // RSL-01: order 130 / profit 30, refunds 52 then 78 → total 30 (not 42).
    #[test]
    fn rsl01_partial_refunds_never_over_deduct() {
        let got = sequence("30", "130", &["52", "78"]);
        assert_eq!(got, vec![d("12.00"), d("18.00")]);
        assert_eq!(got.iter().sum::<Decimal>(), d("30.00"));
        // one full refund
        assert_eq!(sequence("30", "130", &["130"]), vec![d("30.00")]);
        // three thirds converge on the profit exactly
        let got = sequence("30", "130", &["43.33", "43.33", "43.34"]);
        assert_eq!(got.iter().sum::<Decimal>(), d("30.00"));
        // a further refund deducts nothing
        assert_eq!(deduct("30", "130", "10", "130", "-30"), None);
        // profit already consumed
        assert_eq!(deduct("30", "130", "10", "0", "-30"), None);
    }

    // RSL-05: two half refunds claw back exactly the profit.
    #[test]
    fn rsl05_two_halves_equal_profit() {
        let got = sequence("30", "130", &["65", "65"]);
        assert_eq!(got, vec![d("15.00"), d("15.00")]);
    }

    #[test]
    fn refund_is_clamped_to_remaining_amount() {
        let r = refund_deduction(RefundDeductInput {
            profit: d("30"),
            order_amount: d("130"),
            refund_amount: d("500"),
            refunded_before: d("100"),
            deducted_so_far: d("-23.08"),
        })
        .unwrap_or_default();
        let r = r.unwrap_or_else(|| panic!("expected a deduction"));
        assert_eq!(r.refund_amount, d("30.00"));
        assert_eq!(r.deduct, d("6.92"));
        assert!(
            refund_deduction(RefundDeductInput {
                profit: d("30"),
                order_amount: d("0"),
                refund_amount: d("1"),
                refunded_before: d("0"),
                deducted_so_far: d("0"),
            })
            .is_err()
        );
    }

    #[test]
    fn allocation_follows_items() {
        let snap = serde_json::json!({"items": [
            {"order_item_id": 7, "profit_amount": "20.00"},
            {"order_item_id": "8", "profit_amount": 10},
        ]});
        let r = refund_deduction(RefundDeductInput {
            profit: d("30"),
            order_amount: d("130"),
            refund_amount: d("65"),
            refunded_before: d("0"),
            deducted_so_far: d("0"),
        })
        .unwrap_or_default()
        .unwrap_or_else(|| panic!("expected a deduction"));
        let items = allocate_refund(&snap, &r);
        assert_eq!(items[0]["deduct_amount"], "10.00");
        assert_eq!(items[0]["order_item_id"], "7");
        assert_eq!(items[1]["deduct_amount"], "5.00");
        assert_eq!(items[1]["refund_ratio"], "0.50000000");
    }

    // RSL-04: claw-back inside the confirmation window stays pending with the profit's date.
    #[test]
    fn rsl04_deduct_status_follows_profit() {
        let now = at(0);
        let due = at(7 * 86_400);
        assert_eq!(
            deduct_status(Some(LedgerStatus::PendingConfirm), Some(due), now, 7),
            (LedgerStatus::PendingConfirm, Some(due))
        );
        assert_eq!(
            deduct_status(Some(LedgerStatus::PendingConfirm), None, now, 3),
            (LedgerStatus::PendingConfirm, Some(at(3 * 86_400)))
        );
        assert_eq!(
            deduct_status(Some(LedgerStatus::Available), None, now, 7),
            (LedgerStatus::Available, None)
        );
        assert_eq!(
            deduct_status(None, None, now, 7),
            (LedgerStatus::Available, None)
        );
    }

    // RSL-04: confirm days clamped to 0..=3650.
    #[test]
    fn rsl04_confirm_days_clamped() {
        assert_eq!(clamp_confirm_days(-5), 0);
        assert_eq!(clamp_confirm_days(99_999), 3650);
        assert_eq!(clamp_confirm_days(7), 7);
        assert_eq!(available_at(at(0), -5), at(0));
    }

    // RSL-02: caches come from available/locked sums only.
    #[test]
    fn rsl02_balance_cache() {
        let c = balance_cache(BalanceStatus::Normal, d("35"), d("0"));
        assert_eq!(
            (c.available, c.locked, c.negative, c.status),
            (amt("35"), amt("0"), amt("0"), BalanceStatus::Normal)
        );
        let c = balance_cache(BalanceStatus::Normal, d("-15"), d("25"));
        assert_eq!(
            (c.available, c.negative, c.status),
            (amt("-15"), amt("15"), BalanceStatus::NegativeBalance)
        );
        let c = balance_cache(BalanceStatus::NegativeBalance, d("0"), d("0"));
        assert_eq!(c.status, BalanceStatus::Normal);
        let c = balance_cache(BalanceStatus::FrozenReview, d("10"), d("0"));
        assert_eq!(c.status, BalanceStatus::FrozenReview);
        assert!(balance_blocks_withdraw(BalanceStatus::NegativeBalance));
        assert!(!balance_blocks_withdraw(BalanceStatus::Normal));
    }

    // RSL-02: +100 / -50 available → 80 rejected, 50 accepted.
    #[test]
    fn rsl02_withdraw_checks_net_available() {
        let rows = [(1, amt("100")), (2, amt("-50"))];
        assert_eq!(
            plan_withdraw(amt("80"), d("50"), &rows).unwrap_err().key(),
            keys::WITHDRAW_INSUFFICIENT
        );
        let plan = plan_withdraw(amt("50"), d("50"), &rows).unwrap_or_default();
        assert_eq!(plan.lock_ids, vec![1]);
        assert_eq!(plan.split, Some((1, amt("50"), amt("50"))));
    }

    // RSL-05: 30 + 50 available, withdraw 60 → 30 locked, 50 split into 30 + 20.
    #[test]
    fn rsl05_withdraw_split() {
        let plan = plan_withdraw(amt("60"), d("80"), &[(1, amt("30")), (2, amt("50"))])
            .unwrap_or_default();
        assert_eq!(plan.lock_ids, vec![1, 2]);
        assert_eq!(plan.split, Some((2, amt("30"), amt("20"))));
        let exact = plan_withdraw(amt("80"), d("80"), &[(1, amt("30")), (2, amt("50"))])
            .unwrap_or_default();
        assert_eq!((exact.lock_ids.len(), exact.split), (2, None));
    }

    #[test]
    fn withdraw_input_validation() {
        assert_eq!(
            withdraw_draft(d("0"), "CNY", "alipay", "a")
                .unwrap_err()
                .key(),
            keys::WITHDRAW_AMOUNT_INVALID
        );
        assert_eq!(
            withdraw_draft(d("1"), " ", "alipay", "a")
                .unwrap_err()
                .key(),
            keys::WITHDRAW_CURRENCY_UNAVAILABLE
        );
        assert_eq!(
            withdraw_draft(d("1"), "CNY", "", "a").unwrap_err().key(),
            keys::WITHDRAW_AMOUNT_INVALID
        );
        let ok = withdraw_draft(d("1.005"), " CNY ", " alipay ", " a@b ")
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (
                ok.amount.to_string(),
                ok.currency.as_str(),
                ok.account.as_str()
            ),
            ("1.01".into(), "CNY", "a@b")
        );
    }

    #[test]
    fn keys_format() {
        assert_eq!(profit_key(5), "order_profit:5");
        assert_eq!(refund_key(9), "refund_deduct:9");
        assert_eq!(split_key(3, 42), "split:3:42");
        assert_eq!(fixed(d("0.5"), 8), "0.50000000");
    }
}
