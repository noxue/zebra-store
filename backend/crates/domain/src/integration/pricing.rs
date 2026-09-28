//! Local price of supplier goods: `upstream × exchange_rate → markup → rounding`
//! (original `mapping/application/pricing.go`, UPS-05, UPS-08).

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

/// Money scale of local prices.
const SCALE: u32 = 2;

/// How a marked-up price is rounded (`site_connections.price_rounding_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    #[default]
    None,
    /// Up to the next integer: 12.01 → 13.
    CeilInt,
    /// Up to the next tenth: 12.34 → 12.40.
    CeilTenth,
}

impl RoundingMode {
    /// Strict parse used on writes: blank → none, unknown → `None`.
    pub fn parse_input(raw: &str) -> Option<Self> {
        match raw.trim() {
            "" | "none" => Some(Self::None),
            "ceil_int" => Some(Self::CeilInt),
            "ceil_tenth" => Some(Self::CeilTenth),
            _ => None,
        }
    }

    /// Lenient parse of stored values.
    pub fn from_stored(raw: &str) -> Self {
        Self::parse_input(raw).unwrap_or_default()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::CeilInt => "ceil_int",
            Self::CeilTenth => "ceil_tenth",
        }
    }
}

/// Pricing parameters of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pricing {
    pub exchange_rate: Decimal,
    pub markup_percent: Decimal,
    pub rounding: RoundingMode,
}

impl Default for Pricing {
    fn default() -> Self {
        Self {
            exchange_rate: Decimal::ONE,
            markup_percent: Decimal::ZERO,
            rounding: RoundingMode::None,
        }
    }
}

fn effective_rate(rate: Decimal) -> Decimal {
    if rate <= Decimal::ZERO {
        Decimal::ONE
    } else {
        rate
    }
}

fn round2(v: Decimal) -> Decimal {
    v.round_dp_with_strategy(SCALE, RoundingStrategy::MidpointAwayFromZero)
}

/// Upstream price in local currency without markup (the local cost price, UPS-08).
/// A non-positive rate counts as 1.
pub fn convert_currency(upstream: Decimal, rate: Decimal) -> Decimal {
    round2(upstream * effective_rate(rate))
}

/// Applies markup and rounding to an already converted price. A zero markup keeps
/// the price (rounded to cents, rounding mode ignored, like the original); negative
/// results become zero (callers must refuse them, UPS-05).
pub fn marked_up(price: Decimal, markup_percent: Decimal, rounding: RoundingMode) -> Decimal {
    if markup_percent.is_zero() {
        return round2(price);
    }
    let result = price * (Decimal::ONE + markup_percent / Decimal::ONE_HUNDRED);
    if result.is_sign_negative() {
        return Decimal::ZERO;
    }
    match rounding {
        RoundingMode::CeilInt => result.ceil(),
        RoundingMode::CeilTenth => (result * Decimal::TEN).ceil() / Decimal::TEN,
        RoundingMode::None => round2(result),
    }
    .round_dp(SCALE)
}

impl Pricing {
    /// Local selling price of an upstream price.
    pub fn local_price(&self, upstream: Decimal) -> Decimal {
        let converted = upstream * effective_rate(self.exchange_rate);
        marked_up(converted, self.markup_percent, self.rounding)
    }

    /// Local cost price of an upstream price.
    pub fn cost_price(&self, upstream: Decimal) -> Decimal {
        convert_currency(upstream, self.exchange_rate)
    }
}

/// Parses a supplier price string; `None` for blank/invalid/negative values (UPS-08:
/// a parse failure must never become a zero price).
pub fn parse_upstream_price(raw: &str) -> Option<Decimal> {
    let v: Decimal = raw.trim().parse().ok()?;
    (!v.is_sign_negative()).then(|| round2(v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn p(rate: Decimal, markup: Decimal, rounding: RoundingMode) -> Pricing {
        Pricing {
            exchange_rate: rate,
            markup_percent: markup,
            rounding,
        }
    }

    // UPS-05 (1): exchange rate, markup and rounding.
    #[test]
    fn ups05_local_price() {
        assert_eq!(
            p(dec("7.2"), dec("20"), RoundingMode::None).local_price(dec("10")),
            dec("86.40")
        );
        assert_eq!(
            marked_up(dec("12.01"), dec("0.0001"), RoundingMode::CeilInt),
            dec("13")
        );
        assert_eq!(
            marked_up(dec("12.00"), dec("0.0000001"), RoundingMode::CeilInt),
            dec("13")
        );
        assert_eq!(
            marked_up(dec("10"), dec("20"), RoundingMode::CeilInt),
            dec("12")
        );
        assert_eq!(
            marked_up(dec("10.28333"), dec("20"), RoundingMode::CeilTenth),
            dec("12.40")
        );
        assert_eq!(
            marked_up(dec("10.25"), dec("20"), RoundingMode::CeilTenth),
            dec("12.30")
        );
        // zero markup keeps the price and ignores rounding
        assert_eq!(
            marked_up(dec("12.345"), dec("0"), RoundingMode::CeilInt),
            dec("12.35")
        );
        assert_eq!(
            marked_up(dec("10"), dec("-150"), RoundingMode::None),
            dec("0")
        );
        // non-positive rate counts as 1
        assert_eq!(
            p(dec("0"), dec("0"), RoundingMode::None).local_price(dec("3.5")),
            dec("3.50")
        );
    }

    // UPS-08 (3): cost price is converted but never marked up.
    #[test]
    fn ups08_cost_price_uses_exchange_rate() {
        let pricing = p(dec("7.2"), dec("50"), RoundingMode::CeilInt);
        assert_eq!(pricing.cost_price(dec("1.00")), dec("7.20"));
        assert_eq!(pricing.local_price(dec("1.00")), dec("11"));
    }

    // UPS-08: invalid prices are refused instead of becoming zero.
    #[test]
    fn ups08_parse_price() {
        assert_eq!(parse_upstream_price("12.345"), Some(dec("12.35")));
        assert_eq!(parse_upstream_price(" 3 "), Some(dec("3.00")));
        assert_eq!(parse_upstream_price("abc"), None);
        assert_eq!(parse_upstream_price(""), None);
        assert_eq!(parse_upstream_price("-1"), None);
    }

    #[test]
    fn rounding_mode_parse() {
        assert_eq!(RoundingMode::parse_input(""), Some(RoundingMode::None));
        assert_eq!(
            RoundingMode::parse_input("ceil_tenth"),
            Some(RoundingMode::CeilTenth)
        );
        assert_eq!(RoundingMode::parse_input("floor"), None);
    }
}
