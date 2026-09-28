//! Affiliate use cases: click tracking and attribution, profiles, commissions (incl. the
//! order-side hooks), withdrawals and the admin views.

use std::sync::Arc;

use chrono::Duration;
use rust_decimal::Decimal;
use zs_domain::affiliate::rules::{
    ATTRIBUTION_WINDOW_DAYS, CLICK_DEDUPE_MINUTES, CODE_LENGTH, CODE_MAX_RETRY, code_from_random,
    commission_amount, commission_base, commission_schedule, conversion_rate, normalize_code,
    normalize_ids, parse_profile_status, validate_withdraw,
};
use zs_domain::affiliate::{
    AdminUserItem, AffiliateRepo, Commission, CommissionFilter, Dashboard, NewClick, NewCommission,
    Profile, ProfileFilter, Stats, WithdrawFilter, WithdrawRequest, keys, status,
};
use zs_domain::identity::user::STATUS_DISABLED;
use zs_domain::settings::schema::integration::AffiliateSetting;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

/// A click reported by the storefront (`TrackClickInput`).
#[derive(Debug, Clone, Default)]
pub struct ClickInput {
    pub affiliate_code: String,
    pub visitor_key: String,
    pub landing_path: String,
    pub referrer: String,
    pub client_ip: String,
    pub user_agent: String,
}

/// Affiliate service.
#[derive(Clone)]
pub struct AffiliateService {
    repo: Arc<dyn AffiliateRepo>,
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for AffiliateService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AffiliateService")
    }
}

fn not_opened() -> Error {
    Error::invalid()
}

fn disabled() -> Error {
    Error::bad_request(keys::FORBIDDEN)
}

impl AffiliateService {
    pub fn new(
        repo: Arc<dyn AffiliateRepo>,
        settings: Arc<dyn SettingsStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            settings,
            clock,
        }
    }

    /// `affiliate_config` (normalized).
    pub async fn setting(&self) -> Result<AffiliateSetting> {
        let raw = self.settings.get(setting_keys::AFFILIATE_CONFIG).await?;
        Ok(AffiliateSetting::decode(raw.as_ref()))
    }

    /// Records a click (`TrackClick`): ignored when the program is off, the code unknown
    /// or inactive, or the same visitor hit the same landing path within 10 minutes.
    pub async fn track_click(&self, input: &ClickInput) -> Result<()> {
        let code = normalize_code(&input.affiliate_code);
        if code.is_empty() || !self.setting().await?.enabled {
            return Ok(());
        }
        let Some(profile) = self.repo.profile_by_code(&code).await? else {
            return Ok(());
        };
        if profile.status.trim() != status::PROFILE_ACTIVE {
            return Ok(());
        }
        let now = self.clock.now();
        let visitor_key = input.visitor_key.trim();
        let landing_path = input.landing_path.trim();
        if !visitor_key.is_empty()
            && self
                .repo
                .has_recent_click(
                    profile.id,
                    visitor_key,
                    landing_path,
                    now - Duration::minutes(CLICK_DEDUPE_MINUTES),
                )
                .await?
        {
            return Ok(());
        }
        let click = NewClick {
            profile_id: profile.id,
            visitor_key: visitor_key.to_owned(),
            landing_path: landing_path.to_owned(),
            referrer: input.referrer.trim().to_owned(),
            client_ip: input.client_ip.trim().to_owned(),
            user_agent: input.user_agent.trim().to_owned(),
        };
        self.repo.create_click(&click, now).await
    }

    /// Order attribution snapshot (`ResolveOrderAffiliateSnapshot`): the visitor's latest
    /// click within 30 days wins, then the explicit code; self-referral is ignored.
    /// Returns `(affiliate_profile_id, affiliate_code)`.
    pub async fn resolve_order_snapshot(
        &self,
        user_id: Id,
        code: &str,
        visitor_key: &str,
    ) -> Result<Option<(Id, String)>> {
        if !self.setting().await?.enabled {
            return Ok(None);
        }
        let visitor_key = visitor_key.trim();
        if !visitor_key.is_empty() {
            let since = self.clock.now() - Duration::days(ATTRIBUTION_WINDOW_DAYS);
            if let Some(profile) = self
                .repo
                .latest_profile_by_visitor(visitor_key, since)
                .await?
            {
                if user_id > 0 && profile.user_id == user_id {
                    return Ok(None);
                }
                return Ok(Some((profile.id, profile.affiliate_code)));
            }
        }
        let code = normalize_code(code);
        if code.is_empty() {
            return Ok(None);
        }
        let Some(profile) = self.repo.profile_by_code(&code).await? else {
            return Ok(None);
        };
        if profile.status.trim() != status::PROFILE_ACTIVE
            || (user_id > 0 && profile.user_id == user_id)
        {
            return Ok(None);
        }
        Ok(Some((profile.id, profile.affiliate_code)))
    }

    /// Creates the order's commission after payment (`HandleOrderPaid`); idempotent.
    /// Base = affiliate-enabled items' payable amounts; no self-referral.
    pub async fn handle_order_paid(&self, order_id: Id) -> Result<()> {
        if order_id <= 0 {
            return Ok(());
        }
        let setting = self.setting().await?;
        if !setting.enabled || setting.commission_rate <= 0.0 {
            return Ok(());
        }
        let Some(order) = self.repo.commission_order(order_id).await? else {
            return Ok(());
        };
        let profile = match order.affiliate_profile_id.filter(|id| *id > 0) {
            Some(id) => self.repo.profile(id).await?,
            None if !order.affiliate_code.trim().is_empty() => {
                self.repo.profile_by_code(&order.affiliate_code).await?
            }
            None => None,
        };
        let Some(profile) = profile else {
            return Ok(());
        };
        if profile.status.trim() != status::PROFILE_ACTIVE
            || (order.user_id > 0 && profile.user_id == order.user_id)
        {
            return Ok(());
        }
        let base = commission_base(&order.items);
        if !base.is_positive() {
            return Ok(());
        }
        let (rate, amount) = commission_amount(base, setting.commission_rate);
        if !amount.is_positive() {
            return Ok(());
        }
        let now = self.clock.now();
        let (state, confirm_at, available_at) =
            commission_schedule(order.paid_at.unwrap_or(now), setting.confirm_days);
        let created = self
            .repo
            .create_commission(&NewCommission {
                profile_id: profile.id,
                order_id: order.id,
                commission_type: status::COMMISSION_TYPE_ORDER.to_owned(),
                base_amount: base,
                rate_percent: rate,
                commission_amount: amount,
                status: state.to_owned(),
                confirm_at,
                available_at,
                now,
            })
            .await?;
        if !created {
            tracing::debug!(
                order_id,
                profile_id = profile.id,
                "affiliate_commission_exists"
            );
        }
        Ok(())
    }

    /// Rejects the order's unbound pending/available commissions (`HandleOrderCanceled`).
    pub async fn handle_order_canceled(&self, order_id: Id, reason: &str) -> Result<()> {
        if order_id <= 0 {
            return Ok(());
        }
        let reason = match reason.trim() {
            "" => "order_canceled",
            r => r,
        };
        self.repo
            .reject_order_commissions(order_id, reason, self.clock.now())
            .await?;
        Ok(())
    }

    /// Due commissions become available (job `affiliate:confirm_commissions`, AFF-01).
    pub async fn confirm_due(&self) -> Result<u64> {
        self.repo.confirm_due(self.clock.now()).await
    }

    /// Opens the affiliate program for a user (`OpenAffiliate`); idempotent.
    pub async fn open(&self, user_id: Id) -> Result<Profile> {
        if !self.setting().await?.enabled {
            return Err(disabled());
        }
        let user_status = self
            .repo
            .user_status(user_id)
            .await
            .map_err(|e| e.or_internal(keys::SAVE_FAILED))?
            .ok_or_else(|| Error::not_found(keys::USER_NOT_FOUND))?;
        if user_status.trim() == STATUS_DISABLED {
            return Err(Error::internal_msg("user disabled").or_internal(keys::SAVE_FAILED));
        }
        if let Some(existing) = self.repo.profile_by_user(user_id).await? {
            return Ok(existing);
        }
        for _ in 0..CODE_MAX_RETRY {
            let bytes: [u8; CODE_LENGTH] = rand::random();
            let code = code_from_random(&bytes);
            if let Some(profile) = self
                .repo
                .create_profile(user_id, &code, self.clock.now())
                .await?
            {
                return Ok(profile);
            }
            // Conflict: either the code is taken (retry) or a concurrent open won.
            if let Some(existing) = self.repo.profile_by_user(user_id).await? {
                return Ok(existing);
            }
        }
        Err(Error::internal_msg("affiliate code exhausted").or_internal(keys::SAVE_FAILED))
    }

    async fn stats(&self, profile_id: Id) -> Result<Stats> {
        let map = self.repo.profile_stats(&[profile_id]).await?;
        let agg = map.get(&profile_id).copied().unwrap_or_default();
        Ok(Stats {
            click_count: agg.click_count,
            valid_order_count: agg.valid_order_count,
            conversion_rate: conversion_rate(agg.valid_order_count, agg.click_count),
            pending_commission: agg.pending,
            available_commission: agg.available,
            withdrawn_commission: agg.withdrawn,
        })
    }

    /// The user's affiliate center (`GetUserDashboard`).
    pub async fn dashboard(&self, user_id: Id) -> Result<Dashboard> {
        let Some(profile) = self.repo.profile_by_user(user_id).await? else {
            return Ok(Dashboard::default());
        };
        let stats = self.stats(profile.id).await?;
        Ok(Dashboard {
            opened: true,
            promotion_path: format!("/?aff={}", profile.affiliate_code),
            affiliate_code: profile.affiliate_code,
            click_count: stats.click_count,
            valid_order_count: stats.valid_order_count,
            conversion_rate: stats.conversion_rate,
            pending_commission: stats.pending_commission,
            available_commission: stats.available_commission,
            withdrawn_commission: stats.withdrawn_commission,
        })
    }

    pub async fn user_commissions(
        &self,
        user_id: Id,
        state: &str,
        page: PageRequest,
    ) -> Result<Page<Commission>> {
        let Some(profile) = self.repo.profile_by_user(user_id).await? else {
            return Ok(Page {
                items: vec![],
                total: 0,
            });
        };
        let filter = CommissionFilter {
            profile_id: profile.id,
            status: state.trim().to_owned(),
            ..CommissionFilter::default()
        };
        self.repo.list_commissions(&filter, page).await
    }

    pub async fn user_withdraws(
        &self,
        user_id: Id,
        state: &str,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        let Some(profile) = self.repo.profile_by_user(user_id).await? else {
            return Ok(Page {
                items: vec![],
                total: 0,
            });
        };
        let filter = WithdrawFilter {
            profile_id: profile.id,
            status: state.trim().to_owned(),
            ..WithdrawFilter::default()
        };
        self.repo.list_withdraws(&filter, page).await
    }

    /// Requests a withdrawal (`ApplyWithdraw`): validates the amount against
    /// `min_withdraw_amount` and the channel list, confirms due commissions, then freezes
    /// available commissions in one transaction.
    pub async fn apply_withdraw(
        &self,
        user_id: Id,
        amount: &str,
        channel: &str,
        account: &str,
    ) -> Result<WithdrawRequest> {
        let amount: Decimal = amount.trim().parse().map_err(|_| Error::invalid())?;
        let amount = Amount::new(amount);
        let setting = self.setting().await?;
        if !setting.enabled {
            return Err(disabled());
        }
        if user_id <= 0 {
            return Err(not_opened());
        }
        validate_withdraw(
            amount,
            setting.min_withdraw_amount,
            channel,
            account,
            &setting.withdraw_channels,
        )?;
        self.confirm_due().await?;
        self.repo
            .apply_withdraw(
                user_id,
                amount,
                channel.trim(),
                account.trim(),
                self.clock.now(),
            )
            .await
            .map_err(|e| e.or_internal(keys::SAVE_FAILED))
    }

    /// Admin affiliate list with statistics (`ListAdminUsers`).
    pub async fn admin_users(
        &self,
        filter: &ProfileFilter,
        page: PageRequest,
    ) -> Result<Page<AdminUserItem>> {
        let rows = self.repo.list_profiles(filter, page).await?;
        let ids: Vec<Id> = rows.items.iter().map(|p| p.id).collect();
        let stats = if ids.is_empty() {
            Default::default()
        } else {
            self.repo.profile_stats(&ids).await?
        };
        Ok(rows.map(|profile| {
            let agg = stats.get(&profile.id).copied().unwrap_or_default();
            AdminUserItem {
                stats: Stats {
                    click_count: agg.click_count,
                    valid_order_count: agg.valid_order_count,
                    conversion_rate: conversion_rate(agg.valid_order_count, agg.click_count),
                    pending_commission: agg.pending,
                    available_commission: agg.available,
                    withdrawn_commission: agg.withdrawn,
                },
                profile,
            }
        }))
    }

    pub async fn admin_commissions(
        &self,
        filter: &CommissionFilter,
        page: PageRequest,
    ) -> Result<Page<Commission>> {
        self.repo.list_commissions(filter, page).await
    }

    pub async fn admin_withdraws(
        &self,
        filter: &WithdrawFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        self.repo.list_withdraws(filter, page).await
    }

    /// Admin status change (`UpdateAffiliateProfileStatus`).
    pub async fn set_status(&self, profile_id: Id, raw: &str) -> Result<Profile> {
        let next = parse_profile_status(raw).ok_or_else(Error::invalid)?;
        let profile = self
            .repo
            .profile(profile_id)
            .await?
            .ok_or_else(|| Error::not_found(zs_domain::error::keys::BAD_REQUEST))?;
        if profile.status.trim() == next {
            return Ok(profile);
        }
        self.repo
            .set_profile_status(profile_id, next, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::SAVE_FAILED))?;
        self.repo
            .profile(profile_id)
            .await?
            .ok_or_else(|| Error::not_found(zs_domain::error::keys::BAD_REQUEST))
    }

    /// Admin batch status change; returns rows updated.
    pub async fn batch_status(&self, ids: &[Id], raw: &str) -> Result<u64> {
        let next = parse_profile_status(raw).ok_or_else(Error::invalid)?;
        let ids = normalize_ids(ids);
        if ids.is_empty() {
            return Ok(0);
        }
        self.repo
            .batch_profile_status(&ids, next, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::SAVE_FAILED))
    }

    /// Pays or rejects a pending withdrawal (`ReviewWithdraw`).
    pub async fn review_withdraw(
        &self,
        admin_id: Id,
        id: Id,
        pay: bool,
        reason: &str,
    ) -> Result<WithdrawRequest> {
        self.repo
            .review_withdraw(id, admin_id, pay, reason.trim(), self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::SAVE_FAILED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, TimeZone, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use zs_domain::affiliate::{CommissionItem, CommissionOrder, ProfileStats};
    use zs_shared::clock::FixedClock;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap()
    }

    fn profile(id: Id, user_id: Id, code: &str, state: &str) -> Profile {
        Profile {
            id,
            user_id,
            affiliate_code: code.into(),
            status: state.into(),
            created_at: now(),
            updated_at: now(),
            user: None,
        }
    }

    #[derive(Default)]
    struct Repo {
        profiles: Vec<Profile>,
        order: Option<CommissionOrder>,
        commissions: Mutex<Vec<NewCommission>>,
        clicks: Mutex<Vec<NewClick>>,
        recent: bool,
    }

    #[async_trait]
    impl AffiliateRepo for Repo {
        async fn user_status(&self, user_id: Id) -> Result<Option<String>> {
            Ok((user_id == 1).then(|| "active".to_owned()))
        }
        async fn profile(&self, id: Id) -> Result<Option<Profile>> {
            Ok(self.profiles.iter().find(|p| p.id == id).cloned())
        }
        async fn profile_by_user(&self, user_id: Id) -> Result<Option<Profile>> {
            Ok(self.profiles.iter().find(|p| p.user_id == user_id).cloned())
        }
        async fn profile_by_code(&self, code: &str) -> Result<Option<Profile>> {
            Ok(self
                .profiles
                .iter()
                .find(|p| p.affiliate_code.eq_ignore_ascii_case(code))
                .cloned())
        }
        async fn create_profile(
            &self,
            user_id: Id,
            code: &str,
            _: DateTime<Utc>,
        ) -> Result<Option<Profile>> {
            Ok(Some(profile(99, user_id, code, "active")))
        }
        async fn set_profile_status(&self, _: Id, _: &str, _: DateTime<Utc>) -> Result<()> {
            Ok(())
        }
        async fn batch_profile_status(&self, ids: &[Id], _: &str, _: DateTime<Utc>) -> Result<u64> {
            Ok(ids.len() as u64)
        }
        async fn list_profiles(&self, _: &ProfileFilter, _: PageRequest) -> Result<Page<Profile>> {
            Ok(Page {
                items: self.profiles.clone(),
                total: self.profiles.len() as u64,
            })
        }
        async fn profile_stats(&self, ids: &[Id]) -> Result<HashMap<Id, ProfileStats>> {
            Ok(ids
                .iter()
                .map(|id| {
                    (
                        *id,
                        ProfileStats {
                            click_count: 3,
                            valid_order_count: 1,
                            ..ProfileStats::default()
                        },
                    )
                })
                .collect())
        }
        async fn has_recent_click(
            &self,
            _: Id,
            _: &str,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<bool> {
            Ok(self.recent)
        }
        async fn create_click(&self, click: &NewClick, _: DateTime<Utc>) -> Result<()> {
            self.clicks.lock().unwrap().push(click.clone());
            Ok(())
        }
        async fn latest_profile_by_visitor(
            &self,
            key: &str,
            _: DateTime<Utc>,
        ) -> Result<Option<Profile>> {
            Ok((key == "v-self").then(|| profile(1, 7, "SELF", "active")))
        }
        async fn list_commissions(
            &self,
            _: &CommissionFilter,
            _: PageRequest,
        ) -> Result<Page<Commission>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
        async fn list_withdraws(
            &self,
            _: &WithdrawFilter,
            _: PageRequest,
        ) -> Result<Page<WithdrawRequest>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
        async fn withdraw(&self, _: Id) -> Result<Option<WithdrawRequest>> {
            Ok(None)
        }
        async fn confirm_due(&self, _: DateTime<Utc>) -> Result<u64> {
            Ok(0)
        }
        async fn apply_withdraw(
            &self,
            _: Id,
            _: Amount,
            _: &str,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<WithdrawRequest> {
            Err(Error::invalid())
        }
        async fn review_withdraw(
            &self,
            _: Id,
            _: Id,
            _: bool,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<WithdrawRequest> {
            Err(Error::invalid())
        }
        async fn commission_order(&self, _: Id) -> Result<Option<CommissionOrder>> {
            Ok(self.order.clone())
        }
        async fn create_commission(&self, c: &NewCommission) -> Result<bool> {
            self.commissions.lock().unwrap().push(c.clone());
            Ok(true)
        }
        async fn reject_order_commissions(&self, _: Id, _: &str, _: DateTime<Utc>) -> Result<u64> {
            Ok(0)
        }
    }

    struct Settings(serde_json::Value);

    #[async_trait]
    impl SettingsStore for Settings {
        async fn get(&self, _: &str) -> Result<Option<serde_json::Value>> {
            Ok(Some(self.0.clone()))
        }
        async fn set(&self, _: &str, _: &serde_json::Value) -> Result<()> {
            Ok(())
        }
    }

    fn service(repo: Arc<Repo>, setting: serde_json::Value) -> AffiliateService {
        AffiliateService::new(
            repo,
            Arc::new(Settings(setting)),
            Arc::new(FixedClock(now())),
        )
    }

    fn item(total: i64, enabled: bool) -> CommissionItem {
        CommissionItem {
            product_id: 1,
            total_price: Amount::from(total),
            coupon_discount: Amount::ZERO,
            affiliate_enabled: enabled,
        }
    }

    #[tokio::test]
    async fn commission_on_paid_order_and_no_self_referral() {
        let order = CommissionOrder {
            id: 10,
            user_id: 5,
            paid_at: Some(now()),
            affiliate_profile_id: Some(1),
            affiliate_code: String::new(),
            items: vec![item(100, true), item(40, false)],
        };
        let repo = Arc::new(Repo {
            profiles: vec![profile(1, 7, "AAAA", "active")],
            order: Some(order.clone()),
            ..Repo::default()
        });
        let setting =
            serde_json::json!({"enabled": true, "commission_rate": 10, "confirm_days": 7});
        service(repo.clone(), setting.clone())
            .handle_order_paid(10)
            .await
            .unwrap();
        let c = repo.commissions.lock().unwrap()[0].clone();
        assert_eq!(c.base_amount, Amount::from(100));
        assert_eq!(c.commission_amount, Amount::from(10));
        assert_eq!(c.status, "pending_confirm");
        assert_eq!(c.confirm_at, Some(now() + Duration::days(7)));

        // Self-referral: the buyer owns the profile.
        let repo = Arc::new(Repo {
            profiles: vec![profile(1, 5, "AAAA", "active")],
            order: Some(order),
            ..Repo::default()
        });
        service(repo.clone(), setting)
            .handle_order_paid(10)
            .await
            .unwrap();
        assert!(repo.commissions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn attribution_prefers_visitor_and_rejects_self() {
        let repo = Arc::new(Repo {
            profiles: vec![
                profile(2, 8, "CODE2", "active"),
                profile(3, 9, "OFF", "disabled"),
            ],
            ..Repo::default()
        });
        let svc = service(repo, serde_json::json!({"enabled": true}));
        assert_eq!(
            svc.resolve_order_snapshot(7, "CODE2", "v-self")
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            svc.resolve_order_snapshot(1, "code2", "").await.unwrap(),
            Some((2, "CODE2".to_owned()))
        );
        assert_eq!(
            svc.resolve_order_snapshot(8, "CODE2", "").await.unwrap(),
            None
        );
        assert_eq!(
            svc.resolve_order_snapshot(1, "OFF", "").await.unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn clicks_respect_switch_status_and_dedupe() {
        let repo = Arc::new(Repo {
            profiles: vec![profile(2, 8, "CODE2", "active")],
            ..Repo::default()
        });
        let input = ClickInput {
            affiliate_code: " CODE2 ".into(),
            visitor_key: "v1".into(),
            ..ClickInput::default()
        };
        service(repo.clone(), serde_json::json!({"enabled": false}))
            .track_click(&input)
            .await
            .unwrap();
        assert!(repo.clicks.lock().unwrap().is_empty());
        service(repo.clone(), serde_json::json!({"enabled": true}))
            .track_click(&input)
            .await
            .unwrap();
        assert_eq!(repo.clicks.lock().unwrap().len(), 1);
        let dup = Arc::new(Repo {
            profiles: vec![profile(2, 8, "CODE2", "active")],
            recent: true,
            ..Repo::default()
        });
        service(dup.clone(), serde_json::json!({"enabled": true}))
            .track_click(&input)
            .await
            .unwrap();
        assert!(dup.clicks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn withdraw_and_status_validation() {
        let repo = Arc::new(Repo::default());
        let svc = service(
            repo,
            serde_json::json!({"enabled": true, "min_withdraw_amount": 10, "withdraw_channels": ["alipay"]}),
        );
        for (amount, channel) in [("x", "alipay"), ("5", "alipay"), ("10", "bank")] {
            assert_eq!(
                svc.apply_withdraw(1, amount, channel, "acc")
                    .await
                    .unwrap_err()
                    .key(),
                "error.bad_request"
            );
        }
        assert_eq!(
            svc.set_status(1, "banned").await.unwrap_err().key(),
            "error.bad_request"
        );
        let err = svc.set_status(404, "active").await.unwrap_err();
        assert_eq!((err.kind().code(), err.key()), (404, "error.bad_request"));
        assert_eq!(svc.batch_status(&[0, -1], "active").await.unwrap(), 0);
        let off = service(
            Arc::new(Repo::default()),
            serde_json::json!({"enabled": false}),
        );
        assert_eq!(
            off.apply_withdraw(1, "10", "alipay", "a")
                .await
                .unwrap_err()
                .key(),
            "error.forbidden"
        );
        assert_eq!(off.open(1).await.unwrap_err().key(), "error.forbidden");
    }
}
