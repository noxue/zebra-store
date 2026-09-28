//! Captcha settings (`captcha_config`), scenes and verification ports.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{Error, ErrorKind, Result};

/// Providers (original `CaptchaProvider*`).
pub const PROVIDER_NONE: &str = "none";
pub const PROVIDER_IMAGE: &str = "image";
pub const PROVIDER_TURNSTILE: &str = "turnstile";

/// Scenes that can require a captcha (original `CaptchaScene*`).
pub mod scenes {
    pub const LOGIN: &str = "login";
    pub const REGISTER_SEND_CODE: &str = "register_send_code";
    pub const RESET_SEND_CODE: &str = "reset_send_code";
    pub const GUEST_CREATE_ORDER: &str = "guest_create_order";
    pub const GIFT_CARD_REDEEM: &str = "gift_card_redeem";
}

/// i18n keys of captcha failures.
pub mod keys {
    pub const REQUIRED: &str = "error.captcha_required";
    pub const INVALID: &str = "error.captcha_invalid";
    pub const CONFIG_INVALID: &str = "error.captcha_config_invalid";
    pub const VERIFY_FAILED: &str = "error.captcha_verify_failed";
    pub const UNAVAILABLE: &str = "error.captcha_unavailable";
    pub const GENERATE_FAILED: &str = "error.captcha_generate_failed";
}

/// Missing captcha answer (400).
pub fn required() -> Error {
    Error::bad_request(keys::REQUIRED)
}

/// Wrong or expired captcha answer (400).
pub fn invalid() -> Error {
    Error::bad_request(keys::INVALID)
}

/// Captcha configuration unusable (500).
pub fn config_invalid() -> Error {
    Error::new(ErrorKind::Internal, keys::CONFIG_INVALID)
}

/// Provider could not be reached (500).
pub fn verify_failed() -> Error {
    Error::new(ErrorKind::Internal, keys::VERIFY_FAILED)
}

/// Captcha answer sent with protected requests (`captcha_payload`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CaptchaPayload {
    pub captcha_id: String,
    pub captcha_code: String,
    pub turnstile_token: String,
}

/// Per-scene switches.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SceneSetting {
    pub login: bool,
    pub register_send_code: bool,
    pub reset_send_code: bool,
    pub guest_create_order: bool,
    pub gift_card_redeem: bool,
}

/// Image captcha parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageSetting {
    pub length: i64,
    pub width: i64,
    pub height: i64,
    pub noise_count: i64,
    pub show_line: i64,
    pub expire_seconds: i64,
    pub max_store: i64,
}

/// Cloudflare Turnstile parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TurnstileSetting {
    pub site_key: String,
    pub secret_key: String,
    pub verify_url: String,
    pub timeout_ms: i64,
}

/// Default Turnstile siteverify endpoint.
pub const TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

/// The `captcha_config` setting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptchaSetting {
    pub provider: String,
    pub scenes: SceneSetting,
    pub image: ImageSetting,
    pub turnstile: TurnstileSetting,
}

impl CaptchaSetting {
    /// Parses a stored value leniently (unknown/malformed → defaults) and normalises it.
    pub fn from_value(value: Option<Value>) -> Self {
        value
            .and_then(|v| serde_json::from_value::<Self>(v).ok())
            .unwrap_or_default()
            .normalized()
    }

    /// Clamps values like the original `NormalizeCaptchaSetting`.
    pub fn normalized(mut self) -> Self {
        let provider = self.provider.trim().to_lowercase();
        self.provider = match provider.as_str() {
            PROVIDER_IMAGE | PROVIDER_TURNSTILE | PROVIDER_NONE => provider,
            _ => PROVIDER_NONE.into(),
        };
        let img = &mut self.image;
        if !(4..=8).contains(&img.length) {
            img.length = 5;
        }
        if img.width < 100 {
            img.width = 240;
        }
        if img.height < 40 {
            img.height = 80;
        }
        if img.noise_count < 0 {
            img.noise_count = 2;
        }
        if img.show_line < 0 {
            img.show_line = 2;
        }
        if !(30..=3600).contains(&img.expire_seconds) {
            img.expire_seconds = 300;
        }
        if img.max_store < 100 {
            img.max_store = 10240;
        }
        let t = &mut self.turnstile;
        t.site_key = t.site_key.trim().to_owned();
        t.secret_key = t.secret_key.trim().to_owned();
        t.verify_url = t.verify_url.trim().to_owned();
        if t.verify_url.is_empty() {
            t.verify_url = TURNSTILE_VERIFY_URL.into();
        }
        if t.timeout_ms <= 0 {
            t.timeout_ms = 2000;
        }
        self
    }

    pub fn is_scene_enabled(&self, scene: &str) -> bool {
        match scene.trim().to_lowercase().as_str() {
            scenes::LOGIN => self.scenes.login,
            scenes::REGISTER_SEND_CODE => self.scenes.register_send_code,
            scenes::RESET_SEND_CODE => self.scenes.reset_send_code,
            scenes::GUEST_CREATE_ORDER => self.scenes.guest_create_order,
            scenes::GIFT_CARD_REDEEM => self.scenes.gift_card_redeem,
            _ => false,
        }
    }

    /// Public part exposed to the storefront (original `PublicCaptchaSetting`).
    pub fn public(&self) -> Value {
        let mut out = json!({ "provider": self.provider, "scenes": self.scenes });
        if self.provider == PROVIDER_TURNSTILE {
            out["turnstile"] = json!({ "site_key": self.turnstile.site_key });
        }
        out
    }
}

/// A rendered image challenge.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageChallenge {
    /// The expected answer.
    pub answer: String,
    /// `data:image/png;base64,…`.
    pub image_base64: String,
}

/// Renders captcha images.
pub trait ImageRenderer: Send + Sync {
    fn render(&self, setting: &ImageSetting) -> Result<ImageChallenge>;
}

/// Verifies Cloudflare Turnstile tokens: `Ok(())` when accepted,
/// [`invalid`] when rejected, [`verify_failed`] / [`config_invalid`] otherwise.
#[async_trait]
pub trait TurnstileVerifier: Send + Sync {
    async fn verify(&self, setting: &TurnstileSetting, token: &str, client_ip: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_and_checks_scenes() {
        let s = CaptchaSetting::from_value(Some(json!({
            "provider": " IMAGE ",
            "scenes": {"login": true},
            "image": {"length": 20}
        })));
        assert_eq!(s.provider, PROVIDER_IMAGE);
        assert!(s.is_scene_enabled("login"));
        assert!(!s.is_scene_enabled("register_send_code"));
        assert!(!s.is_scene_enabled("unknown"));
        assert_eq!(s.image.length, 5);
        assert_eq!(s.image.expire_seconds, 300);
        assert_eq!(s.turnstile.verify_url, TURNSTILE_VERIFY_URL);
        assert_eq!(CaptchaSetting::from_value(None).provider, PROVIDER_NONE);
        assert_eq!(
            CaptchaSetting::from_value(Some(json!("garbage"))).provider,
            PROVIDER_NONE
        );
    }

    #[test]
    fn public_view_hides_secrets() {
        let s = CaptchaSetting {
            provider: PROVIDER_TURNSTILE.into(),
            turnstile: TurnstileSetting {
                site_key: "site".into(),
                secret_key: "secret".into(),
                ..Default::default()
            },
            ..Default::default()
        }
        .normalized();
        let p = s.public();
        assert_eq!(p["turnstile"], json!({"site_key": "site"}));
        assert!(!p.to_string().contains("secret"));
    }
}
