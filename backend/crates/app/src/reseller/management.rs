//! Onboarding, profile review and domain management (`application/management.go`).

use std::sync::Arc;

use rust_decimal::Decimal;
use zs_domain::reseller::ports::{DomainDns, DomainFilter, ProfileFilter, ProfileRepo};
use zs_domain::reseller::rules::{
    self, ApplyOutcome, DomainAction, DomainPolicy, apply_approval, apply_outcome, apply_review,
    validate_custom_domain, validate_markup, validate_system_subdomain,
};
use zs_domain::reseller::{
    DomainType, Profile, ProfileStatus, ResellerDomain, SettlementStatus, keys, not_found,
    profile_inactive,
};
use zs_domain::settings::{ResellerDomainReviewMode, SettingsStore, keys as setting_keys};
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
    dns: Arc<dyn DomainDns>,
    settings: Arc<dyn SettingsStore>,
    default_cname_target: String,
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
        dns: Arc<dyn DomainDns>,
        settings: Arc<dyn SettingsStore>,
        cfg: &ResellerConfig,
        cache: Arc<TenantCache>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            profiles,
            dns,
            settings,
            default_cname_target: cfg
                .cname_target
                .trim()
                .trim_end_matches('.')
                .to_ascii_lowercase(),
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

    /// Submits a custom domain, activating it after CNAME validation in auto mode.
    pub async fn submit_custom_domain(&self, user_id: Id, raw: &str) -> Result<ResellerDomain> {
        let profile = profile_of_user(self.profiles.as_ref(), user_id).await?;
        if !profile.is_active() {
            return Err(profile_inactive());
        }
        let domain = validate_custom_domain(raw, &self.policy)?;
        let (target, mode) = self.domain_settings().await?;
        let auto = mode == ResellerDomainReviewMode::Auto;
        if auto {
            self.verify_cname(&domain, &target).await?;
        }
        let row = self
            .profiles
            .add_custom_domain(profile.id, &domain, self.clock.now())
            .await?;
        if auto {
            self.apply_domain_action(row.id, DomainAction::Approve)
                .await
        } else {
            Ok(row)
        }
    }

    /// Auto mode is effective only while a shared CNAME target is configured.
    pub async fn domain_settings(&self) -> Result<(String, ResellerDomainReviewMode)> {
        let stored = self
            .settings
            .get(setting_keys::RESELLER_DOMAIN_REVIEW_CONFIG)
            .await?
            .and_then(|value| serde_json::from_value::<ResellerDomainReviewSetting>(value).ok());
        let target = stored
            .as_ref()
            .and_then(|setting| setting.cname_target.as_deref())
            .unwrap_or(&self.default_cname_target)
            .trim()
            .trim_end_matches('.')
            .to_ascii_lowercase();
        let mode = if target.is_empty() {
            ResellerDomainReviewMode::Manual
        } else {
            stored.map_or(ResellerDomainReviewMode::Manual, |setting| setting.mode)
        };
        Ok((target, mode))
    }

    async fn verify_cname(&self, domain: &str, target: &str) -> Result<()> {
        let actual = self.dns.cname_target(domain).await?;
        if actual
            .as_deref()
            .map(|value| value.trim_end_matches('.').to_ascii_lowercase())
            != Some(target.to_owned())
        {
            return Err(Error::bad_request(keys::DOMAIN_CNAME_MISMATCH));
        }
        Ok(())
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
        if action == DomainAction::Approve {
            let (cname_target, _) = self.domain_settings().await?;
            if !cname_target.is_empty() {
                let domain = self
                    .profiles
                    .domain_by_id(id)
                    .await?
                    .ok_or_else(not_found)?;
                if domain.kind == DomainType::Custom {
                    self.verify_cname(&domain.domain, &cname_target).await?;
                }
            }
        }
        self.apply_domain_action(id, action).await
    }

    async fn apply_domain_action(&self, id: Id, action: DomainAction) -> Result<ResellerDomain> {
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

#[derive(serde::Deserialize)]
struct ResellerDomainReviewSetting {
    mode: ResellerDomainReviewMode,
    cname_target: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, MockDomainDns, MockReviewSettings, clock, config};
    use zs_domain::reseller::DomainStatus;

    fn service(store: &Arc<MemStore>, cfg: &ResellerConfig) -> ManagementService {
        ManagementService::new(
            store.clone(),
            Arc::new(MockDomainDns::default()),
            Arc::new(MockReviewSettings::default()),
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
    async fn cname_check_runs_during_admin_approval() {
        let store = Arc::new(MemStore::default());
        let profile_id = store.add_profile(8, ProfileStatus::Active);
        let dns = Arc::new(MockDomainDns::default());
        let mut cfg = config();
        cfg.cname_target = "Connect.Example.com.".into();
        let svc = ManagementService::new(
            store.clone(),
            dns.clone(),
            Arc::new(MockReviewSettings::default()),
            &cfg,
            Arc::new(TenantCache::default()),
            clock(),
        );
        assert_eq!(
            svc.domain_settings().await.unwrap().0,
            "connect.example.com"
        );
        let domain = svc
            .submit_custom_domain(8, "shop.agent.test")
            .await
            .unwrap();
        assert_eq!(domain.status, DomainStatus::PendingReview);
        assert_eq!(store.domains_of(profile_id).await.unwrap().len(), 1);
        assert_eq!(
            svc.domain_action(domain.id, DomainAction::Approve)
                .await
                .unwrap_err()
                .key(),
            keys::DOMAIN_CNAME_MISMATCH
        );
        *dns.answer.lock().unwrap() = Some("other.example.com.".into());
        assert_eq!(
            svc.domain_action(domain.id, DomainAction::Approve)
                .await
                .unwrap_err()
                .key(),
            keys::DOMAIN_CNAME_MISMATCH
        );
        assert_eq!(
            store.domain_by_id(domain.id).await.unwrap().unwrap().status,
            DomainStatus::PendingReview
        );
        *dns.fail.lock().unwrap() = true;
        assert_eq!(
            svc.domain_action(domain.id, DomainAction::Approve)
                .await
                .unwrap_err()
                .kind(),
            zs_domain::ErrorKind::Internal
        );
        assert_eq!(
            store.domain_by_id(domain.id).await.unwrap().unwrap().status,
            DomainStatus::PendingReview
        );
        *dns.fail.lock().unwrap() = false;
        *dns.answer.lock().unwrap() = Some("CONNECT.EXAMPLE.COM.".into());
        let approved = svc
            .domain_action(domain.id, DomainAction::Approve)
            .await
            .unwrap();
        assert_eq!(approved.status, DomainStatus::Active);
        assert_eq!(store.domains_of(profile_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn automatic_review_requires_matching_cname_before_creating_domain() {
        let store = Arc::new(MemStore::default());
        let profile_id = store.add_profile(9, ProfileStatus::Active);
        let dns = Arc::new(MockDomainDns::default());
        let settings = Arc::new(MockReviewSettings::default());
        *settings.value.lock().unwrap() = Some(serde_json::json!({
            "mode": "auto", "cname_target": "connect.example.com"
        }));
        let cfg = config();
        let svc = ManagementService::new(
            store.clone(),
            dns.clone(),
            settings,
            &cfg,
            Arc::new(TenantCache::default()),
            clock(),
        );
        assert_eq!(
            svc.domain_settings().await.unwrap().1,
            ResellerDomainReviewMode::Auto
        );
        assert_eq!(
            svc.submit_custom_domain(9, "shop.agent.test")
                .await
                .unwrap_err()
                .key(),
            keys::DOMAIN_CNAME_MISMATCH
        );
        assert!(store.domains_of(profile_id).await.unwrap().is_empty());
        *dns.answer.lock().unwrap() = Some("other.example.com".into());
        assert_eq!(
            svc.submit_custom_domain(9, "shop.agent.test")
                .await
                .unwrap_err()
                .key(),
            keys::DOMAIN_CNAME_MISMATCH
        );
        assert!(store.domains_of(profile_id).await.unwrap().is_empty());
        *dns.answer.lock().unwrap() = Some("CONNECT.EXAMPLE.COM.".into());
        let approved = svc
            .submit_custom_domain(9, "shop.agent.test")
            .await
            .unwrap();
        assert_eq!(approved.status, DomainStatus::Active);
        assert_eq!(store.domains_of(profile_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_shared_target_keeps_manual_review_even_with_stored_auto_mode() {
        let store = Arc::new(MemStore::default());
        store.add_profile(10, ProfileStatus::Active);
        let settings = Arc::new(MockReviewSettings::default());
        *settings.value.lock().unwrap() = Some(serde_json::json!({"mode": "auto"}));
        let svc = ManagementService::new(
            store,
            Arc::new(MockDomainDns::default()),
            settings,
            &config(),
            Arc::new(TenantCache::default()),
            clock(),
        );
        assert_eq!(
            svc.domain_settings().await.unwrap().1,
            ResellerDomainReviewMode::Manual
        );
        assert_eq!(
            svc.submit_custom_domain(10, "shop.agent.test")
                .await
                .unwrap()
                .status,
            DomainStatus::PendingReview
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
