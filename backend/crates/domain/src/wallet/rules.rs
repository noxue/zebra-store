//! Pure wallet rules: references (idempotency keys), balance planning, admin adjustments
//! and the recharge status matrix (WAL-01, WAL-02, PAY-02).

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use zs_shared::money::Amount;

use super::model::{DEFAULT_CURRENCY, RechargeStatus, direction, keys};
use super::ports::LedgerError;
use crate::payment::types::PaymentStatus;
use crate::{Error, Id};

/// Prefix of recharge numbers (original `serial.Generate("WR")`).
pub const RECHARGE_NO_PREFIX: &str = "WR";
/// Remark of a recharge order created without one (original `余额充值`).
pub const DEFAULT_RECHARGE_REMARK: &str = "余额充值";
/// Remark of the recharge credit when the order has none (original `在线充值到账`).
pub const RECHARGE_CREDIT_REMARK: &str = "在线充值到账";
/// Fallback remark of admin adjustments (original `管理员调整余额`).
pub const ADMIN_ADJUST_REMARK: &str = "管理员调整余额";
/// Fallback remark of generic credits (original `钱包入账`).
pub const CREDIT_REMARK: &str = "钱包入账";
/// Remark of order balance payments (original `订单余额支付`).
pub const ORDER_PAY_REMARK: &str = "订单余额支付";
/// Fallback remark of order balance releases (original `订单余额退回`).
pub const ORDER_RELEASE_REMARK: &str = "订单余额退回";

/// Upper-cased currency, `CNY` when empty (`normalizeCurrency`).
pub fn normalize_currency(raw: &str) -> String {
    let c = raw.trim().to_ascii_uppercase();
    if c.is_empty() {
        DEFAULT_CURRENCY.to_owned()
    } else {
        c
    }
}

/// Trimmed remark or the fallback (`cleanRemark`).
pub fn clean_remark(raw: &str, fallback: &str) -> String {
    let r = raw.trim();
    if r.is_empty() {
        fallback.to_owned()
    } else {
        r.to_owned()
    }
}

/// `order:<id>:<action>` (action defaults to `wallet`).
pub fn order_reference(order_id: Id, action: &str) -> String {
    let action = match action.trim() {
        "" => "wallet",
        a => a,
    };
    format!("order:{order_id}:{action}")
}

/// Round-aware order allocation key (PAY-02): the first round keeps the historical
/// `order:<id>:<action>`; later rounds append `:<n>` where `n = existing + 1`, so a
/// "use balance → release → use again" cycle is never mistaken for a duplicate.
pub fn allocation_reference(order_id: Id, action: &str, existing: u64) -> String {
    let base = order_reference(order_id, action);
    if order_id == 0 || existing == 0 {
        base
    } else {
        format!("{base}:{}", existing + 1)
    }
}

/// Credit key of a successful recharge (`recharge:<id>:success`).
pub fn recharge_reference(recharge_id: Id) -> String {
    format!("recharge:{recharge_id}:success")
}

/// Credit key of an underpaid order payment (`payment:<id>:underpaid_credit`, PAY-02).
pub fn underpaid_reference(payment_id: Id) -> String {
    format!("payment:{payment_id}:underpaid_credit")
}

/// Unique key of one-off movements such as admin adjustments (`uniqueReference`).
pub fn unique_reference(prefix: &str, user_id: Id, now: DateTime<Utc>) -> String {
    let prefix = match prefix.trim() {
        "" => "wallet",
        p => p,
    };
    let nanos = now
        .timestamp_nanos_opt()
        .unwrap_or_else(|| now.timestamp_micros());
    format!("{prefix}:{user_id}:{nanos}")
}

/// Generates a recharge number such as `WR20260924173502588608`.
pub fn recharge_no(now: DateTime<Utc>) -> String {
    zs_shared::serial::generate(RECHARGE_NO_PREFIX, now)
}

/// A planned balance movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BalanceChange {
    pub before: Amount,
    pub after: Amount,
    pub direction: &'static str,
    /// Absolute amount moved.
    pub amount: Amount,
}

/// Plans `balance + delta` (`changeBalance`): zero deltas are invalid and the balance
/// never goes negative.
pub fn plan_change(before: Amount, delta: Amount) -> Result<BalanceChange, LedgerError> {
    if delta.is_zero() {
        return Err(LedgerError::InvalidAmount);
    }
    let after = before + delta;
    if after.is_negative() {
        return Err(LedgerError::InsufficientBalance);
    }
    let (direction, amount) = if delta.is_negative() {
        (direction::OUT, -delta)
    } else {
        (direction::IN, delta)
    };
    Ok(BalanceChange {
        before,
        after,
        direction,
        amount,
    })
}

/// Plans a credit; the amount must be positive.
pub fn plan_credit(before: Amount, amount: Amount) -> Result<BalanceChange, LedgerError> {
    if !amount.is_positive() {
        return Err(LedgerError::InvalidAmount);
    }
    plan_change(before, amount)
}

/// Plans a debit of exactly `amount` (fails when the balance is insufficient).
pub fn plan_debit(before: Amount, amount: Amount) -> Result<BalanceChange, LedgerError> {
    if !amount.is_positive() {
        return Err(LedgerError::InvalidAmount);
    }
    plan_change(before, -amount)
}

/// Balance used for an order (`ApplyOrderBalance`): `min(balance, total)`, never negative.
pub fn order_debit_amount(balance: Amount, total: Amount) -> Amount {
    if !balance.is_positive() || !total.is_positive() {
        return Amount::ZERO;
    }
    balance.min(total)
}

/// Validates an admin adjustment request (WAL-02) and returns the signed delta and the
/// trimmed remark. Checks run in the original order: amount, operation, remark.
pub fn parse_adjustment(
    amount: &str,
    operation: &str,
    remark: &str,
) -> Result<(Amount, String), Error> {
    let amount: Decimal = amount.trim().parse().map_err(|_| Error::invalid())?;
    if amount <= Decimal::ZERO {
        return Err(Error::invalid());
    }
    let sign = match operation.trim().to_ascii_lowercase().as_str() {
        "add" => Decimal::ONE,
        "subtract" => Decimal::NEGATIVE_ONE,
        _ => return Err(Error::invalid()),
    };
    let remark = remark.trim();
    if remark.is_empty() {
        return Err(Error::bad_request(keys::ADJUST_REMARK_REQUIRED));
    }
    let delta = Amount::new(amount * sign);
    if delta.is_zero() {
        return Err(Error::invalid());
    }
    Ok((delta, remark.to_owned()))
}

/// Whether a callback with status `target` may change a recharge (`canApplyWalletRechargeCallback`,
/// WAL-01): success may always override (late gateway success after expiry); other statuses
/// never reopen or rewrite a terminal payment or recharge.
pub fn can_apply_recharge_callback(
    payment: PaymentStatus,
    recharge: RechargeStatus,
    target: PaymentStatus,
) -> bool {
    if target == PaymentStatus::Success {
        return true;
    }
    let payment_terminal = matches!(
        payment,
        PaymentStatus::Success | PaymentStatus::Failed | PaymentStatus::Expired
    );
    !payment_terminal && !recharge.is_terminal()
}

/// Whether the timeout job may expire a recharge (`canExpireWalletRechargePayment`):
/// only a pending recharge whose payment is still initiated/pending.
pub fn can_expire_recharge(payment: PaymentStatus, recharge: RechargeStatus) -> bool {
    recharge == RechargeStatus::Pending && payment.is_open()
}

/// Recharge status mirroring a payment status after a callback.
pub fn recharge_status_after(target: PaymentStatus) -> RechargeStatus {
    match target {
        PaymentStatus::Success => RechargeStatus::Success,
        PaymentStatus::Failed => RechargeStatus::Failed,
        PaymentStatus::Expired => RechargeStatus::Expired,
        PaymentStatus::Initiated | PaymentStatus::Pending => RechargeStatus::Pending,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn amt(v: &str) -> Amount {
        v.parse().unwrap_or_default()
    }

    #[test]
    fn references_and_currency() {
        assert_eq!(normalize_currency(" usd "), "USD");
        assert_eq!(normalize_currency(""), "CNY");
        assert_eq!(clean_remark("  ", "x"), "x");
        assert_eq!(clean_remark(" a ", "x"), "a");
        assert_eq!(order_reference(7, ""), "order:7:wallet");
        assert_eq!(recharge_reference(3), "recharge:3:success");
        assert_eq!(underpaid_reference(9), "payment:9:underpaid_credit");
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert_eq!(
            unique_reference("admin_adjust", 5, now),
            "admin_adjust:5:1767225600000000000"
        );
        assert!(recharge_no(now).starts_with("WR20260101080000"));
    }

    /// PAY-02: allocation keys carry a round number after the first round.
    #[test]
    fn pay_02_allocation_reference_rounds() {
        assert_eq!(
            allocation_reference(10, "order_pay", 0),
            "order:10:order_pay"
        );
        assert_eq!(
            allocation_reference(10, "order_pay", 1),
            "order:10:order_pay:2"
        );
        assert_eq!(
            allocation_reference(10, "order_refund", 2),
            "order:10:order_refund:3"
        );
    }

    #[test]
    fn balance_planning() {
        let c = plan_change(amt("10.00"), amt("-3.50")).unwrap();
        assert_eq!(
            (c.after, c.direction, c.amount),
            (amt("6.50"), "out", amt("3.50"))
        );
        let c = plan_credit(amt("0"), amt("5")).unwrap();
        assert_eq!((c.after, c.direction), (amt("5.00"), "in"));
        assert_eq!(
            plan_change(amt("1"), amt("-1.01")),
            Err(LedgerError::InsufficientBalance)
        );
        assert_eq!(
            plan_change(amt("1"), Amount::ZERO),
            Err(LedgerError::InvalidAmount)
        );
        assert_eq!(
            plan_credit(amt("1"), amt("-1")),
            Err(LedgerError::InvalidAmount)
        );
        assert_eq!(
            plan_debit(amt("5"), amt("5")).map(|c| c.after),
            Ok(Amount::ZERO)
        );
        assert_eq!(order_debit_amount(amt("5"), amt("10")), amt("5"));
        assert_eq!(order_debit_amount(amt("50"), amt("10")), amt("10"));
        assert_eq!(order_debit_amount(amt("-1"), amt("10")), Amount::ZERO);
    }

    /// WAL-02: operation is required and enumerated; remark is required.
    #[test]
    fn wal_02_adjustment_validation() {
        assert_eq!(
            parse_adjustment("10", "add", " fix ").unwrap(),
            (amt("10"), "fix".to_owned())
        );
        assert_eq!(
            parse_adjustment("2.5", "SUBTRACT", "r").unwrap().0,
            amt("-2.50")
        );
        for (a, op, r) in [
            ("x", "add", "r"),
            ("0", "add", "r"),
            ("-1", "add", "r"),
            ("1", "", "r"),
            ("1", "set", "r"),
            ("0.001", "add", "r"),
        ] {
            assert_eq!(
                parse_adjustment(a, op, r).unwrap_err().key(),
                "error.bad_request"
            );
        }
        assert_eq!(
            parse_adjustment("1", "add", "  ").unwrap_err().key(),
            keys::ADJUST_REMARK_REQUIRED
        );
    }

    /// WAL-01: the callback terminal-state matrix.
    #[test]
    fn wal_01_callback_matrix() {
        use PaymentStatus as P;
        use RechargeStatus as R;
        // Success always applies (late success after expiry/failure credits once).
        for (p, r) in [
            (P::Expired, R::Expired),
            (P::Failed, R::Failed),
            (P::Pending, R::Pending),
        ] {
            assert!(can_apply_recharge_callback(p, r, P::Success));
        }
        // Non-success callbacks never reopen terminal states.
        for (p, r) in [
            (P::Expired, R::Expired),
            (P::Success, R::Success),
            (P::Failed, R::Failed),
            (P::Pending, R::Expired),
            (P::Success, R::Pending),
        ] {
            assert!(!can_apply_recharge_callback(p, r, P::Pending));
            assert!(!can_apply_recharge_callback(p, r, P::Failed));
        }
        assert!(can_apply_recharge_callback(
            P::Pending,
            R::Pending,
            P::Failed
        ));
        assert!(can_expire_recharge(P::Initiated, R::Pending));
        assert!(can_expire_recharge(P::Pending, R::Pending));
        assert!(!can_expire_recharge(P::Success, R::Pending));
        assert!(!can_expire_recharge(P::Pending, R::Success));
        assert!(!can_expire_recharge(P::Expired, R::Expired));
        assert_eq!(recharge_status_after(P::Expired), R::Expired);
        assert_eq!(recharge_status_after(P::Initiated), R::Pending);
    }
}
