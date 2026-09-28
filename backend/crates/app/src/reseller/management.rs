//! Onboarding, profile review and domain management (`application/management.go`).

use std::sync::Arc;

use rust_decimal::Decimal;
use zs_domain::reseller::ports::{DomainFilter, ProfileFilter, ProfileRepo};
use zs_domain::reseller::rules::{
    self, ApplyOutcome, DomainAction, DomainPolicy, apply_approval, apply_outcome, apply_review,
    validate_custom_domain, validate_markup, validate_system_subdomain,
};
use zs_domain::reseller::{
    Profile, ProfileStatus, ResellerDomain, SettlementStatus, keys, not_found, profile_inactive,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::profile_of_user;
use super::tenant::TenantCache;
use crate::config::ResellerConfig;

/// What `GET /reseller/profile` shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ManagementSnapshot {
    pub profile: Option<Profile>,
    pub domains: Vec<ResellerDomain>,
    pub can_apply: bool,
}

/// Admin edit of a profile's operating parameters.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProfileUpdate {
    pub default_markup_percent: Decimal,
    pub max_markup_percent: Decimal,
    /// Empty = unchanged; otherwise `normal` / `frozen`.
    pub settlement_status: String,
}

/// Onboarding and domain use cases.
#[derive(Clone)]
pub struct ManagementService {
    profiles: Arc<dyn ProfileRepo>,
    policy: DomainPolicy,
    apply_enabled: bool,
    cache: Arc<TenantCache>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ManagementService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagementService")
            .field("policy", &self.policy)
            .field("apply_enabled", &self.apply_enabled)
            .finish_non_exhaustive()
    }
}

impl ManagementService {
    pub fn new(
        profiles: Arc<dyn ProfileRepo>,
        cfg: &ResellerConfig,
        cache: Arc<TenantCache>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            profiles,
            policy: DomainPolicy {
                main_hosts: cfg.main_hosts.clone(),
                subdomain_base: cfg.subdomain_base.clone(),
            },
            apply_enabled: cfg.enabled && cfg.self_apply_enabled,
            cache,
            clock,
        }
    }

    /// Profile, domains and whether the user may (re)apply.
    pub async fn snapshot(&self, user_id: Id) -> Result<ManagementSnapshot> {
        let Some(profile) = self.profiles.profile_by_user(user_id).await? else {
            return Ok(ManagementSnapshot {
                profile: None,
                domains: Vec::new(),
                can_apply: self.apply_enabled,
            });
        };
        let domains = self.profiles.domains_of(profile.id).await?;
        let can_apply = profile.status == ProfileStatus::Rejected && self.apply_enabled;
        Ok(ManagementSnapshot {
            profile: Some(profile),
            domains,
            can_apply,
        })
    }

    /// Self-service application (`ApplyUserReseller`).
    pub async fn apply(&self, user_id: Id, reason: &str) -> Result<Profile> {
        if !self.apply_enabled {
            return Err(Error::forbidden(keys::FORBIDDEN));
        }
        let reason = reason.trim().to_owned();
        let existing = self.profiles.profile_by_user(user_id).await?;
        let now = self.clock.now();
        match apply_outcome(existing.as_ref())? {
            ApplyOutcome::Create => self.profiles.create_profile(user_id, &reason, now).await,
            ApplyOutcome::Unchanged => existing.ok_or_else(not_found),
            ApplyOutcome::Reapply => {
                let id = existing.map_or(0, |p| p.id);
                let plan = move |mut p: Profile| {
                    if p.status != ProfileStatus::Rejected {
                        return Ok(p);
                    }
                    p.status = ProfileStatus::PendingReview;
                    p.apply_reason.clone_from(&reason);
                    p.reject_reason.clear();
                    p.reviewed_by = None;
                    p.reviewed_at = None;
                    Ok(p)
                };
                self.update_profile(id, &plan).await
            }
        }
    }

    /// Submits a custom domain for manual review (no DNS verification yet, like the original).
    pub async fn submit_custom_domain(&self, user_id: Id, raw: &str) -> Result<ResellerDomain> {
        let profile = profile_of_user(self.profiles.as_ref(), user_id).await?;
        if !profile.is_active() {
            return Err(profile_inactive());
        }
        let domain = validate_custom_domain(raw, &self.policy)?;
        self.profiles
            .add_custom_domain(profile.id, &domain, self.clock.now())
            .await
    }

    pub async fn profile(&self, id: Id) -> Result<Option<Profile>> {
        self.profiles.profile_by_id(id).await
    }

    pub async fn domains_of(&self, reseller_id: Id) -> Result<Vec<ResellerDomain>> {
        self.profiles.domains_of(reseller_id).await
    }

    pub async fn list_profiles(
        &self,
        filter: &ProfileFilter,
        page: PageRequest,
    ) -> Result<Page<Profile>> {
        self.profiles.list_profiles(filter, page).await
    }

    pub async fn list_domains(
        &self,
        filter: &DomainFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerDomain>> {
        self.profiles.list_domains(filter, page).await
    }

    async fn update_profile(
        &self,
        id: Id,
        plan: &(dyn Fn(Profile) -> Result<Profile> + Send + Sync),
    ) -> Result<Profile> {
        let (profile, hosts) = self
            .profiles
            .update_profile(id, plan, self.clock.now())
            .await?
            .ok_or_else(not_found)?;
        // Any status change affects which domains resolve (RSL-03).
        self.cache.invalidate(&hosts);
        Ok(profile)
    }

    /// Approves a pending / rejected application.
    pub async fn approve(
        &self,
        admin_id: Id,
        id: Id,
        default_markup: Decimal,
        max_markup: Decimal,
    ) -> Result<Profile> {
        validate_markup(default_markup, max_markup)?;
        let now = self.clock.now();
        let plan = move |mut p: Profile| {
            apply_approval(&mut p, default_markup, max_markup, admin_id, now)?;
            Ok(p)
        };
        self.update_profile(id, &plan).await
    }

    /// Reject / disable / restore (`updateProfileReviewStatus`).
    pub async fn review(
        &self,
        admin_id: Id,
        id: Id,
        to: ProfileStatus,
        reason: &str,
    ) -> Result<Profile> {
        let now = self.clock.now();
        let reason = reason.to_owned();
        let plan = move |mut p: Profile| {
            apply_review(&mut p, to, &reason, admin_id, now)?;
            Ok(p)
        };
        self.update_profile(id, &plan).await
    }

    /// Markup limits and settlement status (`UpdateProfileOperationalConfig`).
    pub async fn update_operational(
        &self,
        admin_id: Id,
        id: Id,
        input: ProfileUpdate,
    ) -> Result<Profile> {
        validate_markup(input.default_markup_percent, input.max_markup_percent)?;
        let settlement = match input.settlement_status.trim() {
            "" => None,
            raw => Some(SettlementStatus::parse(raw).ok_or_else(Error::invalid)?),
        };
        let now = self.clock.now();
        let plan = move |mut p: Profile| {
            p.default_markup_percent = Amount::new(input.default_markup_percent);
            p.max_markup_percent = Amount::new(input.max_markup_percent);
            if let Some(s) = settlement {
                p.settlement_status = s;
            }
            p.reviewed_by = Some(admin_id);
            p.reviewed_at = Some(now);
            Ok(p)
        };
        self.update_profile(id, &plan).await
    }

    /// Assigns or renames the reseller's system subdomain.
    pub async fn assign_system_domain(&self, id: Id, raw: &str) -> Result<ResellerDomain> {
        let domain = validate_system_subdomain(raw, &self.policy)?;
        let (row, hosts) = self
            .profiles
            .assign_system_domain(id, &domain, self.clock.now())
            .await?
            .ok_or_else(not_found)?;
        self.cache.invalidate(&hosts);
        Ok(row)
    }

    /// Approve / disable / set-primary on a domain.
    pub async fn domain_action(&self, id: Id, action: DomainAction) -> Result<ResellerDomain> {
        let now = self.clock.now();
        let plan = move |target: &ResellerDomain, all: &[ResellerDomain]| {
            rules::plan_domain_action(action, target, all, now)
        };
        let (row, hosts) = self
            .profiles
            .update_domains(id, &plan, now)
            .await?
            .ok_or_else(not_found)?;
        self.cache.invalidate(&hosts);
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, clock, config};
    use zs_domain::reseller::DomainStatus;

    fn service(store: &Arc<MemStore>, cfg: &ResellerConfig) -> ManagementService {
        ManagementService::new(
            store.clone(),
            cfg,
            Arc::new(TenantCache::default()),
            clock(),
        )
    }

    #[tokio::test]
    async fn apply_flow() {
        let store = Arc::new(MemStore::default());
        let svc = service(&store, &config());
        let snap = svc.snapshot(5).await.unwrap_or_else(|e| panic!("{e}"));
        assert!(snap.profile.is_none() && snap.can_apply);
        let p = svc
            .apply(5, "  please  ")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (p.status, p.apply_reason.as_str()),
            (ProfileStatus::PendingReview, "please")
        );
        // pending → unchanged
        assert_eq!(
            svc.apply(5, "again").await.map(|p| p.apply_reason).ok(),
            Some("please".into())
        );
        let rejected = svc
            .review(1, p.id, ProfileStatus::Rejected, "no")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(rejected.reject_reason, "no");
        assert!(
            svc.snapshot(5)
                .await
                .map(|s| s.can_apply)
                .unwrap_or_default()
        );
        let again = svc
            .apply(5, "retry")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (again.status, again.reject_reason.as_str()),
            (ProfileStatus::PendingReview, "")
        );

        let mut off = config();
        off.self_apply_enabled = false;
        let err = service(&store, &off).apply(6, "x").await.unwrap_err();
        assert_eq!(
            (err.kind(), err.key()),
            (zs_domain::ErrorKind::Forbidden, keys::FORBIDDEN)
        );
    }

    #[tokio::test]
    async fn custom_domain_requires_active_profile() {
        let store = Arc::new(MemStore::default());
        let svc = service(&store, &config());
        assert_eq!(
            svc.submit_custom_domain(5, "a.test")
                .await
                .unwrap_err()
                .key(),
            keys::BAD_REQUEST
        );
        store.add_profile(5, ProfileStatus::PendingReview);
        assert_eq!(
            svc.submit_custom_domain(5, "a.test")
                .await
                .unwrap_err()
                .key(),
            keys::PROFILE_INACTIVE
        );
        let pid = store.add_profile(6, ProfileStatus::Active);
        let d = svc
            .submit_custom_domain(6, "Shop.A.test")
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (d.reseller_id, d.domain.as_str(), d.status),
            (pid, "shop.a.test", DomainStatus::PendingReview)
        );
        assert_eq!(
            svc.submit_custom_domain(6, "shop.a.test")
                .await
                .unwrap_err()
                .key(),
            keys::DOMAIN_CONFLICT
        );
    }

    #[tokio::test]
    async fn operational_update_validates() {
        let store = Arc::new(MemStore::default());
        let svc = service(&store, &config());
        let pid = store.add_profile(5, ProfileStatus::Active);
        let input = ProfileUpdate {
            default_markup_percent: Decimal::from(10),
            max_markup_percent: Decimal::from(50),
            settlement_status: "frozen".into(),
        };
        let p = svc
            .update_operational(1, pid, input.clone())
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(p.settlement_status, SettlementStatus::Frozen);
        let bad = ProfileUpdate {
            settlement_status: "weird".into(),
            ..input.clone()
        };
        assert!(svc.update_operational(1, pid, bad).await.is_err());
        assert!(
            svc.update_operational(1, 999, input)
                .await
                .unwrap_err()
                .is_not_found()
        );
    }
}
