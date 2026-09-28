//! Host → tenant resolution with a per-host cache (`application/domain_resolver.go`,
//! RSL-03 / RSL-06).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Duration, Utc};
use zs_domain::reseller::ports::ProfileRepo;
use zs_domain::reseller::tenant::{
    ResellerTenant, UNAVAILABLE_NOT_FOUND, normalize_host, request_host,
};
use zs_domain::{Id, Result};
use zs_shared::clock::Clock;

use crate::config::ResellerConfig;

/// Positive cache lifetime (original: 5 minutes).
const FOUND_TTL_SECONDS: i64 = 300;
/// Negative cache lifetime (original: 60 seconds).
const NOT_FOUND_TTL_SECONDS: i64 = 60;
/// Cached hosts before expired entries are purged (bounds memory under Host floods).
const CACHE_SOFT_LIMIT: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Cached {
    Found {
        reseller_id: Id,
        user_id: Id,
        primary: String,
    },
    NotFound,
}

/// Per-host resolution cache, shared with the services that change domains/profiles.
#[derive(Debug, Default)]
pub struct TenantCache {
    entries: Mutex<HashMap<String, (Cached, DateTime<Utc>)>>,
}

impl TenantCache {
    fn get(&self, host: &str, now: DateTime<Utc>) -> Option<Cached> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(host)
            .filter(|(_, expires)| *expires > now)
            .map(|(c, _)| c.clone())
    }

    fn put(&self, host: &str, value: Cached, expires: DateTime<Utc>) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if entries.len() >= CACHE_SOFT_LIMIT {
            entries.retain(|_, (_, exp)| *exp > expires - Duration::seconds(FOUND_TTL_SECONDS));
            if entries.len() >= CACHE_SOFT_LIMIT {
                entries.clear();
            }
        }
        entries.insert(host.to_owned(), (value, expires));
    }

    /// Drops the positive and negative entries of `hosts`.
    pub fn invalidate<S: AsRef<str>>(&self, hosts: &[S]) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        for host in hosts {
            entries.remove(&normalize_host(host.as_ref()));
        }
    }
}

/// Resolves the tenant of storefront requests.
#[derive(Clone)]
pub struct TenantResolver {
    profiles: Arc<dyn ProfileRepo>,
    enabled: bool,
    trusted_forwarded_host: bool,
    main_hosts: Arc<HashSet<String>>,
    cache: Arc<TenantCache>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for TenantResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantResolver")
            .field("enabled", &self.enabled)
            .field("main_hosts", &self.main_hosts)
            .finish_non_exhaustive()
    }
}

impl TenantResolver {
    pub fn new(
        profiles: Arc<dyn ProfileRepo>,
        cfg: &ResellerConfig,
        cache: Arc<TenantCache>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let main_hosts = cfg
            .main_hosts
            .iter()
            .map(|h| normalize_host(h))
            .filter(|h| !h.is_empty())
            .collect();
        Self {
            profiles,
            enabled: cfg.enabled,
            trusted_forwarded_host: cfg.trusted_forwarded_host,
            main_hosts: Arc::new(main_hosts),
            cache,
            clock,
        }
    }

    /// The shared cache (for invalidation).
    pub fn cache(&self) -> &Arc<TenantCache> {
        &self.cache
    }

    /// Request host from `Host` / `X-Forwarded-Host` (the latter only when trusted).
    pub fn request_host(&self, host: Option<&str>, forwarded_host: Option<&str>) -> String {
        request_host(host, forwarded_host, self.trusted_forwarded_host)
    }

    /// Resolves a host (`ResolveHost`): disabled feature, blank or main hosts are the
    /// main shop; otherwise only an active + verified domain of an active profile is a
    /// reseller site, anything else is unavailable (negative-cached).
    pub async fn resolve(&self, raw_host: &str) -> Result<ResellerTenant> {
        let host = normalize_host(raw_host);
        if !self.enabled || host.is_empty() || self.main_hosts.contains(&host) {
            return Ok(ResellerTenant::main(&host));
        }
        let now = self.clock.now();
        match self.cache.get(&host, now) {
            Some(Cached::Found {
                reseller_id,
                user_id,
                primary,
            }) => {
                return Ok(ResellerTenant::reseller(
                    &host,
                    reseller_id,
                    user_id,
                    &primary,
                ));
            }
            Some(Cached::NotFound) => {
                return Ok(ResellerTenant::unavailable(&host, UNAVAILABLE_NOT_FOUND));
            }
            None => {}
        }
        let domain = self.profiles.live_domain(&host).await?;
        let live = domain.and_then(|d| {
            let profile = d.profile.as_deref().filter(|p| p.is_active())?;
            Some((d.reseller_id, profile.user_id, d.domain.trim().to_owned()))
        });
        match live {
            Some((reseller_id, user_id, primary)) => {
                self.cache.put(
                    &host,
                    Cached::Found {
                        reseller_id,
                        user_id,
                        primary: primary.clone(),
                    },
                    now + Duration::seconds(FOUND_TTL_SECONDS),
                );
                Ok(ResellerTenant::reseller(
                    &host,
                    reseller_id,
                    user_id,
                    &primary,
                ))
            }
            None => {
                self.cache.put(
                    &host,
                    Cached::NotFound,
                    now + Duration::seconds(NOT_FOUND_TTL_SECONDS),
                );
                Ok(ResellerTenant::unavailable(&host, UNAVAILABLE_NOT_FOUND))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, clock, config};
    use zs_domain::reseller::{DomainStatus, ProfileStatus};

    fn resolver(store: &Arc<MemStore>) -> TenantResolver {
        TenantResolver::new(
            store.clone(),
            &config(),
            Arc::new(TenantCache::default()),
            clock(),
        )
    }

    #[tokio::test]
    async fn main_hosts_and_disabled_feature() {
        let store = Arc::new(MemStore::default());
        let r = resolver(&store);
        assert!(
            r.resolve("LOCALHOST:5173")
                .await
                .unwrap_or_default()
                .is_main
        );
        assert!(r.resolve("").await.unwrap_or_default().is_main);
        let mut cfg = config();
        cfg.enabled = false;
        let off = TenantResolver::new(store, &cfg, Arc::new(TenantCache::default()), clock());
        assert!(
            off.resolve("shop.reseller.test")
                .await
                .unwrap_or_default()
                .is_main
        );
    }

    // RSL-03: a disabled profile's live domain resolves to an unavailable tenant.
    #[tokio::test]
    async fn rsl03_disabled_profile_domain_is_unavailable() {
        let store = Arc::new(MemStore::default());
        let pid = store.add_profile(9, ProfileStatus::Disabled);
        store.add_domain(pid, "shop.reseller.test", DomainStatus::Active, true);
        let t = resolver(&store)
            .resolve("shop.reseller.test")
            .await
            .unwrap_or_default();
        assert!(t.unavailable);
        assert_eq!(t.reseller_id, None);
    }

    // RSL-06: unverified domain → unavailable; cache invalidation picks up changes.
    #[tokio::test]
    async fn rsl06_resolution_and_cache_invalidation() {
        let store = Arc::new(MemStore::default());
        let pid = store.add_profile(9, ProfileStatus::Active);
        let did = store.add_domain(
            pid,
            "shop.reseller.test",
            DomainStatus::PendingReview,
            false,
        );
        let r = resolver(&store);
        assert!(
            r.resolve("Shop.Reseller.test:443")
                .await
                .unwrap_or_default()
                .unavailable
        );
        // approve without invalidation: negative cache still answers
        store.set_domain_live(did);
        assert!(
            r.resolve("shop.reseller.test")
                .await
                .unwrap_or_default()
                .unavailable
        );
        r.cache().invalidate(&["shop.reseller.test"]);
        let t = r.resolve("shop.reseller.test").await.unwrap_or_default();
        assert!(t.is_reseller());
        assert_eq!((t.reseller_id, t.reseller_user_id), (Some(pid), 9));
        assert_eq!(t.primary_domain, "shop.reseller.test");
    }
}
