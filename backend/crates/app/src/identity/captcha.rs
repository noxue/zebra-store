//! Captcha verification per scene (`none` / `image` / `turnstile`), reusable by
//! every module through [`CaptchaService::verify`].

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use zs_domain::identity::captcha::{
    self, CaptchaPayload, CaptchaSetting, ImageRenderer, PROVIDER_IMAGE, PROVIDER_TURNSTILE,
    TurnstileVerifier,
};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;

#[derive(Debug, Clone)]
struct Pending {
    answer: String,
    expires_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
}

/// Captcha service (cheap to clone).
#[derive(Clone)]
pub struct CaptchaService {
    settings: Arc<dyn SettingsStore>,
    renderer: Arc<dyn ImageRenderer>,
    turnstile: Arc<dyn TurnstileVerifier>,
    clock: Arc<dyn Clock>,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
}

impl std::fmt::Debug for CaptchaService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CaptchaService")
    }
}

impl CaptchaService {
    pub fn new(
        settings: Arc<dyn SettingsStore>,
        renderer: Arc<dyn ImageRenderer>,
        turnstile: Arc<dyn TurnstileVerifier>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            settings,
            renderer,
            turnstile,
            clock,
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// The normalised `captcha_config` setting.
    pub async fn setting(&self) -> Result<CaptchaSetting> {
        let raw = self.settings.get(setting_keys::CAPTCHA_CONFIG).await?;
        Ok(CaptchaSetting::from_value(raw))
    }

    /// Public part for `/public/config`.
    pub async fn public_setting(&self) -> Result<Value> {
        Ok(self.setting().await?.public())
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Creates an image challenge; returns `(captcha_id, image_base64)`.
    pub async fn generate_image(&self) -> Result<(String, String)> {
        let setting = self
            .setting()
            .await
            .map_err(|e| e.or_internal(captcha::keys::GENERATE_FAILED))?;
        if setting.provider != PROVIDER_IMAGE {
            return Err(Error::bad_request(captcha::keys::UNAVAILABLE));
        }
        let challenge = self
            .renderer
            .render(&setting.image)
            .map_err(|e| e.or_internal(captcha::keys::GENERATE_FAILED))?;
        let id = uuid::Uuid::new_v4().simple().to_string();
        let now = self.clock.now();
        let capacity = usize::try_from(setting.image.max_store).unwrap_or(usize::MAX);
        let mut map = self.lock();
        if map.len() >= capacity {
            map.retain(|_, p| p.expires_at > now);
        }
        while map.len() >= capacity {
            let oldest = map
                .iter()
                .min_by_key(|(_, p)| p.created_at)
                .map(|(k, _)| k.clone());
            match oldest {
                Some(k) => {
                    map.remove(&k);
                }
                None => break,
            }
        }
        map.insert(
            id.clone(),
            Pending {
                answer: challenge.answer,
                expires_at: now + Duration::seconds(setting.image.expire_seconds),
                created_at: now,
            },
        );
        Ok((id, challenge.image_base64))
    }

    /// Verifies the captcha of `scene`. Disabled scenes always pass.
    ///
    /// Errors: `error.captcha_required` / `error.captcha_invalid` (400),
    /// `error.captcha_config_invalid` / `error.captcha_verify_failed` (500).
    pub async fn verify(
        &self,
        scene: &str,
        payload: &CaptchaPayload,
        client_ip: &str,
    ) -> Result<()> {
        let setting = self
            .setting()
            .await
            .map_err(|e| e.or_internal(captcha::keys::VERIFY_FAILED))?;
        if !setting.is_scene_enabled(scene) {
            return Ok(());
        }
        match setting.provider.as_str() {
            PROVIDER_IMAGE => {
                let id = payload.captcha_id.trim();
                let code = payload.captcha_code.trim();
                if id.is_empty() || code.is_empty() {
                    return Err(captcha::required());
                }
                // Single use: the challenge is consumed whatever the outcome.
                let pending = self.lock().remove(id);
                match pending {
                    Some(p)
                        if p.expires_at > self.clock.now()
                            && p.answer.eq_ignore_ascii_case(code) =>
                    {
                        Ok(())
                    }
                    _ => Err(captcha::invalid()),
                }
            }
            PROVIDER_TURNSTILE => {
                let token = payload.turnstile_token.trim();
                if token.is_empty() {
                    return Err(captcha::required());
                }
                self.turnstile
                    .verify(&setting.turnstile, token, client_ip.trim())
                    .await
            }
            _ => Err(captcha::config_invalid()),
        }
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use serde_json::json;
    use zs_domain::identity::captcha::{ImageChallenge, ImageSetting, TurnstileSetting};
    use zs_shared::clock::SystemClock;

    use super::*;

    #[derive(Default)]
    struct MemSettings(Mutex<HashMap<String, Value>>);

    #[async_trait]
    impl SettingsStore for MemSettings {
        async fn get(&self, key: &str) -> Result<Option<Value>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &Value) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.clone());
            Ok(())
        }
    }

    struct FixedRenderer;
    impl ImageRenderer for FixedRenderer {
        fn render(&self, _: &ImageSetting) -> Result<ImageChallenge> {
            Ok(ImageChallenge {
                answer: "12345".into(),
                image_base64: "data:image/png;base64,AAAA".into(),
            })
        }
    }

    struct Turnstile(Mutex<Vec<String>>);
    #[async_trait]
    impl TurnstileVerifier for Turnstile {
        async fn verify(&self, _: &TurnstileSetting, token: &str, ip: &str) -> Result<()> {
            self.0.lock().unwrap().push(format!("{token}@{ip}"));
            if token == "good" {
                Ok(())
            } else {
                Err(captcha::invalid())
            }
        }
    }

    fn service(setting: Value) -> (CaptchaService, Arc<Turnstile>) {
        let settings = Arc::new(MemSettings::default());
        settings
            .0
            .lock()
            .unwrap()
            .insert("captcha_config".into(), setting);
        let t = Arc::new(Turnstile(Mutex::new(Vec::new())));
        (
            CaptchaService::new(
                settings,
                Arc::new(FixedRenderer),
                t.clone(),
                Arc::new(SystemClock),
            ),
            t,
        )
    }

    #[tokio::test]
    async fn skips_disabled_scene() {
        let (svc, _) = service(json!({"provider": "image", "scenes": {"login": false}}));
        svc.verify("login", &CaptchaPayload::default(), "1.1.1.1")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn image_requires_payload_and_is_single_use() {
        let (svc, _) = service(json!({"provider": "image", "scenes": {"login": true}}));
        assert_eq!(
            svc.verify("login", &CaptchaPayload::default(), "")
                .await
                .unwrap_err()
                .key(),
            captcha::keys::REQUIRED
        );
        let (id, img) = svc.generate_image().await.unwrap();
        assert!(img.starts_with("data:image/png;base64,"));
        let wrong = CaptchaPayload {
            captcha_id: id.clone(),
            captcha_code: "00000".into(),
            ..Default::default()
        };
        assert_eq!(
            svc.verify("login", &wrong, "").await.unwrap_err().key(),
            captcha::keys::INVALID
        );
        // Consumed by the failed attempt.
        let right = CaptchaPayload {
            captcha_id: id,
            captcha_code: "12345".into(),
            ..Default::default()
        };
        assert!(svc.verify("login", &right, "").await.is_err());
        let (id2, _) = svc.generate_image().await.unwrap();
        let ok = CaptchaPayload {
            captcha_id: id2,
            captcha_code: " 12345 ".into(),
            ..Default::default()
        };
        svc.verify("login", &ok, "").await.unwrap();
    }

    #[tokio::test]
    async fn turnstile_delegates_and_none_is_misconfigured() {
        let (svc, t) =
            service(json!({"provider": "turnstile", "scenes": {"gift_card_redeem": true}}));
        let payload = CaptchaPayload {
            turnstile_token: "good".into(),
            ..Default::default()
        };
        svc.verify("gift_card_redeem", &payload, " 9.9.9.9 ")
            .await
            .unwrap();
        assert_eq!(t.0.lock().unwrap().as_slice(), ["good@9.9.9.9"]);
        assert_eq!(
            svc.generate_image().await.unwrap_err().key(),
            captcha::keys::UNAVAILABLE
        );
        let (svc, _) = service(json!({"provider": "none", "scenes": {"login": true}}));
        assert_eq!(
            svc.verify("login", &payload, "").await.unwrap_err().key(),
            captcha::keys::CONFIG_INVALID
        );
    }
}
