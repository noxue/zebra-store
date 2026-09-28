//! `telegram_auth_config` and `google_auth_config`
//! (port of `schema/security/telegram_auth.go` and `google_auth.go`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::value::{parse_bool, parse_int};
use crate::Error;

const TELEGRAM_INVALID: &str = "telegram auth config invalid";
const GOOGLE_INVALID: &str = "google auth config invalid";

/// Telegram login settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TelegramAuthSetting {
    pub enabled: bool,
    pub bot_username: String,
    pub bot_token: String,
    pub client_secret: String,
    pub oidc_redirect_uri: String,
    pub mini_app_url: String,
    pub login_expire_seconds: i64,
    pub replay_ttl_seconds: i64,
}

/// Partial update of [`TelegramAuthSetting`]; empty secrets keep the stored ones.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TelegramAuthPatch {
    pub enabled: Option<bool>,
    pub bot_username: Option<String>,
    pub bot_token: Option<String>,
    pub client_secret: Option<String>,
    pub oidc_redirect_uri: Option<String>,
    pub mini_app_url: Option<String>,
    pub login_expire_seconds: Option<i64>,
    pub replay_ttl_seconds: Option<i64>,
}

/// How the storefront performs Telegram web login.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TelegramLoginMode {
    Disabled,
    Widget,
    Oidc,
}

impl TelegramLoginMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "",
            Self::Widget => "widget",
            Self::Oidc => "oidc",
        }
    }
}

/// Numeric prefix of `123456789:ABC…`, used as the OIDC client id.
pub fn telegram_bot_id(token: &str) -> Option<&str> {
    let token = token.trim();
    let (id, _) = token.split_once(':')?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_digit())).then_some(id)
}

fn is_http_url(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    rest.is_some_and(|r| {
        let host = r.split(['/', '?', '#']).next().unwrap_or("");
        !host.is_empty()
    })
}

impl TelegramAuthSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.bot_username = self.bot_username.trim().trim_start_matches('@').to_owned();
        self.bot_token = self.bot_token.trim().to_owned();
        self.client_secret = self.client_secret.trim().to_owned();
        self.oidc_redirect_uri = self
            .oidc_redirect_uri
            .trim()
            .trim_end_matches('/')
            .to_owned();
        self.mini_app_url = self.mini_app_url.trim().to_owned();
        if self.login_expire_seconds <= 0 {
            self.login_expire_seconds = 300;
        }
        self.login_expire_seconds = self.login_expire_seconds.clamp(30, 86400);
        if self.replay_ttl_seconds <= 0 {
            self.replay_ttl_seconds = self.login_expire_seconds;
        }
        self.replay_ttl_seconds = self.replay_ttl_seconds.clamp(60, 86400);
        self
    }

    pub fn validate(&self) -> crate::Result<()> {
        let n = self.clone().normalized();
        let invalid = |d: &str| Error::bad_request(format!("{TELEGRAM_INVALID}: {d}"));
        if !n.enabled {
            return Ok(());
        }
        if n.bot_username.is_empty() {
            return Err(invalid("Bot 用户名不能为空"));
        }
        if n.bot_username.chars().any(char::is_whitespace) {
            return Err(invalid("Bot 用户名格式无效"));
        }
        if n.bot_token.is_empty() {
            return Err(invalid("Bot Token 不能为空"));
        }
        if !n.client_secret.is_empty() {
            if n.oidc_redirect_uri.is_empty() {
                return Err(invalid("配置 client_secret 时必须同时填写 OIDC 回调地址"));
            }
            if !is_http_url(&n.oidc_redirect_uri) {
                return Err(invalid("OIDC 回调地址必须是合法的 http(s) URL"));
            }
            if telegram_bot_id(&n.bot_token).is_none() {
                return Err(invalid("Bot Token 格式无效，无法解析出 OIDC client_id"));
            }
        }
        Ok(())
    }

    /// Login mode of a normalized setting.
    pub fn login_mode(&self) -> TelegramLoginMode {
        if !self.enabled {
            TelegramLoginMode::Disabled
        } else if !self.client_secret.is_empty()
            && !self.oidc_redirect_uri.is_empty()
            && telegram_bot_id(&self.bot_token).is_some()
        {
            TelegramLoginMode::Oidc
        } else if !self.bot_token.is_empty() {
            TelegramLoginMode::Widget
        } else {
            TelegramLoginMode::Disabled
        }
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut next = fallback;
        let Some(o) = raw.and_then(Value::as_object) else {
            return next;
        };
        if let Some(v) = o.get("enabled") {
            next.enabled = parse_bool(Some(v));
        }
        for (key, field) in [
            ("bot_username", &mut next.bot_username),
            ("bot_token", &mut next.bot_token),
            ("client_secret", &mut next.client_secret),
            ("oidc_redirect_uri", &mut next.oidc_redirect_uri),
            ("mini_app_url", &mut next.mini_app_url),
        ] {
            if let Some(Value::String(s)) = o.get(key) {
                field.clone_from(s);
            }
        }
        if let Some(n) = parse_int(o.get("login_expire_seconds")) {
            next.login_expire_seconds = n;
        }
        if let Some(n) = parse_int(o.get("replay_ttl_seconds")) {
            next.replay_ttl_seconds = n;
        }
        next
    }

    pub fn encode(&self) -> Value {
        json!(self.clone().normalized())
    }

    /// Admin view without the bot token / client secret.
    pub fn masked(&self) -> Value {
        let n = self.clone().normalized();
        json!({
            "enabled": n.enabled,
            "bot_username": n.bot_username,
            "bot_token": "",
            "has_bot_token": !n.bot_token.is_empty(),
            "client_secret": "",
            "has_client_secret": !n.client_secret.is_empty(),
            "oidc_redirect_uri": n.oidc_redirect_uri,
            "mode": n.login_mode().as_str(),
            "mini_app_url": n.mini_app_url,
            "login_expire_seconds": n.login_expire_seconds,
            "replay_ttl_seconds": n.replay_ttl_seconds,
        })
    }

    /// Storefront view (`/public/config.telegram_auth`).
    pub fn public(&self) -> Value {
        let n = self.clone().normalized();
        json!({
            "enabled": n.enabled,
            "bot_username": n.bot_username,
            "mini_app_url": n.mini_app_url,
            "mode": n.login_mode().as_str(),
        })
    }

    /// Applies a patch (without validation).
    pub fn apply_patch(&self, patch: TelegramAuthPatch) -> Self {
        let mut next = self.clone();
        if let Some(v) = patch.enabled {
            next.enabled = v;
        }
        if let Some(v) = patch.bot_username {
            next.bot_username = v.trim().to_owned();
        }
        if let Some(v) = patch.bot_token.filter(|v| !v.trim().is_empty()) {
            next.bot_token = v.trim().to_owned();
        }
        if let Some(v) = patch.client_secret.filter(|v| !v.trim().is_empty()) {
            next.client_secret = v.trim().to_owned();
        }
        if let Some(v) = patch.oidc_redirect_uri {
            next.oidc_redirect_uri = v.trim().to_owned();
            // Clearing the callback falls back to widget mode and drops the secret,
            // otherwise validation would fail with no way to fix it.
            if next.oidc_redirect_uri.is_empty() {
                next.client_secret.clear();
            }
        }
        if let Some(v) = patch.mini_app_url {
            next.mini_app_url = v.trim().to_owned();
        }
        if let Some(v) = patch.login_expire_seconds {
            next.login_expire_seconds = v;
        }
        if let Some(v) = patch.replay_ttl_seconds {
            next.replay_ttl_seconds = v;
        }
        next
    }
}

/// Google Identity Services login settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GoogleAuthSetting {
    pub enabled: bool,
    pub client_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GoogleAuthPatch {
    pub enabled: Option<bool>,
    pub client_id: Option<String>,
}

impl GoogleAuthSetting {
    #[must_use]
    pub fn normalized(mut self) -> Self {
        self.client_id = self.client_id.trim().to_owned();
        self
    }

    pub fn validate(&self) -> crate::Result<()> {
        let n = self.clone().normalized();
        if n.enabled && n.client_id.is_empty() {
            return Err(Error::bad_request(format!(
                "{GOOGLE_INVALID}: Client ID 不能为空"
            )));
        }
        Ok(())
    }

    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut next = fallback;
        if let Some(o) = raw.and_then(Value::as_object) {
            if let Some(v) = o.get("enabled") {
                next.enabled = parse_bool(Some(v));
            }
            if let Some(Value::String(s)) = o.get("client_id") {
                next.client_id.clone_from(s);
            }
        }
        next.normalized()
    }

    /// Stored and admin shape (the client id is public).
    pub fn encode(&self) -> Value {
        json!(self.clone().normalized())
    }

    /// Storefront view: only enabled with a client id.
    pub fn public(&self) -> Value {
        let n = self.clone().normalized();
        json!({"enabled": n.enabled && !n.client_id.is_empty(), "client_id": n.client_id})
    }

    pub fn apply_patch(&self, patch: GoogleAuthPatch) -> Self {
        let mut next = self.clone();
        if let Some(v) = patch.enabled {
            next.enabled = v;
        }
        if let Some(v) = patch.client_id {
            next.client_id = v.trim().to_owned();
        }
        next
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled() -> TelegramAuthSetting {
        TelegramAuthSetting {
            enabled: true,
            bot_username: "@shop_bot".into(),
            bot_token: "123456:ABC".into(),
            ..Default::default()
        }
        .normalized()
    }

    #[test]
    fn normalize_clamps_ttls() {
        let s = TelegramAuthSetting {
            login_expire_seconds: 10,
            ..Default::default()
        }
        .normalized();
        assert_eq!(s.login_expire_seconds, 30);
        assert_eq!(s.replay_ttl_seconds, 60);
        assert_eq!(enabled().bot_username, "shop_bot");
    }

    #[test]
    fn modes_and_validation() {
        let s = enabled();
        assert_eq!(s.login_mode(), TelegramLoginMode::Widget);
        assert!(s.validate().is_ok());
        let oidc = s.apply_patch(TelegramAuthPatch {
            client_secret: Some("sec".into()),
            oidc_redirect_uri: Some("https://shop.example/auth/cb/".into()),
            ..Default::default()
        });
        let oidc = oidc.normalized();
        assert_eq!(oidc.login_mode(), TelegramLoginMode::Oidc);
        assert!(oidc.validate().is_ok());
        let bad = TelegramAuthSetting {
            client_secret: "x".into(),
            ..s.clone()
        };
        assert!(bad.validate().is_err());
        let bad_url = TelegramAuthSetting {
            client_secret: "x".into(),
            oidc_redirect_uri: "ftp://x".into(),
            ..s
        };
        assert!(bad_url.validate().is_err());
    }

    // CNT-10: clearing the OIDC callback also drops the client secret.
    #[test]
    fn clearing_redirect_clears_secret_and_empty_token_keeps_existing() {
        let s = TelegramAuthSetting {
            client_secret: "sec".into(),
            oidc_redirect_uri: "https://a/cb".into(),
            ..enabled()
        };
        let next = s.apply_patch(TelegramAuthPatch {
            oidc_redirect_uri: Some(String::new()),
            bot_token: Some("  ".into()),
            ..Default::default()
        });
        assert_eq!(next.client_secret, "");
        assert_eq!(next.bot_token, "123456:ABC");
        let masked = next.normalized().masked();
        assert_eq!(masked["bot_token"], "");
        assert_eq!(masked["has_bot_token"], true);
        assert_eq!(masked["mode"], "widget");
    }

    #[test]
    fn google_public_requires_client_id() {
        let g = GoogleAuthSetting {
            enabled: true,
            client_id: " ".into(),
        };
        assert!(g.validate().is_err());
        assert_eq!(g.public(), json!({"enabled": false, "client_id": ""}));
        let g = g.apply_patch(GoogleAuthPatch {
            client_id: Some(" cid ".into()),
            ..Default::default()
        });
        assert_eq!(g.public(), json!({"enabled": true, "client_id": "cid"}));
    }
}
