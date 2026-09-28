//! Coupons: admin validation, eligibility / discount rules and the usage ledger port.

use std::collections::HashSet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::{Error, Id, Result};

pub mod keys {
    pub const INVALID: &str = "error.coupon_invalid";
    pub const NOT_FOUND: &str = "error.coupon_not_found";
    pub const INACTIVE: &str = "error.coupon_inactive";
    pub const NOT_STARTED: &str = "error.coupon_not_started";
    pub const EXPIRED: &str = "error.coupon_expired";
    pub const USAGE_LIMIT: &str = "error.coupon_usage_limit";
    pub const PER_USER_LIMIT: &str = "error.coupon_per_user_limit";
    pub const MIN_AMOUNT: &str = "error.coupon_min_amount";
    pub const SCOPE_INVALID: &str = "error.coupon_scope_invalid";
    pub const ROLE_NOT_ALLOWED: &str = "error.coupon_payment_role_not_allowed";
    pub const ROLE_GUEST_ONLY: &str = "error.coupon_payment_role_guest_only";
    pub const ROLE_MEMBER_ONLY: &str = "error.coupon_payment_role_member_only";
    pub const MEMBER_LEVEL_NOT_ALLOWED: &str = "error.coupon_member_level_not_allowed";
    pub const WHOLESALE_DISABLED: &str = "error.coupon_wholesale_disabled";
    pub const CREATE_FAILED: &str = "error.coupon_create_failed";
    pub const UPDATE_FAILED: &str = "error.coupon_update_failed";
    pub const DELETE_FAILED: &str = "error.coupon_delete_failed";
    pub const FETCH_FAILED: &str = "error.coupon_fetch_failed";
}

pub const ROLE_GUEST: &str = "guest";
pub const ROLE_MEMBER: &str = "member";
pub const SCOPE_PRODUCT: &str = "product";

/// Coupon kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CouponType {
    Fixed,
    Percent,
}

impl CouponType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "fixed" => Some(Self::Fixed),
            "percent" => Some(Self::Percent),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Percent => "percent",
        }
    }
}

/// A coupon (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Coupon {
    pub id: Id,
    pub code: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub value: Amount,
    pub min_amount: Amount,
    pub max_discount: Amount,
    pub usage_limit: i32,
    pub used_count: i32,
    pub per_user_limit: i32,
    pub disabled_wholesale_price: bool,
    pub per_item_discount: bool,
    pub payment_roles: Vec<String>,
    pub member_levels: Vec<Id>,
    pub scope_type: String,
    /// JSON array text such as `[1,2,3]`.
    pub scope_ref_ids: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A coupon usage record.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CouponUsage {
    pub id: Id,
    pub coupon_id: Id,
    pub user_id: Id,
    pub order_id: Id,
    pub discount_amount: Amount,
    pub created_at: DateTime<Utc>,
}

/// Admin create/update input (both use the same request).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CouponInput {
    pub code: String,
    pub kind: String,
    pub value: Amount,
    pub min_amount: Amount,
    pub max_discount: Amount,
    pub usage_limit: i32,
    pub per_user_limit: i32,
    pub disabled_wholesale_price: Option<bool>,
    pub per_item_discount: Option<bool>,
    pub payment_roles: Vec<String>,
    pub member_levels: Vec<Id>,
    pub scope_ref_ids: Vec<Id>,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_active: Option<bool>,
}

/// Validated fields to persist.
#[derive(Debug, Clone, PartialEq)]
pub struct CouponFields {
    pub code: String,
    pub kind: CouponType,
    pub value: Amount,
    pub min_amount: Amount,
    pub max_discount: Amount,
    pub usage_limit: i32,
    pub per_user_limit: i32,
    pub disabled_wholesale_price: bool,
    pub per_item_discount: bool,
    pub payment_roles: Vec<String>,
    pub member_levels: Vec<Id>,
    pub scope_ref_ids: String,
    pub starts_at: Option<DateTime<Utc>>,
    pub ends_at: Option<DateTime<Utc>>,
    pub is_active: bool,
}

/// Validates admin input. `existing` is the stored coupon on update (flags default to it).
/// Code uniqueness is checked by the service.
pub fn validate_coupon(input: &CouponInput, existing: Option<&Coupon>) -> Result<CouponFields> {
    let invalid = || Error::bad_request(keys::INVALID);
    let code = input.code.trim();
    if code.is_empty() {
        return Err(invalid());
    }
    let kind = CouponType::parse(&input.kind).ok_or_else(invalid)?;
    if !input.value.is_positive() {
        return Err(invalid());
    }
    if kind == CouponType::Percent && input.value > Amount::from(100) {
        return Err(invalid());
    }
    let scope_ref_ids = encode_scope_ids(&input.scope_ref_ids)?;
    let payment_roles = normalize_roles(&input.payment_roles)?;
    let member_levels = normalize_levels(&input.member_levels);
    if let (Some(s), Some(e)) = (input.starts_at, input.ends_at)
        && e < s
    {
        return Err(invalid());
    }
    let per_item = input
        .per_item_discount
        .or(existing.map(|c| c.per_item_discount))
        .unwrap_or(false);
    Ok(CouponFields {
        code: code.to_owned(),
        kind,
        value: input.value,
        min_amount: input.min_amount,
        max_discount: input.max_discount,
        usage_limit: input.usage_limit,
        per_user_limit: input.per_user_limit,
        disabled_wholesale_price: input
            .disabled_wholesale_price
            .or(existing.map(|c| c.disabled_wholesale_price))
            .unwrap_or(false),
        per_item_discount: kind == CouponType::Fixed && per_item,
        payment_roles,
        member_levels,
        scope_ref_ids,
        starts_at: input.starts_at,
        ends_at: input.ends_at,
        is_active: input
            .is_active
            .or(existing.map(|c| c.is_active))
            .unwrap_or(true),
    })
}

/// Encodes scope ids as compact JSON (`[1,2,3]`, no spaces — required by the boundary filter, PRC-15).
pub fn encode_scope_ids(ids: &[Id]) -> Result<String> {
    if ids.is_empty() {
        return Err(Error::bad_request(keys::SCOPE_INVALID));
    }
    let parts: Vec<String> = ids.iter().map(ToString::to_string).collect();
    Ok(format!("[{}]", parts.join(",")))
}

/// Decodes stored scope ids (zero dropped); malformed JSON is an error.
pub fn decode_scope_ids(raw: &str) -> Option<HashSet<Id>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(HashSet::new());
    }
    serde_json::from_str::<Vec<Id>>(trimmed)
        .ok()
        .map(|ids| ids.into_iter().filter(|id| *id != 0).collect())
}

/// LIKE patterns matching `id` as a whole element of a compact JSON array (PRC-15).
pub fn scope_id_patterns(id: Id) -> [String; 4] {
    [
        format!("[{id}]"),
        format!("[{id},%"),
        format!("%,{id},%"),
        format!("%,{id}]"),
    ]
}

fn normalize_roles(raw: &[String]) -> Result<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for item in raw {
        let role = item.trim().to_ascii_lowercase();
        if role.is_empty() {
            continue;
        }
        if role != ROLE_GUEST && role != ROLE_MEMBER {
            return Err(Error::bad_request(keys::INVALID));
        }
        if !out.contains(&role) {
            out.push(role);
        }
    }
    Ok(out)
}

fn normalize_levels(raw: &[Id]) -> Vec<Id> {
    let mut seen = HashSet::new();
    raw.iter()
        .copied()
        .filter(|id| *id != 0 && seen.insert(*id))
        .collect()
}

/// Read-only snapshot of an order line for coupon calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EligibilityItem {
    pub product_id: Id,
    pub quantity: i32,
    /// Line total after promotion / wholesale / member pricing.
    pub total_price: Amount,
    pub wholesale_discount: Amount,
}

/// Who is ordering and how often they already used the coupon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponBuyer {
    pub user_id: Id,
    pub is_guest: bool,
    pub member_level_id: Id,
    /// Non-deleted usages of this coupon by `user_id` (only consulted when `per_user_limit > 0`).
    pub used_by_user: u64,
}

/// Eligible subtotal / quantity after scope and wholesale exclusion (PRC-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eligibility {
    pub subtotal: Amount,
    pub quantity: i32,
}

/// Payment-role restriction; the single-role variants give a precise message.
pub fn check_payment_role(coupon: &Coupon, is_guest: bool) -> Result<()> {
    if coupon.payment_roles.is_empty() {
        return Ok(());
    }
    let target = if is_guest { ROLE_GUEST } else { ROLE_MEMBER };
    if coupon
        .payment_roles
        .iter()
        .any(|r| r.trim().eq_ignore_ascii_case(target))
    {
        return Ok(());
    }
    let roles: HashSet<String> = coupon
        .payment_roles
        .iter()
        .map(|r| r.trim().to_ascii_lowercase())
        .filter(|r| r == ROLE_GUEST || r == ROLE_MEMBER)
        .collect();
    if roles.len() == 1 {
        if roles.contains(ROLE_GUEST) {
            return Err(Error::bad_request(keys::ROLE_GUEST_ONLY));
        }
        return Err(Error::bad_request(keys::ROLE_MEMBER_ONLY));
    }
    Err(Error::bad_request(keys::ROLE_NOT_ALLOWED))
}

/// Member-level restriction (guests/level 0 never match a restricted coupon).
pub fn check_member_level(coupon: &Coupon, member_level_id: Id) -> Result<()> {
    if coupon.member_levels.is_empty()
        || (member_level_id != 0 && coupon.member_levels.contains(&member_level_id))
    {
        Ok(())
    } else {
        Err(Error::bad_request(keys::MEMBER_LEVEL_NOT_ALLOWED))
    }
}

/// Status, time window and total usage checks.
pub fn check_availability(coupon: &Coupon, now: DateTime<Utc>) -> Result<()> {
    if !coupon.is_active {
        return Err(Error::bad_request(keys::INACTIVE));
    }
    if coupon.starts_at.is_some_and(|s| now < s) {
        return Err(Error::bad_request(keys::NOT_STARTED));
    }
    if coupon.ends_at.is_some_and(|e| now > e) {
        return Err(Error::bad_request(keys::EXPIRED));
    }
    if coupon.usage_limit > 0 && coupon.used_count >= coupon.usage_limit {
        return Err(Error::bad_request(keys::USAGE_LIMIT));
    }
    Ok(())
}

/// Per-user usage limit (only for logged-in users).
pub fn check_per_user_limit(coupon: &Coupon, user_id: Id, used_by_user: u64) -> Result<()> {
    if coupon.per_user_limit > 0
        && user_id != 0
        && used_by_user >= u64::try_from(coupon.per_user_limit).unwrap_or(0)
    {
        return Err(Error::bad_request(keys::PER_USER_LIMIT));
    }
    Ok(())
}

/// Lines in scope minus wholesale lines when the coupon excludes them (PRC-03).
pub fn eligibility(coupon: &Coupon, items: &[EligibilityItem]) -> Result<Eligibility> {
    let scope_err = || Error::bad_request(keys::SCOPE_INVALID);
    if !coupon.scope_type.trim().eq_ignore_ascii_case(SCOPE_PRODUCT) {
        return Err(scope_err());
    }
    let ids = decode_scope_ids(&coupon.scope_ref_ids).ok_or_else(scope_err)?;
    if ids.is_empty() {
        return Err(scope_err());
    }
    let mut subtotal = Amount::ZERO;
    let mut quantity = 0i32;
    let mut matched = 0usize;
    let mut excluded = 0usize;
    for item in items.iter().filter(|i| ids.contains(&i.product_id)) {
        matched += 1;
        if coupon.disabled_wholesale_price && item.wholesale_discount.is_positive() {
            excluded += 1;
            continue;
        }
        subtotal += item.total_price;
        if item.quantity > 0 {
            quantity += item.quantity;
        }
    }
    if subtotal.is_zero() {
        if matched > 0 && excluded == matched {
            return Err(Error::bad_request(keys::WHOLESALE_DISABLED));
        }
        return Err(scope_err());
    }
    Ok(Eligibility { subtotal, quantity })
}

/// Raw discount before caps: fixed (× eligible quantity when per-item) or percent of the eligible subtotal.
pub fn raw_discount(coupon: &Coupon, eligible: Eligibility) -> Result<Amount> {
    if !coupon.value.is_positive() {
        return Err(Error::bad_request(keys::INVALID));
    }
    match CouponType::parse(&coupon.kind) {
        Some(CouponType::Fixed) if coupon.per_item_discount => {
            if eligible.quantity <= 0 {
                return Err(Error::bad_request(keys::SCOPE_INVALID));
            }
            Ok(coupon.value * i64::from(eligible.quantity))
        }
        Some(CouponType::Fixed) => Ok(coupon.value),
        Some(CouponType::Percent) => Ok(Amount::new(
            eligible.subtotal.decimal() * coupon.value.decimal() / Decimal::ONE_HUNDRED,
        )),
        None => Err(Error::bad_request(keys::INVALID)),
    }
}

/// Full coupon evaluation in the original order: availability → role → level → per-user
/// limit → scope/eligibility → min amount → discount capped by `max_discount` and the
/// eligible subtotal. Returns the discount and the eligible subtotal.
pub fn evaluate_coupon(
    coupon: &Coupon,
    buyer: &CouponBuyer,
    items: &[EligibilityItem],
    now: DateTime<Utc>,
) -> Result<(Amount, Eligibility)> {
    check_availability(coupon, now)?;
    check_payment_role(coupon, buyer.is_guest)?;
    check_member_level(coupon, buyer.member_level_id)?;
    check_per_user_limit(coupon, buyer.user_id, buyer.used_by_user)?;
    let eligible = eligibility(coupon, items)?;
    if eligible.subtotal < coupon.min_amount {
        return Err(Error::bad_request(keys::MIN_AMOUNT));
    }
    let mut discount = raw_discount(coupon, eligible)?;
    if coupon.max_discount.is_positive() && discount > coupon.max_discount {
        discount = coupon.max_discount;
    }
    Ok((discount.min(eligible.subtotal), eligible))
}

/// Admin list filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CouponFilter {
    pub id: Id,
    pub code: String,
    pub scope_ref_id: Id,
    pub is_active: Option<bool>,
}

/// Persistence port. Soft-deleted rows are never returned.
#[async_trait]
pub trait CouponRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<Coupon>>;
    async fn get_by_code(&self, code: &str) -> Result<Option<Coupon>>;
    async fn list(&self, filter: &CouponFilter, page: PageRequest) -> Result<Page<Coupon>>;
    async fn create(&self, fields: &CouponFields, now: DateTime<Utc>) -> Result<Coupon>;
    async fn update(&self, id: Id, fields: &CouponFields, now: DateTime<Utc>) -> Result<()>;
    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()>;
    /// Non-deleted usages by a user.
    async fn count_user_usages(&self, coupon_id: Id, user_id: Id) -> Result<u64>;
    async fn list_user_usages(&self, user_id: Id, page: PageRequest) -> Result<Page<CouponUsage>>;
}

/// A usage to record when an order is created with a coupon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CouponClaim {
    pub coupon_id: Id,
    pub user_id: Id,
    pub order_id: Id,
    pub discount_amount: Amount,
}

/// Coupon usage ledger for the order group (PRC-01).
///
/// `claim` increments `used_count` with a conditional update
/// (`WHERE usage_limit = 0 OR used_count < usage_limit`), re-checks the per-user limit and
/// inserts the usage row in one transaction; it fails with `coupon_usage_limit` /
/// `coupon_per_user_limit` / `coupon_not_found` without side effects. `release` reverses
/// every usage of an order (cancel / timeout) and returns the number released.
///
/// Order code that already runs its own transaction can use the same conditional
/// statements through `zs_infra::db::repo::marketing::coupon::claim_in` / `release_in`.
#[async_trait]
pub trait CouponLedger: Send + Sync {
    async fn claim(&self, claim: &CouponClaim, now: DateTime<Utc>) -> Result<()>;
    async fn release(&self, order_id: Id, now: DateTime<Utc>) -> Result<u64>;
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn coupon(kind: &str, value: i64, scope: &str) -> Coupon {
        Coupon {
            id: 1,
            code: "C".into(),
            kind: kind.into(),
            value: Amount::from(value),
            min_amount: Amount::ZERO,
            max_discount: Amount::ZERO,
            usage_limit: 0,
            used_count: 0,
            per_user_limit: 0,
            disabled_wholesale_price: false,
            per_item_discount: false,
            payment_roles: Vec::new(),
            member_levels: Vec::new(),
            scope_type: SCOPE_PRODUCT.into(),
            scope_ref_ids: scope.into(),
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
    use super::testkit::coupon;
    use super::*;

    fn item(product_id: Id, qty: i32, total: i64, wholesale: i64) -> EligibilityItem {
        EligibilityItem {
            product_id,
            quantity: qty,
            total_price: Amount::from(total),
            wholesale_discount: Amount::from(wholesale),
        }
    }

    fn member(level: Id) -> CouponBuyer {
        CouponBuyer {
            user_id: 7,
            is_guest: false,
            member_level_id: level,
            used_by_user: 0,
        }
    }

    const GUEST: CouponBuyer = CouponBuyer {
        user_id: 0,
        is_guest: true,
        member_level_id: 0,
        used_by_user: 0,
    };

    fn eval(c: &Coupon, buyer: &CouponBuyer, items: &[EligibilityItem]) -> Result<Amount> {
        evaluate_coupon(c, buyer, items, Utc::now()).map(|(d, _)| d)
    }

    fn key(r: Result<Amount>) -> String {
        r.unwrap_err().key().to_owned()
    }

    #[test]
    fn payment_role_and_member_level() {
        let items = [item(100, 1, 100, 0)];
        let base = coupon("fixed", 10, "[100]");
        assert!(eval(&base, &GUEST, &items).is_ok());

        let member_only = Coupon {
            payment_roles: vec![ROLE_MEMBER.into()],
            ..base.clone()
        };
        assert_eq!(
            key(eval(&member_only, &GUEST, &items)),
            keys::ROLE_MEMBER_ONLY
        );
        let guest_only = Coupon {
            payment_roles: vec![ROLE_GUEST.into()],
            ..base.clone()
        };
        assert_eq!(
            key(eval(&guest_only, &member(1), &items)),
            keys::ROLE_GUEST_ONLY
        );
        let vip2 = Coupon {
            member_levels: vec![2],
            ..base.clone()
        };
        assert_eq!(
            key(eval(&vip2, &member(1), &items)),
            keys::MEMBER_LEVEL_NOT_ALLOWED
        );
        assert_eq!(
            key(eval(&vip2, &GUEST, &items)),
            keys::MEMBER_LEVEL_NOT_ALLOWED
        );
        let vip3 = Coupon {
            member_levels: vec![3],
            ..base.clone()
        };
        assert!(eval(&vip3, &member(3), &items).is_ok());
        let combined = Coupon {
            payment_roles: vec![ROLE_MEMBER.into()],
            member_levels: vec![5],
            ..base
        };
        assert!(eval(&combined, &member(5), &items).is_ok());
    }

    #[test]
    fn availability_window_and_limits() {
        let items = [item(100, 1, 100, 0)];
        let now = Utc::now();
        let base = coupon("fixed", 10, "[100]");
        let cases = [
            (
                Coupon {
                    is_active: false,
                    ..base.clone()
                },
                keys::INACTIVE,
            ),
            (
                Coupon {
                    starts_at: Some(now + chrono::Duration::hours(1)),
                    ..base.clone()
                },
                keys::NOT_STARTED,
            ),
            (
                Coupon {
                    ends_at: Some(now - chrono::Duration::hours(1)),
                    ..base.clone()
                },
                keys::EXPIRED,
            ),
            (
                Coupon {
                    usage_limit: 1,
                    used_count: 1,
                    ..base.clone()
                },
                keys::USAGE_LIMIT,
            ),
            (
                Coupon {
                    min_amount: Amount::from(101),
                    ..base.clone()
                },
                keys::MIN_AMOUNT,
            ),
            (coupon("fixed", 10, "[5]"), keys::SCOPE_INVALID),
            (coupon("fixed", 10, "garbage"), keys::SCOPE_INVALID),
            (coupon("fixed", 10, ""), keys::SCOPE_INVALID),
            (coupon("bogus", 10, "[100]"), keys::INVALID),
        ];
        for (c, expected) in cases {
            assert_eq!(key(eval(&c, &member(1), &items)), expected);
        }
        let per_user = Coupon {
            per_user_limit: 1,
            ..base
        };
        let used = CouponBuyer {
            used_by_user: 1,
            ..member(1)
        };
        assert_eq!(key(eval(&per_user, &used, &items)), keys::PER_USER_LIMIT);
        // guests are not subject to the per-user limit
        let guest_used = CouponBuyer {
            used_by_user: 5,
            ..GUEST
        };
        assert!(eval(&per_user, &guest_used, &items).is_ok());
    }

    // PRC-03
    #[test]
    fn fixed_per_item_and_caps() {
        let items = [item(100, 3, 375, 0)];
        let once = coupon("fixed", 5, "[100]");
        assert_eq!(eval(&once, &member(0), &items).unwrap(), Amount::from(5));
        let per_item = Coupon {
            per_item_discount: true,
            ..once.clone()
        };
        assert_eq!(
            eval(&per_item, &member(0), &items).unwrap(),
            Amount::from(15)
        );
        let capped = Coupon {
            max_discount: Amount::from(10),
            ..per_item.clone()
        };
        assert_eq!(eval(&capped, &member(0), &items).unwrap(), Amount::from(10));
        // discount never exceeds the eligible subtotal
        let big = coupon("fixed", 1000, "[100]");
        assert_eq!(eval(&big, &member(0), &items).unwrap(), Amount::from(375));
        // percent ignores per_item
        let pct = Coupon {
            per_item_discount: true,
            ..coupon("percent", 10, "[100]")
        };
        assert_eq!(
            eval(&pct, &member(0), &[item(100, 3, 300, 0)]).unwrap(),
            Amount::from(30)
        );
    }

    // PRC-03
    #[test]
    fn wholesale_exclusion() {
        let no_wholesale = Coupon {
            disabled_wholesale_price: true,
            per_item_discount: true,
            ..coupon("fixed", 5, "[100,101]")
        };
        let items = [item(100, 5, 600, 25), item(101, 2, 250, 0)];
        assert_eq!(
            eval(&no_wholesale, &member(0), &items).unwrap(),
            Amount::from(10)
        );
        let only_wholesale = Coupon {
            disabled_wholesale_price: true,
            ..coupon("fixed", 5, "[100]")
        };
        assert_eq!(
            key(eval(&only_wholesale, &member(0), &[item(100, 5, 400, 100)])),
            keys::WHOLESALE_DISABLED
        );
        let pct = Coupon {
            disabled_wholesale_price: true,
            ..coupon("percent", 10, "[100,101]")
        };
        let (discount, eligible) = evaluate_coupon(
            &pct,
            &member(0),
            &[item(100, 5, 400, 100), item(101, 1, 100, 0)],
            Utc::now(),
        )
        .unwrap();
        assert_eq!(
            (discount, eligible.subtotal),
            (Amount::from(10), Amount::from(100))
        );
    }

    #[test]
    fn admin_validation() {
        let input = CouponInput {
            code: " SAVE ".into(),
            kind: "Fixed".into(),
            value: Amount::from(5),
            per_item_discount: Some(true),
            payment_roles: vec![" Member ".into(), "member".into(), String::new()],
            member_levels: vec![0, 2, 2, 3],
            scope_ref_ids: vec![1, 2],
            ..CouponInput::default()
        };
        let f = validate_coupon(&input, None).unwrap();
        assert_eq!(f.code, "SAVE");
        assert!(f.per_item_discount && f.is_active && !f.disabled_wholesale_price);
        assert_eq!(f.payment_roles, vec!["member"]);
        assert_eq!(f.member_levels, vec![2, 3]);
        assert_eq!(f.scope_ref_ids, "[1,2]");
        let pct = CouponInput {
            kind: "percent".into(),
            ..input.clone()
        };
        assert!(!validate_coupon(&pct, None).unwrap().per_item_discount);
        let bad = [
            CouponInput {
                code: " ".into(),
                ..input.clone()
            },
            CouponInput {
                kind: "x".into(),
                ..input.clone()
            },
            CouponInput {
                value: Amount::ZERO,
                ..input.clone()
            },
            CouponInput {
                kind: "percent".into(),
                value: Amount::from(101),
                ..input.clone()
            },
            CouponInput {
                payment_roles: vec!["admin".into()],
                ..input.clone()
            },
            CouponInput {
                starts_at: Some(Utc::now()),
                ends_at: Some(Utc::now() - chrono::Duration::days(1)),
                ..input.clone()
            },
        ];
        for b in bad {
            assert_eq!(validate_coupon(&b, None).unwrap_err().key(), keys::INVALID);
        }
        let no_scope = CouponInput {
            scope_ref_ids: vec![],
            ..input.clone()
        };
        assert_eq!(
            validate_coupon(&no_scope, None).unwrap_err().key(),
            keys::SCOPE_INVALID
        );
        // update keeps stored flags when omitted
        let stored = Coupon {
            disabled_wholesale_price: true,
            is_active: false,
            ..coupon("fixed", 1, "[1]")
        };
        let omitted = CouponInput {
            disabled_wholesale_price: None,
            is_active: None,
            ..input
        };
        let f = validate_coupon(&omitted, Some(&stored)).unwrap();
        assert!(f.disabled_wholesale_price && !f.is_active);
    }

    // PRC-15
    #[test]
    fn scope_patterns_are_boundary_safe() {
        assert_eq!(
            scope_id_patterns(1),
            [
                "[1]".to_owned(),
                "[1,%".into(),
                "%,1,%".into(),
                "%,1]".into()
            ]
        );
        assert_eq!(decode_scope_ids("[0,11,21]").unwrap().len(), 2);
    }
}
