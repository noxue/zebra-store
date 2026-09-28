//! Reseller site branding and the `/public/config` overlay (`application/site_config.go`).

use std::sync::{Arc, PoisonError, RwLock};

use async_trait::async_trait;
use serde_json::Value;
use zs_domain::content::public::{PublicConfigOverlay, Tenant};
use zs_domain::reseller::ports::{ProfileRepo, SiteConfigFilter, SiteConfigRepo};
use zs_domain::reseller::site::{SiteConfigInput, normalize_site_config, overlay_public_config};
use zs_domain::reseller::tenant::normalize_host;
use zs_domain::reseller::{Profile, SiteConfig, not_found, profile_inactive};
use zs_domain::{Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

use super::profile_of_user;

/// Callback clearing the cached `/public/config` (installed by the wiring).
pub type Invalidate = Arc<dyn Fn() + Send + Sync>;

/// The console view of a site configuration.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteSnapshot {
    pub profile: Profile,
    pub config: Option<SiteConfig>,
    pub can_edit: bool,
}

/// Site configuration use cases; also the content group's [`PublicConfigOverlay`].
#[derive(Clone)]
pub struct SiteConfigService {
    profiles: Arc<dyn ProfileRepo>,
    repo: Arc<dyn SiteConfigRepo>,
    invalidate: Arc<RwLock<Option<Invalidate>>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SiteConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SiteConfigService")
    }
}

impl SiteConfigService {
    pub fn new(
        profiles: Arc<dyn ProfileRepo>,
        repo: Arc<dyn SiteConfigRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            profiles,
            repo,
            invalidate: Arc::new(RwLock::new(None)),
            clock,
        }
    }

    /// Installs the public-config cache invalidation (content group).
    pub fn set_invalidator(&self, f: Invalidate) {
        *self
            .invalidate
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(f);
    }

    fn invalidate_public_config(&self) {
        let f = self
            .invalidate
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(f) = f {
            f();
        }
    }

    /// `GET /reseller/site-config`.
    pub async fn user_snapshot(&self, user_id: Id) -> Result<SiteSnapshot> {
        let profile = profile_of_user(self.profiles.as_ref(), user_id).await?;
        let config = self.repo.site_config(profile.id).await?;
        let can_edit = profile.is_active();
        Ok(SiteSnapshot {
            profile,
            config,
            can_edit,
        })
    }

    /// Whether the user may edit (and upload images for) their site.
    pub async fn can_edit(&self, user_id: Id) -> Result<bool> {
        Ok(profile_of_user(self.profiles.as_ref(), user_id)
            .await?
            .is_active())
    }

    /// `PUT /reseller/site-config` (active profiles only).
    pub async fn update_user(&self, user_id: Id, input: &SiteConfigInput) -> Result<SiteConfig> {
        let profile = profile_of_user(self.profiles.as_ref(), user_id).await?;
        if !profile.is_active() {
            return Err(profile_inactive());
        }
        self.save(profile.id, input).await
    }

    /// `PUT /admin/resellers/site-configs/:reseller_id` (any existing profile).
    pub async fn update_admin(
        &self,
        reseller_id: Id,
        input: &SiteConfigInput,
    ) -> Result<SiteConfig> {
        self.profiles
            .profile_by_id(reseller_id)
            .await?
            .ok_or_else(not_found)?;
        self.save(reseller_id, input).await
    }

    async fn save(&self, reseller_id: Id, input: &SiteConfigInput) -> Result<SiteConfig> {
        let draft = normalize_site_config(input)?;
        let saved = self
            .repo
            .upsert_site_config(reseller_id, &draft, self.clock.now())
            .await?;
        self.invalidate_public_config();
        Ok(saved)
    }

    /// `POST /admin/resellers/site-configs/:reseller_id/reset`.
    pub async fn reset_admin(&self, reseller_id: Id) -> Result<()> {
        self.profiles
            .profile_by_id(reseller_id)
            .await?
            .ok_or_else(not_found)?;
        self.repo
            .delete_site_config(reseller_id, self.clock.now())
            .await?;
        self.invalidate_public_config();
        Ok(())
    }

    /// Saved configuration, or `None` plus the profile when none is saved.
    /// `Err(not found)` when neither exists.
    pub async fn admin_get(&self, reseller_id: Id) -> Result<(Option<SiteConfig>, Profile)> {
        let cfg = self.repo.site_config(reseller_id).await?;
        if let Some(profile) = cfg.as_ref().and_then(|c| c.profile.clone()) {
            return Ok((cfg, profile));
        }
        let profile = self
            .profiles
            .profile_by_id(reseller_id)
            .await?
            .ok_or_else(not_found)?;
        Ok((cfg, profile))
    }

    pub async fn site_config(&self, reseller_id: Id) -> Result<Option<SiteConfig>> {
        self.repo.site_config(reseller_id).await
    }

    pub async fn list_admin(
        &self,
        filter: &SiteConfigFilter,
        page: PageRequest,
    ) -> Result<Page<SiteConfig>> {
        self.repo.list_site_configs(filter, page).await
    }

    /// Applies the reseller branding to a main-site public config (`ApplyPublicConfigOverlay`).
    pub async fn overlay(&self, reseller_id: Id, host: &str, base: Value) -> Result<Value> {
        let host = normalize_host(host);
        let active = self
            .profiles
            .profile_by_id(reseller_id)
            .await?
            .is_some_and(|p| p.is_active());
        let site = if active {
            self.repo.site_config(reseller_id).await?
        } else {
            None
        };
        Ok(overlay_public_config(base, &host, &host, site.as_ref()))
    }
}

#[async_trait]
impl PublicConfigOverlay for SiteConfigService {
    async fn apply(&self, tenant: &Tenant, base: Value) -> Result<Value> {
        match tenant.reseller_id {
            Some(id) => self.overlay(id, &tenant.host, base).await,
            None => Ok(base),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, clock};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zs_domain::reseller::{ProfileStatus, keys};

    fn service(store: &Arc<MemStore>) -> SiteConfigService {
        SiteConfigService::new(store.clone(), store.clone(), clock())
    }

    #[tokio::test]
    async fn update_invalidates_public_config_and_overlays() {
        let store = Arc::new(MemStore::default());
        let svc = service(&store);
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        svc.set_invalidator(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        let input: SiteConfigInput =
            serde_json::from_value(json!({"site_name": "R"})).unwrap_or_default();
        assert_eq!(
            svc.update_user(5, &input).await.unwrap_err().key(),
            keys::BAD_REQUEST
        );
        store.add_profile(5, ProfileStatus::PendingReview);
        assert_eq!(
            svc.update_user(5, &input).await.unwrap_err().key(),
            keys::PROFILE_INACTIVE
        );
        let pid = store.add_profile(6, ProfileStatus::Active);
        let saved = svc
            .update_user(6, &input)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(saved.site_name, "R");
        assert_eq!(hits.load(Ordering::SeqCst), 1);

        let tenant = Tenant {
            reseller_id: Some(pid),
            host: "R.test".into(),
        };
        let out = svc
            .apply(&tenant, json!({"brand": {"site_name": "Main"}}))
            .await
            .unwrap_or_default();
        assert_eq!(out["brand"]["site_name"], "R");
        assert_eq!(out["tenant"]["mode"], "reseller");
        assert_eq!(out["tenant"]["host"], "r.test");

        svc.reset_admin(pid).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        let out = svc
            .apply(&tenant, json!({"brand": {"site_name": "Main"}}))
            .await
            .unwrap_or_default();
        assert_eq!(out["brand"]["site_name"], "Main");
        assert!(svc.reset_admin(999).await.unwrap_err().is_not_found());
    }
}
