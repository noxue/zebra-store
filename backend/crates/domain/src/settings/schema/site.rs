//! `site_config`, `nav_config`, `registration_config`, `order_config`,
//! `payment_config` and `wallet_config` normalizers (port of `site_normalize.go`
//! and `general.go`), plus the Zebra Store `site_config.theme` object.

use serde::Serialize;
use serde_json::{Value, json};
use zs_shared::i18n::LOCALES;

use super::value::{
    Obj, as_obj, localized_block, localized_field, localized_is_blank, parse_bool, parse_int, text,
    text_limit, truncate_chars,
};

/// Default site currency.
pub const CURRENCY_DEFAULT: &str = "CNY";
/// Default storefront template.
pub const STOREFRONT_TEMPLATE_DEFAULT: &str = "classic";
const STOREFRONT_TEMPLATE_VAULT: &str = "vault";

// Limits from the original `site_normalize.go`.
const SCRIPTS_MAX: usize = 20;
const SCRIPT_NAME_MAX: usize = 120;
const SCRIPT_CODE_MAX: usize = 20000;
const FOOTER_LINKS_MAX: usize = 20;
const FOOTER_LINK_NAME_MAX: usize = 120;
const FOOTER_LINK_URL_MAX: usize = 2000;
const NAV_ITEMS_MAX: usize = 10;
const NAV_TITLE_MAX: usize = 120;
const NAV_URL_MAX: usize = 2000;
const EMAIL_DOMAINS_MAX: usize = 100;
const EMAIL_DOMAIN_MAX_LEN: usize = 253;
const ABOUT_SERVICES_MAX: usize = 12;

/// Normalizes the whole `site_config` object; unknown keys are kept.
pub fn normalize_site(value: &Value) -> Value {
    let input = value.as_object().cloned().unwrap_or_default();
    let mut out = input.clone();
    out.insert("brand".into(), normalize_brand(input.get("brand")));
    out.insert("contact".into(), normalize_contact(input.get("contact")));
    out.insert(
        "seo".into(),
        localized_block(input.get("seo"), &["title", "keywords", "description"]),
    );
    out.insert(
        "legal".into(),
        localized_block(input.get("legal"), &["terms", "privacy"]),
    );
    out.insert("about".into(), normalize_about(input.get("about")));
    out.insert("scripts".into(), normalize_scripts(input.get("scripts")));
    out.insert(
        "footer_links".into(),
        normalize_footer_links(input.get("footer_links")),
    );
    out.insert(
        "currency".into(),
        Value::String(normalize_currency(input.get("currency"))),
    );
    out.insert(
        "template_mode".into(),
        Value::String(normalize_template_mode(input.get("template_mode"))),
    );
    out.insert(
        "storefront_template".into(),
        Value::String(normalize_storefront_template(
            input.get("storefront_template"),
        )),
    );
    out.insert("theme".into(), normalize_theme(input.get("theme")));
    if let Some(raw) = input.get("languages") {
        out.insert("languages".into(), json!(normalize_languages(raw)));
    }
    Value::Object(out)
}

fn normalize_brand(raw: Option<&Value>) -> Value {
    let b = as_obj(raw);
    let get = |k: &str| text(b.and_then(|o| o.get(k)));
    // site_logo (navbar/footer) stays independent of the favicon site_icon.
    json!({
        "site_name": get("site_name"),
        "site_url": get("site_url").trim_end_matches('/'),
        "site_icon": get("site_icon"),
        "site_logo": get("site_logo"),
        "site_description": localized_field(b.and_then(|o| o.get("site_description"))),
    })
}

fn normalize_contact(raw: Option<&Value>) -> Value {
    let c = as_obj(raw);
    json!({
        "telegram": text(c.and_then(|o| o.get("telegram"))),
        "whatsapp": text(c.and_then(|o| o.get("whatsapp"))),
    })
}

fn normalize_about(raw: Option<&Value>) -> Value {
    let a = as_obj(raw);
    let get = |k: &str| a.and_then(|o| o.get(k));
    let services = as_obj(get("services"));
    let contact = as_obj(get("contact"));
    json!({
        "hero": localized_block(get("hero"), &["title", "subtitle"]),
        "introduction": localized_field(get("introduction")),
        "services": {
            "title": localized_field(services.and_then(|o| o.get("title"))),
            "items": localized_list(services.and_then(|o| o.get("items")), ABOUT_SERVICES_MAX),
        },
        "contact": {
            "title": localized_field(contact.and_then(|o| o.get("title"))),
            "text": localized_field(contact.and_then(|o| o.get("text"))),
        },
    })
}

fn localized_list(raw: Option<&Value>, max: usize) -> Value {
    let items = raw.and_then(Value::as_array).cloned().unwrap_or_default();
    let list: Vec<Value> = items
        .iter()
        .map(|i| localized_field(Some(i)))
        .filter(|v| !localized_is_blank(v))
        .take(max)
        .collect();
    Value::Array(list)
}

fn normalize_scripts(raw: Option<&Value>) -> Value {
    let items = raw.and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for item in items.iter().filter_map(Value::as_object) {
        let code = text_limit(item.get("code"), SCRIPT_CODE_MAX);
        if code.is_empty() {
            continue;
        }
        let position = match text(item.get("position")).as_str() {
            "body_end" => "body_end",
            _ => "head",
        };
        out.push(json!({
            "name": text_limit(item.get("name"), SCRIPT_NAME_MAX),
            "enabled": parse_bool(item.get("enabled")),
            "position": position,
            "code": code,
        }));
        if out.len() >= SCRIPTS_MAX {
            break;
        }
    }
    Value::Array(out)
}

fn normalize_footer_links(raw: Option<&Value>) -> Value {
    let items = raw.and_then(Value::as_array).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for item in items.iter().filter_map(Value::as_object) {
        let name = text_limit(item.get("name"), FOOTER_LINK_NAME_MAX);
        if name.is_empty() {
            continue;
        }
        out.push(json!({
            "name": name,
            "url": text_limit(item.get("url"), FOOTER_LINK_URL_MAX),
        }));
        if out.len() >= FOOTER_LINKS_MAX {
            break;
        }
    }
    Value::Array(out)
}

/// A three-letter upper-case currency code, defaulting to CNY.
pub fn normalize_currency(raw: Option<&Value>) -> String {
    let currency = text(raw).to_ascii_uppercase();
    if is_currency_code(&currency) {
        currency
    } else {
        CURRENCY_DEFAULT.to_owned()
    }
}

/// True for a three-letter currency code (case-insensitive).
pub fn is_currency_code(value: &str) -> bool {
    let v = value.trim();
    v.len() == 3 && v.chars().all(|c| c.is_ascii_alphabetic())
}

fn normalize_template_mode(raw: Option<&Value>) -> String {
    if text(raw) == "list" { "list" } else { "card" }.to_owned()
}

fn normalize_storefront_template(raw: Option<&Value>) -> String {
    if text(raw) == STOREFRONT_TEMPLATE_VAULT {
        STOREFRONT_TEMPLATE_VAULT
    } else {
        STOREFRONT_TEMPLATE_DEFAULT
    }
    .to_owned()
}

fn normalize_languages(raw: &Value) -> Vec<String> {
    let Some(items) = raw.as_array() else {
        return default_languages();
    };
    let mut out: Vec<String> = Vec::new();
    for lang in items.iter().map(|i| text(Some(i))) {
        if !lang.is_empty() && !out.contains(&lang) {
            out.push(lang);
        }
    }
    if out.is_empty() {
        default_languages()
    } else {
        out
    }
}

/// Every supported locale, in fallback order.
pub fn default_languages() -> Vec<String> {
    LOCALES.iter().map(|l| (*l).to_owned()).collect()
}

// ---------------------------------------------------------------------------
// Theme (Zebra Store extension, see docs/DESIGN.md "后台可配置项")
// ---------------------------------------------------------------------------

/// Default theme colours (樱花粉 / 梦幻紫 / 天空蓝).
pub const THEME_PRIMARY_DEFAULT: &str = "#ff5fa2";
pub const THEME_SECONDARY_DEFAULT: &str = "#8b5cf6";
pub const THEME_ACCENT_DEFAULT: &str = "#38bdf8";
/// Longest accepted theme image URL.
const THEME_URL_MAX: usize = 2000;

/// Normalizes `site_config.theme`: hex colours, safe image URLs, effect toggles, mode.
pub fn normalize_theme(raw: Option<&Value>) -> Value {
    let t = as_obj(raw);
    let get = |k: &str| t.and_then(|o| o.get(k));
    let effects = as_obj(get("effects"));
    let effect = |k: &str| match effects.and_then(|o| o.get(k)) {
        Some(Value::Bool(b)) => *b,
        Some(v @ (Value::String(_) | Value::Number(_))) => parse_bool(Some(v)),
        _ => true,
    };
    let mode = match text(get("default_mode")).as_str() {
        m @ ("light" | "dark") => m.to_owned(),
        _ => "system".to_owned(),
    };
    json!({
        "primary_color": hex_color(get("primary_color"), THEME_PRIMARY_DEFAULT),
        "secondary_color": hex_color(get("secondary_color"), THEME_SECONDARY_DEFAULT),
        "accent_color": hex_color(get("accent_color"), THEME_ACCENT_DEFAULT),
        "background_image": safe_url(get("background_image")),
        "mascot_image": safe_url(get("mascot_image")),
        "login_background": safe_url(get("login_background")),
        "effects": { "sakura": effect("sakura"), "sparkle": effect("sparkle") },
        "default_mode": mode,
    })
}

/// `#rgb` or `#rrggbb` (lower-cased), otherwise `fallback`.
pub fn hex_color(raw: Option<&Value>, fallback: &str) -> String {
    let v = text(raw).to_ascii_lowercase();
    let valid = v.strip_prefix('#').is_some_and(|hex| {
        matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
    });
    if valid { v } else { fallback.to_owned() }
}

/// An `http(s)://` or site-relative (`/…`) URL; anything else (e.g. `javascript:`) becomes `""`.
pub fn safe_url(raw: Option<&Value>) -> String {
    let v = text(raw);
    if v.chars()
        .any(|c| c.is_control() || c == '"' || c == '<' || c == '>')
    {
        return String::new();
    }
    let lower = v.to_ascii_lowercase();
    let ok = lower.starts_with("https://")
        || lower.starts_with("http://")
        || (v.starts_with('/') && !v.starts_with("//"));
    if ok {
        truncate_chars(&v, THEME_URL_MAX)
    } else {
        String::new()
    }
}

// ---------------------------------------------------------------------------
// nav_config
// ---------------------------------------------------------------------------

/// Default `nav_config` used when none is stored.
pub fn default_nav() -> Value {
    json!({"builtin": {"blog": true, "notice": true, "about": true}, "custom_items": []})
}

/// FE-06: external links must be `http(s)://`, internal ones a site path (`/…`, never the
/// protocol-relative `//host`); anything else (e.g. `javascript:`) is dropped.
fn nav_url_allowed(link_type: &str, url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    if link_type == "external" {
        return lower.starts_with("https://") || lower.starts_with("http://");
    }
    lower.starts_with('/') && !lower.starts_with("//") && !lower.starts_with("/\\")
}

/// Normalizes `nav_config` (builtin toggles + up to 10 custom items).
pub fn normalize_nav(value: &Value) -> Value {
    let input = value.as_object();
    let mut builtin = Obj::new();
    let raw_builtin = as_obj(input.and_then(|o| o.get("builtin")));
    for key in ["blog", "notice", "about"] {
        let enabled = raw_builtin
            .and_then(|b| b.get(key))
            .map(|v| parse_bool(Some(v)))
            .unwrap_or(true);
        builtin.insert(key.into(), Value::Bool(enabled));
    }

    let mut items = Vec::new();
    let raw_items = input
        .and_then(|o| o.get("custom_items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    for item in raw_items.iter().filter_map(Value::as_object) {
        let mut title = localized_field(item.get("title"));
        if localized_is_blank(&title) {
            continue;
        }
        if let Some(map) = title.as_object_mut() {
            for v in map.values_mut() {
                if let Some(s) = v.as_str() {
                    *v = Value::String(truncate_chars(s, NAV_TITLE_MAX));
                }
            }
        }
        let link_type = match text(item.get("link_type")).as_str() {
            "external" => "external",
            _ => "internal",
        };
        let target = match text(item.get("target")).as_str() {
            "_blank" => "_blank",
            _ => "_self",
        };
        let icon = match text(item.get("icon")) {
            s if s.is_empty() => "link".to_owned(),
            s => s,
        };
        let url = text_limit(item.get("url"), NAV_URL_MAX);
        if !nav_url_allowed(link_type, &url) {
            continue;
        }
        // The frontend-generated id is kept as-is (a JSON number).
        let id = item
            .get("id")
            .filter(|v| v.is_number())
            .cloned()
            .unwrap_or(json!(0));
        items.push(json!({
            "id": id,
            "title": title,
            "link_type": link_type,
            "url": url,
            "target": target,
            "sort_order": parse_int(item.get("sort_order")).unwrap_or(0),
            "enabled": parse_bool(item.get("enabled")),
            "icon": icon,
        }));
        if items.len() >= NAV_ITEMS_MAX {
            break;
        }
    }
    json!({"builtin": builtin, "custom_items": items})
}

// ---------------------------------------------------------------------------
// registration_config
// ---------------------------------------------------------------------------

/// Normalizes `registration_config`.
pub fn normalize_registration(value: &Value) -> Value {
    let o = value.as_object();
    let flag = |k: &str, default: bool| {
        o.and_then(|m| m.get(k))
            .map(|v| parse_bool(Some(v)))
            .unwrap_or(default)
    };
    json!({
        "registration_enabled": flag("registration_enabled", true),
        "email_verification_enabled": flag("email_verification_enabled", true),
        "email_domain_allowlist_enabled": flag("email_domain_allowlist_enabled", false),
        "allowed_email_domains": normalize_email_domains(o.and_then(|m| m.get("allowed_email_domains"))),
    })
}

/// Deduplicated, lower-cased, validated e-mail domains (max 100).
pub fn normalize_email_domains(raw: Option<&Value>) -> Vec<String> {
    let candidates: Vec<String> = match raw {
        Some(Value::String(s)) => s
            .split([',', '\n', '\r', '\t', ' '])
            .filter(|p| !p.is_empty())
            .map(str::to_owned)
            .collect(),
        Some(Value::Array(items)) => items.iter().map(|i| text(Some(i))).collect(),
        _ => return Vec::new(),
    };
    let mut out: Vec<String> = Vec::new();
    for candidate in candidates {
        let lower = candidate.trim().to_ascii_lowercase();
        let domain = lower.strip_prefix('@').unwrap_or(&lower).to_owned();
        if domain.is_empty() || domain.len() > EMAIL_DOMAIN_MAX_LEN || !is_valid_domain(&domain) {
            continue;
        }
        if out.contains(&domain) {
            continue;
        }
        out.push(domain);
        if out.len() >= EMAIL_DOMAINS_MAX {
            break;
        }
    }
    out
}

fn is_valid_domain(domain: &str) -> bool {
    if domain.contains("..") || !domain.contains('.') {
        return false;
    }
    domain.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

/// Registration e-mail domain allow-list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EmailDomainPolicy {
    pub enabled: bool,
    pub allowed_domains: Vec<String>,
}

/// Reads the domain policy from a stored `registration_config`.
pub fn email_domain_policy(value: Option<&Value>) -> EmailDomainPolicy {
    let o = value.and_then(Value::as_object);
    EmailDomainPolicy {
        enabled: o
            .and_then(|m| m.get("email_domain_allowlist_enabled"))
            .map(|v| parse_bool(Some(v)))
            .unwrap_or(false),
        allowed_domains: normalize_email_domains(o.and_then(|m| m.get("allowed_email_domains"))),
    }
}

/// Why an e-mail address is rejected at registration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmailRejection {
    Invalid,
    DomainNotAllowed,
}

/// Validates an e-mail against the registration domain policy.
pub fn check_email_domain(email: &str, policy: &EmailDomainPolicy) -> Result<(), EmailRejection> {
    let normalized = email.trim().to_ascii_lowercase();
    if normalized.is_empty()
        || normalized.contains('<')
        || !super::value::is_email_address(&normalized)
    {
        return Err(EmailRejection::Invalid);
    }
    if !policy.enabled {
        return Ok(());
    }
    let Some((_, domain)) = normalized.rsplit_once('@') else {
        return Err(EmailRejection::Invalid);
    };
    if policy.allowed_domains.iter().any(|d| d == domain) {
        Ok(())
    } else {
        Err(EmailRejection::DomainNotAllowed)
    }
}

// ---------------------------------------------------------------------------
// order_config / payment_config
// ---------------------------------------------------------------------------

const PAYMENT_EXPIRE_DEFAULT: i64 = 15;
const PAYMENT_EXPIRE_MIN: i64 = 1;
const PAYMENT_EXPIRE_MAX: i64 = 10080;
const REFUND_DAYS_DEFAULT: i64 = 30;
const REFUND_DAYS_MIN: i64 = 0;
const REFUND_DAYS_MAX: i64 = 3650;

/// Order timing configuration (`order_config`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OrderSetting {
    pub payment_expire_minutes: i64,
    /// `0` means refunds are not time-limited.
    pub max_refund_days: i64,
}

impl Default for OrderSetting {
    fn default() -> Self {
        Self {
            payment_expire_minutes: PAYMENT_EXPIRE_DEFAULT,
            max_refund_days: REFUND_DAYS_DEFAULT,
        }
    }
}

impl OrderSetting {
    /// Clamps both values into their accepted ranges.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        if self.payment_expire_minutes < PAYMENT_EXPIRE_MIN {
            self.payment_expire_minutes = PAYMENT_EXPIRE_DEFAULT;
        }
        self.payment_expire_minutes = self.payment_expire_minutes.min(PAYMENT_EXPIRE_MAX);
        if self.max_refund_days < REFUND_DAYS_MIN {
            self.max_refund_days = REFUND_DAYS_DEFAULT;
        }
        self.max_refund_days = self.max_refund_days.min(REFUND_DAYS_MAX);
        self
    }

    /// Builds the fallback from `config.yml` values (`defaultOrderConfigWithFallback`).
    pub fn from_config(payment_expire_minutes: i64, max_refund_days: i64) -> Self {
        let mut cfg = Self::default();
        if payment_expire_minutes > 0 {
            cfg.payment_expire_minutes = payment_expire_minutes;
        }
        if (REFUND_DAYS_MIN..=REFUND_DAYS_MAX).contains(&max_refund_days) {
            cfg.max_refund_days = max_refund_days;
        }
        cfg.normalized()
    }

    /// Overlays a stored `order_config` onto `fallback`.
    pub fn decode(raw: Option<&Value>, fallback: Self) -> Self {
        let mut out = fallback.normalized();
        if let Some(o) = raw.and_then(Value::as_object) {
            if let Some(v) = parse_int(o.get("payment_expire_minutes")) {
                out.payment_expire_minutes = v;
            }
            if let Some(v) = parse_int(o.get("max_refund_days")) {
                out.max_refund_days = v;
            }
        }
        out.normalized()
    }
}

/// Payment fee policy (`payment_config`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct PaymentFeeSetting {
    pub customer_fee_enabled: bool,
    pub reuse_legacy_order_fee_payment: bool,
}

impl PaymentFeeSetting {
    pub fn decode(raw: Option<&Value>) -> Self {
        let o = raw.and_then(Value::as_object);
        Self {
            customer_fee_enabled: parse_bool(o.and_then(|m| m.get("customer_fee_enabled"))),
            reuse_legacy_order_fee_payment: parse_bool(
                o.and_then(|m| m.get("reuse_legacy_order_fee_payment")),
            ),
        }
    }
}

/// Wallet flags read from `wallet_config`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalletSetting {
    pub wallet_only_payment: bool,
    pub recharge_channel_ids: Vec<i64>,
}

impl WalletSetting {
    pub fn decode(raw: Option<&Value>) -> Self {
        let o = raw.and_then(Value::as_object);
        let ids = o
            .and_then(|m| m.get("recharge_channel_ids"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_f64)
                    .filter(|f| *f > 0.0)
                    .map(|f| f as i64)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            wallet_only_payment: parse_bool(o.and_then(|m| m.get("wallet_only_payment"))),
            recharge_channel_ids: ids,
        }
    }
}

/// `brand.site_name` / `brand.site_url` of a stored `site_config`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SiteBrand {
    pub site_name: String,
    pub site_url: String,
}

impl SiteBrand {
    pub fn decode(site_config: Option<&Value>) -> Self {
        let brand = site_config.and_then(|v| as_obj(v.get("brand")));
        Self {
            site_name: text(brand.and_then(|b| b.get("site_name"))),
            site_url: text(brand.and_then(|b| b.get("site_url")))
                .trim_end_matches('/')
                .to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_normalization_fills_structure_and_limits() {
        let scripts: Vec<Value> = (0..25)
            .map(|i| json!({"name": format!("s{i}"), "code": "console.log(1)", "position": "x", "enabled": "true"}))
            .collect();
        let out = normalize_site(&json!({
            "brand": {"site_name": " Zebra ", "site_url": "https://a.com///", "site_logo": "/uploads/l.png"},
            "scripts": scripts,
            "footer_links": [{"name": "", "url": "x"}, {"name": "A", "url": " /a "}],
            "currency": "usd",
            "template_mode": "grid",
            "storefront_template": "vault",
            "custom": 1,
        }));
        assert_eq!(out["brand"]["site_name"], "Zebra");
        assert_eq!(out["brand"]["site_url"], "https://a.com");
        // CNT-11: logo is kept independent of the favicon
        assert_eq!(out["brand"]["site_logo"], "/uploads/l.png");
        assert_eq!(out["brand"]["site_icon"], "");
        assert_eq!(out["scripts"].as_array().unwrap().len(), SCRIPTS_MAX);
        assert_eq!(out["scripts"][0]["position"], "head");
        assert_eq!(out["scripts"][0]["enabled"], true);
        assert_eq!(out["footer_links"], json!([{"name": "A", "url": "/a"}]));
        assert_eq!(out["currency"], "USD");
        assert_eq!(out["template_mode"], "card");
        assert_eq!(out["storefront_template"], "vault");
        assert_eq!(out["custom"], 1);
        assert_eq!(out["seo"]["title"]["en-US"], "");
        assert!(out.get("languages").is_none());
        assert_eq!(out["theme"]["primary_color"], THEME_PRIMARY_DEFAULT);
    }

    #[test]
    fn currency_and_languages() {
        assert_eq!(normalize_currency(Some(&json!("us"))), "CNY");
        assert_eq!(normalize_currency(Some(&json!(" eur "))), "EUR");
        let out = normalize_site(&json!({"languages": ["en-US", " en-US", ""]}));
        assert_eq!(out["languages"], json!(["en-US"]));
        let out = normalize_site(&json!({"languages": "x"}));
        assert_eq!(out["languages"], json!(["zh-CN", "zh-TW", "en-US"]));
    }

    #[test]
    fn theme_rejects_bad_colors_and_script_urls() {
        let t = normalize_theme(Some(&json!({
            "primary_color": "#ABC",
            "secondary_color": "red",
            "accent_color": "#12345g",
            "background_image": "javascript:alert(1)",
            "mascot_image": "/uploads/m.png",
            "login_background": "//evil.com/x.png",
            "effects": {"sakura": false},
            "default_mode": "dark",
        })));
        assert_eq!(t["primary_color"], "#abc");
        assert_eq!(t["secondary_color"], THEME_SECONDARY_DEFAULT);
        assert_eq!(t["accent_color"], THEME_ACCENT_DEFAULT);
        assert_eq!(t["background_image"], "");
        assert_eq!(t["mascot_image"], "/uploads/m.png");
        assert_eq!(t["login_background"], "");
        assert_eq!(t["effects"], json!({"sakura": false, "sparkle": true}));
        assert_eq!(t["default_mode"], "dark");
        assert_eq!(normalize_theme(None)["default_mode"], "system");
    }

    #[test]
    fn nav_normalization() {
        let out = normalize_nav(&json!({
            "builtin": {"blog": false},
            "custom_items": [
                {"title": {"zh-CN": ""}},
                {"id": 17, "title": {"en-US": "Docs"}, "link_type": "external", "target": "_top", "url": "https://d", "sort_order": "3", "enabled": true},
            ]
        }));
        assert_eq!(
            out["builtin"],
            json!({"blog": false, "notice": true, "about": true})
        );
        let items = out["custom_items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["id"], 17);
        assert_eq!(items[0]["target"], "_self");
        assert_eq!(items[0]["link_type"], "external");
        assert_eq!(items[0]["sort_order"], 3);
        assert_eq!(items[0]["icon"], "link");
    }

    /// FE-06: unsafe or protocol-relative URLs are dropped; at most 10 items are kept.
    #[test]
    fn fe_06_nav_urls_are_restricted() {
        let item = |link_type: &str, url: &str| json!({"title": {"zh-CN": url}, "link_type": link_type, "url": url, "enabled": true});
        let out = normalize_nav(&json!({"custom_items": [
            item("external", "javascript:alert(1)"),
            item("external", " JavaScript:alert(1)"),
            item("internal", "//evil.com"),
            item("internal", "/\\evil.com"),
            item("internal", "javascript:alert(1)"),
            item("external", "data:text/html,x"),
            item("external", "HTTPS://docs.example.com"),
            item("internal", "/blog"),
        ]}));
        let urls: Vec<&str> = out["custom_items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| i["url"].as_str())
            .collect();
        assert_eq!(urls, ["HTTPS://docs.example.com", "/blog"]);
        let many: Vec<Value> = (0..11)
            .map(|i| item("internal", &format!("/p{i}")))
            .collect();
        let out = normalize_nav(&json!({"custom_items": many}));
        assert_eq!(out["custom_items"].as_array().unwrap().len(), 10);
    }

    #[test]
    fn registration_domains() {
        let out = normalize_registration(&json!({
            "email_domain_allowlist_enabled": "yes",
            "allowed_email_domains": "@Gmail.com, qq.com\nbad_domain.com -x.com gmail.com nodot",
        }));
        assert_eq!(out["registration_enabled"], true);
        assert_eq!(out["email_domain_allowlist_enabled"], true);
        assert_eq!(out["allowed_email_domains"], json!(["gmail.com", "qq.com"]));
        let policy = email_domain_policy(Some(&out));
        assert_eq!(check_email_domain("A@gmail.com", &policy), Ok(()));
        assert_eq!(
            check_email_domain("a@x.com", &policy),
            Err(EmailRejection::DomainNotAllowed)
        );
        assert_eq!(
            check_email_domain("bad", &policy),
            Err(EmailRejection::Invalid)
        );
    }

    #[test]
    fn order_setting_clamps() {
        let s = OrderSetting::decode(
            Some(&json!({"payment_expire_minutes": 0, "max_refund_days": 99999})),
            OrderSetting::default(),
        );
        assert_eq!(s.payment_expire_minutes, 15);
        assert_eq!(s.max_refund_days, 3650);
        assert_eq!(OrderSetting::from_config(30, 0).max_refund_days, 0);
    }
}
