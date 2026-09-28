//! Lenient readers over raw setting JSON (port of the original `schema/value`).
//!
//! Settings are admin-edited JSON blobs; every reader tolerates wrong types and
//! falls back instead of failing, exactly like the original decoders.

use serde_json::{Map, Value, json};
use zs_shared::i18n::LOCALES;

/// A JSON object.
pub type Obj = Map<String, Value>;

/// Returns the value as an object when it is one.
pub fn as_obj(v: Option<&Value>) -> Option<&Obj> {
    v.and_then(Value::as_object)
}

/// A trimmed string; non-strings become `""`.
pub fn text(v: Option<&Value>) -> String {
    v.and_then(Value::as_str)
        .map(|s| s.trim().to_owned())
        .unwrap_or_default()
}

/// Trimmed string truncated to `max_chars` characters.
pub fn text_limit(v: Option<&Value>, max_chars: usize) -> String {
    let t = text(v);
    truncate_chars(&t, max_chars)
}

/// Truncates to at most `max_chars` Unicode scalar values.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return s.to_owned();
    }
    s.chars().take(max_chars).collect()
}

/// Truthiness of a raw value (`ParseBool`): bools, non-zero numbers, `1/true/yes/on`.
pub fn parse_bool(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => matches!(
            s.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        _ => false,
    }
}

/// Integer value of numbers (truncated) or numeric strings (`ParseInt`).
pub fn parse_int(v: Option<&Value>) -> Option<i64> {
    match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)),
        Some(Value::String(s)) => s.trim().parse::<i64>().ok(),
        _ => None,
    }
}

/// Float value of numbers or numeric strings (`ParseFloat`).
pub fn parse_float(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// `ReadString`: present strings are trimmed, anything else yields `fallback`.
pub fn read_string(obj: &Obj, key: &str, fallback: &str) -> String {
    match obj.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        _ => fallback.to_owned(),
    }
}

/// `ReadBool`: bools and boolean-like strings, otherwise `fallback`.
pub fn read_bool(obj: &Obj, key: &str, fallback: bool) -> bool {
    match obj.get(key) {
        Some(Value::Bool(b)) => *b,
        Some(Value::String(s)) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => fallback,
        },
        _ => fallback,
    }
}

/// `ReadInt`: numbers or numeric strings, otherwise `fallback`.
pub fn read_int(obj: &Obj, key: &str, fallback: i64) -> i64 {
    parse_int(obj.get(key)).unwrap_or(fallback)
}

/// `ReadStringList`: arrays keep their string items; other shapes yield `fallback`.
pub fn read_string_list(obj: &Obj, key: &str, fallback: &[String]) -> Vec<String> {
    match obj.get(key) {
        None => fallback.to_vec(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_str().map(str::to_owned))
            .collect(),
        Some(_) => fallback.to_vec(),
    }
}

/// `ReadUintList`: arrays keep their positive numeric items; other shapes yield `fallback`.
pub fn read_id_list(obj: &Obj, key: &str, fallback: &[i64]) -> Vec<i64> {
    match obj.get(key) {
        None => fallback.to_vec(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|i| i.as_f64())
            .filter(|f| *f > 0.0)
            .map(|f| f as i64)
            .collect(),
        Some(_) => fallback.to_vec(),
    }
}

/// `NormalizeStringList`: arrays become trimmed strings (non-strings → `""`).
pub fn string_list(v: Option<&Value>) -> Option<Vec<String>> {
    match v {
        Some(Value::Array(items)) => Some(
            items
                .iter()
                .map(|i| i.as_str().unwrap_or_default().trim().to_owned())
                .collect(),
        ),
        _ => None,
    }
}

/// A localized field with every supported locale present and trimmed.
pub fn localized_field(v: Option<&Value>) -> Value {
    let source = as_obj(v);
    let mut out = Obj::new();
    for locale in LOCALES {
        let value = source.map(|o| text(o.get(locale))).unwrap_or_default();
        out.insert(locale.to_owned(), Value::String(value));
    }
    Value::Object(out)
}

/// True when every locale of a normalized localized field is empty.
pub fn localized_is_blank(v: &Value) -> bool {
    LOCALES
        .iter()
        .all(|l| v.get(*l).and_then(Value::as_str).unwrap_or("").is_empty())
}

/// A block of localized fields (`{field: {locale: text}}`).
pub fn localized_block(v: Option<&Value>, fields: &[&str]) -> Value {
    let source = as_obj(v);
    let mut out = Obj::new();
    for field in fields {
        out.insert(
            (*field).to_owned(),
            localized_field(source.and_then(|o| o.get(*field))),
        );
    }
    Value::Object(out)
}

/// Rounds to two decimals (`math.Round(v*100)/100`).
pub fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

/// Encodes a float like Go's `encoding/json`: integral values without a fraction.
pub fn number(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 9.0e15 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// Minimal `net/mail.ParseAddress` check: `local@domain` or `Name <local@domain>`.
pub fn is_email_address(raw: &str) -> bool {
    let raw = raw.trim();
    let addr = match (raw.rfind('<'), raw.ends_with('>')) {
        (Some(start), true) => &raw[start + 1..raw.len() - 1],
        _ => raw,
    };
    let Some((local, domain)) = addr.rsplit_once('@') else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && !addr
            .chars()
            .any(|c| c.is_whitespace() || c == '<' || c == '>')
        && !local.contains('@')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_go_style_scalars() {
        assert!(parse_bool(Some(&json!("on"))));
        assert!(parse_bool(Some(&json!(2))));
        assert!(!parse_bool(Some(&json!("off"))));
        assert!(!parse_bool(None));
        assert_eq!(parse_int(Some(&json!(12.9))), Some(12));
        assert_eq!(parse_int(Some(&json!(" 7 "))), Some(7));
        assert_eq!(parse_int(Some(&json!(""))), None);
        assert_eq!(number(10.0), json!(10));
        assert_eq!(number(10.5), json!(10.5));
    }

    #[test]
    fn localized_field_fills_every_locale() {
        let v = localized_field(Some(&json!({"zh-CN": "  你好 ", "fr": "x"})));
        assert_eq!(v, json!({"zh-CN": "你好", "zh-TW": "", "en-US": ""}));
        assert!(localized_is_blank(&localized_field(None)));
    }

    #[test]
    fn email_address_check() {
        assert!(is_email_address("a@b.com"));
        assert!(is_email_address("Shop <a@b.com>"));
        assert!(!is_email_address("a b@c.com"));
        assert!(!is_email_address("abc"));
        assert!(!is_email_address("a@"));
    }
}
