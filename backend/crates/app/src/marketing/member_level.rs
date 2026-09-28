//! Member level use cases: level CRUD, level prices, user level management and upgrades.

use std::sync::Arc;

use zs_domain::marketing::member_level::{
    LevelPriceInput, MAX_UPGRADE_ATTEMPTS, MemberLevel, MemberLevelFields, MemberLevelPrice,
    MemberLevelRepo, MemberPrice, MemberUserRepo, PublicMemberLevel, find_upgrade_target, keys,
    not_found, resolve_member_price,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

/// Member level service.
#[derive(Clone)]
pub struct MemberLevelService {
    levels: Arc<dyn MemberLevelRepo>,
    users: Arc<dyn MemberUserRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for MemberLevelService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MemberLevelService")
    }
}

impl MemberLevelService {
    pub fn new(
        levels: Arc<dyn MemberLevelRepo>,
        users: Arc<dyn MemberUserRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            levels,
            users,
            clock,
        }
    }

    pub async fn get(&self, id: Id) -> Result<MemberLevel> {
        self.levels
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?
            .ok_or_else(not_found)
    }

    pub async fn list(
        &self,
        is_active: Option<bool>,
        page: PageRequest,
    ) -> Result<Page<MemberLevel>> {
        self.levels
            .list(is_active, page)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))
    }

    /// `GET /public/member-levels`.
    pub async fn list_public(&self) -> Result<Vec<PublicMemberLevel>> {
        let levels = self
            .levels
            .list_active()
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        Ok(levels.iter().map(PublicMemberLevel::from).collect())
    }

    /// Slug uniqueness and unique sort order among active levels (PRC-06).
    async fn validate(&self, id: Id, fields: &MemberLevelFields) -> Result<()> {
        // Live QA I-20: a rate outside 0–100 % or a negative threshold used to be
        // stored silently (and then ignored by pricing).
        if !fields_in_range(fields) {
            return Err(Error::invalid());
        }
        if let Some(existing) = self.levels.get_by_slug(&fields.slug).await?
            && existing.id != id
        {
            return Err(Error::bad_request(keys::SLUG_EXISTS));
        }
        if fields.is_active
            && self
                .levels
                .find_active_by_sort_order(fields.sort_order, id)
                .await?
                .is_some()
        {
            return Err(Error::bad_request(keys::SORT_ORDER_USED));
        }
        Ok(())
    }

    pub async fn create(&self, fields: MemberLevelFields) -> Result<MemberLevel> {
        self.validate(0, &fields).await?;
        self.levels
            .create(&fields, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::CREATE_FAILED))
    }

    pub async fn update(&self, id: Id, fields: MemberLevelFields) -> Result<MemberLevel> {
        self.validate(id, &fields).await?;
        self.levels
            .update(id, &fields, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::UPDATE_FAILED))?;
        self.get(id).await
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        let level = self
            .levels
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::DELETE_FAILED))?
            .ok_or_else(not_found)?;
        if level.is_default {
            return Err(Error::bad_request(keys::CANNOT_DELETE_DEFAULT));
        }
        // Live QA I-13 (original gap): users would keep a dangling level id.
        let assigned = self
            .users
            .count_with_level(id)
            .await
            .map_err(|e| e.or_internal(keys::DELETE_FAILED))?;
        if assigned > 0 {
            return Err(Error::bad_request(keys::IN_USE));
        }
        self.levels
            .delete(id, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::DELETE_FAILED))
    }

    pub async fn prices(&self, product_id: Id) -> Result<Vec<MemberLevelPrice>> {
        self.levels
            .list_prices(product_id)
            .await
            .map_err(|e| e.or_internal(keys::PRICE_FETCH_FAILED))
    }

    pub async fn upsert_prices(&self, rows: &[LevelPriceInput]) -> Result<()> {
        self.levels
            .upsert_prices(rows, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::PRICE_SAVE_FAILED))
    }

    pub async fn delete_price(&self, id: Id) -> Result<()> {
        self.levels
            .delete_price(id, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::PRICE_DELETE_FAILED))
    }

    /// `PUT /admin/users/:id/member-level` (`level_id` 0 clears the level).
    pub async fn set_user_level(&self, user_id: Id, level_id: Id) -> Result<()> {
        let failed = |e: Error| e.or_internal(keys::USER_LEVEL_UPDATE_FAILED);
        if self
            .users
            .progress(user_id)
            .await
            .map_err(failed)?
            .is_none()
        {
            // The original maps an unknown user to the generic update failure.
            return Err(
                Error::internal_msg("user not found").or_internal(keys::USER_LEVEL_UPDATE_FAILED)
            );
        }
        if level_id > 0 && self.levels.get(level_id).await.map_err(failed)?.is_none() {
            return Err(not_found());
        }
        self.users
            .set_level(user_id, level_id, self.clock.now())
            .await
            .map_err(failed)
    }

    /// Gives every user without a level the default level; returns the number updated.
    pub async fn backfill_default(&self) -> Result<u64> {
        let failed = |e: Error| e.or_internal(keys::BACKFILL_FAILED);
        let default = self
            .levels
            .get_default()
            .await
            .map_err(failed)?
            .ok_or_else(|| Error::bad_request(keys::NO_DEFAULT))?;
        self.users
            .backfill_level(default.id, self.clock.now())
            .await
            .map_err(failed)
    }

    /// Assigns the default level to a new user that has none (called after registration, PRC-12).
    pub async fn assign_default_level(&self, user_id: Id) -> Result<()> {
        let Some(default) = self.levels.get_default().await? else {
            return Ok(());
        };
        let Some(progress) = self.users.progress(user_id).await? else {
            return Ok(());
        };
        if progress.member_level_id == 0 {
            self.users
                .set_level_if_current(user_id, 0, default.id, self.clock.now())
                .await?;
        }
        Ok(())
    }

    /// Member unit price for the pricing engine / storefront (PRC-04, PRC-07).
    pub async fn resolve_price(
        &self,
        level_id: Id,
        product_id: Id,
        sku_id: Id,
        base: Amount,
    ) -> Result<MemberPrice> {
        if level_id <= 0 {
            return Ok(MemberPrice {
                price: base,
                discount: Amount::ZERO,
            });
        }
        let level = self.levels.get(level_id).await?;
        let prices = if level.as_ref().is_some_and(|l| l.is_active) {
            self.levels
                .list_prices_for_level(level_id, &[product_id])
                .await?
        } else {
            Vec::new()
        };
        Ok(resolve_member_price(
            level.as_ref(),
            &prices,
            product_id,
            sku_id,
            base,
        ))
    }

    /// Upgrades a user when a higher level's threshold is met; only ever upgrades and
    /// uses compare-and-set on `member_level_id` (PRC-06).
    pub async fn check_and_upgrade(&self, user_id: Id) -> Result<()> {
        for _ in 0..MAX_UPGRADE_ATTEMPTS {
            let Some(progress) = self.users.progress(user_id).await? else {
                return Ok(());
            };
            let levels = self.levels.list_active().await?;
            if levels.is_empty() {
                return Ok(());
            }
            let fallback = if progress.member_level_id > 0
                && !levels.iter().any(|l| l.id == progress.member_level_id)
            {
                self.levels
                    .get(progress.member_level_id)
                    .await?
                    .map(|l| l.sort_order)
            } else {
                None
            };
            let Some(target) = find_upgrade_target(&progress, &levels, fallback) else {
                return Ok(());
            };
            let affected = self
                .users
                .set_level_if_current(
                    user_id,
                    progress.member_level_id,
                    target.id,
                    self.clock.now(),
                )
                .await?;
            if affected > 0 {
                return Ok(());
            }
        }
        Ok(())
    }

    /// Wallet recharge credited: atomically adds to `total_recharged`, then upgrades.
    pub async fn on_recharge_completed(&self, user_id: Id, amount: Amount) -> Result<()> {
        if user_id <= 0 {
            return Ok(());
        }
        self.users
            .add_recharged(user_id, amount, self.clock.now())
            .await?;
        self.check_and_upgrade(user_id).await
    }

    /// Order paid (call exactly once per pending→paid transition, PRC-12): adds to
    /// `total_spent` atomically, then upgrades.
    pub async fn on_order_paid(&self, user_id: Id, amount: Amount) -> Result<()> {
        if user_id <= 0 {
            return Ok(());
        }
        self.users
            .add_spent(user_id, amount, self.clock.now())
            .await?;
        self.check_and_upgrade(user_id).await
    }
}

/// Discount rate within 0–100 % and non-negative upgrade thresholds.
fn fields_in_range(fields: &MemberLevelFields) -> bool {
    !fields.discount_rate.is_negative()
        && fields.discount_rate <= Amount::from(100)
        && !fields.recharge_threshold.is_negative()
        && !fields.spend_threshold.is_negative()
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::Map;
    use std::sync::Mutex;
    use zs_domain::marketing::member_level::MemberProgress;
    use zs_shared::clock::SystemClock;

    fn level(id: Id, sort: i32, spend: i64) -> MemberLevel {
        MemberLevel {
            id,
            name: Map::new(),
            slug: format!("l{id}"),
            icon: String::new(),
            discount_rate: Amount::from(100),
            recharge_threshold: Amount::ZERO,
            spend_threshold: Amount::from(spend),
            is_default: false,
            sort_order: sort,
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[derive(Default)]
    struct Levels(Vec<MemberLevel>);

    #[async_trait]
    impl MemberLevelRepo for Levels {
        async fn get(&self, id: Id) -> Result<Option<MemberLevel>> {
            Ok(self.0.iter().find(|l| l.id == id).cloned())
        }
        async fn get_by_slug(&self, _: &str) -> Result<Option<MemberLevel>> {
            Ok(None)
        }
        async fn get_default(&self) -> Result<Option<MemberLevel>> {
            Ok(self.0.iter().find(|l| l.is_default).cloned())
        }
        async fn find_active_by_sort_order(&self, _: i32, _: Id) -> Result<Option<MemberLevel>> {
            Ok(None)
        }
        async fn list_active(&self) -> Result<Vec<MemberLevel>> {
            let mut v: Vec<_> = self.0.iter().filter(|l| l.is_active).cloned().collect();
            v.sort_by(|a, b| b.sort_order.cmp(&a.sort_order).then(a.id.cmp(&b.id)));
            Ok(v)
        }
        async fn list(&self, _: Option<bool>, _: PageRequest) -> Result<Page<MemberLevel>> {
            Ok(Page {
                items: Vec::new(),
                total: 0,
            })
        }
        async fn create(&self, _: &MemberLevelFields, _: DateTime<Utc>) -> Result<MemberLevel> {
            Err(Error::internal_msg("unused"))
        }
        async fn update(&self, _: Id, _: &MemberLevelFields, _: DateTime<Utc>) -> Result<()> {
            Ok(())
        }
        async fn delete(&self, _: Id, _: DateTime<Utc>) -> Result<()> {
            Ok(())
        }
        async fn get_price(&self, _: Id) -> Result<Option<MemberLevelPrice>> {
            Ok(None)
        }
        async fn list_prices(&self, _: Id) -> Result<Vec<MemberLevelPrice>> {
            Ok(Vec::new())
        }
        async fn list_prices_for_level(&self, _: Id, _: &[Id]) -> Result<Vec<MemberLevelPrice>> {
            Ok(Vec::new())
        }
        async fn upsert_prices(&self, _: &[LevelPriceInput], _: DateTime<Utc>) -> Result<()> {
            Ok(())
        }
        async fn delete_price(&self, _: Id, _: DateTime<Utc>) -> Result<()> {
            Ok(())
        }
    }

    /// A user whose level is raised concurrently right after the first read.
    struct RacingUser {
        state: Mutex<MemberProgress>,
        race_to: Option<Id>,
        cas_calls: Mutex<u32>,
    }

    #[async_trait]
    impl MemberUserRepo for RacingUser {
        async fn progress(&self, _: Id) -> Result<Option<MemberProgress>> {
            Ok(Some(*self.state.lock().unwrap()))
        }
        async fn set_level(&self, _: Id, level: Id, _: DateTime<Utc>) -> Result<()> {
            self.state.lock().unwrap().member_level_id = level;
            Ok(())
        }
        async fn set_level_if_current(
            &self,
            _: Id,
            current: Id,
            next: Id,
            _: DateTime<Utc>,
        ) -> Result<u64> {
            *self.cas_calls.lock().unwrap() += 1;
            let mut state = self.state.lock().unwrap();
            if let Some(raced) = self.race_to
                && *self.cas_calls.lock().unwrap() == 1
            {
                state.member_level_id = raced;
            }
            if state.member_level_id != current {
                return Ok(0);
            }
            state.member_level_id = next;
            Ok(1)
        }
        async fn add_recharged(&self, _: Id, amount: Amount, _: DateTime<Utc>) -> Result<()> {
            self.state.lock().unwrap().total_recharged += amount;
            Ok(())
        }
        async fn add_spent(&self, _: Id, amount: Amount, _: DateTime<Utc>) -> Result<()> {
            if amount.is_positive() {
                self.state.lock().unwrap().total_spent += amount;
            }
            Ok(())
        }
        async fn backfill_level(&self, _: Id, _: DateTime<Utc>) -> Result<u64> {
            Ok(0)
        }
        async fn count_with_level(&self, level: Id) -> Result<u64> {
            Ok(u64::from(
                self.state.lock().unwrap().member_level_id == level,
            ))
        }
    }

    fn service(users: Arc<RacingUser>) -> MemberLevelService {
        let levels = Levels(vec![level(1, 0, 0), level(2, 5, 100), level(3, 10, 1000)]);
        MemberLevelService::new(Arc::new(levels), users, Arc::new(SystemClock))
    }

    fn user(level: Id, spent: i64, race_to: Option<Id>) -> Arc<RacingUser> {
        Arc::new(RacingUser {
            state: Mutex::new(MemberProgress {
                member_level_id: level,
                total_recharged: Amount::ZERO,
                total_spent: Amount::from(spent),
            }),
            race_to,
            cas_calls: Mutex::new(0),
        })
    }

    // PRC-06: an upgrade computed from a stale read never overwrites a concurrent higher level.
    #[tokio::test]
    async fn stale_upgrade_does_not_downgrade_concurrent_higher_level() {
        let u = user(1, 100, Some(3));
        service(u.clone())
            .on_order_paid(9, Amount::ZERO)
            .await
            .unwrap();
        assert_eq!(u.state.lock().unwrap().member_level_id, 3);
        assert_eq!(
            *u.cas_calls.lock().unwrap(),
            1,
            "no retry needed once the user is above the target"
        );
    }

    #[tokio::test]
    async fn upgrade_happens_once_threshold_reached() {
        let u = user(1, 90, None);
        let svc = service(u.clone());
        svc.on_order_paid(9, Amount::from(5)).await.unwrap();
        assert_eq!(u.state.lock().unwrap().member_level_id, 1);
        svc.on_order_paid(9, Amount::from(5)).await.unwrap();
        assert_eq!(u.state.lock().unwrap().member_level_id, 2);
        // non-positive amounts do nothing
        svc.on_order_paid(0, Amount::from(5000)).await.unwrap();
        assert_eq!(u.state.lock().unwrap().total_spent, Amount::from(100));
    }
}
