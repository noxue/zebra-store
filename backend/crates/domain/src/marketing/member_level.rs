//! Member levels, level-specific prices, member price resolution and automatic upgrades.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use serde_json::{Map, Value};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::{Error, Id, Result};

pub mod keys {
    pub const NOT_FOUND: &str = "error.member_level_not_found";
    pub const SLUG_EXISTS: &str = "error.member_level_slug_exists";
    pub const SORT_ORDER_USED: &str = "error.member_level_sort_order_used";
    pub const CANNOT_DELETE_DEFAULT: &str = "error.member_level_cannot_delete_default";
    /// Level still assigned to users (live QA I-13).
    pub const IN_USE: &str = "error.member_level_in_use";
    pub const NO_DEFAULT: &str = "error.member_level_no_default";
    pub const FETCH_FAILED: &str = "error.member_level_fetch_failed";
    pub const CREATE_FAILED: &str = "error.member_level_create_failed";
    pub const UPDATE_FAILED: &str = "error.member_level_update_failed";
    pub const DELETE_FAILED: &str = "error.member_level_delete_failed";
    pub const BACKFILL_FAILED: &str = "error.member_level_backfill_failed";
    pub const PRICE_FETCH_FAILED: &str = "error.member_level_price_fetch_failed";
    pub const PRICE_SAVE_FAILED: &str = "error.member_level_price_save_failed";
    pub const PRICE_DELETE_FAILED: &str = "error.member_level_price_delete_failed";
    pub const USER_LEVEL_UPDATE_FAILED: &str = "error.user_member_level_update_failed";
    pub const USER_NOT_FOUND: &str = "error.user_not_found";
}

/// Maximum compare-and-set attempts of an automatic upgrade (original `maxUpgradeAttempts`).
pub const MAX_UPGRADE_ATTEMPTS: usize = 3;

/// A member level (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemberLevel {
    pub id: Id,
    pub name: Map<String, Value>,
    pub slug: String,
    pub icon: String,
    /// 100 = full price, 90 = 10% off.
    pub discount_rate: Amount,
    pub recharge_threshold: Amount,
    pub spend_threshold: Amount,
    pub is_default: bool,
    pub sort_order: i32,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public member level: amounts as JSON numbers, no status/timestamps.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PublicMemberLevel {
    pub id: Id,
    pub name: Map<String, Value>,
    pub slug: String,
    pub icon: String,
    pub discount_rate: f64,
    pub recharge_threshold: f64,
    pub spend_threshold: f64,
    pub is_default: bool,
    pub sort_order: i32,
}

impl From<&MemberLevel> for PublicMemberLevel {
    fn from(l: &MemberLevel) -> Self {
        let num = |a: Amount| a.decimal().to_f64().unwrap_or_default();
        Self {
            id: l.id,
            name: l.name.clone(),
            slug: l.slug.clone(),
            icon: l.icon.clone(),
            discount_rate: num(l.discount_rate),
            recharge_threshold: num(l.recharge_threshold),
            spend_threshold: num(l.spend_threshold),
            is_default: l.is_default,
            sort_order: l.sort_order,
        }
    }
}

/// A level-specific price (`sku_id` 0 = whole product).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MemberLevelPrice {
    pub id: Id,
    pub member_level_id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub price_amount: Amount,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Level fields written by create/update.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberLevelFields {
    pub name: Map<String, Value>,
    pub slug: String,
    pub icon: String,
    pub discount_rate: Amount,
    pub recharge_threshold: Amount,
    pub spend_threshold: Amount,
    pub is_default: bool,
    pub sort_order: i32,
    pub is_active: bool,
}

/// One row of a batch price upsert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelPriceInput {
    pub member_level_id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub price_amount: Amount,
}

/// Member price and discount for one unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberPrice {
    pub price: Amount,
    pub discount: Amount,
}

/// Resolves the member unit price (PRC-04, PRC-07).
///
/// The level must exist and be active; then SKU override > product override (a positive
/// override not below `base` means no discount, without falling back) > `base × rate / 100`
/// for a rate strictly between 0 and 100.
pub fn resolve_member_price(
    level: Option<&MemberLevel>,
    prices: &[MemberLevelPrice],
    product_id: Id,
    sku_id: Id,
    base: Amount,
) -> MemberPrice {
    let none = MemberPrice {
        price: base,
        discount: Amount::ZERO,
    };
    let Some(level) = level.filter(|l| l.is_active) else {
        return none;
    };
    let lookup = |sku: Id| {
        prices
            .iter()
            .find(|p| {
                p.member_level_id == level.id
                    && p.product_id == product_id
                    && p.sku_id == sku
                    && p.price_amount.is_positive()
            })
            .map(|p| p.price_amount)
    };
    let override_price = if sku_id > 0 { lookup(sku_id) } else { None }.or_else(|| lookup(0));
    let candidate = match override_price {
        Some(p) => p,
        None => {
            let rate = level.discount_rate.decimal();
            if rate <= Decimal::ZERO || rate >= Decimal::ONE_HUNDRED {
                return none;
            }
            Amount::new(base.decimal() * rate / Decimal::ONE_HUNDRED)
        }
    };
    if candidate < base {
        MemberPrice {
            price: candidate,
            discount: base - candidate,
        }
    } else {
        none
    }
}

/// User totals used for automatic upgrades.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemberProgress {
    pub member_level_id: Id,
    pub total_recharged: Amount,
    pub total_spent: Amount,
}

/// Recharge OR spend threshold reached (zero thresholds are ignored).
pub fn meets_threshold(progress: &MemberProgress, level: &MemberLevel) -> bool {
    (level.recharge_threshold.is_positive() && progress.total_recharged >= level.recharge_threshold)
        || (level.spend_threshold.is_positive() && progress.total_spent >= level.spend_threshold)
}

/// Picks the upgrade target among active levels (sorted `sort_order DESC, id ASC`):
/// only strictly higher sort orders qualify (no sideways moves, never a downgrade, PRC-06/PRC-12).
/// `current_sort` is the current level's sort order when it is inactive/not listed
/// (`None` with a non-zero level id = unknown level → no upgrade).
pub fn find_upgrade_target<'a>(
    progress: &MemberProgress,
    active_levels: &'a [MemberLevel],
    current_sort_fallback: Option<i32>,
) -> Option<&'a MemberLevel> {
    let current_sort = if progress.member_level_id == 0 {
        i64::MIN
    } else {
        i64::from(
            active_levels
                .iter()
                .find(|l| l.id == progress.member_level_id)
                .map(|l| l.sort_order)
                .or(current_sort_fallback)?,
        )
    };
    active_levels.iter().find(|l| {
        l.id != progress.member_level_id
            && i64::from(l.sort_order) > current_sort
            && meets_threshold(progress, l)
    })
}

/// Persistence port for levels. Soft-deleted rows are never returned.
#[async_trait]
pub trait MemberLevelRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<MemberLevel>>;
    async fn get_by_slug(&self, slug: &str) -> Result<Option<MemberLevel>>;
    /// The active default level.
    async fn get_default(&self) -> Result<Option<MemberLevel>>;
    /// Another active level using `sort_order`.
    async fn find_active_by_sort_order(
        &self,
        sort_order: i32,
        exclude: Id,
    ) -> Result<Option<MemberLevel>>;
    /// Active levels sorted `sort_order DESC, id ASC`.
    async fn list_active(&self) -> Result<Vec<MemberLevel>>;
    async fn list(&self, is_active: Option<bool>, page: PageRequest) -> Result<Page<MemberLevel>>;
    /// Creates the level; when `is_default`, clears the flag on others in the same transaction.
    async fn create(&self, fields: &MemberLevelFields, now: DateTime<Utc>) -> Result<MemberLevel>;
    async fn update(&self, id: Id, fields: &MemberLevelFields, now: DateTime<Utc>) -> Result<()>;
    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()>;

    async fn get_price(&self, id: Id) -> Result<Option<MemberLevelPrice>>;
    /// Prices of a product (`member_level_id ASC, sku_id ASC`).
    async fn list_prices(&self, product_id: Id) -> Result<Vec<MemberLevelPrice>>;
    /// Prices of one level for several products.
    async fn list_prices_for_level(
        &self,
        level_id: Id,
        product_ids: &[Id],
    ) -> Result<Vec<MemberLevelPrice>>;
    /// Inserts or updates each row (reviving a soft-deleted row with the same key) in one transaction.
    async fn upsert_prices(&self, rows: &[LevelPriceInput], now: DateTime<Utc>) -> Result<()>;
    async fn delete_price(&self, id: Id, now: DateTime<Utc>) -> Result<()>;
}

/// Member columns of `users` owned by the level module.
#[async_trait]
pub trait MemberUserRepo: Send + Sync {
    async fn progress(&self, user_id: Id) -> Result<Option<MemberProgress>>;
    /// Writes only `member_level_id` (no full-row save, PRC-12).
    async fn set_level(&self, user_id: Id, level_id: Id, now: DateTime<Utc>) -> Result<()>;
    /// Compare-and-set: `WHERE member_level_id = current`; returns rows affected (PRC-06).
    async fn set_level_if_current(
        &self,
        user_id: Id,
        current: Id,
        next: Id,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    /// Atomic `total_recharged = total_recharged + amount` (skipped when amount ≤ 0).
    async fn add_recharged(&self, user_id: Id, amount: Amount, now: DateTime<Utc>) -> Result<()>;
    /// Atomic `total_spent = total_spent + amount` (skipped when amount ≤ 0).
    async fn add_spent(&self, user_id: Id, amount: Amount, now: DateTime<Utc>) -> Result<()>;
    /// Assigns `level_id` to users without a level; returns rows affected.
    async fn backfill_level(&self, level_id: Id, now: DateTime<Utc>) -> Result<u64>;
    /// Users (not deleted) currently assigned `level_id`.
    async fn count_with_level(&self, level_id: Id) -> Result<u64>;
}

/// Shorthand for a not-found level error.
pub fn not_found() -> Error {
    Error::not_found(keys::NOT_FOUND)
}

#[cfg(test)]
pub(crate) mod testkit {
    use super::*;

    pub fn level(id: Id, sort: i32, rate: i64) -> MemberLevel {
        MemberLevel {
            id,
            name: Map::new(),
            slug: format!("l{id}"),
            icon: String::new(),
            discount_rate: Amount::from(rate),
            recharge_threshold: Amount::ZERO,
            spend_threshold: Amount::ZERO,
            is_default: false,
            sort_order: sort,
            is_active: true,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        }
    }

    pub fn price(level: Id, product: Id, sku: Id, amount: &str) -> MemberLevelPrice {
        MemberLevelPrice {
            id: 0,
            member_level_id: level,
            product_id: product,
            sku_id: sku,
            price_amount: amount.parse().unwrap_or_default(),
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::{level, price};
    use super::*;

    fn amt(s: &str) -> Amount {
        s.parse().unwrap()
    }

    // PRC-07
    #[test]
    fn inactive_level_gets_no_discount() {
        let mut vip = level(1, 10, 90);
        let prices = [price(1, 5, 9, "8.00")];
        let active = resolve_member_price(Some(&vip), &prices, 5, 9, amt("10"));
        assert_eq!((active.price, active.discount), (amt("8.00"), amt("2.00")));
        vip.is_active = false;
        let inactive = resolve_member_price(Some(&vip), &prices, 5, 9, amt("10"));
        assert_eq!(
            (inactive.price, inactive.discount),
            (amt("10"), Amount::ZERO)
        );
        assert_eq!(
            resolve_member_price(None, &prices, 5, 9, amt("10")).price,
            amt("10")
        );
    }

    // PRC-04 (2) / PRC-07 ④
    #[test]
    fn override_priority_and_rate_bounds() {
        let vip = level(1, 10, 90);
        let prices = [
            price(1, 5, 0, "7.00"),
            price(1, 5, 9, "8.00"),
            price(2, 5, 9, "1.00"),
        ];
        // SKU override wins over product override
        assert_eq!(
            resolve_member_price(Some(&vip), &prices, 5, 9, amt("10")).price,
            amt("8.00")
        );
        // other SKU falls back to the product override
        assert_eq!(
            resolve_member_price(Some(&vip), &prices, 5, 3, amt("10")).price,
            amt("7.00")
        );
        // no override → rate
        assert_eq!(
            resolve_member_price(Some(&vip), &[], 5, 3, amt("59.90")).price,
            amt("53.91")
        );
        // override not below base → no discount, no fallback to the rate
        let high = [price(1, 5, 9, "120")];
        let r = resolve_member_price(Some(&vip), &high, 5, 9, amt("100"));
        assert_eq!((r.price, r.discount), (amt("100"), Amount::ZERO));
        for rate in [0, 100, 120] {
            let l = level(1, 1, rate);
            assert_eq!(
                resolve_member_price(Some(&l), &[], 5, 0, amt("10")).price,
                amt("10")
            );
        }
    }

    // PRC-06 / PRC-12
    #[test]
    fn upgrade_target_rules() {
        let mut default = level(1, 0, 100);
        default.is_default = true;
        let mut vip = level(2, 0, 100);
        vip.spend_threshold = amt("0.01");
        let mut gold = level(3, 10, 100);
        gold.spend_threshold = amt("100");
        let levels = [gold.clone(), default.clone(), vip.clone()];
        let spent = |level_id, spent: &str| MemberProgress {
            member_level_id: level_id,
            total_recharged: Amount::ZERO,
            total_spent: amt(spent),
        };
        // equal sort order never moves sideways
        assert!(find_upgrade_target(&spent(1, "0.01"), &levels, None).is_none());
        // higher sort order wins when threshold met
        assert_eq!(
            find_upgrade_target(&spent(1, "100"), &levels, None).map(|l| l.id),
            Some(3)
        );
        // already highest → keep
        assert!(find_upgrade_target(&spent(3, "1000"), &levels, None).is_none());
        // no level yet → any qualifying level (highest first)
        assert_eq!(
            find_upgrade_target(&spent(0, "0.01"), &levels, None).map(|l| l.id),
            Some(2)
        );
        // unknown current level → nothing
        assert!(find_upgrade_target(&spent(99, "1000"), &levels, None).is_none());
        // inactive current level with known sort order
        assert_eq!(
            find_upgrade_target(&spent(99, "1000"), &levels, Some(5)).map(|l| l.id),
            Some(3)
        );
        let mut recharge = level(4, 20, 100);
        recharge.recharge_threshold = amt("50");
        let p = MemberProgress {
            member_level_id: 1,
            total_recharged: amt("50"),
            total_spent: Amount::ZERO,
        };
        assert!(meets_threshold(&p, &recharge));
        assert!(!meets_threshold(&p, &default));
    }

    #[test]
    fn public_shape_uses_numbers() {
        let v = serde_json::to_value(PublicMemberLevel::from(&level(1, 0, 90))).unwrap();
        assert_eq!(v["discount_rate"], 90.0);
        assert!(v.get("is_active").is_none());
    }
}
