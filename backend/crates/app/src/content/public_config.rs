//! `GET /public/config` (port of `settings/transport/http/public/handler.go`).
//!
//! The expensive part is cached per tenant for 60 s and invalidated whenever a
//! setting that feeds it is saved. The announcement, tenant, `server_time` and
//! `app_version` are evaluated per request so schedule boundaries are exact (SET-02).

use std::sync::{Arc, PoisonError, RwLock};

use serde_json::{Map, Value, json};
use zs_domain::Result;
use zs_domain::content::public::{PaymentChannelReader, PublicConfigOverlay, Tenant};
use zs_domain::settings::keys;
use zs_domain::settings::schema::site::{
    CURRENCY_DEFAULT, STOREFRONT_TEMPLATE_DEFAULT, default_languages, default_nav, normalize_theme,
};
use zs_domain::settings::schema::storefront::active_announcement;
use zs_shared::clock::Clock;

use super::settings::{PublicConfigCache, SettingsService};

const FETCH_FAILED: &str = "error.config_fetch_failed";
/// Version reported as `app_version`.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Builds the storefront bootstrap configuration.
#[derive(Clone)]
pub struct PublicConfigService {
    settings: SettingsService,
    channels: Arc<dyn PaymentChannelReader>,
    /// Reseller overlay; late-bound so the reseller wiring can install it
    /// (`services.content.public_config.set_overlay(..)`).
    overlay: Arc<RwLock<Option<Arc<dyn PublicConfigOverlay>>>>,
    cache: Arc<PublicConfigCache>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for PublicConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PublicConfigService")
    }
}

/// Defaults under the stored `site_config`.
fn defaults() -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("languages".into(), json!(default_languages()));
    m.insert("currency".into(), json!(CURRENCY_DEFAULT));
    m.insert(
        "storefront_template".into(),
        json!(STOREFRONT_TEMPLATE_DEFAULT),
    );
    m.insert(
        "contact".into(),
        json!({"telegram": "https://telegram.me/dujiaoka", "whatsapp": "https://wa.me/1234567890"}),
    );
    m.insert("scripts".into(), json!([]));
    m
}

impl PublicConfigService {
    pub fn new(
        settings: SettingsService,
        channels: Arc<dyn PaymentChannelReader>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        let cache = settings.public_cache();
        Self {
            settings,
            channels,
            overlay: Arc::new(RwLock::new(None)),
            cache,
            clock,
        }
    }

    /// Installs the reseller overlay hook (applied to reseller tenants only).
    pub fn set_overlay(&self, overlay: Arc<dyn PublicConfigOverlay>) {
        *self.overlay.write().unwrap_or_else(PoisonError::into_inner) = Some(overlay);
        self.cache.clear();
    }

    /// The public configuration for `tenant`.
    pub async fn get(&self, tenant: &Tenant) -> Result<Value> {
        let overlay = self
            .overlay
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let reseller = tenant.reseller_id.filter(|_| overlay.is_some());
        let cache_key = reseller.map_or_else(|| "main".to_owned(), |id| format!("reseller:{id}"));
        let mut data = match self.cache.get(&cache_key) {
            Some(v) => v,
            None => {
                let mut base = self
                    .build_base()
                    .await
                    .map_err(|e| e.or_internal(FETCH_FAILED))?;
                if let (Some(_), Some(overlay)) = (reseller, &overlay) {
                    self.add_announcement(&mut base).await?;
                    base = overlay
                        .apply(tenant, base)
                        .await
                        .map_err(|e| e.or_internal(FETCH_FAILED))?;
                }
                self.cache.put(&cache_key, base.clone());
                base
            }
        };
        if reseller.is_none() {
            self.add_announcement(&mut data).await?;
            data["tenant"] = json!({"mode": "main", "host": tenant.host});
        }
        data["server_time"] = json!(self.clock.now().timestamp_millis());
        data["app_version"] = json!(APP_VERSION);
        Ok(data)
    }

    async fn add_announcement(&self, data: &mut Value) -> Result<()> {
        let raw = self
            .settings
            .get(keys::HOME_ANNOUNCEMENT)
            .await
            .map_err(|e| e.or_internal(FETCH_FAILED))?;
        match raw.and_then(|v| active_announcement(&v, self.clock.now())) {
            Some(a) => data["announcement"] = a,
            None => {
                if let Some(m) = data.as_object_mut() {
                    m.remove("announcement");
                }
            }
        }
        Ok(())
    }

    async fn build_base(&self) -> Result<Value> {
        let s = &self.settings;
        let mut data = defaults();
        if let Some(Value::Object(site)) = s.site_config().await? {
            data.extend(site);
        }
        let theme = normalize_theme(data.get("theme"));
        data.insert("theme".into(), theme);

        let fee = s.payment_fee().await?;
        let channels: Vec<Value> = self
            .channels
            .active_channels()
            .await?
            .iter()
            .filter(|c| c.visible_to_guest_order())
            .map(|c| c.to_public(fee.customer_fee_enabled))
            .collect();
        data.insert("payment_channels".into(), json!(channels));

        let wallet = s.wallet().await?;
        if !wallet.recharge_channel_ids.is_empty() {
            data.insert(
                "wallet_recharge_channel_ids".into(),
                json!(wallet.recharge_channel_ids),
            );
        }
        if wallet.wallet_only_payment {
            data.insert("wallet_only_payment".into(), json!(true));
        }

        data.insert("captcha".into(), s.captcha().await?.public());
        data.insert("telegram_auth".into(), s.telegram_auth().await?.public());
        data.insert("google_auth".into(), s.google_auth().await?.public());
        data.insert("affiliate".into(), s.affiliate().await?.encode());
        data.insert("smtp_enabled".into(), json!(s.smtp().await?.enabled));
        let (registration, verification) = s.registration_flags().await?;
        data.insert("registration_enabled".into(), json!(registration));
        data.insert("email_verification_enabled".into(), json!(verification));
        let policy = s.email_domain_policy().await?;
        data.insert(
            "email_domain_allowlist_enabled".into(),
            json!(policy.enabled),
        );
        data.insert(
            "allowed_email_domains".into(),
            json!(policy.allowed_domains),
        );
        let nav = s.get(keys::NAV_CONFIG).await?.unwrap_or_else(default_nav);
        data.insert("nav_config".into(), nav);
        Ok(Value::Object(data))
    }
}
