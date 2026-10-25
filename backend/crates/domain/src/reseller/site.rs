//! Reseller site branding: input normalisation (`site_config.go`) and the overlay
//! of the main-site `/public/config` (RSL-06, RSL-08).

use std::collections::HashMap;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::keys;
use super::model::SiteConfig;
use crate::settings::schema::site::normalize_scripts;
use crate::{Error, Result};

/// Locales of localized fields, in the original order (`constants.SupportedLocales`).
pub const LOCALES: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];
/// Longest URL / image path kept.
const URL_MAX: usize = 500;
/// Longest e-mail address kept.
const EMAIL_MAX: usize = 320;
/// At most this many footer / navigation links.
const MAX_LINKS: usize = 10;
/// Rich-text announcement content budget per locale (HTML included).
const ANNOUNCEMENT_CONTENT_MAX: usize = 4000;

/// Localized text as submitted (`{"zh-CN": "...", ...}`).
pub type LocalizedInput = HashMap<String, String>;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct AnnouncementInput {
    pub enabled: bool,
    #[serde(rename = "type")]
    pub kind: String,
    pub title: LocalizedInput,
    pub content: LocalizedInput,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SupportInput {
    pub telegram: String,
    pub whatsapp: String,
    pub email: String,
    pub support_url: String,
    pub wechat_id: String,
    pub wechat_qr_code: String,
    pub qq_number: String,
    pub qq_group: String,
    pub wechat_group_qr_code: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SeoInput {
    pub title: LocalizedInput,
    pub keywords: LocalizedInput,
    pub description: LocalizedInput,
    pub default_og_image: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct LinkInput {
    pub name: LocalizedInput,
    pub url: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct NavConfigInput {
    pub builtin: HashMap<String, bool>,
    pub custom_items: Vec<LinkInput>,
}

/// `PUT site-config` body.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SiteConfigInput {
    pub site_name: String,
    pub logo: String,
    pub favicon: String,
    pub announcement: AnnouncementInput,
    pub support: SupportInput,
    pub seo: SeoInput,
    pub footer_links: Vec<LinkInput>,
    pub nav_config: NavConfigInput,
    pub scripts: Value,
    pub storefront_template: String,
    pub template_mode: String,
    pub theme: ThemeInput,
}

/// Optional visual overrides. Empty values inherit the main site's colours.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThemeInput {
    pub primary_color: String,
    pub secondary_color: String,
    pub accent_color: String,
    pub background_color: String,
    pub surface_color: String,
    pub text_color: String,
}

const THEME_COLORS: [&str; 6] = [
    "primary_color",
    "secondary_color",
    "accent_color",
    "background_color",
    "surface_color",
    "text_color",
];

fn normalize_visual(input: &SiteConfigInput) -> Result<Value> {
    let template = input.storefront_template.trim();
    if !template.is_empty()
        && !matches!(
            template,
            "classic" | "vault" | "aurora" | "atelier" | "noir"
        )
    {
        return Err(field(keys::SITE_CONFIG_INVALID));
    }
    let mode = input.template_mode.trim();
    if !mode.is_empty() && !matches!(mode, "card" | "list") {
        return Err(field(keys::SITE_CONFIG_INVALID));
    }
    let mut theme = Map::new();
    if !template.is_empty() {
        theme.insert("storefront_template".into(), json!(template));
    }
    if !mode.is_empty() {
        theme.insert("template_mode".into(), json!(mode));
    }
    for (key, raw) in THEME_COLORS.into_iter().zip([
        &input.theme.primary_color,
        &input.theme.secondary_color,
        &input.theme.accent_color,
        &input.theme.background_color,
        &input.theme.surface_color,
        &input.theme.text_color,
    ]) {
        let color = raw.trim();
        if color.is_empty() {
            continue;
        }
        let hex = color
            .strip_prefix('#')
            .ok_or_else(|| field(keys::SITE_CONFIG_INVALID))?;
        if !matches!(hex.len(), 3 | 6) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(field(keys::SITE_CONFIG_INVALID));
        }
        theme.insert(key.into(), json!(color.to_ascii_lowercase()));
    }
    Ok(Value::Object(theme))
}

/// Normalized site configuration ready to be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteConfigDraft {
    pub site_name: String,
    pub logo: String,
    pub favicon: String,
    pub announcement: Value,
    pub support: Value,
    pub seo: Value,
    pub footer_links: Value,
    pub nav_config: Value,
    pub theme: Value,
    pub scripts: Value,
}

/// Trims and truncates to `max` characters (`trimLimit`).
fn trim_limit(raw: &str, max: usize) -> String {
    let v = raw.trim();
    if v.chars().count() > max {
        v.chars().take(max).collect()
    } else {
        v.to_owned()
    }
}

fn localized(raw: &LocalizedInput, max: usize) -> Value {
    let mut m = Map::new();
    for lang in LOCALES {
        m.insert(
            lang.into(),
            Value::String(trim_limit(raw.get(lang).map_or("", String::as_str), max)),
        );
    }
    Value::Object(m)
}

/// Host of an absolute URL with the given scheme set, like `url.Parse(v).Host != ""`.
fn url_host(value: &str) -> Option<(String, &str)> {
    let (scheme, rest) = value.split_once("://")?;
    if scheme.is_empty()
        || !scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
    {
        return None;
    }
    if value.chars().any(char::is_control) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host = authority.rsplit('@').next().unwrap_or_default();
    (!host.is_empty() && !host.contains(' ')).then(|| (scheme.to_ascii_lowercase(), host))
}

/// `http(s)://…` or an `/uploads/…` path (`validateHTTPOrUploadPath`).
fn http_or_upload(raw: &str) -> Option<String> {
    let v = trim_limit(raw, URL_MAX);
    if v.is_empty() || v.starts_with("/uploads/") {
        return Some(v);
    }
    match url_host(&v) {
        Some((scheme, _)) if scheme == "http" || scheme == "https" => Some(v),
        _ => None,
    }
}

/// A minimal `net/mail.ParseAddress`: `local@domain` or `Name <local@domain>`.
fn is_mail_address(raw: &str) -> bool {
    let raw = raw.trim();
    let addr = match (raw.find('<'), raw.strip_suffix('>')) {
        (Some(open), Some(inner)) => &inner[open + 1..],
        (None, None) => raw,
        _ => return false,
    };
    let Some((local, domain)) = addr.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !addr
            .chars()
            .any(|c| c.is_whitespace() || c == ',' || c == ';')
}

/// `tg://…`, `mailto:<address>` or `https://…` (`validateSupportURL`).
fn support_url(raw: &str) -> Option<String> {
    let v = trim_limit(raw, URL_MAX);
    if v.is_empty() || v.starts_with("tg://") {
        return Some(v);
    }
    if let Some(addr) = v.strip_prefix("mailto:") {
        return is_mail_address(addr).then_some(v);
    }
    match url_host(&v) {
        Some((scheme, _)) if scheme == "https" => Some(v),
        _ => None,
    }
}

fn field(key: &'static str) -> Error {
    Error::bad_request(key)
}

/// Contact links (`NormalizeResellerSupport`).
pub fn normalize_support(input: &SupportInput) -> Result<Value> {
    let telegram = trim_limit(&input.telegram, URL_MAX);
    if !telegram.is_empty()
        && !["https://telegram.me/", "https://t.me/", "tg://"]
            .iter()
            .any(|p| telegram.starts_with(p))
    {
        return Err(field(keys::SUPPORT_TELEGRAM_INVALID));
    }
    let whatsapp = trim_limit(&input.whatsapp, URL_MAX);
    if !whatsapp.is_empty()
        && !["https://wa.me/", "https://api.whatsapp.com/"]
            .iter()
            .any(|p| whatsapp.starts_with(p))
    {
        return Err(field(keys::SUPPORT_WHATSAPP_INVALID));
    }
    let email = trim_limit(&input.email, EMAIL_MAX);
    let email = email.strip_prefix("mailto:").unwrap_or(&email).to_owned();
    if !email.is_empty() && !is_mail_address(&email) {
        return Err(field(keys::SUPPORT_EMAIL_INVALID));
    }
    let url = support_url(&input.support_url).ok_or_else(|| field(keys::SUPPORT_URL_INVALID))?;
    let wechat_qr_code =
        http_or_upload(&input.wechat_qr_code).ok_or_else(|| field(keys::IMAGE_INVALID))?;
    let wechat_group_qr_code =
        http_or_upload(&input.wechat_group_qr_code).ok_or_else(|| field(keys::IMAGE_INVALID))?;
    Ok(json!({
        "telegram": telegram,
        "whatsapp": whatsapp,
        "email": email,
        "support_url": url,
        "wechat_id": trim_limit(&input.wechat_id, 100),
        "wechat_qr_code": wechat_qr_code,
        "qq_number": trim_limit(&input.qq_number, 100),
        "qq_group": trim_limit(&input.qq_group, 100),
        "wechat_group_qr_code": wechat_group_qr_code,
    }))
}

fn normalize_announcement(input: &AnnouncementInput) -> Value {
    let mut kind = trim_limit(&input.kind, 32);
    if !matches!(kind.as_str(), "info" | "success" | "warning") {
        "info".clone_into(&mut kind);
    }
    json!({
        "enabled": input.enabled,
        "type": kind,
        "title": localized(&input.title, 120),
        "content": localized(&input.content, ANNOUNCEMENT_CONTENT_MAX),
    })
}

fn normalize_seo(input: &SeoInput) -> Result<Value> {
    let image =
        http_or_upload(&input.default_og_image).ok_or_else(|| field(keys::IMAGE_INVALID))?;
    Ok(json!({
        "title": localized(&input.title, 120),
        "keywords": localized(&input.keywords, 200),
        "description": localized(&input.description, 300),
        "default_og_image": image,
    }))
}

fn normalize_links(input: &[LinkInput]) -> Result<Vec<Value>> {
    let mut items = Vec::new();
    for link in input.iter().take(MAX_LINKS) {
        let url = support_url(&link.url).ok_or_else(|| field(keys::LINK_INVALID))?;
        if url.is_empty() {
            continue;
        }
        items.push(json!({"name": localized(&link.name, 80), "url": url}));
    }
    Ok(items)
}

fn normalize_nav(input: &NavConfigInput) -> Result<Value> {
    let mut builtin = Map::new();
    for key in ["blog", "notice", "about"] {
        builtin.insert(
            key.into(),
            Value::Bool(input.builtin.get(key).copied().unwrap_or(true)),
        );
    }
    Ok(json!({"builtin": builtin, "custom_items": normalize_links(&input.custom_items)?}))
}

/// Validates and normalizes a site configuration (`buildModel`); the first invalid
/// field yields its specific error key.
pub fn normalize_site_config(input: &SiteConfigInput) -> Result<SiteConfigDraft> {
    let logo = http_or_upload(&input.logo).ok_or_else(|| field(keys::IMAGE_INVALID))?;
    let favicon = http_or_upload(&input.favicon).ok_or_else(|| field(keys::IMAGE_INVALID))?;
    let support = normalize_support(&input.support)?;
    let seo = normalize_seo(&input.seo)?;
    let footer_links = json!({"items": normalize_links(&input.footer_links)?});
    let nav_config = normalize_nav(&input.nav_config)?;
    Ok(SiteConfigDraft {
        site_name: trim_limit(&input.site_name, 120),
        logo,
        favicon,
        announcement: normalize_announcement(&input.announcement),
        support,
        seo,
        footer_links,
        nav_config,
        theme: normalize_visual(input)?,
        scripts: normalize_scripts(Some(&input.scripts), true),
    })
}

/// `footer_links_json.items` as an array (never null).
pub fn footer_items(footer_links: &Value) -> Vec<Value> {
    footer_links
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

/// 8-hex-digit FNV-1a fingerprint of an announcement (`homeAnnouncementVersion`).
pub fn announcement_version(kind: &str, title: &Value, content: &Value) -> String {
    const OFFSET: u32 = 0x811c_9dc5;
    const PRIME: u32 = 0x0100_0193;
    let mut hash = OFFSET;
    let mut write = |bytes: &[u8]| {
        for b in bytes {
            hash ^= u32::from(*b);
            hash = hash.wrapping_mul(PRIME);
        }
    };
    write(kind.as_bytes());
    for lang in LOCALES {
        write(&[0]);
        if let Some(t) = title.get(lang).and_then(Value::as_str) {
            write(t.as_bytes());
        }
        write(&[0]);
        if let Some(c) = content.get(lang).and_then(Value::as_str) {
            write(c.as_bytes());
        }
    }
    format!("{hash:08x}")
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => matches!(
            s.trim().to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        _ => false,
    }
}

/// Reseller announcement in the main-site public shape (RSL-08): the main-site
/// announcement never leaks; disabled / empty → no field; `enabled` is not exposed.
pub fn apply_announcement(out: &mut Map<String, Value>, stored: &Value) {
    out.remove("announcement");
    if !truthy(stored.get("enabled")) {
        return;
    }
    let content = stored
        .get("content")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let has_content = LOCALES.iter().any(|l| {
        content
            .get(*l)
            .and_then(Value::as_str)
            .is_some_and(|s| !s.trim().is_empty())
    });
    if !has_content {
        return;
    }
    let title = stored
        .get("title")
        .cloned()
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let kind = stored
        .get("type")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("info")
        .to_owned();
    let version = announcement_version(&kind, &title, &content);
    out.insert(
        "announcement".into(),
        json!({"type": kind, "title": title, "content": content, "version": version}),
    );
}

fn non_empty_object(v: &Value) -> bool {
    v.as_object().is_some_and(|m| !m.is_empty())
}

/// Rewrites a main-site public config for a reseller site (`ApplyPublicConfigOverlay`).
/// `site` is `None` when the profile is inactive or has no saved configuration; the
/// tenant block and `brand.site_url` are set regardless.
pub fn overlay_public_config(
    base: Value,
    host: &str,
    primary_domain: &str,
    site: Option<&SiteConfig>,
) -> Value {
    let mut out = match base {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    out.insert(
        "tenant".into(),
        json!({"mode": "reseller", "host": host, "primary_domain": primary_domain}),
    );
    let brand_host = if host.trim().is_empty() {
        primary_domain
    } else {
        host
    }
    .trim();
    let mut brand = out
        .remove("brand")
        .and_then(|b| match b {
            Value::Object(m) => Some(m),
            _ => None,
        })
        .unwrap_or_default();
    let site_url = if brand_host.is_empty() {
        String::new()
    } else {
        format!("https://{brand_host}")
    };
    brand.insert("site_url".into(), Value::String(site_url));
    if let Some(cfg) = site {
        if !cfg.logo.is_empty() {
            // A reseller's own logo must not inherit the main site's dark variant.
            brand.remove("site_logo_dark");
        }
        for (key, value) in [
            ("site_name", &cfg.site_name),
            ("site_logo", &cfg.logo),
            ("site_icon", &cfg.favicon),
        ] {
            if !value.is_empty() {
                brand.insert(key.into(), Value::String(value.clone()));
            }
        }
    }
    out.insert("brand".into(), Value::Object(brand));
    let mut scripts = out
        .remove("scripts")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|script| script.get("scope").and_then(Value::as_str) == Some("global"))
        .collect::<Vec<_>>();
    if let Some(cfg) = site {
        scripts.extend(
            cfg.scripts
                .as_array()
                .into_iter()
                .flatten()
                .filter(|script| script.get("scope").and_then(Value::as_str) == Some("reseller"))
                .cloned(),
        );
    }
    out.insert("scripts".into(), Value::Array(scripts));
    let Some(cfg) = site else {
        // A reseller without its own support settings must not expose the main site's contacts.
        out.insert("contact".into(), json!({}));
        return Value::Object(out);
    };
    let mut contact = Map::new();
    for key in [
        "telegram",
        "whatsapp",
        "email",
        "support_url",
        "wechat_id",
        "wechat_qr_code",
        "qq_number",
        "qq_group",
        "wechat_group_qr_code",
    ] {
        if let Some(v) = cfg
            .support
            .get(key)
            .and_then(Value::as_str)
            .filter(|v| !v.trim().is_empty())
        {
            contact.insert(key.into(), Value::String(v.to_owned()));
        }
    }
    out.insert("contact".into(), Value::Object(contact));
    if non_empty_object(&cfg.seo) {
        out.insert("seo".into(), cfg.seo.clone());
    }
    apply_announcement(&mut out, &cfg.announcement);
    if non_empty_object(&cfg.footer_links) {
        out.insert(
            "footer_links".into(),
            Value::Array(footer_items(&cfg.footer_links)),
        );
    }
    if non_empty_object(&cfg.nav_config) {
        out.insert("nav_config".into(), cfg.nav_config.clone());
    }
    if let Some(visual) = cfg.theme.as_object().filter(|visual| !visual.is_empty()) {
        if let Some(template) = visual
            .get("storefront_template")
            .and_then(Value::as_str)
            .filter(|v| matches!(*v, "classic" | "vault" | "aurora" | "atelier" | "noir"))
        {
            out.insert("storefront_template".into(), json!(template));
            // The selected template supplies its own palette and default colour mode.
            let mut base_theme = out
                .remove("theme")
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            for key in THEME_COLORS {
                base_theme.remove(key);
            }
            base_theme.insert(
                "default_mode".into(),
                json!(if matches!(template, "vault" | "noir") {
                    "dark"
                } else if template == "classic" {
                    "system"
                } else {
                    "light"
                }),
            );
            base_theme.insert("effects".into(), json!({"sakura": template == "classic", "sparkle": matches!(template, "classic" | "aurora")}));
            out.insert("theme".into(), Value::Object(base_theme));
        }
        if let Some(mode) = visual
            .get("template_mode")
            .and_then(Value::as_str)
            .filter(|v| matches!(*v, "card" | "list"))
        {
            out.insert("template_mode".into(), json!(mode));
        }
        let mut base_theme = out
            .remove("theme")
            .and_then(|v| v.as_object().cloned())
            .unwrap_or_default();
        for key in THEME_COLORS {
            if let Some(color) = visual.get(key).and_then(Value::as_str) {
                base_theme.insert(key.into(), json!(color));
            }
        }
        out.insert("theme".into(), Value::Object(base_theme));
    }
    Value::Object(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::rules::fixtures::at;

    fn input() -> SiteConfigInput {
        serde_json::from_value(json!({
            "site_name": "  My Shop  ",
            "logo": "/uploads/reseller/a.png",
            "favicon": "https://cdn.example.test/f.ico",
            "announcement": {"enabled": true, "type": "danger", "title": {"zh-CN": "T"}, "content": {"zh-CN": "<p>x</p>"}},
            "support": {"telegram": "https://t.me/shop", "email": "mailto:help@shop.test", "support_url": "https://help.shop.test"},
            "seo": {"title": {"en-US": "S"}},
            "footer_links": [{"name": {"zh-CN": "A"}, "url": "https://a.test"}, {"name": {}, "url": ""}],
            "nav_config": {"builtin": {"blog": false}, "custom_items": [{"name": {"zh-CN": "B"}, "url": "tg://b"}]}
        }))
        .unwrap_or_default()
    }

    #[test]
    fn normalizes_site_config() {
        let draft = normalize_site_config(&input()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(draft.site_name, "My Shop");
        assert_eq!(draft.announcement["type"], "info");
        assert_eq!(draft.announcement["title"]["zh-TW"], "");
        assert_eq!(draft.support["email"], "help@shop.test");
        assert_eq!(footer_items(&draft.footer_links).len(), 1);
        assert_eq!(
            draft.nav_config["builtin"],
            json!({"blog": false, "notice": true, "about": true})
        );
        assert_eq!(draft.nav_config["custom_items"][0]["url"], "tg://b");
        assert_eq!(draft.theme, json!({}));
    }

    #[test]
    fn reseller_visual_overrides_are_validated_and_isolated() {
        let mut input = input();
        input.storefront_template = "noir".into();
        input.template_mode = "list".into();
        input.theme.primary_color = "#A1b2C3".into();
        let draft = normalize_site_config(&input).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(draft.theme["primary_color"], "#a1b2c3");
        let mut reseller = site(json!({"enabled": false}));
        reseller.theme = draft.theme;
        let mut main = base();
        main["storefront_template"] = json!("classic");
        main["template_mode"] = json!("card");
        main["theme"] = json!({"primary_color": "#ff5fa2", "secondary_color": "#8b5cf6", "background_image": "/main.png", "default_mode": "light"});
        let out = overlay_public_config(main.clone(), "r.test", "r.test", Some(&reseller));
        assert_eq!(out["storefront_template"], "noir");
        assert_eq!(out["template_mode"], "list");
        assert_eq!(out["theme"]["primary_color"], "#a1b2c3");
        assert!(out["theme"].get("secondary_color").is_none());
        assert_eq!(out["theme"]["background_image"], "/main.png");
        assert_eq!(out["theme"]["default_mode"], "dark");
        reseller.theme = json!({});
        assert_eq!(
            overlay_public_config(main.clone(), "r.test", "r.test", Some(&reseller))["theme"],
            main["theme"]
        );
        assert_eq!(
            overlay_public_config(main.clone(), "r.test", "r.test", Some(&reseller))["storefront_template"],
            "classic"
        );
        input.storefront_template = "invalid".into();
        assert_eq!(
            normalize_site_config(&input)
                .err()
                .map(|e| e.key().to_owned()),
            Some(keys::SITE_CONFIG_INVALID.into())
        );
        input.storefront_template = String::new();
        input.theme.primary_color = "red;display:none".into();
        assert_eq!(
            normalize_site_config(&input)
                .err()
                .map(|e| e.key().to_owned()),
            Some(keys::SITE_CONFIG_INVALID.into())
        );
    }

    #[test]
    fn field_errors_have_specific_keys() {
        let cases: [(&str, Value, &str); 7] = [
            (
                "logo",
                json!({"logo": "ftp://x.test/a.png"}),
                keys::IMAGE_INVALID,
            ),
            (
                "telegram",
                json!({"support": {"telegram": "https://evil.test"}}),
                keys::SUPPORT_TELEGRAM_INVALID,
            ),
            (
                "whatsapp",
                json!({"support": {"whatsapp": "http://wa.me/1"}}),
                keys::SUPPORT_WHATSAPP_INVALID,
            ),
            (
                "email",
                json!({"support": {"email": "not-an-email"}}),
                keys::SUPPORT_EMAIL_INVALID,
            ),
            (
                "url",
                json!({"support": {"support_url": "http://plain.test"}}),
                keys::SUPPORT_URL_INVALID,
            ),
            (
                "seo image",
                json!({"seo": {"default_og_image": "javascript:alert(1)"}}),
                keys::IMAGE_INVALID,
            ),
            (
                "link",
                json!({"footer_links": [{"url": "javascript:alert(1)"}]}),
                keys::LINK_INVALID,
            ),
        ];
        for (name, body, key) in cases {
            let input: SiteConfigInput = serde_json::from_value(body).unwrap_or_default();
            assert_eq!(
                normalize_site_config(&input).unwrap_err().key(),
                key,
                "{name}"
            );
        }
    }

    fn site(announcement: Value) -> SiteConfig {
        SiteConfig {
            id: 1,
            reseller_id: 2,
            site_name: "R Shop".into(),
            logo: String::new(),
            favicon: "/uploads/f.ico".into(),
            announcement,
            support: json!({"telegram": "https://t.me/r", "email": ""}),
            seo: json!({}),
            footer_links: json!({"items": [{"url": "https://a.test"}]}),
            nav_config: json!({"builtin": {"blog": false}}),
            theme: json!({}),
            scripts: json!([]),
            created_at: at(0),
            updated_at: at(0),
            profile: None,
        }
    }

    fn base() -> Value {
        json!({
            "brand": {"site_name": "Main", "site_logo": "/m.png", "site_logo_dark": "/m-dark.png"},
            "contact": {"telegram": "https://t.me/main", "whatsapp": "https://wa.me/1"},
            "announcement": {"type": "info", "version": "main0000", "content": {"zh-CN": "main"}},
            "nav_config": {"builtin": {"blog": true}},
        })
    }

    #[test]
    fn custom_reseller_logo_does_not_inherit_main_dark_logo() {
        let mut reseller = site(json!({"enabled": false}));
        reseller.logo = "/r.png".into();
        let out = overlay_public_config(base(), "r.test", "r.test", Some(&reseller));
        assert_eq!(out["brand"]["site_logo"], "/r.png");
        assert!(out["brand"].get("site_logo_dark").is_none());
    }

    // RSL-08: disabled reseller announcement removes the main-site one.
    #[test]
    fn rsl08_disabled_announcement_does_not_leak() {
        let out = overlay_public_config(
            base(),
            "r.test",
            "r.test",
            Some(&site(json!({"enabled": false, "content": {"zh-CN": "x"}}))),
        );
        assert!(out.get("announcement").is_none());
        assert_eq!(
            out["tenant"],
            json!({"mode": "reseller", "host": "r.test", "primary_domain": "r.test"})
        );
        assert_eq!(out["brand"]["site_name"], "R Shop");
        assert_eq!(out["brand"]["site_logo"], "/m.png");
        assert_eq!(out["brand"]["site_icon"], "/uploads/f.ico");
        assert_eq!(out["brand"]["site_url"], "https://r.test");
        assert_eq!(out["contact"]["telegram"], "https://t.me/r");
        assert!(out["contact"].get("whatsapp").is_none());
        assert_eq!(out["footer_links"], json!([{"url": "https://a.test"}]));
        assert_eq!(out["nav_config"], json!({"builtin": {"blog": false}}));
    }

    // RSL-08: enabled announcement uses the public shape with its own fingerprint.
    #[test]
    fn rsl08_enabled_announcement_shape() {
        let stored = json!({"enabled": true, "type": "success", "title": {"zh-CN": "T"}, "content": {"zh-CN": "<p>测试测试</p>"}});
        let out = overlay_public_config(base(), "r.test", "r.test", Some(&site(stored)));
        let a = &out["announcement"];
        assert_eq!(a["type"], "success");
        assert!(a.get("enabled").is_none());
        assert_eq!(a["content"]["zh-CN"], "<p>测试测试</p>");
        let version = a["version"].as_str().unwrap_or_default();
        assert_eq!(version.len(), 8);
        assert_ne!(version, "main0000");
        // empty content → no field
        let out = overlay_public_config(
            base(),
            "r.test",
            "r.test",
            Some(&site(
                json!({"enabled": "true", "content": {"zh-CN": "  "}}),
            )),
        );
        assert!(out.get("announcement").is_none());
    }

    #[test]
    fn overlay_without_site_config_keeps_main_branding() {
        let out = overlay_public_config(base(), "", "p.test", None);
        assert_eq!(out["brand"]["site_name"], "Main");
        assert_eq!(out["brand"]["site_url"], "https://p.test");
        assert_eq!(out["contact"], json!({}));
        assert_eq!(out["announcement"]["version"], "main0000");
    }

    #[test]
    fn reseller_scripts_include_global_and_own_only() {
        let mut base = base();
        base["scripts"] = json!([
            {"name": "global", "scope": "global", "code": "a()"},
            {"name": "main", "scope": "main", "code": "b()"},
            {"name": "legacy", "code": "legacy()"},
        ]);
        let without_config = overlay_public_config(base.clone(), "r.test", "r.test", None);
        assert_eq!(without_config["scripts"].as_array().unwrap().len(), 1);
        assert_eq!(without_config["scripts"][0]["name"], "global");

        let mut reseller = site(json!({}));
        reseller.scripts = json!([{"name": "own", "scope": "reseller", "code": "c()"}]);
        let with_config = overlay_public_config(base, "r.test", "r.test", Some(&reseller));
        assert_eq!(with_config["scripts"].as_array().unwrap().len(), 2);
        assert_eq!(with_config["scripts"][1]["name"], "own");
    }

    #[test]
    fn fnv_fingerprint_is_stable() {
        // FNV-1a 32 of the empty input is the offset basis.
        let v = announcement_version("", &json!({}), &json!({}));
        assert_eq!(v.len(), 8);
        assert_ne!(v, announcement_version("info", &json!({}), &json!({})));
        assert_eq!(
            announcement_version("info", &json!({"zh-CN": "a"}), &json!({})),
            announcement_version("info", &json!({"zh-CN": "a"}), &json!({}))
        );
    }

    #[test]
    fn mail_and_url_helpers() {
        assert!(is_mail_address("a@b.test"));
        assert!(is_mail_address("Name <a@b.test>"));
        assert!(!is_mail_address("a b@c.test"));
        assert!(!is_mail_address("abc"));
        assert_eq!(
            support_url("mailto:a@b.test"),
            Some("mailto:a@b.test".into())
        );
        assert_eq!(support_url("https://"), None);
        assert_eq!(
            http_or_upload("http://x.test/a.png"),
            Some("http://x.test/a.png".into())
        );
    }
}
