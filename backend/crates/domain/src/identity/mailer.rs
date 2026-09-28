//! Outgoing email port and verification-code message content.

use async_trait::async_trait;

use super::verify_code::Purpose;
use crate::{Error, ErrorKind, Id, Result};

/// SMTP enabled but host/port/from missing (reported as a 500 like the original).
pub const KEY_NOT_CONFIGURED: &str = "error.email_service_not_configured";
/// The SMTP server rejected the recipient (5xx such as 550).
pub const KEY_RECIPIENT_REJECTED: &str = "error.email_recipient_not_found";

/// Error for a missing SMTP configuration.
pub fn not_configured() -> Error {
    Error::new(ErrorKind::Internal, KEY_NOT_CONFIGURED)
}

/// Error for a rejected recipient.
pub fn recipient_rejected() -> Error {
    Error::bad_request(KEY_RECIPIENT_REJECTED)
}

/// A plain-text email.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Email {
    pub to: String,
    pub subject: String,
    pub body: String,
    /// Display name of `From` (empty: the SMTP setting's `from_name`), NTF-02.
    pub from_name: String,
    /// `Reply-To` address (empty: none), already normalised by [`normalize_reply_to`].
    pub reply_to: String,
}

/// Customer-visible identity of one email (original `mailbrand.Brand`, NTF-02).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MailBrand {
    pub site_name: String,
    pub site_url: String,
    pub from_name: String,
    pub reply_to: String,
}

impl MailBrand {
    /// Tenant-safe brand of a reseller site without a configured name: the host is the
    /// name and `https://host` the URL — never the main shop's brand.
    pub fn reseller_fallback(host: &str) -> Self {
        let host = host.trim();
        Self {
            site_name: host.to_owned(),
            site_url: if host.is_empty() {
                String::new()
            } else {
                format!("https://{host}")
            },
            from_name: host.to_owned(),
            reply_to: String::new(),
        }
    }
}

/// Storefront whose brand an email carries: `reseller_id = None` is the main shop.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrandScope {
    pub reseller_id: Option<Id>,
    pub host: String,
}

/// Resolves the brand of a storefront scope (original `mailbrand.Resolver`).
#[async_trait]
pub trait MailBrands: Send + Sync {
    async fn resolve(&self, scope: &BrandScope) -> Result<MailBrand>;
}

/// `Reply-To` candidate reduced to a bare address like Go's `mail.ParseAddress`
/// (`"Name <a@b.c>"` → `a@b.c`); anything invalid (incl. CR/LF header injection) → `""`.
pub fn normalize_reply_to(raw: &str) -> String {
    let raw = raw.trim();
    if raw.contains(['\r', '\n']) {
        return String::new();
    }
    let addr = match (raw.rfind('<'), raw.rfind('>')) {
        (Some(open), Some(close)) if open < close && close == raw.len() - 1 => {
            &raw[open + 1..close]
        }
        (None, None) => raw,
        _ => return String::new(),
    };
    super::email::normalize(addr).unwrap_or_default()
}

/// Adds the site name / URL to a verification email like the original
/// `applyVerifyCodeBrand`: `"<site> - <subject>"` and a site/URL footer.
pub fn apply_verify_code_brand(
    locale: &str,
    subject: String,
    mut body: String,
    brand: &MailBrand,
) -> (String, String) {
    let name = brand.site_name.trim();
    let url = brand.site_url.trim().trim_end_matches('/');
    if name.is_empty() && url.is_empty() {
        return (subject, body);
    }
    let subject = if name.is_empty() {
        subject
    } else {
        format!("{name} - {subject}")
    };
    let (site, link) = match zs_shared::i18n::normalize_locale(locale) {
        "zh-TW" => ("站點：", "網址："),
        "en-US" => ("Site: ", "URL: "),
        _ => ("站点：", "网址："),
    };
    if !name.is_empty() {
        body.push_str(&format!("\n\n{site}{name}"));
    }
    if !url.is_empty() {
        body.push_str(&format!("\n{link}{url}"));
    }
    (subject, body)
}

/// Sends emails. Implementations read the live SMTP settings.
#[async_trait]
pub trait Mailer: Send + Sync {
    async fn send(&self, email: &Email) -> Result<()>;
}

/// Subject and body of a verification code email (original `buildVerifyCodeContent`).
pub fn verify_code_content(code: &str, purpose: Purpose, locale: &str) -> (String, String) {
    let locale = zs_shared::i18n::normalize_locale(locale);
    let (subject, purpose_text, template): (&str, &str, fn(&str, &str) -> String) = match locale {
        "zh-TW" => {
            let (s, p) = match purpose {
                Purpose::Register => ("註冊驗證碼", "註冊"),
                Purpose::Reset => ("重置密碼驗證碼", "重置密碼"),
                Purpose::TelegramBind => ("Telegram 綁定驗證碼", "綁定 Telegram"),
                Purpose::ChangeEmailOld | Purpose::ChangeEmailNew => ("更換郵箱驗證碼", "更換郵箱"),
            };
            (s, p, |c, p| {
                format!("您的驗證碼是：{c}\n\n該驗證碼用於 {p}，請勿洩露。")
            })
        }
        "en-US" => {
            let (s, p) = match purpose {
                Purpose::Register => ("Registration Code", "registration"),
                Purpose::Reset => ("Password Reset Code", "password reset"),
                Purpose::TelegramBind => ("Telegram Binding Code", "binding Telegram"),
                Purpose::ChangeEmailOld | Purpose::ChangeEmailNew => {
                    ("Change Email Code", "change email")
                }
            };
            (s, p, |c, p| {
                format!("Your verification code is: {c}\n\nThis code is for {p}. Do not share it.")
            })
        }
        _ => {
            let (s, p) = match purpose {
                Purpose::Register => ("注册验证码", "注册"),
                Purpose::Reset => ("重置密码验证码", "重置密码"),
                Purpose::TelegramBind => ("Telegram 绑定验证码", "绑定 Telegram"),
                Purpose::ChangeEmailOld | Purpose::ChangeEmailNew => ("更换邮箱验证码", "更换邮箱"),
            };
            (s, p, |c, p| {
                format!("您的验证码是：{c}\n\n该验证码用于 {p}，请勿泄露。")
            })
        }
    };
    (subject.to_owned(), template(code, purpose_text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localized_content() {
        let (s, b) = verify_code_content("123456", Purpose::Register, "zh-CN");
        assert_eq!(s, "注册验证码");
        assert_eq!(b, "您的验证码是：123456\n\n该验证码用于 注册，请勿泄露。");
        let (s, b) = verify_code_content("1", Purpose::Reset, "en-US");
        assert_eq!(s, "Password Reset Code");
        assert!(b.contains("password reset"));
        let (s, _) = verify_code_content("1", Purpose::ChangeEmailNew, "zh-TW");
        assert_eq!(s, "更換郵箱驗證碼");
    }

    /// NTF-02: the brand prefixes the subject and appends the site / URL footer.
    #[test]
    fn ntf_02_verify_code_brand() {
        let brand = MailBrand::reseller_fallback("shop.example");
        let (s, b) = apply_verify_code_brand("zh-CN", "注册验证码".into(), "code".into(), &brand);
        assert_eq!(s, "shop.example - 注册验证码");
        assert_eq!(b, "code\n\n站点：shop.example\n网址：https://shop.example");
        let named = MailBrand {
            site_name: "White".into(),
            site_url: "https://w.example/".into(),
            ..MailBrand::default()
        };
        let (s, b) = apply_verify_code_brand("en-US", "Code".into(), "x".into(), &named);
        assert_eq!(
            (s.as_str(), b.as_str()),
            ("White - Code", "x\n\nSite: White\nURL: https://w.example")
        );
        let (s, b) =
            apply_verify_code_brand("zh-CN", "S".into(), "B".into(), &MailBrand::default());
        assert_eq!((s.as_str(), b.as_str()), ("S", "B"));
    }

    #[test]
    fn ntf_02_reply_to_is_a_bare_address() {
        assert_eq!(
            normalize_reply_to(" Support <help@shop.example> "),
            "help@shop.example"
        );
        assert_eq!(normalize_reply_to("help@shop.example"), "help@shop.example");
        assert_eq!(normalize_reply_to("a@b.c\r\nBcc: x@y.z"), "");
        assert_eq!(normalize_reply_to("not an address"), "");
        assert_eq!(MailBrand::reseller_fallback("").site_url, "");
    }
}
