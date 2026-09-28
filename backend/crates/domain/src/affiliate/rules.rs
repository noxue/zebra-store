//! Pure affiliate rules: codes, commission calculation and schedule, refund clawback,
//! withdrawal allocation and validation.

use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use zs_shared::money::Amount;

use super::model::status;
use super::ports::CommissionItem;
use crate::{Error, Id};

/// Length of generated affiliate codes (original `affiliateCodeLength`).
pub const CODE_LENGTH: usize = 8;
/// Alphabet of generated codes (no 0/O/1/I).
pub const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
/// Longest stored code (`varchar(32)`).
pub const CODE_MAX_LEN: usize = 32;
/// Attempts to find a free code (original `maxRetry`).
pub const CODE_MAX_RETRY: usize = 8;
/// Visitor-key attribution window (original `affiliateAttributionWindow`, 30 days).
pub const ATTRIBUTION_WINDOW_DAYS: i64 = 30;
/// Duplicate-click window (original `affiliateClickDedupeWindow`, 10 minutes).
pub const CLICK_DEDUPE_MINUTES: i64 = 10;
/// Prefix of split commission types (original `affiliateSplitTypePrefix`).
pub const SPLIT_TYPE_PREFIX: &str = "sp";
/// `commission_type` column width.
const COMMISSION_TYPE_MAX_LEN: usize = 20;
/// Modulus of the split-type suffix (original `UnixNano() % 1000000`).
const SPLIT_SUFFIX_MODULUS: i64 = 1_000_000;

/// Converts a settings float through its shortest decimal form (like Go's
/// `decimal.NewFromFloat`), avoiding binary artefacts such as `12.3449999…`.
pub fn decimal_of(value: f64) -> Decimal {
    if !value.is_finite() {
        return Decimal::ZERO;
    }
    value.to_string().parse().unwrap_or_default()
}

/// Trims a code and caps it at the column width (`NormalizeCode`).
pub fn normalize_code(raw: &str) -> String {
    let code = raw.trim();
    let mut end = code.len().min(CODE_MAX_LEN);
    while !code.is_char_boundary(end) {
        end -= 1;
    }
    code[..end].to_owned()
}

/// Builds a code from random bytes (each byte picks one alphabet letter).
pub fn code_from_random(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(CODE_LENGTH)
        .map(|b| char::from(CODE_ALPHABET[usize::from(*b) % CODE_ALPHABET.len()]))
        .collect()
}

/// Valid orders / clicks in percent, rounded to two decimals (`calcAffiliateConversion`).
pub fn conversion_rate(valid_orders: i64, clicks: i64) -> f64 {
    if clicks <= 0 || valid_orders <= 0 {
        return 0.0;
    }
    let value = valid_orders as f64 / clicks as f64 * 100.0;
    (value * 100.0).round() / 100.0
}

/// Commission base: payable amount (`total_price − coupon_discount`, never negative) of
/// the affiliate-enabled items (`calculateCommissionBaseAmount`).
pub fn commission_base(items: &[CommissionItem]) -> Amount {
    items
        .iter()
        .filter(|i| i.affiliate_enabled)
        .map(|i| (i.total_price - i.coupon_discount).non_negative())
        .sum()
}

/// Rate rounded to two decimals and the commission `round2(base × rate / 100)`.
pub fn commission_amount(base: Amount, rate_percent: f64) -> (Amount, Amount) {
    let rate = Amount::new(decimal_of(rate_percent));
    let amount = Amount::new(base.decimal() * rate.decimal() / Decimal::ONE_HUNDRED);
    (rate, amount)
}

/// Initial status and dates of a new commission: `available` at once when `confirm_days`
/// is 0, otherwise `pending_confirm` until `paid_at + confirm_days`.
pub fn commission_schedule(
    paid_at: DateTime<Utc>,
    confirm_days: i64,
) -> (&'static str, Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    if confirm_days <= 0 {
        (status::COMMISSION_AVAILABLE, None, Some(paid_at))
    } else {
        (
            status::COMMISSION_PENDING_CONFIRM,
            Some(paid_at + Duration::days(confirm_days)),
            None,
        )
    }
}

/// Refund window of an order: the delta capped to what is still unrefunded, and the
/// remaining unrefunded amount before this refund. `None` when nothing is left to claw back.
pub fn refund_window(
    total: Amount,
    refunded_before: Amount,
    delta: Amount,
) -> Option<(Amount, Amount)> {
    if !delta.is_positive() || !total.is_positive() {
        return None;
    }
    let before = refunded_before.non_negative().min(total);
    let remaining = total - before;
    if !remaining.is_positive() {
        return None;
    }
    Some((delta.min(remaining), remaining))
}

/// Proportional clawback of one commission (`HandleOrderRefunded`): the commission and
/// base shrink by `delta / remaining`; a commission reaching zero is rejected.
/// Returns `(next_commission, next_base, rejected)`.
pub fn clawback(
    commission: Amount,
    base: Amount,
    delta: Amount,
    remaining: Amount,
) -> (Amount, Amount, bool) {
    if !commission.is_positive() {
        return (commission, base, true);
    }
    let ratio = |v: Amount| Amount::new(v.decimal() * delta.decimal() / remaining.decimal());
    let next = (commission - ratio(commission)).non_negative();
    let next_base = if base.is_positive() {
        (base - ratio(base)).non_negative()
    } else {
        base
    };
    (next, next_base, !next.is_positive())
}

/// How a withdrawal is covered by available commissions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawPlan {
    /// Commissions bound to the request (the split one included).
    pub selected: Vec<Id>,
    /// `(commission id, bound amount, remainder)` when the last commission is split.
    pub split: Option<(Id, Amount, Amount)>,
}

/// Allocates available commissions (oldest first) to `amount`, splitting the last one
/// so nothing beyond the request is frozen. `None` when the balance is insufficient.
pub fn plan_withdraw(commissions: &[(Id, Amount)], amount: Amount) -> Option<WithdrawPlan> {
    let mut remaining = amount;
    let mut plan = WithdrawPlan {
        selected: Vec::new(),
        split: None,
    };
    for (id, row) in commissions {
        if !remaining.is_positive() {
            break;
        }
        if !row.is_positive() {
            continue;
        }
        if *row <= remaining {
            plan.selected.push(*id);
            remaining -= *row;
            continue;
        }
        plan.split = Some((*id, remaining, *row - remaining));
        plan.selected.push(*id);
        remaining = Amount::ZERO;
        break;
    }
    (!remaining.is_positive()).then_some(plan)
}

/// Commission type of the remainder row of a split (`buildSplitCommissionType`).
pub fn split_type(source_id: Id, now: DateTime<Utc>) -> String {
    let nanos = now
        .timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp_micros());
    let suffix = nanos.rem_euclid(SPLIT_SUFFIX_MODULUS);
    let mut out = format!("{SPLIT_TYPE_PREFIX}{}{suffix}", to_base36(source_id));
    out.truncate(COMMISSION_TYPE_MAX_LEN);
    out
}

fn to_base36(value: Id) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut n = u64::try_from(value).unwrap_or_default();
    if n == 0 {
        return "0".to_owned();
    }
    let mut out = Vec::new();
    while n > 0 {
        // `n % 36` is always a valid index.
        out.push(DIGITS[usize::try_from(n % 36).unwrap_or_default()]);
        n /= 36;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Validates a withdrawal request (`ApplyWithdraw` checks before the transaction):
/// positive amount of at least `min_amount`, channel and account present, channel in the
/// configured list (case-insensitive) when one is configured. All failures are `bad_request`.
pub fn validate_withdraw(
    amount: Amount,
    min_amount: f64,
    channel: &str,
    account: &str,
    channels: &[String],
) -> Result<(), Error> {
    if !amount.is_positive() {
        return Err(Error::invalid());
    }
    let min = Amount::new(decimal_of(min_amount));
    if amount < min {
        return Err(Error::invalid());
    }
    let channel = channel.trim();
    if channel.is_empty() || account.trim().is_empty() {
        return Err(Error::invalid());
    }
    let target = channel.to_lowercase();
    if !channels.is_empty() && !channels.iter().any(|c| c.trim().to_lowercase() == target) {
        return Err(Error::invalid());
    }
    Ok(())
}

/// `active` / `disabled` only.
pub fn parse_profile_status(raw: &str) -> Option<&'static str> {
    match raw.trim() {
        status::PROFILE_ACTIVE => Some(status::PROFILE_ACTIVE),
        status::PROFILE_DISABLED => Some(status::PROFILE_DISABLED),
        _ => None,
    }
}

/// Deduplicates ids and drops non-positive ones, keeping order.
pub fn normalize_ids(ids: &[Id]) -> Vec<Id> {
    let mut out: Vec<Id> = Vec::with_capacity(ids.len());
    for id in ids {
        if *id > 0 && !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn amt(v: &str) -> Amount {
        v.parse().unwrap_or_default()
    }

    fn item(total: &str, coupon: &str, enabled: bool) -> CommissionItem {
        CommissionItem {
            product_id: 1,
            total_price: amt(total),
            coupon_discount: amt(coupon),
            affiliate_enabled: enabled,
        }
    }

    #[test]
    fn codes() {
        assert_eq!(normalize_code("  abc "), "abc");
        assert_eq!(normalize_code(&"x".repeat(40)).len(), 32);
        let code = code_from_random(&[0, 1, 31, 32, 255, 7, 8, 9, 10]);
        assert_eq!(code, "AB9A9HJK");
        assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)));
    }

    #[test]
    fn conversion_and_commission() {
        assert!((conversion_rate(1, 3) - 33.33).abs() < f64::EPSILON);
        assert!(conversion_rate(0, 3).abs() < f64::EPSILON);
        assert!(conversion_rate(3, 0).abs() < f64::EPSILON);
        // Only affiliate-enabled items count; coupons reduce the base, never below zero.
        let base = commission_base(&[
            item("100.00", "10.00", true),
            item("50.00", "0", false),
            item("5.00", "8.00", true),
        ]);
        assert_eq!(base, amt("90.00"));
        assert_eq!(
            commission_amount(amt("90.00"), 12.345),
            (amt("12.35"), amt("11.12"))
        );
        assert_eq!(commission_amount(amt("0.01"), 10.0).1, Amount::ZERO);
    }

    #[test]
    fn schedule() {
        let paid = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(
            commission_schedule(paid, 0),
            (status::COMMISSION_AVAILABLE, None, Some(paid))
        );
        let (s, confirm, avail) = commission_schedule(paid, 7);
        assert_eq!(s, status::COMMISSION_PENDING_CONFIRM);
        assert_eq!(
            confirm,
            Some(Utc.with_ymd_and_hms(2026, 3, 8, 0, 0, 0).unwrap())
        );
        assert_eq!(avail, None);
    }

    #[test]
    fn refund_clawback_is_proportional_to_remaining() {
        // Order 100, commission 10 on base 100: refund 30 then 70.
        let (delta, remaining) = refund_window(amt("100"), Amount::ZERO, amt("30")).unwrap();
        assert_eq!((delta, remaining), (amt("30"), amt("100")));
        let (c, b, rejected) = clawback(amt("10"), amt("100"), delta, remaining);
        assert_eq!((c, b, rejected), (amt("7.00"), amt("70.00"), false));
        let (delta, remaining) = refund_window(amt("100"), amt("30"), amt("80")).unwrap();
        assert_eq!((delta, remaining), (amt("70"), amt("70")));
        let (c, b, rejected) = clawback(c, b, delta, remaining);
        assert_eq!((c, b, rejected), (Amount::ZERO, Amount::ZERO, true));
        assert!(refund_window(amt("100"), amt("100"), amt("1")).is_none());
        assert!(refund_window(amt("100"), Amount::ZERO, Amount::ZERO).is_none());
    }

    #[test]
    fn withdraw_allocation_splits_last_row() {
        let rows = [(1, amt("3")), (2, amt("0")), (3, amt("5")), (4, amt("9"))];
        assert_eq!(
            plan_withdraw(&rows, amt("6")),
            Some(WithdrawPlan {
                selected: vec![1, 3],
                split: Some((3, amt("3"), amt("2"))),
            })
        );
        assert_eq!(
            plan_withdraw(&rows, amt("8")),
            Some(WithdrawPlan {
                selected: vec![1, 3],
                split: None
            })
        );
        assert_eq!(plan_withdraw(&rows, amt("17.01")), None);
        let now = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
        let t = split_type(1295, now);
        assert!(t.starts_with("spzz"));
        assert!(t.len() <= 20);
    }

    #[test]
    fn withdraw_validation() {
        let channels = vec!["Alipay".to_owned()];
        assert!(validate_withdraw(amt("10"), 10.0, "alipay", "a@b", &channels).is_ok());
        assert!(validate_withdraw(amt("9.99"), 10.0, "alipay", "a", &channels).is_err());
        assert!(validate_withdraw(amt("0"), 0.0, "alipay", "a", &channels).is_err());
        assert!(validate_withdraw(amt("10"), 0.0, "bank", "a", &channels).is_err());
        assert!(validate_withdraw(amt("10"), 0.0, "alipay", " ", &channels).is_err());
        assert!(validate_withdraw(amt("10"), 0.0, "anything", "a", &[]).is_ok());
        assert_eq!(parse_profile_status(" disabled"), Some("disabled"));
        assert_eq!(parse_profile_status("banned"), None);
        assert_eq!(normalize_ids(&[3, 0, 3, -1, 2]), vec![3, 2]);
    }
}
