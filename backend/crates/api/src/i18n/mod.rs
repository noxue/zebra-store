//! Translated API messages (ported verbatim from the original `internal/i18n/messages.go`).

use std::collections::HashMap;
use std::sync::OnceLock;

use axum::http::HeaderMap;
use zs_shared::i18n::DEFAULT_LOCALE;

type Bundle = HashMap<String, HashMap<String, String>>;

const MESSAGES_JSON: &str = include_str!("messages.json");

fn bundle() -> &'static Bundle {
    static BUNDLE: OnceLock<Bundle> = OnceLock::new();
    // The JSON is embedded at compile time and covered by tests, so parsing cannot fail at runtime.
    BUNDLE.get_or_init(|| serde_json::from_str(MESSAGES_JSON).unwrap_or_default())
}

/// Returns the message for `key`, falling back to zh-CN and finally the key itself.
pub fn translate(locale: &str, key: &str) -> String {
    let b = bundle();
    b.get(locale)
        .and_then(|m| m.get(key))
        .or_else(|| b.get(DEFAULT_LOCALE).and_then(|m| m.get(key)))
        .cloned()
        .unwrap_or_else(|| key.to_owned())
}

/// Translates and substitutes positional `%d` / `%s` / `%v` placeholders.
pub fn translate_args(locale: &str, key: &str, args: &[String]) -> String {
    let template = translate(locale, key);
    if args.is_empty() {
        return template;
    }
    let mut out = String::with_capacity(template.len());
    let mut args = args.iter();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && matches!(chars.peek(), Some('d' | 's' | 'v')) {
            chars.next();
            match args.next() {
                Some(a) => out.push_str(a),
                None => out.push_str("%!(MISSING)"),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Renders a bind-validation error like the original `ginutil.formatFieldError`: each
/// `Field\ttag[\tparam]` argument becomes `validation.Field.tag` when translated, else
/// `Field: <validation.rule.tag with %s = param>`, else `Field: tag[=param]`; entries are
/// joined with `"; "`.
pub fn bind_message(locale: &str, args: &[String]) -> String {
    args.iter()
        .map(|arg| {
            let mut parts = arg.splitn(3, '\t');
            let field = parts.next().unwrap_or_default();
            let tag = parts.next().unwrap_or("required");
            let param = parts.next().unwrap_or_default();
            let custom = format!("validation.{field}.{tag}");
            let msg = translate(locale, &custom);
            if msg != custom {
                return msg;
            }
            let rule_key = format!("validation.rule.{tag}");
            let rule = translate(locale, &rule_key);
            match (rule != rule_key, param.is_empty()) {
                (true, true) => format!("{field}: {rule}"),
                (true, false) => format!("{field}: {}", rule.replacen("%s", param, 1)),
                (false, true) => format!("{field}: {tag}"),
                (false, false) => format!("{field}: {tag}={param}"),
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Resolves the request locale: `?lang=` → `X-Lang` → `Accept-Language` → zh-CN.
pub fn resolve_locale(query: Option<&str>, headers: &HeaderMap) -> &'static str {
    let query_lang = query.and_then(|q| {
        q.split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "lang")
            .map(|(_, v)| v)
    });
    if let Some(lang) = query_lang.filter(|l| !l.trim().is_empty()) {
        return zs_shared::i18n::normalize_locale(lang);
    }
    let header = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
    };
    if let Some(lang) = header("x-lang").filter(|l| !l.is_empty()) {
        return zs_shared::i18n::normalize_locale(lang);
    }
    match header("accept-language").filter(|l| !l.is_empty()) {
        Some(accept) => zs_shared::i18n::normalize_locale(accept.split(',').next().unwrap_or("")),
        None => DEFAULT_LOCALE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_has_all_locales() {
        for l in zs_shared::i18n::LOCALES {
            assert!(bundle().get(l).is_some_and(|m| m.len() > 300), "{l}");
        }
    }

    /// Every `"error.*"` literal in the workspace sources (tests excluded).
    fn error_keys_in_sources() -> std::collections::BTreeSet<String> {
        fn walk(dir: &std::path::Path, out: &mut std::collections::BTreeSet<String>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if path.file_name().is_some_and(|n| n != "tests") {
                        walk(&path, out);
                    }
                } else if path.extension().is_some_and(|e| e == "rs")
                    && let Ok(text) = std::fs::read_to_string(&path)
                {
                    for (i, _) in text.match_indices("\"error.") {
                        let rest = &text[i + 1..];
                        let key: String = rest
                            .chars()
                            .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.'))
                            .collect();
                        if rest[key.len()..].starts_with('"') && key.len() > "error.".len() {
                            out.insert(key);
                        }
                    }
                }
            }
        }
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut out = std::collections::BTreeSet::new();
        for krate in ["shared", "domain", "app", "infra", "api", "server"] {
            walk(&crates.join(krate).join("src"), &mut out);
        }
        out
    }

    // QA-A07 (live QA I-7): no raw `error.*` key reaches a user — every key used in
    // the backend has a message in all three locales.
    #[test]
    fn qa_a07_every_error_key_is_translated() {
        let keys = error_keys_in_sources();
        assert!(keys.len() > 300, "scan found {} keys", keys.len());
        let missing: Vec<String> = keys
            .iter()
            .flat_map(|k| {
                zs_shared::i18n::LOCALES
                    .iter()
                    .filter(|l| bundle().get(**l).is_none_or(|m| !m.contains_key(k)))
                    .map(move |l| format!("{l}:{k}"))
            })
            .collect();
        assert!(missing.is_empty(), "untranslated: {missing:?}");
    }

    #[test]
    fn translates_with_args_and_fallback() {
        assert_eq!(translate("zh-CN", "error.bad_request"), "请求参数错误");
        assert_eq!(
            translate_args("zh-CN", "error.password_min_length", &["8".into()]),
            "密码长度至少 8 位"
        );
        assert_eq!(translate("en-US", "no.such.key"), "no.such.key");
    }

    #[test]
    fn resolves_locale_priority() {
        let mut h = HeaderMap::new();
        h.insert("accept-language", "en-US,en;q=0.9".parse().unwrap());
        assert_eq!(resolve_locale(None, &h), "en-US");
        h.insert("x-lang", "zh-TW".parse().unwrap());
        assert_eq!(resolve_locale(None, &h), "zh-TW");
        assert_eq!(resolve_locale(Some("a=1&lang=zh-CN"), &h), "zh-CN");
    }
}
