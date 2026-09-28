//! Home page banners.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use zs_shared::i18n::LOCALES;
use zs_shared::page::{Page, PageRequest};

use crate::{Error, Id, Result};

pub const POSITION_HOME_HERO: &str = "home_hero";
pub const LINK_NONE: &str = "none";
pub const LINK_INTERNAL: &str = "internal";
pub const LINK_EXTERNAL: &str = "external";
const INVALID: &str = "error.banner_invalid";

/// A banner as returned by the admin API (`models.Banner`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Banner {
    pub id: Id,
    pub name: String,
    pub position: String,
    pub title: Value,
    pub subtitle: Value,
    pub image: String,
    pub mobile_image: String,
    pub link_type: String,
    pub link_value: String,
    pub open_in_new_tab: bool,
    pub is_active: bool,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Admin create/update payload.
#[derive(Debug, Clone, Default)]
pub struct BannerInput {
    pub name: String,
    pub position: String,
    pub title: Option<Value>,
    pub subtitle: Option<Value>,
    pub image: String,
    pub mobile_image: String,
    pub link_type: String,
    pub link_value: String,
    pub open_in_new_tab: Option<bool>,
    pub is_active: Option<bool>,
    pub start_at: Option<DateTime<Utc>>,
    pub end_at: Option<DateTime<Utc>>,
    pub sort_order: i32,
}

/// Admin list filter.
#[derive(Debug, Clone)]
pub struct BannerQuery {
    pub page: PageRequest,
    pub position: String,
    pub search: String,
    pub is_active: Option<bool>,
}

/// Only `home_hero` exists; anything else maps to it.
pub fn normalize_position(_raw: &str) -> String {
    POSITION_HOME_HERO.to_owned()
}

fn normalize_link_type(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | LINK_NONE => Some(LINK_NONE),
        LINK_INTERNAL => Some(LINK_INTERNAL),
        LINK_EXTERNAL => Some(LINK_EXTERNAL),
        _ => None,
    }
}

/// Live QA I-18: external links must be `http(s)://…`, internal links must be
/// site-relative; `javascript:`/`data:` (also hidden behind whitespace or control
/// characters, which browsers ignore) and protocol-relative `//host` are refused.
fn is_safe_link(link_type: &str, value: &str) -> bool {
    let squashed: String = value
        .chars()
        .filter(|c| *c > ' ' && *c != '\u{7f}')
        .flat_map(char::to_lowercase)
        .collect();
    if squashed.is_empty() {
        return false;
    }
    let scheme = squashed.split_once(':').map(|(s, _)| s).filter(|s| {
        s.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if link_type == LINK_EXTERNAL {
        matches!(scheme, Some("http" | "https")) && squashed.contains("://")
    } else {
        scheme.is_none() && !squashed.starts_with("//") && !squashed.starts_with("\\")
    }
}

/// `{zh-CN, zh-TW, en-US}` with trimmed strings; non-strings become `""`.
pub fn normalize_multi_lang(raw: Option<&Value>) -> Value {
    let mut out = Map::new();
    for locale in LOCALES {
        let v = raw
            .and_then(|r| r.get(locale))
            .and_then(Value::as_str)
            .map(|s| s.trim().to_owned())
            .unwrap_or_default();
        out.insert(locale.to_owned(), Value::String(v));
    }
    Value::Object(out)
}

impl BannerInput {
    /// Validates the input and applies it onto `existing` (or a new banner).
    pub fn build(self, existing: Option<Banner>, now: DateTime<Utc>) -> Result<Banner> {
        let name = self.name.trim().to_owned();
        let image = self.image.trim().to_owned();
        if name.is_empty() || image.is_empty() {
            return Err(Error::bad_request(INVALID));
        }
        let link_type =
            normalize_link_type(&self.link_type).ok_or_else(|| Error::bad_request(INVALID))?;
        if let (Some(s), Some(e)) = (self.start_at, self.end_at)
            && e < s
        {
            return Err(Error::bad_request(INVALID));
        }
        let link_value = if link_type == LINK_NONE {
            String::new()
        } else {
            self.link_value.trim().to_owned()
        };
        if link_type != LINK_NONE && !is_safe_link(link_type, &link_value) {
            return Err(Error::bad_request(INVALID));
        }
        let is_new = existing.is_none();
        let mut b = existing.unwrap_or(Banner {
            id: 0,
            name: String::new(),
            position: String::new(),
            title: Value::Null,
            subtitle: Value::Null,
            image: String::new(),
            mobile_image: String::new(),
            link_type: String::new(),
            link_value: String::new(),
            open_in_new_tab: false,
            is_active: true,
            start_at: None,
            end_at: None,
            sort_order: 0,
            created_at: now,
            updated_at: now,
        });
        b.name = name;
        b.position = normalize_position(&self.position);
        b.title = normalize_multi_lang(self.title.as_ref());
        b.subtitle = normalize_multi_lang(self.subtitle.as_ref());
        b.image = image;
        b.mobile_image = self.mobile_image.trim().to_owned();
        b.link_type = link_type.to_owned();
        b.link_value = link_value;
        b.start_at = self.start_at;
        b.end_at = self.end_at;
        b.sort_order = self.sort_order;
        if let Some(v) = self.open_in_new_tab {
            b.open_in_new_tab = v;
        }
        match self.is_active {
            Some(v) => b.is_active = v,
            None if is_new => b.is_active = true,
            None => {}
        }
        b.updated_at = now;
        Ok(b)
    }
}

/// Persistence port for banners (ordered `sort_order DESC, created_at DESC`).
#[async_trait]
pub trait BannerRepo: Send + Sync {
    async fn list(&self, query: &BannerQuery) -> Result<Page<Banner>>;
    /// Active banners of `position` whose time window contains `now`.
    async fn list_valid(
        &self,
        position: &str,
        limit: u64,
        now: DateTime<Utc>,
    ) -> Result<Vec<Banner>>;
    async fn get(&self, id: Id) -> Result<Option<Banner>>;
    async fn create(&self, banner: &Banner) -> Result<Banner>;
    async fn update(&self, banner: &Banner) -> Result<()>;
    async fn delete(&self, id: Id) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input() -> BannerInput {
        BannerInput {
            name: " Hero ".into(),
            image: "/uploads/banner/a.png".into(),
            ..BannerInput::default()
        }
    }

    #[test]
    fn builds_with_defaults() {
        let b = BannerInput {
            title: Some(json!({"zh-CN": " 标题 ", "en-US": 5})),
            link_value: "ignored".into(),
            position: "sidebar".into(),
            ..input()
        }
        .build(None, Utc::now())
        .unwrap();
        assert_eq!(b.name, "Hero");
        assert_eq!(b.position, "home_hero");
        assert_eq!(b.link_type, "none");
        assert_eq!(b.link_value, "");
        assert!(b.is_active);
        assert_eq!(b.title, json!({"zh-CN": "标题", "zh-TW": "", "en-US": ""}));
    }

    #[test]
    fn rejects_invalid_links_and_windows() {
        let bad_type = BannerInput {
            link_type: "javascript".into(),
            ..input()
        };
        assert_eq!(bad_type.build(None, Utc::now()).unwrap_err().key(), INVALID);
        let missing_value = BannerInput {
            link_type: "external".into(),
            ..input()
        };
        assert!(missing_value.build(None, Utc::now()).is_err());
        // QA-A18 (live QA I-18): script and protocol-relative links are refused
        for (kind, value) in [
            ("external", "javascript:alert(1)"),
            ("external", " JaVa\tScript:alert(1)"),
            ("external", "data:text/html,x"),
            ("external", "ftp://x"),
            ("internal", "javascript:alert(1)"),
            ("internal", "//evil.example"),
        ] {
            let bad = BannerInput {
                link_type: kind.into(),
                link_value: value.into(),
                ..input()
            };
            assert!(bad.build(None, Utc::now()).is_err(), "{kind} {value}");
        }
        for (kind, value) in [
            ("external", "https://example.com/a?b=c:d"),
            ("internal", "/products/abc"),
            ("internal", "products/abc?x=1:2"),
        ] {
            let good = BannerInput {
                link_type: kind.into(),
                link_value: value.into(),
                ..input()
            };
            assert!(good.build(None, Utc::now()).is_ok(), "{kind} {value}");
        }
        let now = Utc::now();
        let window = BannerInput {
            start_at: Some(now),
            end_at: Some(now - chrono::Duration::hours(1)),
            ..input()
        };
        assert!(window.build(None, now).is_err());
        assert!(
            BannerInput {
                image: " ".into(),
                ..input()
            }
            .build(None, now)
            .is_err()
        );
    }

    #[test]
    fn update_keeps_active_flag_when_absent() {
        let now = Utc::now();
        let mut existing = input().build(None, now).unwrap();
        existing.is_active = false;
        let b = input().build(Some(existing), now).unwrap();
        assert!(!b.is_active);
    }
}
