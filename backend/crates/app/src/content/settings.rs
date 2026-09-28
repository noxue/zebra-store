//! Site settings use cases (port of `modules/settings/application`).
//!
//! Every read goes to the [`SettingsStore`] and falls back to `config.yml`
//! defaults; writes are normalized through [`schema::normalize`] or the typed
//! patch rules and trigger the declared cache invalidations.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;
use zs_domain::content::public::SmtpSender;
use zs_domain::identity::mailer::{Email, KEY_RECIPIENT_REJECTED};
use zs_domain::settings::schema::captcha::{CaptchaPatch, CaptchaSetting};
use zs_domain::settings::schema::integration::{
    AffiliateSetting, CallbackRoutes, UpstreamSyncSetting,
};
use zs_domain::settings::schema::login::{
    GoogleAuthPatch, GoogleAuthSetting, TelegramAuthPatch, TelegramAuthSetting,
};
use zs_domain::settings::schema::notification::{
    NotificationCenterPatch, NotificationCenterSetting,
};
use zs_domain::settings::schema::order_email::{
    OrderEmailTemplatePatch, OrderEmailTemplateSetting,
};
use zs_domain::settings::schema::risk::OrderRiskSetting;
use zs_domain::settings::schema::site::{
    EmailDomainPolicy, OrderSetting, PaymentFeeSetting, SiteBrand, WalletSetting,
    email_domain_policy, normalize_currency,
};
use zs_domain::settings::schema::smtp::{SmtpPatch, SmtpSetting, SmtpVerifyCode};
use zs_domain::settings::schema::storefront::{DashboardSetting, active_announcement};
use zs_domain::settings::schema::telegram_bot::{
    TelegramBotRuntimeStatus, TelegramBotSetting, ensure_builtin_menu,
};
use zs_domain::settings::schema::value::is_email_address;
use zs_domain::settings::schema::{self, Effect};
use zs_domain::settings::{SettingsStore, keys};
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;

use crate::config::Config;

const FETCH_FAILED: &str = "error.settings_fetch_failed";
const SAVE_FAILED: &str = "error.settings_save_failed";
/// Unknown or server-owned key in the generic settings endpoints (live QA I-1).
pub const KEY_INVALID: &str = "error.setting_key_invalid";
/// Lifetime of the cached custom callback routes (original: 5 minutes).
const CALLBACK_ROUTES_TTL: Duration = Duration::from_secs(300);
/// Lifetime of a cached `/public/config` payload (original: 60 s).
pub const PUBLIC_CONFIG_TTL: Duration = Duration::from_secs(60);
const SMTP_TEST_SUBJECT: &str = "SMTP 配置测试邮件";
const SMTP_TEST_BODY: &str = "这是一封来自 Dujiao-Next 的 SMTP 测试邮件，说明当前配置可正常发送。";

/// Process-local TTL cache for `/public/config` payloads, keyed by tenant.
#[derive(Debug, Default)]
pub struct PublicConfigCache {
    entries: Mutex<HashMap<String, (Instant, Value)>>,
}

impl PublicConfigCache {
    pub fn get(&self, key: &str) -> Option<Value> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(key)
            .filter(|(at, _)| at.elapsed() < PUBLIC_CONFIG_TTL)
            .map(|(_, v)| v.clone())
    }

    pub fn put(&self, key: &str, value: Value) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries.insert(key.to_owned(), (Instant::now(), value));
    }

    /// Drops every tenant's cached payload.
    pub fn clear(&self) {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

/// `config.yml` values used when a setting has never been saved.
#[derive(Debug, Clone)]
struct Defaults {
    smtp: SmtpSetting,
    telegram_auth: TelegramAuthSetting,
    google_auth: GoogleAuthSetting,
    order: OrderSetting,
    upstream_sync: UpstreamSyncSetting,
}

impl Defaults {
    fn from_config(cfg: &Config) -> Self {
        let e = &cfg.email;
        let t = &cfg.telegram_auth;
        Self {
            smtp: SmtpSetting {
                enabled: e.enabled,
                host: e.host.clone(),
                port: i64::from(e.port),
                username: e.username.clone(),
                password: e.password.clone(),
                from: e.from.clone(),
                from_name: e.from_name.clone(),
                use_tls: e.use_tls,
                use_ssl: e.use_ssl,
                order_notification_enabled: true,
                verify_code: SmtpVerifyCode {
                    expire_minutes: e.verify_code.expire_minutes,
                    send_interval_seconds: e.verify_code.send_interval_seconds,
                    max_attempts: i64::from(e.verify_code.max_attempts),
                    length: i64::try_from(e.verify_code.length).unwrap_or(6),
                },
            }
            .normalized(),
            telegram_auth: TelegramAuthSetting {
                enabled: t.enabled,
                bot_username: t.bot_username.clone(),
                bot_token: t.bot_token.clone(),
                client_secret: t.client_secret.clone(),
                oidc_redirect_uri: t.oidc_redirect_uri.clone(),
                mini_app_url: t.mini_app_url.clone(),
                login_expire_seconds: t.login_expire_seconds,
                replay_ttl_seconds: t.replay_ttl_seconds,
            }
            .normalized(),
            google_auth: GoogleAuthSetting {
                enabled: cfg.google_auth.enabled,
                client_id: cfg.google_auth.client_id.clone(),
            }
            .normalized(),
            order: OrderSetting::from_config(
                cfg.order.payment_expire_minutes,
                cfg.order.max_refund_days,
            ),
            upstream_sync: UpstreamSyncSetting::fallback(&cfg.queue.upstream_sync_interval),
        }
    }
}

/// Cached custom callback routes with the time they were loaded.
type CallbackRoutesCache = Arc<Mutex<Option<(Instant, Option<CallbackRoutes>)>>>;

/// Settings reads, typed patches and cache invalidation.
#[derive(Clone)]
pub struct SettingsService {
    store: Arc<dyn SettingsStore>,
    defaults: Arc<Defaults>,
    public_cache: Arc<PublicConfigCache>,
    callback_routes: CallbackRoutesCache,
    smtp: Arc<dyn SmtpSender>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SettingsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SettingsService")
    }
}

impl SettingsService {
    pub fn new(
        store: Arc<dyn SettingsStore>,
        cfg: &Config,
        smtp: Arc<dyn SmtpSender>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            store,
            defaults: Arc::new(Defaults::from_config(cfg)),
            public_cache: Arc::new(PublicConfigCache::default()),
            callback_routes: Arc::new(Mutex::new(None)),
            smtp,
            clock,
        }
    }

    /// Cache shared with the public config service.
    pub fn public_cache(&self) -> Arc<PublicConfigCache> {
        self.public_cache.clone()
    }

    /// Drops the cached `/public/config` payloads.
    pub fn invalidate_public_config(&self) {
        self.public_cache.clear();
    }

    /// Raw stored JSON of `key` (`None` when never saved).
    pub async fn get(&self, key: &str) -> Result<Option<Value>> {
        self.store
            .get(key)
            .await
            .map_err(|e| e.or_internal(FETCH_FAILED))
    }

    async fn read(&self, key: &str) -> Result<Option<Value>> {
        self.get(key).await
    }

    /// Stores `value` and applies the effects declared for `key`, so every
    /// typed patch invalidates the caches the key feeds (live QA I-4).
    async fn write(&self, key: &str, value: &Value) -> Result<()> {
        self.store
            .set(key, value)
            .await
            .map_err(|e| e.or_internal(SAVE_FAILED))?;
        self.apply_effects(key);
        Ok(())
    }

    fn apply_effects(&self, key: &str) {
        for effect in schema::effects(key) {
            match effect {
                Effect::InvalidatePublicConfig => self.invalidate_public_config(),
                Effect::InvalidateCallbackRoutes => self.invalidate_callback_routes(),
            }
        }
    }

    /// Generic write of a plain (secret-free) key: normalizes and applies effects.
    pub async fn update(&self, key: &str, value: &Value) -> Result<Value> {
        let normalized = schema::normalize(key, value);
        self.write(key, &normalized).await?;
        Ok(normalized)
    }

    /// Generic read (`GET /admin/settings?key=`): only known keys, and keys
    /// holding secrets are returned in their masked admin shape (live QA I-1;
    /// the original returned the raw JSON, SMTP password included).
    pub async fn admin_view(&self, key: &str) -> Result<Value> {
        if !schema::is_known(key) {
            return Err(Error::bad_request(KEY_INVALID));
        }
        Ok(match key {
            keys::SMTP_CONFIG => self.smtp().await?.masked(),
            keys::CAPTCHA_CONFIG => self.captcha().await?.masked(),
            keys::TELEGRAM_AUTH_CONFIG => self.telegram_auth().await?.masked(),
            keys::NOTIFICATION_CENTER_CONFIG => self.notification_center().await?.masked(),
            _ => self
                .get(key)
                .await?
                .unwrap_or_else(|| Value::Object(Default::default())),
        })
    }

    /// Generic write (`PUT /admin/settings`): unknown and server-owned keys are
    /// refused; keys with a dedicated endpoint go through its validated patch
    /// (an empty secret keeps the stored one) and answer in the masked shape.
    pub async fn admin_update(&self, key: &str, value: &Value) -> Result<Value> {
        if !schema::is_writable(key) {
            return Err(Error::bad_request(KEY_INVALID));
        }
        fn patch<T: serde::de::DeserializeOwned>(value: &Value) -> Result<T> {
            serde_json::from_value(value.clone()).map_err(|_| Error::invalid())
        }
        Ok(match key {
            keys::SMTP_CONFIG => self.patch_smtp(patch(value)?).await?.masked(),
            keys::CAPTCHA_CONFIG => self.patch_captcha(patch(value)?).await?.masked(),
            keys::TELEGRAM_AUTH_CONFIG => self.patch_telegram_auth(patch(value)?).await?.masked(),
            keys::NOTIFICATION_CENTER_CONFIG => self
                .patch_notification_center(patch(value)?)
                .await?
                .masked(),
            keys::GOOGLE_AUTH_CONFIG => self.patch_google_auth(patch(value)?).await?.encode(),
            keys::ORDER_EMAIL_TEMPLATE_CONFIG => self
                .patch_order_email_template(patch(value)?)
                .await?
                .encode(),
            keys::AFFILIATE_CONFIG => self
                .update_affiliate(AffiliateSetting::decode(Some(value)))
                .await?
                .encode(),
            keys::TELEGRAM_BOT_CONFIG => {
                let cfg = TelegramBotSetting::decode(Some(value), TelegramBotSetting::defaults());
                self.update_telegram_bot(cfg).await?.encode()
            }
            _ => {
                schema::validate(key, value)?;
                self.update(key, value).await?
            }
        })
    }

    /// Stored keys that are not [`schema::is_known`] (for `admin prune-settings`).
    pub fn unknown_keys(stored: impl IntoIterator<Item = String>) -> Vec<String> {
        stored
            .into_iter()
            .filter(|k| !schema::is_known(k))
            .collect()
    }

    // --- SMTP ------------------------------------------------------------

    pub async fn smtp(&self) -> Result<SmtpSetting> {
        let raw = self.read(keys::SMTP_CONFIG).await?;
        Ok(match raw {
            Some(v) => SmtpSetting::decode(Some(&v), self.defaults.smtp.clone()).normalized(),
            None => self.defaults.smtp.clone(),
        })
    }

    pub async fn patch_smtp(&self, patch: SmtpPatch) -> Result<SmtpSetting> {
        let next = self.smtp().await?.apply_patch(patch)?;
        self.write(keys::SMTP_CONFIG, &next.encode()).await?;
        Ok(next)
    }

    /// Sends a test e-mail with the stored SMTP settings (forced enabled).
    pub async fn test_smtp(&self, to: &str, subject: &str, body: &str) -> Result<()> {
        let to = to.trim();
        if to.is_empty() {
            return Err(Error::bad_request("error.email_invalid"));
        }
        let setting = self.smtp().await?;
        if setting.host.is_empty() || setting.port == 0 || setting.from.is_empty() {
            return Err(Error::bad_request("error.email_service_not_configured"));
        }
        if !is_email_address(to) {
            return Err(Error::bad_request("error.email_invalid"));
        }
        let subject = match subject.trim() {
            "" => SMTP_TEST_SUBJECT,
            s => s,
        };
        let body = match body.trim() {
            "" => SMTP_TEST_BODY,
            b => b,
        };
        let email = Email {
            to: to.to_owned(),
            subject: subject.to_owned(),
            body: body.to_owned(),
            ..Email::default()
        };
        let enabled = SmtpSetting {
            enabled: true,
            ..setting
        };
        match self.smtp.send(&enabled, &email).await {
            Ok(()) => Ok(()),
            Err(e) if e.key() == KEY_RECIPIENT_REJECTED => Err(e),
            Err(e) => Err(Error::internal(e).or_internal("error.send_verify_code_failed")),
        }
    }

    // --- captcha / login providers ----------------------------------------

    pub async fn captcha(&self) -> Result<CaptchaSetting> {
        let raw = self.read(keys::CAPTCHA_CONFIG).await?;
        Ok(CaptchaSetting::decode(raw.as_ref(), CaptchaSetting::default()).normalized())
    }

    pub async fn patch_captcha(&self, patch: CaptchaPatch) -> Result<CaptchaSetting> {
        let next = self.captcha().await?.apply_patch(patch)?;
        self.write(keys::CAPTCHA_CONFIG, &next.encode()).await?;
        self.invalidate_public_config();
        Ok(next)
    }

    pub async fn telegram_auth(&self) -> Result<TelegramAuthSetting> {
        let raw = self.read(keys::TELEGRAM_AUTH_CONFIG).await?;
        Ok(match raw {
            Some(v) => TelegramAuthSetting::decode(Some(&v), self.defaults.telegram_auth.clone())
                .normalized(),
            None => self.defaults.telegram_auth.clone(),
        })
    }

    pub async fn patch_telegram_auth(
        &self,
        patch: TelegramAuthPatch,
    ) -> Result<TelegramAuthSetting> {
        let next = self.telegram_auth().await?.apply_patch(patch).normalized();
        next.validate()?;
        self.write(keys::TELEGRAM_AUTH_CONFIG, &next.encode())
            .await?;
        self.invalidate_public_config();
        Ok(next)
    }

    pub async fn google_auth(&self) -> Result<GoogleAuthSetting> {
        let raw = self.read(keys::GOOGLE_AUTH_CONFIG).await?;
        Ok(match raw {
            Some(v) => GoogleAuthSetting::decode(Some(&v), self.defaults.google_auth.clone()),
            None => self.defaults.google_auth.clone(),
        })
    }

    pub async fn patch_google_auth(&self, patch: GoogleAuthPatch) -> Result<GoogleAuthSetting> {
        let next = self.google_auth().await?.apply_patch(patch).normalized();
        next.validate()?;
        self.write(keys::GOOGLE_AUTH_CONFIG, &next.encode()).await?;
        self.invalidate_public_config();
        Ok(next)
    }

    // --- affiliate / templates / notifications / bot ----------------------

    pub async fn affiliate(&self) -> Result<AffiliateSetting> {
        let raw = self.read(keys::AFFILIATE_CONFIG).await?;
        Ok(AffiliateSetting::decode(raw.as_ref()))
    }

    pub async fn update_affiliate(&self, setting: AffiliateSetting) -> Result<AffiliateSetting> {
        let next = setting.normalized();
        next.validate()?;
        self.write(keys::AFFILIATE_CONFIG, &next.encode()).await?;
        Ok(next)
    }

    pub async fn order_email_template(&self) -> Result<OrderEmailTemplateSetting> {
        let raw = self.read(keys::ORDER_EMAIL_TEMPLATE_CONFIG).await?;
        Ok(
            OrderEmailTemplateSetting::decode(raw.as_ref(), OrderEmailTemplateSetting::defaults())
                .normalized(),
        )
    }

    pub async fn patch_order_email_template(
        &self,
        patch: OrderEmailTemplatePatch,
    ) -> Result<OrderEmailTemplateSetting> {
        let next = self.order_email_template().await?.apply_patch(patch)?;
        self.write(keys::ORDER_EMAIL_TEMPLATE_CONFIG, &next.encode())
            .await?;
        Ok(next)
    }

    pub async fn reset_order_email_template(&self) -> Result<OrderEmailTemplateSetting> {
        let d = OrderEmailTemplateSetting::defaults();
        self.write(keys::ORDER_EMAIL_TEMPLATE_CONFIG, &d.encode())
            .await?;
        Ok(d)
    }

    pub async fn notification_center(&self) -> Result<NotificationCenterSetting> {
        let raw = self.read(keys::NOTIFICATION_CENTER_CONFIG).await?;
        Ok(
            NotificationCenterSetting::decode(raw.as_ref(), NotificationCenterSetting::default())
                .normalized(),
        )
    }

    pub async fn patch_notification_center(
        &self,
        patch: NotificationCenterPatch,
    ) -> Result<NotificationCenterSetting> {
        let next = self.notification_center().await?.apply_patch(patch)?;
        self.write(keys::NOTIFICATION_CENTER_CONFIG, &next.encode())
            .await?;
        Ok(next)
    }

    pub async fn telegram_bot(&self) -> Result<TelegramBotSetting> {
        let raw = self.read(keys::TELEGRAM_BOT_CONFIG).await?;
        let mut cfg = TelegramBotSetting::decode(raw.as_ref(), TelegramBotSetting::defaults());
        cfg.menu.items = ensure_builtin_menu(std::mem::take(&mut cfg.menu.items));
        Ok(cfg)
    }

    /// Whole-object update; bumps `config_version` and mirrors it into the runtime status.
    pub async fn update_telegram_bot(&self, cfg: TelegramBotSetting) -> Result<TelegramBotSetting> {
        let current = self.telegram_bot().await?;
        let next = TelegramBotSetting {
            config_version: current.config_version + 1,
            ..cfg
        }
        .normalized();
        self.write(keys::TELEGRAM_BOT_CONFIG, &next.encode())
            .await?;
        let mut status = self.telegram_bot_runtime_status().await.unwrap_or_default();
        status.config_version = next.config_version;
        if let Err(error) = self
            .write(keys::TELEGRAM_BOT_RUNTIME_STATUS, &status.encode())
            .await
        {
            tracing::warn!(%error, "telegram bot runtime status update failed");
        }
        Ok(next)
    }

    pub async fn telegram_bot_runtime_status(&self) -> Result<TelegramBotRuntimeStatus> {
        let raw = self.read(keys::TELEGRAM_BOT_RUNTIME_STATUS).await?;
        Ok(TelegramBotRuntimeStatus::decode(raw.as_ref()))
    }

    // --- read helpers for other modules -----------------------------------

    /// Order timing with `config.yml` fallback.
    pub async fn order(&self) -> Result<OrderSetting> {
        let raw = self.read(keys::ORDER_CONFIG).await?;
        Ok(OrderSetting::decode(raw.as_ref(), self.defaults.order))
    }

    pub async fn dashboard(&self) -> Result<DashboardSetting> {
        let raw = self.read(keys::DASHBOARD_CONFIG).await?;
        Ok(DashboardSetting::decode(
            raw.as_ref(),
            DashboardSetting::default(),
        ))
    }

    pub async fn upstream_sync(&self) -> Result<UpstreamSyncSetting> {
        let raw = self.read(keys::UPSTREAM_SYNC_CONFIG).await?;
        Ok(UpstreamSyncSetting::decode(
            raw.as_ref(),
            self.defaults.upstream_sync,
        ))
    }

    pub async fn order_risk(&self) -> Result<OrderRiskSetting> {
        let raw = self.read(keys::ORDER_RISK_CONTROL_CONFIG).await?;
        Ok(OrderRiskSetting::decode(
            raw.as_ref(),
            OrderRiskSetting::default(),
        ))
    }

    pub async fn payment_fee(&self) -> Result<PaymentFeeSetting> {
        let raw = self.read(keys::PAYMENT_CONFIG).await?;
        Ok(PaymentFeeSetting::decode(raw.as_ref()))
    }

    pub async fn wallet(&self) -> Result<WalletSetting> {
        let raw = self.read(keys::WALLET_CONFIG).await?;
        Ok(WalletSetting::decode(raw.as_ref()))
    }

    /// Stored `site_config` (raw).
    pub async fn site_config(&self) -> Result<Option<Value>> {
        self.read(keys::SITE_CONFIG).await
    }

    pub async fn site_brand(&self) -> Result<SiteBrand> {
        let raw = self.site_config().await?;
        Ok(SiteBrand::decode(raw.as_ref()))
    }

    /// Site currency (defaults to CNY).
    pub async fn currency(&self) -> Result<String> {
        let raw = self.site_config().await?;
        Ok(normalize_currency(
            raw.as_ref().and_then(|v| v.get("currency")),
        ))
    }

    /// `(registration_enabled, email_verification_enabled)`, both default `true`.
    pub async fn registration_flags(&self) -> Result<(bool, bool)> {
        let raw = self.read(keys::REGISTRATION_CONFIG).await?;
        let flag = |k: &str| {
            raw.as_ref()
                .and_then(|v| v.get(k))
                .map(|v| schema::value::parse_bool(Some(v)))
                .unwrap_or(true)
        };
        Ok((
            flag("registration_enabled"),
            flag("email_verification_enabled"),
        ))
    }

    pub async fn email_domain_policy(&self) -> Result<EmailDomainPolicy> {
        let raw = self.read(keys::REGISTRATION_CONFIG).await?;
        Ok(email_domain_policy(raw.as_ref()))
    }

    /// The home announcement to show now, if any.
    pub async fn active_announcement(&self) -> Result<Option<Value>> {
        let raw = self.read(keys::HOME_ANNOUNCEMENT).await?;
        Ok(raw.and_then(|v| active_announcement(&v, self.clock.now())))
    }

    // --- callback routes ---------------------------------------------------

    /// Custom callback routes (cached 5 minutes); `None` when none is configured.
    pub async fn callback_routes(&self) -> Result<Option<CallbackRoutes>> {
        {
            let cached = self
                .callback_routes
                .lock()
                .unwrap_or_else(PoisonError::into_inner);
            if let Some((at, routes)) = cached.as_ref()
                && at.elapsed() < CALLBACK_ROUTES_TTL
            {
                return Ok(routes.clone());
            }
        }
        let raw = self.read(keys::CALLBACK_ROUTES_CONFIG).await?;
        let routes = raw
            .map(|v| CallbackRoutes::decode(Some(&v)))
            .filter(CallbackRoutes::has_custom_routes);
        *self
            .callback_routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = Some((Instant::now(), routes.clone()));
        Ok(routes)
    }

    pub fn invalidate_callback_routes(&self) {
        *self
            .callback_routes
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use zs_shared::clock::SystemClock;

    #[derive(Default)]
    pub(crate) struct MemStore(Mutex<HashMap<String, Value>>);

    #[async_trait]
    impl SettingsStore for MemStore {
        async fn get(&self, key: &str) -> Result<Option<Value>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &Value) -> Result<()> {
            self.0.lock().unwrap().insert(key.to_owned(), value.clone());
            Ok(())
        }
    }

    #[derive(Default)]
    struct RecordingSender {
        sent: Mutex<Vec<(SmtpSetting, Email)>>,
        reject: bool,
    }

    #[async_trait]
    impl SmtpSender for RecordingSender {
        async fn send(&self, setting: &SmtpSetting, email: &Email) -> Result<()> {
            if self.reject {
                return Err(Error::bad_request(KEY_RECIPIENT_REJECTED));
            }
            self.sent
                .lock()
                .unwrap()
                .push((setting.clone(), email.clone()));
            Ok(())
        }
    }

    fn service_with(sender: Arc<RecordingSender>) -> (SettingsService, Arc<MemStore>) {
        let store = Arc::new(MemStore::default());
        let svc = SettingsService::new(
            store.clone(),
            &Config::default(),
            sender,
            Arc::new(SystemClock),
        );
        (svc, store)
    }

    fn service() -> (SettingsService, Arc<MemStore>) {
        service_with(Arc::new(RecordingSender::default()))
    }

    #[tokio::test]
    async fn update_normalizes_and_invalidates_public_cache() {
        let (svc, store) = service();
        svc.public_cache().put("main", json!({"cached": true}));
        let out = svc
            .update(keys::SITE_CONFIG, &json!({"currency": "usd"}))
            .await
            .unwrap();
        assert_eq!(out["currency"], "USD");
        assert!(svc.public_cache().get("main").is_none());
        assert_eq!(
            store.0.lock().unwrap()[keys::SITE_CONFIG]["currency"],
            "USD"
        );
        // unrelated key keeps the cache
        svc.public_cache().put("main", json!({}));
        svc.update(keys::ORDER_CONFIG, &json!({})).await.unwrap();
        assert!(svc.public_cache().get("main").is_some());
    }

    // SET-01 ⑥: saving the routes drops the cache so the next read sees the new paths.
    #[tokio::test]
    async fn set_01_callback_routes_cache_invalidated_on_save() {
        let (svc, _) = service();
        assert_eq!(svc.callback_routes().await.unwrap(), None);
        svc.update(
            keys::CALLBACK_ROUTES_CONFIG,
            &json!({"stripe_webhook": "/api/hooks/s"}),
        )
        .await
        .unwrap();
        let routes = svc.callback_routes().await.unwrap().unwrap();
        assert_eq!(routes.stripe_webhook, "/api/hooks/s");
        assert_eq!(routes.paypal_webhook, "");
    }

    #[tokio::test]
    async fn smtp_defaults_come_from_config_and_patch_keeps_password() {
        let (svc, _) = service();
        let d = svc.smtp().await.unwrap();
        assert_eq!(d.port, i64::from(Config::default().email.port));
        assert!(d.order_notification_enabled);
        svc.patch_smtp(SmtpPatch {
            password: Some("pw".into()),
            ..SmtpPatch::default()
        })
        .await
        .unwrap();
        let next = svc
            .patch_smtp(SmtpPatch {
                password: Some(String::new()),
                ..SmtpPatch::default()
            })
            .await
            .unwrap();
        assert_eq!(next.password, "pw");
    }

    #[tokio::test]
    async fn smtp_test_validates_and_forces_enabled() {
        let sender = Arc::new(RecordingSender::default());
        let (svc, _) = service_with(sender.clone());
        assert_eq!(
            svc.test_smtp("a@b.com", "", "").await.unwrap_err().key(),
            "error.email_service_not_configured"
        );
        svc.patch_smtp(SmtpPatch {
            host: Some("smtp.x".into()),
            from: Some("shop@x.com".into()),
            ..SmtpPatch::default()
        })
        .await
        .unwrap();
        assert_eq!(
            svc.test_smtp("bad", "", "").await.unwrap_err().key(),
            "error.email_invalid"
        );
        svc.test_smtp("a@b.com", "", "").await.unwrap();
        let sent = sender.sent.lock().unwrap();
        assert!(sent[0].0.enabled);
        assert_eq!(sent[0].1.subject, SMTP_TEST_SUBJECT);
    }

    #[tokio::test]
    async fn smtp_test_maps_recipient_rejection() {
        let (svc, _) = service_with(Arc::new(RecordingSender {
            reject: true,
            ..RecordingSender::default()
        }));
        svc.patch_smtp(SmtpPatch {
            host: Some("smtp.x".into()),
            from: Some("shop@x.com".into()),
            ..SmtpPatch::default()
        })
        .await
        .unwrap();
        assert_eq!(
            svc.test_smtp("a@b.com", "s", "b").await.unwrap_err().key(),
            KEY_RECIPIENT_REJECTED
        );
    }

    #[tokio::test]
    async fn telegram_bot_update_bumps_version_and_status() {
        let (svc, _) = service();
        let cfg = svc.telegram_bot().await.unwrap();
        assert_eq!(cfg.config_version, 0);
        let next = svc.update_telegram_bot(cfg).await.unwrap();
        assert_eq!(next.config_version, 1);
        let next = svc
            .update_telegram_bot(TelegramBotSetting::default())
            .await
            .unwrap();
        assert_eq!(next.config_version, 2);
        assert_eq!(next.menu.items.len(), 7);
        assert_eq!(
            svc.telegram_bot_runtime_status()
                .await
                .unwrap()
                .config_version,
            2
        );
    }

    #[tokio::test]
    async fn google_patch_validates_and_invalidates() {
        let (svc, _) = service();
        svc.public_cache().put("main", json!({}));
        let err = svc
            .patch_google_auth(GoogleAuthPatch {
                enabled: Some(true),
                client_id: None,
            })
            .await
            .unwrap_err();
        assert_eq!(err.key(), "google auth config invalid: Client ID 不能为空");
        assert!(svc.public_cache().get("main").is_some());
        svc.patch_google_auth(GoogleAuthPatch {
            enabled: Some(true),
            client_id: Some("cid".into()),
        })
        .await
        .unwrap();
        assert!(svc.public_cache().get("main").is_none());
    }
}
