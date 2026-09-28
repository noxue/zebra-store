//! Monetary amounts with two-decimal precision.
//!
//! Mirrors the original `money.Amount`: stored as `decimal(20,2)` and serialized
//! as a JSON string such as `"12.30"`. Deserialization also accepts JSON numbers
//! because the original admin API accepts `float64` inputs.

use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// Number of decimal places kept for every amount.
pub const SCALE: u32 = 2;

/// A two-decimal monetary amount.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Amount(Decimal);

impl Amount {
    /// The zero amount.
    pub const ZERO: Self = Self(Decimal::ZERO);

    /// Creates an amount, rounding half away from zero to two decimals.
    pub fn new(value: Decimal) -> Self {
        Self(value.round_dp_with_strategy(SCALE, RoundingStrategy::MidpointAwayFromZero))
    }

    /// Creates an amount from integer cents.
    pub fn from_cents(cents: i64) -> Self {
        Self(Decimal::new(cents, SCALE))
    }

    /// Returns the inner decimal.
    pub fn decimal(self) -> Decimal {
        self.0
    }

    /// Returns the amount in integer cents.
    pub fn cents(self) -> i64 {
        (self.0 * Decimal::ONE_HUNDRED)
            .trunc()
            .try_into()
            .unwrap_or(i64::MAX)
    }

    pub fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    pub fn is_positive(self) -> bool {
        self.0 > Decimal::ZERO
    }

    pub fn is_negative(self) -> bool {
        self.0 < Decimal::ZERO
    }

    /// Returns `self * percent / 100`, rounded to two decimals.
    pub fn percent(self, percent: Decimal) -> Self {
        Self::new(self.0 * percent / Decimal::ONE_HUNDRED)
    }

    /// Returns `self` clamped at zero.
    pub fn non_negative(self) -> Self {
        if self.is_negative() { Self::ZERO } else { self }
    }

    pub fn min(self, other: Self) -> Self {
        if self <= other { self } else { other }
    }

    pub fn max(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }
}

impl From<Decimal> for Amount {
    fn from(value: Decimal) -> Self {
        Self::new(value)
    }
}

impl From<Amount> for Decimal {
    fn from(value: Amount) -> Self {
        value.0
    }
}

impl From<i64> for Amount {
    fn from(value: i64) -> Self {
        Self(Decimal::from(value))
    }
}

/// Error returned when parsing an [`Amount`] fails.
#[derive(Debug, thiserror::Error)]
#[error("invalid amount: {0}")]
pub struct ParseAmountError(String);

impl FromStr for Amount {
    type Err = ParseAmountError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Ok(Self::ZERO);
        }
        Decimal::from_str(trimmed)
            .map(Self::new)
            .map_err(|_| ParseAmountError(s.to_owned()))
    }
}

impl fmt::Display for Amount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut value = self.0;
        value.rescale(SCALE);
        write!(f, "{value}")
    }
}

impl Serialize for Amount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Amount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl de::Visitor<'_> for Visitor {
            type Value = Amount;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a decimal number or numeric string")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<Amount, E> {
                Amount::from_str(v).map_err(E::custom)
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Amount, E> {
                Ok(Amount::from(v))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Amount, E> {
                Ok(Amount::new(Decimal::from(v)))
            }

            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Amount, E> {
                Decimal::from_str(&v.to_string())
                    .map(Amount::new)
                    .map_err(E::custom)
            }

            fn visit_unit<E: de::Error>(self) -> Result<Amount, E> {
                Ok(Amount::ZERO)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl Add for Amount {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self(self.0 + rhs.0)
    }
}

impl AddAssign for Amount {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Amount {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self(self.0 - rhs.0)
    }
}

impl SubAssign for Amount {
    fn sub_assign(&mut self, rhs: Self) {
        self.0 -= rhs.0;
    }
}

impl Neg for Amount {
    type Output = Self;
    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl Mul<i64> for Amount {
    type Output = Self;
    fn mul(self, rhs: i64) -> Self {
        Self::new(self.0 * Decimal::from(rhs))
    }
}

impl Sum for Amount {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Add::add)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn serializes_as_two_decimal_string() {
        assert_eq!(
            serde_json::to_string(&Amount::from(12)).unwrap(),
            "\"12.00\""
        );
        assert_eq!(
            serde_json::to_string(&Amount::new(dec!(0.1))).unwrap(),
            "\"0.10\""
        );
    }

    #[test]
    fn deserializes_numbers_and_strings() {
        let a: Amount = serde_json::from_str("98").unwrap();
        let b: Amount = serde_json::from_str("\"98.005\"").unwrap();
        let c: Amount = serde_json::from_str("19.6").unwrap();
        assert_eq!(a.to_string(), "98.00");
        assert_eq!(b.to_string(), "98.01");
        assert_eq!(c.to_string(), "19.60");
    }

    #[test]
    fn percent_rounds_half_away_from_zero() {
        assert_eq!(Amount::from(98).percent(dec!(20)).to_string(), "19.60");
        assert_eq!(Amount::from_cents(5).percent(dec!(50)).to_string(), "0.03");
    }

    #[test]
    fn cents_roundtrip() {
        assert_eq!(Amount::from_cents(1234).cents(), 1234);
        assert_eq!(Amount::from_cents(1234).to_string(), "12.34");
    }
}
