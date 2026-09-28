//! Promotions (活动价): per-product price rules with optional spend thresholds.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::{Error, Id, Result};

pub mod keys {
    pub const INVALID: &str = "error.promotion_invalid";
    pub const NOT_FOUND: &str = "error.promotion_not_found";
    pub const CREATE_FAILED: &str = "error.promotion_create_failed";
    pub const UPDATE_FAILED: &str = "error.promotion_update_failed";
    pub const DELETE_FAILED: &str = "error.promotion_delete_failed";
    pub const FETCH_FAILED: &str = "error.promotion_fetch_failed";
}

/// The only supported scope (`constants.ScopeTypeProduct`).
pub const SCOPE_PRODUCT: &str = "product";

/// Promotion kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionType {
    /// Subtract `value` from the unit price.
    Fixed,
    /// Take `value` percent off.
    Percent,
    /// Sell at `value`.
    SpecialPrice,
}

impl PromotionType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "fixed" => Some(Self::Fixed),
            "percent" => Some(Self::Percent),
            "special_price" => Some(Self::SpecialPrice),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Percent => "percent",
            Self::SpecialPrice => "special_price",
        }
    }
}

/// A promotion (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Promotion {
    pub id: Id,
    pub name: String,
    pub scope_type: String,
    pub scope_ref_id: Id,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Amount,
    pub min_amount: Amount,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Promotion {
    /// Active and within its time window at `now` (bounds inclusive).
    pub fn is_effective(&self, now: DateTime<Utc>) -> bool {
        self.is_active
            && self.starts_at.is_none_or(|s| s <= now)
            && self.ends_at.is_none_or(|e| e >= now)
    }
}

/// Validated create/update input.
#[derive(Debug, Clone, PartialEq)]
pub struct PromotionInput {
    pub name: String,
    pub kind: String,
    pub scope_ref_id: Id,
    pub value: Amount,
    pub min_amount: Amount,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_active: Option<bool>,
}

/// Validated fields to persist.
#[derive(Debug, Clone, PartialEq)]
pub struct PromotionFields {
    pub name: String,
    pub kind: PromotionType,
    pub scope_ref_id: Id,
    pub value: Amount,
    pub min_amount: Amount,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

/// Validates admin input; `current_active` is the stored flag on update (`None` on create → default true).
pub fn validate_promotion(
    input: &PromotionInput,
    current_active: Option<bool>,
) -> Result<PromotionFields> {
    let invalid = || Error::bad_request(keys::INVALID);
    let name = input.name.trim();
    if name.is_empty() || input.scope_ref_id <= 0 {
        return Err(invalid());
    }
    let kind = PromotionType::parse(&input.kind).ok_or_else(invalid)?;
    if !input.value.is_positive() {
        return Err(invalid());
    }
    if kind == PromotionType::Percent && input.value > Amount::from(100) {
        return Err(invalid());
    }
    if let (Some(s), Some(e)) = (input.starts_at, input.ends_at)
        && e < s
    {
        return Err(invalid());
    }
    Ok(PromotionFields {
        name: name.to_owned(),
        kind,
        scope_ref_id: input.scope_ref_id,
        value: input.value,
        min_amount: input.min_amount,
        starts_at: input.starts_at,
        ends_at: input.ends_at,
        is_active: input.is_active.or(current_active).unwrap_or(true),
    })
}

/// Unit price after applying one promotion (never negative).
pub fn promotion_unit_price(base: Amount, promotion: &Promotion) -> Result<Amount> {
    let value = promotion.value;
    if !value.is_positive() {
        return Err(Error::bad_request(keys::INVALID));
    }
    match PromotionType::parse(&promotion.kind) {
        Some(PromotionType::Fixed) => Ok((base - value).non_negative()),
        Some(PromotionType::Percent) => {
            let percent = (Decimal::ONE_HUNDRED - value.decimal()).max(Decimal::ZERO);
            Ok(Amount::new(base.decimal() * percent / Decimal::ONE_HUNDRED))
        }
        Some(PromotionType::SpecialPrice) => Ok(value),
        None => Err(Error::bad_request(keys::INVALID)),
    }
}

/// Picks the tier matching `unit_price × quantity` among the product's effective promotions
/// (`promotions` sorted by `min_amount ASC`; the highest satisfied threshold wins) and
/// returns it with the discounted unit price (PRC-08). `Ok(None)` = no promotion applies.
pub fn apply_promotion(
    promotions: &[Promotion],
    unit_price: Amount,
    quantity: i32,
) -> Result<Option<(Promotion, Amount)>> {
    if quantity <= 0 {
        return Err(Error::bad_request(keys::INVALID));
    }
    let subtotal = unit_price * i64::from(quantity);
    let matched = promotions.iter().rev().find(|p| {
        p.scope_type.trim().eq_ignore_ascii_case(SCOPE_PRODUCT)
            && (!p.min_amount.is_positive() || subtotal >= p.min_amount)
    });
    match matched {
        None => Ok(None),
        Some(p) => Ok(Some((p.clone(), promotion_unit_price(unit_price, p)?))),
    }
}

/// Admin list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromotionFilter {
    pub id: Id,
    /// Case-insensitive substring.
    pub name: String,
    pub scope_ref_id: Id,
    pub is_active: Option<bool>,
}

/// Persistence port. Soft-deleted rows are never returned.
#[async_trait]
pub trait PromotionRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<Promotion>>;
    /// Effective promotions of a product at `now`, sorted by `min_amount ASC`.
    async fn list_effective(&self, product_id: Id, now: DateTime<Utc>) -> Result<Vec<Promotion>>;
    async fn list(&self, filter: &PromotionFilter, page: PageRequest) -> Result<Page<Promotion>>;
    async fn create(&self, fields: &PromotionFields, now: DateTime<Utc>) -> Result<Promotion>;
    async fn update(&self, id: Id, fields: &PromotionFields, now: DateTime<Utc>) -> Result<()>;
    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()>;
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn promo(id: Id, kind: &str, value: i64, min: i64) -> Promotion {
        Promotion {
            id,
            name: format!("p{id}"),
            scope_type: SCOPE_PRODUCT.into(),
            scope_ref_id: 1,
            kind: kind.into(),
            value: Amount::from(value),
            min_amount: Amount::from(min),
            starts_at: None,
            ends_at: None,
            is_active: true,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::promo;
    use super::*;

    fn amt(s: &str) -> Amount {
        s.parse().unwrap()
    }

    #[test]
    fn unit_price_per_type() {
        let base = amt("59.90");
        assert_eq!(
            promotion_unit_price(base, &promo(1, "fixed", 10, 0)).unwrap(),
            amt("49.90")
        );
        assert_eq!(
            promotion_unit_price(base, &promo(1, "fixed", 100, 0)).unwrap(),
            Amount::ZERO
        );
        assert_eq!(
            promotion_unit_price(base, &promo(1, "percent", 20, 0)).unwrap(),
            amt("47.92")
        );
        assert_eq!(
            promotion_unit_price(base, &promo(1, "special_price", 30, 0)).unwrap(),
            amt("30.00")
        );
        assert!(promotion_unit_price(base, &promo(1, "bogus", 30, 0)).is_err());
        assert!(promotion_unit_price(base, &promo(1, "fixed", 0, 0)).is_err());
    }

    // PRC-08
    #[test]
    fn tiered_rules_match_by_subtotal() {
        let rules = vec![
            promo(1, "percent", 5, 0),
            promo(2, "percent", 10, 100),
            promo(3, "percent", 20, 300),
        ];
        let price = amt("50");
        let at = |q| apply_promotion(&rules, price, q).unwrap().unwrap();
        assert_eq!(at(1).1, amt("47.50"));
        assert_eq!(at(2).1, amt("45.00"));
        assert_eq!(at(6).1, amt("40.00"));
        assert_eq!(at(6).0.id, 3);
        assert!(apply_promotion(&[], price, 1).unwrap().is_none());
        assert!(apply_promotion(&rules, price, 0).is_err());
        let high_only = vec![promo(9, "fixed", 5, 1000)];
        assert!(apply_promotion(&high_only, price, 1).unwrap().is_none());
    }

    #[test]
    fn effective_window() {
        let now = Utc::now();
        let mut p = promo(1, "fixed", 1, 0);
        assert!(p.is_effective(now));
        p.starts_at = Some(now + chrono::Duration::hours(1));
        assert!(!p.is_effective(now));
        p.starts_at = None;
        p.ends_at = Some(now - chrono::Duration::hours(1));
        assert!(!p.is_effective(now));
        p.ends_at = None;
        p.is_active = false;
        assert!(!p.is_effective(now));
    }

    #[test]
    fn validation() {
        let base = PromotionInput {
            name: " Summer ".into(),
            kind: "PERCENT".into(),
            scope_ref_id: 1,
            value: amt("20"),
            min_amount: Amount::ZERO,
            starts_at: None,
            ends_at: None,
            is_active: None,
        };
        let ok = validate_promotion(&base, None).unwrap();
        assert_eq!(
            (ok.name.as_str(), ok.kind, ok.is_active),
            ("Summer", PromotionType::Percent, true)
        );
        assert!(!validate_promotion(&base, Some(false)).unwrap().is_active);
        for bad in [
            PromotionInput {
                name: " ".into(),
                ..base.clone()
            },
            PromotionInput {
                scope_ref_id: 0,
                ..base.clone()
            },
            PromotionInput {
                kind: "x".into(),
                ..base.clone()
            },
            PromotionInput {
                value: Amount::ZERO,
                ..base.clone()
            },
            PromotionInput {
                value: amt("100.01"),
                ..base.clone()
            },
            PromotionInput {
                starts_at: Some(Utc::now()),
                ends_at: Some(Utc::now() - chrono::Duration::days(1)),
                ..base.clone()
            },
        ] {
            assert_eq!(
                validate_promotion(&bad, None).unwrap_err().key(),
                keys::INVALID
            );
        }
    }
}
