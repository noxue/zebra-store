//! Key/value settings port (`settings` table: key → JSON).
//!
//! Every module may read its own settings keys through this port; the rich
//! validation/normalisation lives in the settings application service.

pub mod schema;

use async_trait::async_trait;
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Result;

/// Setting keys (identical to the original `constants`).
pub mod keys {
    pub const SITE_CONFIG: &str = "site_config";
    pub const ORDER_CONFIG: &str = "order_config";
    pub const SMTP_CONFIG: &str = "smtp_config";
    pub const CAPTCHA_CONFIG: &str = "captcha_config";
    pub const TELEGRAM_AUTH_CONFIG: &str = "telegram_auth_config";
    pub const GOOGLE_AUTH_CONFIG: &str = "google_auth_config";
    pub const DASHBOARD_CONFIG: &str = "dashboard_config";
    pub const NOTIFICATION_CENTER_CONFIG: &str = "notification_center_config";
    pub const AFFILIATE_CONFIG: &str = "affiliate_config";
    pub const TELEGRAM_BOT_CONFIG: &str = "telegram_bot_config";
    pub const TELEGRAM_BOT_RUNTIME_STATUS: &str = "telegram_bot_runtime_status";
    pub const ORDER_EMAIL_TEMPLATE_CONFIG: &str = "order_email_template_config";
    pub const NAV_CONFIG: &str = "nav_config";
    pub const WALLET_CONFIG: &str = "wallet_config";
    pub const PAYMENT_CONFIG: &str = "payment_config";
    pub const REGISTRATION_CONFIG: &str = "registration_config";
    pub const ORDER_RISK_CONTROL_CONFIG: &str = "order_risk_control_config";
    pub const UPSTREAM_SYNC_CONFIG: &str = "upstream_sync_config";
    pub const CALLBACK_ROUTES_CONFIG: &str = "callback_routes_config";
    pub const HOME_ANNOUNCEMENT: &str = "home_announcement";
    pub const COMPLIANCE_ACK: &str = "compliance_acknowledgement";
}

/// Raw JSON settings storage.
#[async_trait]
pub trait SettingsStore: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>>;
    async fn set(&self, key: &str, value: &serde_json::Value) -> Result<()>;
}

/// Typed helpers over [`SettingsStore`].
#[async_trait]
pub trait SettingsExt: SettingsStore {
    /// Reads `key` into `T`, falling back to `T::default()` when absent or malformed.
    async fn typed<T: DeserializeOwned + Default + Send>(&self, key: &str) -> Result<T> {
        Ok(self
            .get(key)
            .await?
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default())
    }

    async fn put<T: Serialize + Sync>(&self, key: &str, value: &T) -> Result<()> {
        self.set(key, &serde_json::to_value(value)?).await
    }
}

impl<S: SettingsStore + ?Sized> SettingsExt for S {}
