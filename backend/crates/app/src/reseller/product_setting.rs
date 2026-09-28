//! Reseller product listing / pricing rules (`application/product_setting.go`).

use std::sync::Arc;

use zs_domain::reseller::ports::{
    ProductListFilter, ProductSettingRepo, ProfileRepo, SettingFilter, SettingSummary,
};
use zs_domain::reseller::pricing::{
    EffectivePrices, PreviewItem, PricedProduct, SettingInput, effective_prices, normalize_setting,
    preview,
};
use zs_domain::reseller::{ProductSetting, Profile, not_found};
use zs_domain::{Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

use super::{active_profile_by_id, active_profile_of_user};

/// A product with the reseller's rules and resulting prices.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingView {
    pub product: PricedProduct,
    pub settings: Vec<ProductSetting>,
    pub effective: EffectivePrices,
}

/// Console list filter (the reseller comes from the user).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductQuery {
    pub category_id: Option<Id>,
    pub keyword: String,
    pub configured: String,
    pub listed: String,
}

/// Product setting use cases for the console (by user) and admin (by reseller id).
#[derive(Clone)]
pub struct ProductSettingService {
    profiles: Arc<dyn ProfileRepo>,
    repo: Arc<dyn ProductSettingRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ProductSettingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProductSettingService")
    }
}

/// Whose rules an operation targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// The console user.
    User(Id),
    /// A reseller chosen by an administrator.
    Reseller(Id),
}

impl ProductSettingService {
    pub fn new(
        profiles: Arc<dyn ProfileRepo>,
        repo: Arc<dyn ProductSettingRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            profiles,
            repo,
            clock,
        }
    }

    async fn profile(&self, owner: Owner) -> Result<Profile> {
        match owner {
            Owner::User(id) => active_profile_of_user(self.profiles.as_ref(), id).await,
            Owner::Reseller(id) => active_profile_by_id(self.profiles.as_ref(), id).await,
        }
    }

    fn view(
        profile: &Profile,
        product: PricedProduct,
        settings: Vec<ProductSetting>,
    ) -> SettingView {
        let effective = effective_prices(profile, &product, &settings);
        SettingView {
            product,
            settings,
            effective,
        }
    }

    /// Console product list (active products only).
    pub async fn list(
        &self,
        user_id: Id,
        query: &ProductQuery,
        page: PageRequest,
    ) -> Result<Page<SettingView>> {
        let profile = self.profile(Owner::User(user_id)).await?;
        let filter = ProductListFilter {
            reseller_id: profile.id,
            category_id: query.category_id,
            keyword: query.keyword.trim().to_owned(),
            configured: query.configured.trim().to_lowercase(),
            listed: query.listed.trim().to_lowercase(),
        };
        let rows = self.repo.list_products(&filter, page).await?;
        Ok(rows.map(|(product, settings)| Self::view(&profile, product, settings)))
    }

    /// One product with its rules.
    pub async fn get(&self, owner: Owner, product_id: Id) -> Result<SettingView> {
        let profile = self.profile(owner).await?;
        self.detail(&profile, product_id).await
    }

    async fn detail(&self, profile: &Profile, product_id: Id) -> Result<SettingView> {
        let (product, settings) = self
            .repo
            .product_with_settings(profile.id, product_id)
            .await?
            .ok_or_else(not_found)?;
        Ok(Self::view(profile, product, settings))
    }

    /// Prices proposed rules without saving.
    pub async fn preview(
        &self,
        owner: Owner,
        product_id: Id,
        inputs: &[SettingInput],
    ) -> Result<Vec<PreviewItem>> {
        let profile = self.profile(owner).await?;
        let (product, _) = self
            .repo
            .product_with_settings(profile.id, product_id)
            .await?
            .ok_or_else(not_found)?;
        preview(&profile, &product, inputs)
    }

    /// Validates every rule, saves them in one transaction and returns the new detail.
    pub async fn save(
        &self,
        owner: Owner,
        product_id: Id,
        inputs: &[SettingInput],
    ) -> Result<SettingView> {
        let profile = self.profile(owner).await?;
        let product = self
            .repo
            .product_for_save(product_id)
            .await?
            .ok_or_else(not_found)?;
        let rows = inputs
            .iter()
            .map(|input| {
                let mut row = normalize_setting(&profile, &product, input)?;
                row.reseller_id = profile.id;
                row.product_id = product.id;
                Ok(row)
            })
            .collect::<Result<Vec<_>>>()?;
        self.repo
            .save_settings(profile.id, product.id, &rows, self.clock.now())
            .await?;
        self.detail(&profile, product_id).await
    }

    /// Deletes one rule (`sku_id = 0` = product rule).
    pub async fn reset(&self, owner: Owner, product_id: Id, sku_id: Id) -> Result<()> {
        let profile = self.profile(owner).await?;
        if product_id == 0 {
            return Ok(());
        }
        self.repo
            .delete_setting(profile.id, product_id, sku_id, self.clock.now())
            .await
    }

    pub async fn list_admin(
        &self,
        filter: &SettingFilter,
        page: PageRequest,
    ) -> Result<Page<ProductSetting>> {
        self.repo.list_settings(filter, page).await
    }

    pub async fn summarize(&self, reseller_id: Id) -> Result<SettingSummary> {
        self.repo.summarize(reseller_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, clock};
    use rust_decimal::Decimal;
    use zs_domain::reseller::{ProfileStatus, keys};

    #[tokio::test]
    async fn save_validates_and_resolves_owner() {
        let store = Arc::new(MemStore::default());
        let svc = ProductSettingService::new(store.clone(), store.clone(), clock());
        let input = SettingInput {
            sku_id: 0,
            is_listed: true,
            pricing_mode: "markup_percent".into(),
            markup_percent: Decimal::from(10),
            ..SettingInput::default()
        };
        assert_eq!(
            svc.save(Owner::User(5), 1, std::slice::from_ref(&input))
                .await
                .unwrap_err()
                .key(),
            keys::BAD_REQUEST
        );
        assert!(
            svc.save(Owner::Reseller(77), 1, std::slice::from_ref(&input))
                .await
                .unwrap_err()
                .is_not_found()
        );
        let pid = store.add_profile(5, ProfileStatus::Active);
        assert!(
            svc.save(Owner::User(5), 404, std::slice::from_ref(&input))
                .await
                .unwrap_err()
                .is_not_found()
        );
        store.add_product(1, "100.00", &[(11, "100.00")]);
        let view = svc
            .save(Owner::User(5), 1, &[input])
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(view.settings.len(), 1);
        assert_eq!(view.settings[0].reseller_id, pid);
        assert_eq!(
            view.effective.prices.get(&11).map(ToString::to_string),
            Some("110.00".into())
        );
        svc.reset(Owner::Reseller(pid), 1, 0)
            .await
            .unwrap_or_else(|e| panic!("{e}"));
        assert!(
            svc.get(Owner::User(5), 1)
                .await
                .map(|v| v.settings.is_empty())
                .unwrap_or_default()
        );
    }
}
