//! Multi-language text stored as a JSON object keyed by locale.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Default and fallback locale of the storefront and admin panel.
pub const DEFAULT_LOCALE: &str = "zh-CN";

/// Every locale the product ships language packs for.
pub const LOCALES: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];

/// Localized text such as `{"zh-CN": "…", "en-US": "…"}`.
///
/// Unknown keys are preserved so that round-tripping never loses data.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LocalizedText(BTreeMap<String, String>);

impl LocalizedText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates text with a single value for [`DEFAULT_LOCALE`].
    pub fn single(value: impl Into<String>) -> Self {
        let mut map = BTreeMap::new();
        map.insert(DEFAULT_LOCALE.to_owned(), value.into());
        Self(map)
    }

    /// Returns the text for `locale`, falling back to zh-CN, then to any non-empty value.
    pub fn resolve(&self, locale: &str) -> &str {
        self.non_empty(locale)
            .or_else(|| self.non_empty(DEFAULT_LOCALE))
            .or_else(|| {
                self.0
                    .values()
                    .find(|v| !v.trim().is_empty())
                    .map(String::as_str)
            })
            .unwrap_or("")
    }

    pub fn get(&self, locale: &str) -> Option<&str> {
        self.0.get(locale).map(String::as_str)
    }

    pub fn set(&mut self, locale: impl Into<String>, value: impl Into<String>) {
        self.0.insert(locale.into(), value.into());
    }

    /// True when every locale value is blank.
    pub fn is_blank(&self) -> bool {
        self.0.values().all(|v| v.trim().is_empty())
    }

    /// Returns a copy with every known locale present (missing ones empty).
    pub fn normalized(&self) -> Self {
        let mut map = self.0.clone();
        for locale in LOCALES {
            map.entry(locale.to_owned()).or_default();
        }
        Self(map)
    }

    fn non_empty(&self, locale: &str) -> Option<&str> {
        self.0
            .get(locale)
            .map(String::as_str)
            .filter(|v| !v.trim().is_empty())
    }
}

impl From<BTreeMap<String, String>> for LocalizedText {
    fn from(value: BTreeMap<String, String>) -> Self {
        Self(value)
    }
}

/// Normalizes a user-supplied locale into one of [`LOCALES`].
pub fn normalize_locale(raw: &str) -> &'static str {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.starts_with("zh-tw") || lower.starts_with("zh-hk") || lower.starts_with("zh-mo") {
        "zh-TW"
    } else if lower.starts_with("en") {
        "en-US"
    } else {
        DEFAULT_LOCALE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_falls_back_to_default_locale() {
        let mut text = LocalizedText::single("你好");
        text.set("en-US", "");
        assert_eq!(text.resolve("en-US"), "你好");
        text.set("en-US", "Hello");
        assert_eq!(text.resolve("en-US"), "Hello");
    }

    #[test]
    fn normalizes_browser_locales() {
        assert_eq!(normalize_locale("en"), "en-US");
        assert_eq!(normalize_locale("zh-HK"), "zh-TW");
        assert_eq!(normalize_locale("ja"), "zh-CN");
    }
}
