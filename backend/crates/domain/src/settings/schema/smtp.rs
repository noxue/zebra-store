//! `smtp_config` (port of `schema/messaging/smtp.go`).

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::value::{as_obj, is_email_address, parse_bool, parse_int};
use crate::Error;

const INVALID: &str = "smtp config invalid";

/// Verification code limits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SmtpVerifyCode {
    pub expire_minutes: i64,
    pub send_interval_seconds: i64,
    pub max_attempts: i64,
    pub length: i64,
}

/// SMTP settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SmtpSetting {
    pub enabled: bool,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub password: String,
    pub from: String,
    pub from_name: String,
    pub use_tls: bool,
    pub use_ssl: bool,
    pub order_notification_enabled: bool,
    pub verify_code: SmtpVerifyCode,
}

/// Partial update of the verification code limits.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SmtpVerifyCodePatch {
    pub expire_minutes: Option<i64>,
    pub send_interval_seconds: Option<i64>,
    pub max_attempts: Option<i64>,
    pub length: Option<i64>,
}

/// Partial update of [`SmtpSetting`]; an empty password keeps the stored one.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SmtpPatch {
    pub enabled: Option<bool>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub from: Option<String>,
    pub from_name: Option<String>,
    pub use_tls: Option<bool>,
    pub use_ssl: Option<bool>,
    pub order_notification_enabled: Option<bool>,
    pub verify_code: Option<SmtpVerifyCodePatch>,
}

fn invalid(detail: &str) -> Error {
    Error::bad_request(format!("{INVALID}: {detail}"))
}

impl SmtpSetting {
    /// Trims text fields and fills defaults for out-of-range numbers.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        for field in [
            &mut self.host,
            &mut self.username,
            &mut self.password,
            &mut self.from,
            &mut self.from_name,
        ] {
            *field = field.trim().to_owned();
        }
        if !(1..=65535).contains(&self.port) {
            self.port = 587;
        }
        let v = &mut self.verify_code;
        if v.expire_minutes <= 0 {
            v.expire_minutes = 10;
        }
        if v.send_interval_seconds <= 0 {
            v.send_interval_seconds = 60;
        }
        if v.max_attempts <= 0 {
            v.max_attempts = 5;
        }
        if !(4..=10).contains(&v.length) {
            v.length = 6;
        }
        self
    }

    /// Validates a normalized setting (messages match the original).
    pub fn validate(&self) -> crate::Result<()> {
        if !(1..=65535).contains(&self.port) {
            return Err(invalid("SMTP 端口必须在 1-65535"));
        }
        if self.use_tls && self.use_ssl {
            return Err(invalid("TLS 与 SSL 不能同时开启"));
        }
        let v = &self.verify_code;
        if !(4..=10).contains(&v.length) {
            return Err(invalid("验证码长度需在 4-10 之间"));
        }
        if v.expire_minutes <= 0 {
            return Err(invalid("验证码过期时间必须大于 0"));
        }
        if v.send_interval_seconds <= 0 {
            return Err(invalid("验证码发送间隔必须大于 0"));
        }
        if v.max_attempts <= 0 {
            return Err(invalid("验证码尝试次数必须大于 0"));
        }
        if !self.enabled {
            return Ok(());
        }
        if self.host.trim().is_empty() {
            return Err(invalid("SMTP 主机不能为空"));
        }
        if self.from.trim().is_empty() {
            return Err(invalid("发件人邮箱不能为空"));
        }
        if !is_email_address(&self.from) {
            return Err(invalid("发件人邮箱格式无效"));
        }
        Ok(())
    }

    /// Overlays stored JSON onto `fallback` (missing fields keep the fallback).
    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut next = fallback;
        let Some(o) = raw.and_then(Value::as_object) else {
            return next;
        };
        if let Some(v) = o.get("enabled") {
            next.enabled = parse_bool(Some(v));
        }
        let strings: [(&str, &mut String); 5] = [
            ("host", &mut next.host),
            ("username", &mut next.username),
            ("password", &mut next.password),
            ("from", &mut next.from),
            ("from_name", &mut next.from_name),
        ];
        for (key, field) in strings {
            if let Some(Value::String(s)) = o.get(key) {
                field.clone_from(s);
            }
        }
        if let Some(p) = parse_int(o.get("port")) {
            next.port = p;
        }
        for (key, field) in [
            ("use_tls", &mut next.use_tls),
            ("use_ssl", &mut next.use_ssl),
            (
                "order_notification_enabled",
                &mut next.order_notification_enabled,
            ),
        ] {
            if let Some(v) = o.get(key) {
                *field = parse_bool(Some(v));
            }
        }
        if let Some(vc) = as_obj(o.get("verify_code")) {
            let v = &mut next.verify_code;
            for (key, field) in [
                ("expire_minutes", &mut v.expire_minutes),
                ("send_interval_seconds", &mut v.send_interval_seconds),
                ("max_attempts", &mut v.max_attempts),
                ("length", &mut v.length),
            ] {
                if let Some(n) = parse_int(vc.get(key)) {
                    *field = n;
                }
            }
        }
        next
    }

    /// Stored JSON shape.
    pub fn encode(&self) -> Value {
        json!(self.clone().normalized())
    }

    /// Admin view: the password is never returned, only `has_password`.
    pub fn masked(&self) -> Value {
        let n = self.clone().normalized();
        let mut v = json!(n);
        v["password"] = json!("");
        v["has_password"] = json!(!n.password.is_empty());
        v
    }

    /// Applies a patch, then normalizes and validates.
    pub fn apply_patch(&self, patch: SmtpPatch) -> crate::Result<Self> {
        let mut next = self.clone();
        if let Some(v) = patch.enabled {
            next.enabled = v;
        }
        if let Some(v) = patch.host {
            next.host = v.trim().to_owned();
        }
        if let Some(v) = patch.port {
            next.port = v;
        }
        if let Some(v) = patch.username {
            next.username = v.trim().to_owned();
        }
        if let Some(v) = patch.password {
            let v = v.trim();
            if !v.is_empty() {
                v.clone_into(&mut next.password);
            }
        }
        if let Some(v) = patch.from {
            next.from = v.trim().to_owned();
        }
        if let Some(v) = patch.from_name {
            next.from_name = v.trim().to_owned();
        }
        if let Some(v) = patch.use_tls {
            next.use_tls = v;
        }
        if let Some(v) = patch.use_ssl {
            next.use_ssl = v;
        }
        if let Some(v) = patch.order_notification_enabled {
            next.order_notification_enabled = v;
        }
        if let Some(vc) = patch.verify_code {
            let v = &mut next.verify_code;
            if let Some(x) = vc.expire_minutes {
                v.expire_minutes = x;
            }
            if let Some(x) = vc.send_interval_seconds {
                v.send_interval_seconds = x;
            }
            if let Some(x) = vc.max_attempts {
                v.max_attempts = x;
            }
            if let Some(x) = vc.length {
                v.length = x;
            }
        }
        let next = next.normalized();
        next.validate()?;
        Ok(next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> SmtpSetting {
        SmtpSetting {
            enabled: true,
            host: "smtp.example.com".into(),
            port: 465,
            from: "shop@example.com".into(),
            password: "secret".into(),
            ..SmtpSetting::default()
        }
        .normalized()
    }

    #[test]
    fn normalize_fills_defaults() {
        let s = SmtpSetting {
            port: 0,
            host: " h ".into(),
            ..SmtpSetting::default()
        }
        .normalized();
        assert_eq!(s.port, 587);
        assert_eq!(s.host, "h");
        assert_eq!(
            s.verify_code,
            SmtpVerifyCode {
                expire_minutes: 10,
                send_interval_seconds: 60,
                max_attempts: 5,
                length: 6
            }
        );
    }

    #[test]
    fn validate_rejects_tls_and_ssl_together() {
        let mut s = base();
        s.use_tls = true;
        s.use_ssl = true;
        assert_eq!(
            s.validate().unwrap_err().key(),
            "smtp config invalid: TLS 与 SSL 不能同时开启"
        );
        let mut s = base();
        s.from = "not-an-email".into();
        assert!(s.validate().is_err());
        let mut s = base();
        s.enabled = false;
        s.host.clear();
        assert!(s.validate().is_ok());
    }

    // CNT-09: an empty password in a patch keeps the stored secret.
    #[test]
    fn patch_keeps_password_when_empty_and_masks() {
        let s = base();
        let next = s
            .apply_patch(SmtpPatch {
                password: Some("  ".into()),
                host: Some(" smtp2.example.com ".into()),
                ..SmtpPatch::default()
            })
            .unwrap();
        assert_eq!(next.password, "secret");
        assert_eq!(next.host, "smtp2.example.com");
        let masked = next.masked();
        assert_eq!(masked["password"], "");
        assert_eq!(masked["has_password"], true);
    }

    #[test]
    fn decode_accepts_string_numbers() {
        let s = SmtpSetting::decode(
            Some(&json!({"port": "2525", "enabled": "on", "verify_code": {"length": 8}})),
            SmtpSetting::default(),
        );
        assert_eq!(s.port, 2525);
        assert!(s.enabled);
        assert_eq!(s.verify_code.length, 8);
    }
}
