//! [`LoginProviders`] backed by `telegram_auth_config` / `google_auth_config`
//! (falling back to `config.yml` like the settings service).

use std::sync::Arc;

use async_trait::async_trait;
use zs_domain::Result;
use zs_domain::dashboard::users::{LoginProviders, UsableProviders};
use zs_domain::settings::SettingsStore;
use zs_domain::settings::keys::{GOOGLE_AUTH_CONFIG, TELEGRAM_AUTH_CONFIG};
use zs_domain::settings::schema::login::{
    GoogleAuthSetting, TelegramAuthSetting, TelegramLoginMode,
};

/// Settings-backed [`LoginProviders`].
#[derive(Clone)]
pub struct SettingsLoginProviders {
    settings: Arc<dyn SettingsStore>,
    telegram_default: TelegramAuthSetting,
    google_default: GoogleAuthSetting,
}

impl std::fmt::Debug for SettingsLoginProviders {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettingsLoginProviders")
    }
}

impl SettingsLoginProviders {
    pub fn new(
        settings: Arc<dyn SettingsStore>,
        telegram_default: TelegramAuthSetting,
        google_default: GoogleAuthSetting,
    ) -> Self {
        Self {
            settings,
            telegram_default: telegram_default.normalized(),
            google_default: google_default.normalized(),
        }
    }
}

#[async_trait]
impl LoginProviders for SettingsLoginProviders {
    async fn usable(&self) -> Result<UsableProviders> {
        let google = match self.settings.get(GOOGLE_AUTH_CONFIG).await? {
            Some(v) => GoogleAuthSetting::decode(Some(&v), self.google_default.clone()),
            None => self.google_default.clone(),
        };
        let telegram = match self.settings.get(TELEGRAM_AUTH_CONFIG).await? {
            Some(v) => {
                TelegramAuthSetting::decode(Some(&v), self.telegram_default.clone()).normalized()
            }
            None => self.telegram_default.clone(),
        };
        Ok(UsableProviders {
            google: google.enabled && !google.client_id.trim().is_empty(),
            telegram: telegram.enabled
                && !telegram.bot_username.trim().is_empty()
                && matches!(
                    telegram.login_mode(),
                    TelegramLoginMode::Widget | TelegramLoginMode::Oidc
                ),
        })
    }
}
