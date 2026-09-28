//! Banner use cases (port of `banner_service.go`).

use std::sync::Arc;

use zs_domain::content::banner::{
    Banner, BannerInput, BannerQuery, BannerRepo, normalize_position,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::Page;

const NOT_FOUND: &str = "error.banner_not_found";
const FETCH_FAILED: &str = "error.banner_fetch_failed";

/// Banner queries and admin commands.
#[derive(Clone)]
pub struct BannerService {
    repo: Arc<dyn BannerRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for BannerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BannerService")
    }
}

impl BannerService {
    pub fn new(repo: Arc<dyn BannerRepo>, clock: Arc<dyn Clock>) -> Self {
        Self { repo, clock }
    }

    pub async fn list_admin(&self, mut query: BannerQuery) -> Result<Page<Banner>> {
        query.position = query.position.trim().to_owned();
        query.search = query.search.trim().to_owned();
        self.repo
            .list(&query)
            .await
            .map_err(|e| e.or_internal(FETCH_FAILED))
    }

    /// Active banners inside their time window.
    pub async fn list_public(&self, position: &str, limit: u64) -> Result<Vec<Banner>> {
        self.repo
            .list_valid(&normalize_position(position), limit, self.clock.now())
            .await
            .map_err(|e| e.or_internal(FETCH_FAILED))
    }

    pub async fn get(&self, id: Id) -> Result<Banner> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(NOT_FOUND))
    }

    pub async fn create(&self, input: BannerInput) -> Result<Banner> {
        let banner = input.build(None, self.clock.now())?;
        self.repo
            .create(&banner)
            .await
            .map_err(|e| e.or_internal("error.banner_create_failed"))
    }

    pub async fn update(&self, id: Id, input: BannerInput) -> Result<Banner> {
        let existing = self.get(id).await?;
        let banner = input.build(Some(existing), self.clock.now())?;
        self.repo
            .update(&banner)
            .await
            .map_err(|e| e.or_internal("error.banner_update_failed"))?;
        Ok(banner)
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        self.get(id).await?;
        self.repo
            .delete(id)
            .await
            .map_err(|e| e.or_internal("error.banner_delete_failed"))
    }
}
