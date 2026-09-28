//! Email brand resolution (original `bootstrap/mailbrand.Resolver`, NTF-02): the main shop
//! uses `site_config.brand`; a reseller site uses its own site config and never falls back
//! to the main brand.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use zs_domain::Result;
use zs_domain::identity::mailer::{BrandScope, MailBrand, MailBrands, normalize_reply_to};
use zs_domain::reseller::ports::SiteConfigRepo;
use zs_domain::reseller::tenant::normalize_host;
use zs_domain::settings::schema::site::SiteBrand;
use zs_domain::settings::{SettingsStore, keys as setting_keys};

/// [`MailBrands`] over the settings store and reseller site configs.
#[derive(Clone)]
pub struct MailBrandResolver {
    settings: Arc<dyn SettingsStore>,
    sites: Arc<dyn SiteConfigRepo>,
}

impl std::fmt::Debug for MailBrandResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MailBrandResolver")
    }
}

impl MailBrandResolver {
    pub fn new(settings: Arc<dyn SettingsStore>, sites: Arc<dyn SiteConfigRepo>) -> Self {
        Self { settings, sites }
    }
}

#[async_trait]
impl MailBrands for MailBrandResolver {
    async fn resolve(&self, scope: &BrandScope) -> Result<MailBrand> {
        let Some(reseller_id) = scope.reseller_id else {
            let raw = self.settings.get(setting_keys::SITE_CONFIG).await?;
            let brand = SiteBrand::decode(raw.as_ref());
            return Ok(MailBrand {
                site_name: brand.site_name,
                site_url: brand.site_url,
                ..MailBrand::default()
            });
        };
        let mut brand = MailBrand::reseller_fallback(&normalize_host(&scope.host));
        if reseller_id <= 0 {
            return Ok(brand);
        }
        // Read errors fail closed (the caller retries); the main brand is never used.
        let Some(cfg) = self.sites.site_config(reseller_id).await? else {
            return Ok(brand);
        };
        let name = cfg.site_name.trim();
        if !name.is_empty() {
            brand.site_name = name.to_owned();
            brand.from_name = name.to_owned();
        }
        if let Some(Value::String(raw)) = cfg.support.get("email") {
            brand.reply_to = normalize_reply_to(raw);
        }
        Ok(brand)
    }
}
