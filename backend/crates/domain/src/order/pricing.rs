//! The order pricing engine (`buildOrderResult`, `applyCouponDiscountToItems`, reseller
//! re-pricing of `ApplyToOrderBuildResult`).
//!
//! Stacking order (PRC-04, latest rule): base SKU price → the cheaper of promotion price and
//! wholesale price (never both, PRC-02/PRC-05/PRC-08) → member price on top of that unit →
//! coupon allocated over the eligible lines (PRC-03). Reseller orders get none of these
//! (their unit is the reseller site price). Every amount is rounded to two decimals, the
//! original amount accumulates pre-discount SKU prices, and a non-positive total is rejected
//! (PRC-09).

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use zs_shared::money::Amount;

use super::model::keys;
use crate::catalog::wholesale::{WholesaleTier, resolve_wholesale_unit_price};
use crate::marketing::coupon::{
    Coupon, CouponBuyer, EligibilityItem, SCOPE_PRODUCT, decode_scope_ids, evaluate_coupon,
    keys as coupon_keys,
};
use crate::marketing::member_level::{MemberLevel, MemberLevelPrice, resolve_member_price};
use crate::marketing::promotion::{Promotion, apply_promotion};
use crate::reseller::pricing::OrderPricingItem;
use crate::{Error, Id, Result};

/// A requested line (after merging duplicates) with everything needed to price it.
#[derive(Debug, Clone, PartialEq)]
pub struct PricingLine {
    pub product_id: Id,
    pub sku_id: Id,
    pub sku_code: String,
    pub quantity: i32,
    /// SKU price before any discount.
    pub base_price: Amount,
    /// Effective promotions of the product, `min_amount ASC`.
    pub promotions: Vec<Promotion>,
    pub wholesale_tiers: Vec<WholesaleTier>,
    /// Locked unit price of an API quote: the line never costs more (the difference is
    /// booked as a promotion discount without a promotion).
    pub price_cap: Option<Amount>,
}

/// The buyer's member level and its price overrides for the ordered products.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberContext {
    pub level: MemberLevel,
    pub prices: Vec<MemberLevelPrice>,
}

/// A coupon to apply with the buyer facts it is evaluated against.
#[derive(Debug, Clone, PartialEq)]
pub struct CouponContext {
    pub coupon: Coupon,
    pub buyer: CouponBuyer,
}

/// Everything the engine needs.
#[derive(Debug, Clone, PartialEq)]
pub struct PricingInput {
    pub lines: Vec<PricingLine>,
    /// Reseller-site order: no promotion, wholesale, member or coupon pricing.
    pub reseller: bool,
    pub member: Option<MemberContext>,
    pub coupon: Option<CouponContext>,
    pub now: DateTime<Utc>,
}

/// One priced line (amounts of the child order and its single item).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PricedLine {
    pub product_id: Id,
    pub sku_id: Id,
    pub quantity: i32,
    pub original_unit_price: Amount,
    pub unit_price: Amount,
    pub original_total_price: Amount,
    /// `unit_price × quantity` (before the coupon share).
    pub total_price: Amount,
    pub member_discount: Amount,
    pub promotion_discount: Amount,
    pub wholesale_discount: Amount,
    pub coupon_discount: Amount,
    pub promotion_id: Option<Id>,
}

impl PricedLine {
    /// What the buyer pays for the line (`total − coupon`, never negative).
    pub fn payable(&self) -> Amount {
        (self.total_price - self.coupon_discount).non_negative()
    }
}

/// Result of [`price_order`].
#[derive(Debug, Clone, PartialEq)]
pub struct PricingResult {
    pub lines: Vec<PricedLine>,
    pub original_amount: Amount,
    pub member_discount_amount: Amount,
    pub promotion_discount_amount: Amount,
    pub wholesale_discount_amount: Amount,
    /// Coupon discount (the original's `discount_amount`, PRC-11).
    pub discount_amount: Amount,
    pub total_amount: Amount,
    /// Set when every line used the same promotion.
    pub promotion_id: Option<Id>,
    pub applied_coupon: Option<Coupon>,
    pub member_level_id: Option<Id>,
}

fn price_invalid() -> Error {
    Error::bad_request(keys::PRODUCT_PRICE_INVALID)
}

/// Prices a whole order.
pub fn price_order(input: &PricingInput) -> Result<PricingResult> {
    if input.lines.is_empty() {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
    }
    let mut product_qty: HashMap<Id, i32> = HashMap::new();
    for line in &input.lines {
        if line.product_id <= 0 || line.quantity <= 0 {
            return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
        }
        *product_qty.entry(line.product_id).or_default() += line.quantity;
    }
    let member = if input.reseller {
        None
    } else {
        input.member.as_ref()
    };

    let mut lines = Vec::with_capacity(input.lines.len());
    let mut original = Amount::ZERO;
    let mut member_total = Amount::ZERO;
    let mut promotion_total = Amount::ZERO;
    let mut wholesale_total = Amount::ZERO;
    let mut promotion_ids: Vec<Option<Id>> = Vec::with_capacity(input.lines.len());

    for line in &input.lines {
        let qty = i64::from(line.quantity);
        let base = line.base_price;
        // 1. promotion price
        let (mut promotion, promo_unit) = if input.reseller {
            (None, base)
        } else {
            match apply_promotion(&line.promotions, base, line.quantity)? {
                Some((p, unit)) => (Some(p), unit),
                None => (None, base),
            }
        };
        let mut unit = promo_unit;
        let mut promotion_discount = if promotion.is_some() && base > promo_unit {
            (base - promo_unit) * qty
        } else {
            Amount::ZERO
        };
        // 2. the cheaper of promotion and wholesale price, never both
        let wholesale = if input.reseller {
            None
        } else {
            let product_quantity = product_qty
                .get(&line.product_id)
                .copied()
                .filter(|q| *q > 0)
                .unwrap_or(line.quantity);
            Some(resolve_wholesale_unit_price(
                &line.wholesale_tiers,
                base,
                line.sku_id,
                &line.sku_code,
                product_quantity,
                line.quantity,
            ))
        };
        let mut wholesale_discount = Amount::ZERO;
        match wholesale {
            Some(w) if w.matched && w.unit_price < unit => {
                unit = w.unit_price;
                promotion = None;
                promotion_discount = Amount::ZERO;
                wholesale_discount = w.discount;
                wholesale_total += w.discount;
            }
            _ => {
                if promotion_discount.is_positive() {
                    promotion_total += promotion_discount;
                }
            }
        }
        // 3. member price on top of the unit reached so far
        let mut member_discount = Amount::ZERO;
        if let Some(ctx) = member {
            let resolved = resolve_member_price(
                Some(&ctx.level),
                &ctx.prices,
                line.product_id,
                line.sku_id,
                unit,
            );
            if resolved.price < unit {
                member_discount = (unit - resolved.price) * qty;
                member_total += member_discount;
                unit = resolved.price;
            }
        }
        // 4. a promotion without an actual discount is not recorded
        if promotion.is_some() && promotion_discount.is_zero() && base <= promo_unit {
            promotion = None;
        }
        // 5. a quote's locked price caps the unit price
        if let Some(cap) = line.price_cap.filter(|c| c.is_positive() && *c < unit) {
            let diff = (unit - cap) * qty;
            promotion_discount += diff;
            promotion_total += diff;
            unit = cap;
        }
        if !unit.is_positive() {
            return Err(price_invalid());
        }
        let original_total = base * qty;
        original += original_total;
        let promotion_id = promotion.as_ref().map(|p| p.id);
        promotion_ids.push(promotion_id);
        lines.push(PricedLine {
            product_id: line.product_id,
            sku_id: line.sku_id,
            quantity: line.quantity,
            original_unit_price: base,
            unit_price: unit,
            original_total_price: original_total,
            total_price: unit * qty,
            member_discount,
            promotion_discount,
            wholesale_discount,
            coupon_discount: Amount::ZERO,
            promotion_id,
        });
    }

    let promotion_id = match promotion_ids.first().copied().flatten() {
        Some(first) if promotion_ids.iter().all(|p| *p == Some(first)) => Some(first),
        _ => None,
    };

    let mut applied_coupon = None;
    let mut discount = Amount::ZERO;
    if let Some(ctx) = input.coupon.as_ref().filter(|_| !input.reseller) {
        let items: Vec<EligibilityItem> = lines
            .iter()
            .map(|l| EligibilityItem {
                product_id: l.product_id,
                quantity: l.quantity,
                total_price: l.total_price,
                wholesale_discount: l.wholesale_discount,
            })
            .collect();
        let (raw, _) = evaluate_coupon(&ctx.coupon, &ctx.buyer, &items, input.now)?;
        if raw.is_positive() {
            allocate_coupon(&mut lines, &ctx.coupon, raw)?;
            discount = lines.iter().map(|l| l.coupon_discount).sum();
        }
        applied_coupon = Some(ctx.coupon.clone());
    }

    let total: Amount = lines.iter().map(PricedLine::payable).sum();
    if !total.is_positive() {
        return Err(Error::bad_request(keys::ORDER_AMOUNT_INVALID));
    }
    Ok(PricingResult {
        lines,
        original_amount: original,
        member_discount_amount: member_total,
        promotion_discount_amount: promotion_total,
        wholesale_discount_amount: wholesale_total,
        discount_amount: discount,
        total_amount: total,
        promotion_id,
        applied_coupon,
        member_level_id: member.map(|m| m.level.id),
    })
}

/// Allocates a coupon discount over its eligible lines proportionally to their totals,
/// the last line taking the remainder (`applyCouponDiscountToItems`, PRC-03).
pub fn allocate_coupon(lines: &mut [PricedLine], coupon: &Coupon, discount: Amount) -> Result<()> {
    if !discount.is_positive() {
        return Ok(());
    }
    let scope_err = || Error::bad_request(coupon_keys::SCOPE_INVALID);
    if !coupon.scope_type.trim().eq_ignore_ascii_case(SCOPE_PRODUCT) {
        return Err(scope_err());
    }
    let ids: HashSet<Id> = decode_scope_ids(&coupon.scope_ref_ids).ok_or_else(scope_err)?;
    let mut eligible = Vec::new();
    let mut eligible_total = Amount::ZERO;
    let mut matched = 0usize;
    let mut excluded = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if !ids.contains(&line.product_id) {
            continue;
        }
        matched += 1;
        if coupon.disabled_wholesale_price && line.wholesale_discount.is_positive() {
            excluded += 1;
            continue;
        }
        eligible.push(i);
        eligible_total += line.total_price;
    }
    if eligible.is_empty() || !eligible_total.is_positive() {
        if matched > 0 && excluded == matched {
            return Err(Error::bad_request(coupon_keys::WHOLESALE_DISABLED));
        }
        return Err(scope_err());
    }
    let mut remaining = discount;
    let last = eligible.len() - 1;
    for (n, idx) in eligible.into_iter().enumerate() {
        let line_total = lines[idx].total_price;
        let alloc = if n == last {
            remaining.non_negative().min(line_total)
        } else {
            let share =
                Amount::new(discount.decimal() * line_total.decimal() / eligible_total.decimal());
            share.min(remaining).non_negative().min(line_total)
        };
        lines[idx].coupon_discount = alloc;
        remaining -= alloc;
    }
    Ok(())
}

/// Re-prices a reseller order with the reseller site prices (`ApplyToOrderBuildResult`):
/// every discount is zeroed, the unit becomes the reseller unit and totals are recomputed.
/// `items` must be aligned with `result.lines`.
pub fn apply_reseller_prices(result: &mut PricingResult, items: &[OrderPricingItem]) -> Result<()> {
    if items.len() != result.lines.len() {
        return Err(Error::bad_request(keys::ORDER_ITEM_INVALID));
    }
    for (line, item) in result.lines.iter_mut().zip(items) {
        let unit = Amount::new(item.reseller_unit_amount);
        let total = Amount::new(item.reseller_total_amount);
        line.original_unit_price = unit;
        line.unit_price = unit;
        line.original_total_price = total;
        line.total_price = total;
        line.member_discount = Amount::ZERO;
        line.promotion_discount = Amount::ZERO;
        line.wholesale_discount = Amount::ZERO;
        line.coupon_discount = Amount::ZERO;
        line.promotion_id = None;
    }
    result.original_amount = result.lines.iter().map(|l| l.total_price).sum();
    result.total_amount = result.lines.iter().map(PricedLine::payable).sum();
    result.discount_amount = Amount::ZERO;
    result.member_discount_amount = Amount::ZERO;
    result.promotion_discount_amount = Amount::ZERO;
    result.wholesale_discount_amount = Amount::ZERO;
    result.applied_coupon = None;
    result.promotion_id = None;
    result.member_level_id = None;
    Ok(())
}

/// Coupon share of a child order's payable total (`plan.TotalAmount − plan.CouponDiscount`).
pub fn child_total(line: &PricedLine) -> Amount {
    line.payable()
}

/// Reseller profit of one line when the order is profit-eligible.
pub fn child_profit(item: Option<&OrderPricingItem>, eligible: bool) -> Amount {
    match item {
        Some(i) if eligible => Amount::new(i.profit_amount),
        _ => Amount::ZERO,
    }
}

/// `Decimal` helper for tests and callers that build lines by hand.
pub fn dec(v: &str) -> Decimal {
    v.parse().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::marketing::coupon::testkit::coupon;
    use crate::marketing::member_level::testkit::{level, price};
    use crate::marketing::promotion::testkit::promo;
    use chrono::Utc;

    fn a(s: &str) -> Amount {
        Amount::new(dec(s))
    }

    fn line(product: Id, sku: Id, qty: i32, base: &str) -> PricingLine {
        PricingLine {
            product_id: product,
            sku_id: sku,
            sku_code: format!("S{sku}"),
            quantity: qty,
            base_price: a(base),
            promotions: vec![],
            wholesale_tiers: vec![],
            price_cap: None,
        }
    }

    // A locked quote price caps the unit price; a higher cap changes nothing.
    #[test]
    fn price_cap_limits_unit_price() {
        let mut l = line(1, 11, 2, "10.00");
        l.price_cap = Some(a("8.50"));
        let r = price_order(&input(vec![l])).unwrap();
        assert_eq!(r.lines[0].unit_price, a("8.50"));
        assert_eq!(r.lines[0].promotion_discount, a("3.00"));
        assert_eq!(r.total_amount, a("17.00"));
        let mut l = line(1, 11, 2, "10.00");
        l.price_cap = Some(a("12.00"));
        let r = price_order(&input(vec![l])).unwrap();
        assert_eq!(r.total_amount, a("20.00"));
    }

    fn tier(sku_id: Id, min: i32, price: &str) -> WholesaleTier {
        WholesaleTier {
            sku_id,
            sku_code: if sku_id > 0 {
                format!("S{sku_id}")
            } else {
                String::new()
            },
            min_quantity: min,
            unit_price: a(price),
        }
    }

    fn input(lines: Vec<PricingLine>) -> PricingInput {
        PricingInput {
            lines,
            reseller: false,
            member: None,
            coupon: None,
            now: Utc::now(),
        }
    }

    fn buyer() -> CouponBuyer {
        CouponBuyer {
            user_id: 7,
            is_guest: false,
            member_level_id: 0,
            used_by_user: 0,
        }
    }

    fn with_coupon(mut i: PricingInput, c: Coupon) -> PricingInput {
        i.coupon = Some(CouponContext {
            coupon: c,
            buyer: buyer(),
        });
        i
    }

    /// PRC-09 (3): original accumulates pre-promotion prices.
    #[test]
    fn prc_09_original_amount_uses_base_price() {
        let mut l = line(1, 1, 2, "59.90");
        l.promotions = vec![promo(3, "percent", 20, 0)];
        let r = price_order(&input(vec![l])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.original_amount, a("119.80"));
        assert_eq!(r.promotion_discount_amount, a("23.96"));
        assert_eq!(r.discount_amount, Amount::ZERO);
        assert_eq!(r.total_amount, a("95.84"));
        assert_eq!(r.promotion_id, Some(3));
        assert_eq!(r.lines[0].unit_price, a("47.92"));
    }

    /// PRC-09 (1)(2): zero unit price / zero total are rejected.
    #[test]
    fn prc_09_zero_price_guards() {
        let mut l = line(1, 1, 1, "10");
        l.promotions = vec![promo(3, "percent", 100, 0)];
        let err = price_order(&input(vec![l]))
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.product_price_invalid"));

        let mut c = coupon("fixed", 10, "[1]");
        c.max_discount = Amount::ZERO;
        let err = price_order(&with_coupon(input(vec![line(1, 1, 1, "10")]), c))
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.order_amount_invalid"));
    }

    /// PRC-04 (1): wholesale beats a weaker promotion; a stronger promotion beats wholesale;
    /// the coupon applies after.
    #[test]
    fn prc_04_promotion_vs_wholesale_then_coupon() {
        let mut l = line(1, 0, 5, "100");
        l.promotions = vec![promo(3, "percent", 10, 0)];
        l.wholesale_tiers = vec![tier(0, 5, "80")];
        let c = coupon("percent", 10, "[1]");
        let r = price_order(&with_coupon(input(vec![l.clone()]), c.clone()))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.lines[0].unit_price, a("80"));
        assert_eq!(r.wholesale_discount_amount, a("100"));
        assert_eq!(r.promotion_discount_amount, Amount::ZERO);
        assert_eq!(r.lines[0].promotion_id, None);
        assert_eq!(r.discount_amount, a("40"));
        assert_eq!(r.total_amount, a("360"));

        l.promotions = vec![promo(3, "percent", 30, 0)];
        let r = price_order(&with_coupon(input(vec![l]), c)).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.lines[0].unit_price, a("70"));
        assert_eq!(r.promotion_discount_amount, a("150"));
        assert_eq!(r.wholesale_discount_amount, Amount::ZERO);
        assert_eq!(r.discount_amount, a("35"));
        assert_eq!(r.total_amount, a("315"));
    }

    /// PRC-04 (1): member price stacks on the wholesale unit.
    #[test]
    fn prc_04_member_price_on_top_of_wholesale() {
        let mut l = line(1, 0, 5, "100");
        l.wholesale_tiers = vec![tier(0, 5, "80")];
        let mut i = input(vec![l]);
        i.member = Some(MemberContext {
            level: level(2, 10, 90),
            prices: vec![],
        });
        let r = price_order(&i).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.lines[0].unit_price, a("72"));
        assert_eq!(r.member_discount_amount, a("40"));
        assert_eq!(r.wholesale_discount_amount, a("100"));
        assert_eq!(r.total_amount, a("360"));
        assert_eq!(r.member_level_id, Some(2));
    }

    /// PRC-04 (2) + PRC-11: promotion 80, member override below it, coupon split.
    #[test]
    fn prc_11_amount_semantics() {
        let mut l = line(1, 1, 2, "100");
        l.promotions = vec![promo(3, "special_price", 80, 0)];
        let mut i = input(vec![l]);
        i.member = Some(MemberContext {
            level: level(2, 10, 0),
            prices: vec![price(2, 1, 1, "75")],
        });
        let i = with_coupon(i, coupon("fixed", 10, "[1]"));
        let r = price_order(&i).unwrap_or_else(|e| panic!("{e}"));
        let l = &r.lines[0];
        assert_eq!(l.original_unit_price, a("100"));
        assert_eq!(l.unit_price, a("75"));
        assert_eq!(l.total_price, a("150"));
        assert_eq!(l.promotion_discount, a("40"));
        assert_eq!(l.member_discount, a("10"));
        assert_eq!(l.coupon_discount, a("10"));
        assert_eq!(l.payable(), a("140"));
        // 共减 = promotion + member + coupon = 60
        assert_eq!(
            l.promotion_discount + l.member_discount + l.coupon_discount,
            a("60")
        );
    }

    /// PRC-07: an inactive member level gives no member price.
    #[test]
    fn prc_07_inactive_level() {
        let mut lvl = level(2, 10, 0);
        lvl.is_active = false;
        let mut i = input(vec![line(1, 1, 1, "10")]);
        i.member = Some(MemberContext {
            level: lvl,
            prices: vec![price(2, 1, 1, "8")],
        });
        let r = price_order(&i).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.lines[0].unit_price, a("10"));
        assert_eq!(r.member_discount_amount, Amount::ZERO);
    }

    /// PRC-02 (2): shared tiers count every SKU of the product; line discounts differ.
    #[test]
    fn prc_02_shared_tier_uses_product_quantity() {
        let mut la = line(1, 1, 6, "100");
        la.wholesale_tiers = vec![tier(0, 10, "80")];
        let mut lb = line(1, 2, 6, "50");
        lb.wholesale_tiers = vec![tier(0, 10, "80")];
        let r = price_order(&input(vec![la.clone(), lb])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.original_amount, a("900"));
        assert_eq!(r.wholesale_discount_amount, a("120"));
        assert_eq!(r.total_amount, a("780"));
        assert_eq!(r.lines[0].unit_price, a("80"));
        assert_eq!(r.lines[1].unit_price, a("50"));
        assert_eq!(r.lines[1].wholesale_discount, Amount::ZERO);
        la.quantity = 9;
        let r = price_order(&input(vec![la])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.wholesale_discount_amount, Amount::ZERO);
    }

    /// PRC-02 (1): SKU-specific tiers never fall back to shared ones.
    #[test]
    fn prc_02_specific_tiers() {
        let tiers = vec![tier(0, 10, "80"), tier(1, 10, "70")];
        let mut la = line(1, 1, 6, "100");
        la.wholesale_tiers = tiers.clone();
        let mut lb = line(1, 2, 6, "100");
        lb.wholesale_tiers = tiers;
        let r = price_order(&input(vec![la, lb])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.lines[0].unit_price, a("100"));
        assert_eq!(r.lines[1].unit_price, a("80"));
        assert_eq!(r.total_amount, a("1080"));
    }

    /// PRC-05: the cheapest satisfied tier wins even with dirty data.
    #[test]
    fn prc_05_lowest_tier() {
        let mut l = line(1, 0, 10, "100");
        l.wholesale_tiers = vec![tier(0, 5, "80"), tier(0, 10, "90")];
        let r = price_order(&input(vec![l])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.lines[0].unit_price, a("80"));
        assert_eq!(r.wholesale_discount_amount, a("200"));
    }

    /// PRC-08: tiered promotion picks by SKU subtotal.
    #[test]
    fn prc_08_tiered_promotion() {
        let promos = vec![
            promo(1, "percent", 5, 0),
            promo(2, "percent", 10, 100),
            promo(3, "percent", 20, 300),
        ];
        let price_for = |qty| {
            let mut l = line(1, 1, qty, "50");
            l.promotions = promos.clone();
            price_order(&input(vec![l]))
                .map(|r| r.lines[0].unit_price)
                .unwrap_or_default()
        };
        assert_eq!(price_for(1), a("47.50"));
        assert_eq!(price_for(2), a("45"));
        assert_eq!(price_for(6), a("40"));
    }

    /// PRC-03: coupon excluding wholesale lines only discounts the other lines.
    #[test]
    fn prc_03_coupon_skips_wholesale_lines() {
        let mut la = line(1, 0, 5, "100");
        la.wholesale_tiers = vec![tier(0, 5, "80")];
        let lb = line(2, 0, 1, "100");
        let mut c = coupon("percent", 10, "[1,2]");
        c.disabled_wholesale_price = true;
        let r = price_order(&with_coupon(input(vec![la.clone(), lb]), c.clone()))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.discount_amount, a("10"));
        assert_eq!(r.lines[0].coupon_discount, Amount::ZERO);
        assert_eq!(r.lines[1].coupon_discount, a("10"));
        assert_eq!(r.total_amount, a("490"));
        // only the wholesale line in scope → dedicated error
        let err = price_order(&with_coupon(input(vec![la]), c))
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.coupon_wholesale_disabled"));
    }

    /// PRC-03: per-item fixed coupon.
    #[test]
    fn prc_03_per_item_fixed() {
        let mut c = coupon("fixed", 5, "[1]");
        c.per_item_discount = true;
        let r = price_order(&with_coupon(input(vec![line(1, 0, 3, "100")]), c.clone()))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.discount_amount, a("15"));
        c.max_discount = a("10");
        let r = price_order(&with_coupon(input(vec![line(1, 0, 3, "100")]), c))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.discount_amount, a("10"));
    }

    /// Coupon allocation: proportional shares, the last line takes the rest.
    #[test]
    fn coupon_allocation_sums_exactly() {
        let r = price_order(&with_coupon(
            input(vec![
                line(1, 0, 1, "10"),
                line(2, 0, 1, "10"),
                line(3, 0, 1, "10"),
            ]),
            coupon("fixed", 10, "[1,2,3]"),
        ))
        .unwrap_or_else(|e| panic!("{e}"));
        let shares: Vec<Amount> = r.lines.iter().map(|l| l.coupon_discount).collect();
        assert_eq!(shares, vec![a("3.33"), a("3.33"), a("3.34")]);
        assert_eq!(r.discount_amount, a("10"));
        assert_eq!(r.total_amount, a("20"));
    }

    #[test]
    fn coupon_scope_mismatch_is_rejected() {
        let err = price_order(&with_coupon(
            input(vec![line(1, 0, 1, "10")]),
            coupon("fixed", 1, "[9]"),
        ))
        .err()
        .map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.coupon_scope_invalid"));
    }

    #[test]
    fn order_promotion_id_requires_every_line() {
        let mut l1 = line(1, 0, 1, "10");
        l1.promotions = vec![promo(3, "fixed", 1, 0)];
        let l2 = line(2, 0, 1, "10");
        let r = price_order(&input(vec![l1.clone(), l2])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.promotion_id, None);
        assert_eq!(r.lines[0].promotion_id, Some(3));
        let mut l3 = line(3, 0, 1, "10");
        l3.promotions = vec![promo(3, "fixed", 1, 0)];
        let r = price_order(&input(vec![l1, l3])).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.promotion_id, Some(3));
    }

    /// Reseller orders ignore promotions, wholesale, member prices and coupons.
    #[test]
    fn reseller_orders_skip_discounts() {
        let mut l = line(1, 0, 5, "100");
        l.promotions = vec![promo(3, "percent", 30, 0)];
        l.wholesale_tiers = vec![tier(0, 5, "80")];
        let mut i = input(vec![l]);
        i.reseller = true;
        i.member = Some(MemberContext {
            level: level(2, 10, 50),
            prices: vec![],
        });
        let r = price_order(&i).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.total_amount, a("500"));
        assert_eq!(r.member_level_id, None);
        let item = OrderPricingItem {
            product_id: 1,
            sku_id: 0,
            quantity: 5,
            child_order_id: 0,
            base_unit_amount: dec("100"),
            reseller_unit_amount: dec("120"),
            base_total_amount: dec("500"),
            reseller_total_amount: dec("600"),
            profit_amount: dec("100"),
            pricing_mode: crate::reseller::PricingMode::Inherit,
            rule_source: crate::reseller::pricing::RuleSource::Inherit,
            setting_id: None,
            order_id: 0,
            order_item_id: 0,
        };
        let mut r = r;
        apply_reseller_prices(&mut r, std::slice::from_ref(&item))
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(r.total_amount, a("600"));
        assert_eq!(r.original_amount, a("600"));
        assert_eq!(r.lines[0].unit_price, a("120"));
        assert_eq!(child_profit(Some(&item), true), a("100"));
        assert_eq!(child_profit(Some(&item), false), Amount::ZERO);
    }
}
