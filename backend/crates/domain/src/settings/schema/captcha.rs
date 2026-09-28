//! `captcha_config` (port of `schema/security/captcha.go`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::value::{as_obj, read_bool, read_int, read_string};
use crate::Error;

const INVALID: &str = "captcha config invalid";
pub const PROVIDER_NONE: &str = "none";
pub const PROVIDER_IMAGE: &str = "image";
pub const PROVIDER_TURNSTILE: &str = "turnstile";
const TURNSTILE_VERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

/// Scenes protected by a captcha.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CaptchaScenes {
    pub login: bool,
    pub register_send_code: bool,
    pub reset_send_code: bool,
    pub guest_create_order: bool,
    pub gift_card_redeem: bool,
}

impl CaptchaScenes {
    fn any(self) -> bool {
        self.login
            || self.register_send_code
            || self.reset_send_code
            || self.guest_create_order
            || self.gift_card_redeem
    }
}

/// Image captcha parameters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CaptchaImage {
    pub length: i64,
    pub width: i64,
    pub height: i64,
    pub noise_count: i64,
    pub show_line: i64,
    pub expire_seconds: i64,
    pub max_store: i64,
}

/// Cloudflare Turnstile parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CaptchaTurnstile {
    pub site_key: String,
    pub secret_key: String,
    pub verify_url: String,
    pub timeout_ms: i64,
}

/// Captcha settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptchaSetting {
    pub provider: String,
    pub scenes: CaptchaScenes,
    pub image: CaptchaImage,
    pub turnstile: CaptchaTurnstile,
}

impl Default for CaptchaSetting {
    /// The original `config.yml` defaults (provider `none`, every scene off).
    fn default() -> Self {
        Self {
            provider: PROVIDER_NONE.into(),
            scenes: CaptchaScenes::default(),
            image: CaptchaImage {
                length: 5,
                width: 240,
                height: 80,
                noise_count: 2,
                show_line: 2,
                expire_seconds: 300,
                max_store: 10240,
            },
            turnstile: CaptchaTurnstile {
                verify_url: TURNSTILE_VERIFY_URL.into(),
                timeout_ms: 2000,
                ..CaptchaTurnstile::default()
            },
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CaptchaScenesPatch {
    pub login: Option<bool>,
    pub register_send_code: Option<bool>,
    pub reset_send_code: Option<bool>,
    pub guest_create_order: Option<bool>,
    pub gift_card_redeem: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CaptchaImagePatch {
    pub length: Option<i64>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub noise_count: Option<i64>,
    pub show_line: Option<i64>,
    pub expire_seconds: Option<i64>,
    pub max_store: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct CaptchaTurnstilePatch {
    pub site_key: Option<String>,
    pub secret_key: Option<String>,
    pub verify_url: Option<String>,
    pub timeout_ms: Option<i64>,
}

/// Partial update of [`CaptchaSetting`]; an empty secret keeps the stored one.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CaptchaPatch {
    pub provider: Option<String>,
    pub scenes: Option<CaptchaScenesPatch>,
    pub image: Option<CaptchaImagePatch>,
    pub turnstile: Option<CaptchaTurnstilePatch>,
}

fn invalid(detail: &str) -> Error {
    Error::bad_request(format!("{INVALID}: {detail}"))
}

fn set<T>(target: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *target = v;
    }
}

impl CaptchaSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        let provider = self.provider.trim().to_ascii_lowercase();
        self.provider = match provider.as_str() {
            PROVIDER_IMAGE | PROVIDER_TURNSTILE | PROVIDER_NONE => provider,
            _ => PROVIDER_NONE.into(),
        };
        let i = &mut self.image;
        if !(4..=8).contains(&i.length) {
            i.length = 5;
        }
        if i.width < 100 {
            i.width = 240;
        }
        if i.height < 40 {
            i.height = 80;
        }
        if i.noise_count < 0 {
            i.noise_count = 2;
        }
        if i.show_line < 0 {
            i.show_line = 2;
        }
        if !(30..=3600).contains(&i.expire_seconds) {
            i.expire_seconds = 300;
        }
        if i.max_store < 100 {
            i.max_store = 10240;
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

    pub fn validate(&self) -> crate::Result<()> {
        let n = self.clone().normalized();
        if n.provider == PROVIDER_NONE && n.scenes.any() {
            return Err(invalid("已启用验证码场景时必须选择验证码提供方"));
        }
        if n.provider == PROVIDER_TURNSTILE {
            if n.turnstile.site_key.is_empty() {
                return Err(invalid("Turnstile Site Key 不能为空"));
            }
            if n.turnstile.secret_key.is_empty() {
                return Err(invalid("Turnstile Secret Key 不能为空"));
            }
        }
        if !(500..=10000).contains(&n.turnstile.timeout_ms) {
            return Err(invalid("Turnstile 超时时间需在 500-10000ms"));
        }
        Ok(())
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut next = fallback;
        let Some(o) = raw.and_then(Value::as_object) else {
            return next;
        };
        next.provider = read_string(o, "provider", &next.provider);
        if let Some(s) = as_obj(o.get("scenes")) {
            let sc = &mut next.scenes;
            sc.login = read_bool(s, "login", sc.login);
            sc.register_send_code = read_bool(s, "register_send_code", sc.register_send_code);
            sc.reset_send_code = read_bool(s, "reset_send_code", sc.reset_send_code);
            sc.guest_create_order = read_bool(s, "guest_create_order", sc.guest_create_order);
            sc.gift_card_redeem = read_bool(s, "gift_card_redeem", sc.gift_card_redeem);
        }
        if let Some(m) = as_obj(o.get("image")) {
            let i = &mut next.image;
            i.length = read_int(m, "length", i.length);
            i.width = read_int(m, "width", i.width);
            i.height = read_int(m, "height", i.height);
            i.noise_count = read_int(m, "noise_count", i.noise_count);
            i.show_line = read_int(m, "show_line", i.show_line);
            i.expire_seconds = read_int(m, "expire_seconds", i.expire_seconds);
            i.max_store = read_int(m, "max_store", i.max_store);
        }
        if let Some(m) = as_obj(o.get("turnstile")) {
            let t = &mut next.turnstile;
            t.site_key = read_string(m, "site_key", &t.site_key);
            t.secret_key = read_string(m, "secret_key", &t.secret_key);
            t.verify_url = read_string(m, "verify_url", &t.verify_url);
            t.timeout_ms = read_int(m, "timeout_ms", t.timeout_ms);
        }
        next
    }

    pub fn encode(&self) -> Value {
        json!(self.clone().normalized())
    }

    /// Admin view without the Turnstile secret.
    pub fn masked(&self) -> Value {
        let n = self.clone().normalized();
        let mut v = json!(n);
        v["turnstile"]["secret_key"] = json!("");
        v["turnstile"]["has_secret"] = json!(!n.turnstile.secret_key.is_empty());
        v
    }

    /// Part of the setting exposed through `/public/config`.
    pub fn public(&self) -> Value {
        let n = self.clone().normalized();
        let mut v = json!({"provider": n.provider, "scenes": n.scenes});
        if n.provider == PROVIDER_TURNSTILE {
            v["turnstile"] = json!({"site_key": n.turnstile.site_key});
        }
        v
    }

    /// Whether `scene` requires a captcha.
    pub fn is_scene_enabled(&self, scene: &str) -> bool {
        match scene.trim().to_ascii_lowercase().as_str() {
            "login" => self.scenes.login,
            "register_send_code" => self.scenes.register_send_code,
            "reset_send_code" => self.scenes.reset_send_code,
            "guest_create_order" => self.scenes.guest_create_order,
            "gift_card_redeem" => self.scenes.gift_card_redeem,
            _ => false,
        }
    }

    pub fn apply_patch(&self, patch: CaptchaPatch) -> crate::Result<Self> {
        let mut next = self.clone();
        if let Some(p) = patch.provider {
            next.provider = p.trim().to_ascii_lowercase();
        }
        if let Some(s) = patch.scenes {
            let sc = &mut next.scenes;
            set(&mut sc.login, s.login);
            set(&mut sc.register_send_code, s.register_send_code);
            set(&mut sc.reset_send_code, s.reset_send_code);
            set(&mut sc.guest_create_order, s.guest_create_order);
            set(&mut sc.gift_card_redeem, s.gift_card_redeem);
        }
        if let Some(i) = patch.image {
            let img = &mut next.image;
            set(&mut img.length, i.length);
            set(&mut img.width, i.width);
            set(&mut img.height, i.height);
            set(&mut img.noise_count, i.noise_count);
            set(&mut img.show_line, i.show_line);
            set(&mut img.expire_seconds, i.expire_seconds);
            set(&mut img.max_store, i.max_store);
        }
        if let Some(t) = patch.turnstile {
            let ts = &mut next.turnstile;
            if let Some(v) = t.site_key {
                ts.site_key = v.trim().to_owned();
            }
            if let Some(v) = t.secret_key {
                let v = v.trim();
                if !v.is_empty() {
                    v.clone_into(&mut ts.secret_key);
                }
            }
            if let Some(v) = t.verify_url {
                ts.verify_url = v.trim().to_owned();
            }
            set(&mut ts.timeout_ms, t.timeout_ms);
        }
        let next = next.normalized();
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scenes_require_a_provider() {
        let err = CaptchaSetting::default()
            .apply_patch(CaptchaPatch {
                scenes: Some(CaptchaScenesPatch {
                    login: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap_err();
        assert_eq!(
            err.key(),
            "captcha config invalid: 已启用验证码场景时必须选择验证码提供方"
        );
    }

    #[test]
    fn turnstile_secret_is_masked_and_kept() {
        let s = CaptchaSetting::default()
            .apply_patch(CaptchaPatch {
                provider: Some(" TurnStile ".into()),
                turnstile: Some(CaptchaTurnstilePatch {
                    site_key: Some("site".into()),
                    secret_key: Some("sec".into()),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(s.provider, "turnstile");
        let kept = s
            .apply_patch(CaptchaPatch {
                turnstile: Some(CaptchaTurnstilePatch {
                    secret_key: Some(String::new()),
                    ..Default::default()
                }),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(kept.turnstile.secret_key, "sec");
        assert_eq!(kept.masked()["turnstile"]["secret_key"], "");
        assert_eq!(kept.masked()["turnstile"]["has_secret"], true);
        assert_eq!(kept.public()["turnstile"], json!({"site_key": "site"}));
        assert!(kept.public().get("image").is_none());
    }

    #[test]
    fn normalize_resets_out_of_range() {
        let s = CaptchaSetting {
            provider: "recaptcha".into(),
            image: CaptchaImage {
                length: 20,
                expire_seconds: 5,
                ..CaptchaImage::default()
            },
            ..CaptchaSetting::default()
        }
        .normalized();
        assert_eq!(s.provider, "none");
        assert_eq!(s.image.length, 5);
        assert_eq!(s.image.expire_seconds, 300);
        assert_eq!(s.image.width, 240);
    }
}
