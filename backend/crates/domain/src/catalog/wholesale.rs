//! Wholesale (quantity) price tiers: write-side normalisation and read-side resolution (PRC-02, PRC-05).

use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};
use zs_shared::money::Amount;

use crate::{Error, Id, Result};

/// Message key of every invalid tier configuration.
pub const WHOLESALE_PRICE_INVALID: &str = "error.wholesale_price_invalid";

/// A stored tier. `sku_id`/`sku_code` empty = shared by every SKU of the product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WholesaleTier {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub sku_id: Id,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub sku_code: String,
    pub min_quantity: i32,
    pub unit_price: Amount,
}

fn is_zero(v: &Id) -> bool {
    *v == 0
}

/// A tier as submitted by the admin.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct WholesalePriceInput {
    #[serde(default)]
    pub sku_id: Id,
    #[serde(default)]
    pub sku_code: String,
    #[serde(default)]
    pub min_quantity: i32,
    #[serde(default)]
    pub unit_price: Amount,
}

/// SKU identity used to canonicalise tiers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkuRef {
    pub id: Id,
    pub code: String,
}

fn invalid() -> Error {
    Error::bad_request(WHOLESALE_PRICE_INVALID)
}

fn scope_key(sku_id: Id, sku_code: &str) -> String {
    let code = sku_code.trim().to_lowercase();
    if !code.is_empty() {
        format!("code:{code}")
    } else if sku_id > 0 {
        format!("id:{sku_id}")
    } else {
        "all".to_owned()
    }
}

/// Validates, sorts and normalises tiers: quantity and price positive, no duplicate
/// quantity within a scope, and prices strictly decreasing as the threshold grows.
pub fn normalize_wholesale_prices(inputs: &[WholesalePriceInput]) -> Result<Vec<WholesaleTier>> {
    let mut seen = BTreeSet::new();
    let mut tiers = Vec::with_capacity(inputs.len());
    for input in inputs {
        let code = input.sku_code.trim().to_owned();
        if input.min_quantity <= 0 || !input.unit_price.is_positive() {
            return Err(invalid());
        }
        let key = format!("{}:{}", scope_key(input.sku_id, &code), input.min_quantity);
        if !seen.insert(key) {
            return Err(invalid());
        }
        tiers.push(WholesaleTier {
            sku_id: input.sku_id,
            sku_code: code,
            min_quantity: input.min_quantity,
            unit_price: input.unit_price,
        });
    }
    tiers.sort_by(|a, b| {
        scope_key(a.sku_id, &a.sku_code)
            .cmp(&scope_key(b.sku_id, &b.sku_code))
            .then(a.min_quantity.cmp(&b.min_quantity))
    });
    for pair in tiers.windows(2) {
        if scope_key(pair[0].sku_id, &pair[0].sku_code)
            == scope_key(pair[1].sku_id, &pair[1].sku_code)
            && pair[1].unit_price >= pair[0].unit_price
        {
            return Err(invalid());
        }
    }
    Ok(tiers)
}

/// Checks SKU ownership and canonicalises each tier's SKU identity to the product's current SKUs.
pub fn normalize_wholesale_prices_for_skus(
    inputs: &[WholesalePriceInput],
    skus: &[SkuRef],
) -> Result<Vec<WholesaleTier>> {
    let by_id: HashMap<Id, &SkuRef> = skus
        .iter()
        .filter(|s| s.id > 0)
        .map(|s| (s.id, s))
        .collect();
    let by_code: HashMap<String, &SkuRef> = skus
        .iter()
        .filter(|s| !s.code.trim().is_empty())
        .map(|s| (s.code.trim().to_lowercase(), s))
        .collect();
    let mut normalized = Vec::with_capacity(inputs.len());
    for input in inputs {
        let code = input.sku_code.trim();
        let mut next = input.clone();
        if input.sku_id > 0 {
            let sku = by_id.get(&input.sku_id).ok_or_else(invalid)?;
            if !code.is_empty() && !code.eq_ignore_ascii_case(sku.code.trim()) {
                return Err(invalid());
            }
            next.sku_code = sku.code.trim().to_owned();
        } else if !code.is_empty() {
            let sku = by_code.get(&code.to_lowercase()).ok_or_else(invalid)?;
            next.sku_id = sku.id;
            next.sku_code = sku.code.trim().to_owned();
        } else {
            next.sku_code = String::new();
        }
        normalized.push(next);
    }
    normalize_wholesale_prices(&normalized)
}

/// Result of resolving a wholesale price for one order line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WholesaleMatch {
    /// Unit price to charge (the base price when nothing matched).
    pub unit_price: Amount,
    /// `(base - tier) * quantity` for this line.
    pub discount: Amount,
    pub matched: bool,
}

/// Match priority: 3 = sku_id (and code) match, 2 = sku_code match, 1 = shared tier, 0 = no match.
fn match_priority(tier: &WholesaleTier, sku_id: Id, sku_code: &str) -> u8 {
    let tier_code = tier.sku_code.trim().to_lowercase();
    if tier.sku_id <= 0 && tier_code.is_empty() {
        return 1;
    }
    let sku_code = sku_code.trim().to_lowercase();
    if tier.sku_id > 0 {
        if sku_id <= 0 || tier.sku_id != sku_id {
            return 0;
        }
        if !tier_code.is_empty() && tier_code != sku_code {
            return 0;
        }
        return 3;
    }
    if tier_code == sku_code { 2 } else { 0 }
}

/// Resolves the wholesale unit price of one order line.
///
/// Shared tiers are matched against `product_quantity` (the product's total quantity
/// across SKUs); SKU-specific tiers against `line_quantity`. A SKU with its own tiers
/// never falls back to shared tiers. Among matching tiers the cheapest wins, and only
/// when cheaper than `base` (PRC-02, PRC-05).
pub fn resolve_wholesale_unit_price(
    tiers: &[WholesaleTier],
    base: Amount,
    sku_id: Id,
    sku_code: &str,
    product_quantity: i32,
    line_quantity: i32,
) -> WholesaleMatch {
    let none = WholesaleMatch {
        unit_price: base,
        discount: Amount::ZERO,
        matched: false,
    };
    if product_quantity <= 0 || line_quantity <= 0 || !base.is_positive() || tiers.is_empty() {
        return none;
    }
    let has_specific = tiers
        .iter()
        .any(|t| match_priority(t, sku_id, sku_code) >= 2);
    let mut best: Option<Amount> = None;
    for tier in tiers {
        if tier.min_quantity <= 0 || !tier.unit_price.is_positive() {
            continue;
        }
        let priority = match_priority(tier, sku_id, sku_code);
        let eligible = if has_specific {
            priority >= 2
        } else {
            priority == 1
        };
        if !eligible {
            continue;
        }
        let qty = if priority >= 2 {
            line_quantity
        } else {
            product_quantity
        };
        if qty < tier.min_quantity {
            continue;
        }
        if best.is_none_or(|b| tier.unit_price < b) {
            best = Some(tier.unit_price);
        }
    }
    match best {
        Some(price) if price < base => WholesaleMatch {
            unit_price: price,
            discount: (base - price) * i64::from(line_quantity),
            matched: true,
        },
        _ => none,
    }
}

/// Tiers shown publicly: invalid rows are dropped.
pub fn public_tiers(tiers: &[WholesaleTier]) -> Vec<WholesaleTier> {
    tiers
        .iter()
        .filter(|t| t.min_quantity > 0 && t.unit_price.is_positive())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amt(v: i64) -> Amount {
        Amount::from(v)
    }

    fn input(sku_id: Id, code: &str, min: i32, price: i64) -> WholesalePriceInput {
        WholesalePriceInput {
            sku_id,
            sku_code: code.into(),
            min_quantity: min,
            unit_price: amt(price),
        }
    }

    fn tier(sku_id: Id, code: &str, min: i32, price: i64) -> WholesaleTier {
        WholesaleTier {
            sku_id,
            sku_code: code.into(),
            min_quantity: min,
            unit_price: amt(price),
        }
    }

    #[test]
    fn sorts_tiers() {
        let tiers =
            normalize_wholesale_prices(&[input(0, "", 10, 70), input(0, "", 5, 80)]).unwrap();
        assert_eq!(tiers[0].min_quantity, 5);
        assert_eq!(tiers[0].unit_price.to_string(), "80.00");
        assert_eq!(tiers[1].min_quantity, 10);
    }

    // PRC-05
    #[test]
    fn rejects_invalid_tiers() {
        let cases = [
            vec![input(0, "", 0, 80)],
            vec![input(0, "", 5, 0)],
            vec![input(0, "", 5, 80), input(0, "", 5, 70)],
            vec![input(0, "", 5, 80), input(0, "", 10, 90)],
            vec![input(0, "", 5, 80), input(0, "", 10, 80)],
            // canonical scope duplicates / non-decreasing
            vec![input(5, "SKU-A", 10, 80), input(0, "SKU-A", 10, 70)],
            vec![input(5, "SKU-A", 5, 80), input(0, "SKU-A", 10, 90)],
        ];
        for case in cases {
            let err = normalize_wholesale_prices(&case).unwrap_err();
            assert_eq!(err.key(), WHOLESALE_PRICE_INVALID);
        }
        assert!(normalize_wholesale_prices(&[]).unwrap().is_empty());
    }

    #[test]
    fn same_quantity_allowed_for_different_skus() {
        let tiers = normalize_wholesale_prices(&[
            input(2, "SKU-B", 5, 70),
            input(1, "SKU-A", 5, 80),
            input(0, "", 5, 90),
        ])
        .unwrap();
        assert_eq!(tiers[0].sku_id, 0);
        assert_eq!(tiers[1].sku_code, "SKU-A");
        assert_eq!(tiers[2].sku_code, "SKU-B");
    }

    // PRC-02
    #[test]
    fn canonicalises_sku_identity() {
        let skus = [SkuRef {
            id: 21,
            code: "SKU-A".into(),
        }];
        let tiers =
            normalize_wholesale_prices_for_skus(&[input(0, "sku-a", 5, 80)], &skus).unwrap();
        assert_eq!((tiers[0].sku_id, tiers[0].sku_code.as_str()), (21, "SKU-A"));
        // foreign id
        assert!(normalize_wholesale_prices_for_skus(&[input(99, "", 5, 80)], &skus).is_err());
        // id/code mismatch
        assert!(normalize_wholesale_prices_for_skus(&[input(21, "SKU-X", 5, 80)], &skus).is_err());
        // unknown code
        assert!(normalize_wholesale_prices_for_skus(&[input(0, "SKU-X", 5, 80)], &skus).is_err());
    }

    #[test]
    fn matches_best_tier() {
        let tiers = [tier(0, "", 5, 80), tier(0, "", 10, 70)];
        let m = resolve_wholesale_unit_price(&tiers, amt(100), 0, "", 12, 12);
        assert!(m.matched);
        assert_eq!(m.unit_price, amt(70));
        assert_eq!(m.discount, amt(360));
        let below = resolve_wholesale_unit_price(&[tier(0, "", 5, 80)], amt(100), 0, "", 4, 4);
        assert!(!below.matched);
        assert_eq!(below.unit_price, amt(100));
        assert!(below.discount.is_zero());
    }

    // PRC-02
    #[test]
    fn sku_specific_tier_preferred_and_uses_line_quantity() {
        let tiers = [tier(0, "", 5, 80), tier(11, "SKU-A", 5, 70)];
        let m = resolve_wholesale_unit_price(&tiers, amt(100), 11, "SKU-A", 12, 6);
        assert_eq!((m.unit_price, m.discount), (amt(70), amt(180)));

        let only = [tier(11, "SKU-A", 5, 70)];
        assert!(!resolve_wholesale_unit_price(&only, amt(100), 11, "SKU-B", 6, 6).matched);
        assert!(!resolve_wholesale_unit_price(&only, amt(100), 12, "SKU-A", 6, 6).matched);

        // no fallback to shared tiers when a specific tier exists
        let tiers = [tier(0, "", 10, 80), tier(11, "SKU-A", 10, 70)];
        let m = resolve_wholesale_unit_price(&tiers, amt(100), 11, "SKU-A", 12, 6);
        assert!(!m.matched);
        assert_eq!(m.unit_price, amt(100));
        // other SKU uses the shared tier by product quantity
        let m = resolve_wholesale_unit_price(&tiers, amt(100), 12, "SKU-B", 12, 6);
        assert_eq!((m.unit_price, m.discount), (amt(80), amt(120)));
    }

    // PRC-02 (1): A/B each own tier
    #[test]
    fn per_sku_tiers_each_line() {
        let tiers = [tier(1, "A", 5, 70), tier(2, "B", 5, 60)];
        let a = resolve_wholesale_unit_price(&tiers, amt(100), 1, "A", 10, 5);
        let b = resolve_wholesale_unit_price(&tiers, amt(100), 2, "B", 10, 5);
        assert_eq!((a.unit_price, a.discount), (amt(70), amt(150)));
        assert_eq!((b.unit_price, b.discount), (amt(60), amt(200)));
        assert_eq!(a.discount + b.discount, amt(350));
    }

    // PRC-02 (2): shared tier never raises a cheaper SKU's price
    #[test]
    fn shared_tier_uses_product_total_and_never_raises_price() {
        let tiers = [tier(0, "", 10, 80)];
        let a = resolve_wholesale_unit_price(&tiers, amt(100), 1, "A", 12, 6);
        let b = resolve_wholesale_unit_price(&tiers, amt(50), 2, "B", 12, 6);
        assert_eq!((a.unit_price, a.discount), (amt(80), amt(120)));
        assert_eq!(
            (b.unit_price, b.discount, b.matched),
            (amt(50), Amount::ZERO, false)
        );
        assert!(!resolve_wholesale_unit_price(&tiers, amt(100), 1, "A", 9, 9).matched);
    }

    // PRC-05
    #[test]
    fn legacy_non_monotonic_data_picks_cheapest() {
        let tiers = [tier(0, "", 5, 80), tier(0, "", 10, 90)];
        let m = resolve_wholesale_unit_price(&tiers, amt(100), 0, "", 10, 10);
        assert_eq!((m.unit_price, m.discount), (amt(80), amt(200)));
        let higher = [tier(0, "", 5, 120)];
        assert!(!resolve_wholesale_unit_price(&higher, amt(100), 0, "", 5, 5).matched);
    }
}
