//! The reseller's sales orders (`application/order_query.go`).

use std::sync::Arc;

use zs_domain::reseller::orders::{ResellerOrderItem, ResellerOrderLine, detail_lines, list_item};
use zs_domain::reseller::ports::{OrderFilter, OrderStats, OrderViewRepo, ProfileRepo};
use zs_domain::reseller::{keys, not_found};
use zs_domain::{Error, Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::active_profile_of_user;

/// Order query use cases (active profile required; settlement status not checked).
#[derive(Clone)]
pub struct OrderQueryService {
    profiles: Arc<dyn ProfileRepo>,
    repo: Arc<dyn OrderViewRepo>,
}

impl std::fmt::Debug for OrderQueryService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OrderQueryService")
    }
}

impl OrderQueryService {
    pub fn new(profiles: Arc<dyn ProfileRepo>, repo: Arc<dyn OrderViewRepo>) -> Self {
        Self { profiles, repo }
    }

    pub async fn list_user(
        &self,
        user_id: Id,
        filter: &OrderFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerOrderItem>> {
        let profile = active_profile_of_user(self.profiles.as_ref(), user_id).await?;
        Ok(self
            .repo
            .list_orders(profile.id, filter, page)
            .await?
            .map(|r| list_item(&r)))
    }

    pub async fn stats_user(&self, user_id: Id, filter: &OrderFilter) -> Result<OrderStats> {
        let profile = active_profile_of_user(self.profiles.as_ref(), user_id).await?;
        self.repo.order_stats(profile.id, filter).await
    }

    /// Order detail; orders of other resellers / the main site are `error.order_not_found`.
    pub async fn detail_user(
        &self,
        user_id: Id,
        order_no: &str,
    ) -> Result<(ResellerOrderItem, Vec<ResellerOrderLine>)> {
        let profile = active_profile_of_user(self.profiles.as_ref(), user_id).await?;
        let row = self
            .repo
            .order_by_no(profile.id, order_no.trim())
            .await?
            .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
        Ok((list_item(&row), detail_lines(&row)))
    }

    /// Admin view of a reseller's orders.
    pub async fn list_admin(
        &self,
        reseller_id: Id,
        filter: &OrderFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerOrderItem>> {
        self.profiles
            .profile_by_id(reseller_id)
            .await?
            .ok_or_else(not_found)?;
        Ok(self
            .repo
            .list_orders(reseller_id, filter, page)
            .await?
            .map(|r| list_item(&r)))
    }
}
