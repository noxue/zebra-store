//! `reseller` use cases: tenant resolution, onboarding / domain management, site
//! branding, product pricing rules, sales orders, profit ledger and withdrawals.

pub mod finance;
pub mod ledger;
pub mod management;
pub mod operations;
pub mod orders;
pub mod product_setting;
pub mod site_config;
pub mod tenant;

use std::sync::Arc;

use zs_domain::reseller::ports::{PricingRepo, ProfileRepo};
use zs_domain::reseller::{Profile, not_found, not_opened, profile_inactive};
use zs_domain::{Id, Result};

/// Services of the `reseller` group.
#[derive(Debug, Clone)]
pub struct ResellerServices {
    pub tenant: tenant::TenantResolver,
    pub management: management::ManagementService,
    pub site_config: site_config::SiteConfigService,
    pub product_settings: product_setting::ProductSettingService,
    pub orders: orders::OrderQueryService,
    pub finance: finance::FinanceService,
    /// Profit posting / refund claw-back / confirmation — the port the order group calls.
    pub ledger: ledger::LedgerService,
    pub operations: operations::OperationsService,
    /// Pricing reads for the order and catalog groups (reseller storefront prices).
    pub pricing: PricingReads,
}

/// Shared handle on [`PricingRepo`] (reseller site pricing reads for other groups).
#[derive(Clone)]
pub struct PricingReads {
    pub profiles: Arc<dyn ProfileRepo>,
    pub repo: Arc<dyn PricingRepo>,
}

impl std::fmt::Debug for PricingReads {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PricingReads")
    }
}

impl PricingReads {
    /// Active profile of a reseller site, or `error.reseller_product_not_listed`
    /// (`loadActiveProfile` of the original pricing resolver).
    pub async fn active_profile(&self, reseller_id: Id) -> Result<Profile> {
        match self.profiles.profile_by_id(reseller_id).await? {
            Some(p) if p.is_active() => Ok(p),
            _ => Err(zs_domain::Error::bad_request(
                zs_domain::reseller::keys::PRODUCT_NOT_LISTED,
            )),
        }
    }
}

/// The user's profile or `ErrNotOpened`.
pub(crate) async fn profile_of_user(profiles: &dyn ProfileRepo, user_id: Id) -> Result<Profile> {
    profiles
        .profile_by_user(user_id)
        .await?
        .ok_or_else(not_opened)
}

/// The user's active profile (`requireActiveProfileByUser`).
pub(crate) async fn active_profile_of_user(
    profiles: &dyn ProfileRepo,
    user_id: Id,
) -> Result<Profile> {
    let profile = profile_of_user(profiles, user_id).await?;
    if !profile.is_active() {
        return Err(profile_inactive());
    }
    Ok(profile)
}

/// An existing, active profile by id (`requireActiveProfileByID`).
pub(crate) async fn active_profile_by_id(profiles: &dyn ProfileRepo, id: Id) -> Result<Profile> {
    let profile = profiles.profile_by_id(id).await?.ok_or_else(not_found)?;
    if !profile.is_active() {
        return Err(profile_inactive());
    }
    Ok(profile)
}

#[cfg(test)]
pub(crate) mod testkit;
