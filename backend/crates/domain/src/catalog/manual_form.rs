//! Manual-delivery form schema (`products.manual_form_schema_json`) and submission validation.
//!
//! Port of the original `catalog/product/manualform` package.

use std::collections::{BTreeSet, HashSet};

use regex::Regex;
use serde_json::{Map, Number, Value, json};

use crate::{Error, Result};

pub const SCHEMA_INVALID: &str = "error.manual_form_schema_invalid";
pub const REQUIRED_MISSING: &str = "error.manual_form_required_missing";
pub const FIELD_INVALID: &str = "error.manual_form_field_invalid";
pub const TYPE_INVALID: &str = "error.manual_form_type_invalid";
pub const OPTION_INVALID: &str = "error.manual_form_option_invalid";

/// Maximum field key: lowercase letters, digits and `_`, 1–64 chars.
const KEY_MAX_LEN: usize = 64;

const TYPES: [&str; 8] = [
    "text", "textarea", "phone", "email", "number", "select", "radio", "checkbox",
];

#[derive(Debug, Clone)]
struct Field {
    key: String,
    kind: String,
    required: bool,
    regex: String,
    min: Option<f64>,
    max: Option<f64>,
    max_len: Option<usize>,
    options: Vec<String>,
}

fn schema_err() -> Error {
    Error::bad_request(SCHEMA_INVALID)
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= KEY_MAX_LEN
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

fn number_of(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                t.parse::<f64>().ok()
            }
        }
        _ => None,
    }
}

fn number_value(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "checked integral and within i64 range"
        )]
        return Value::Number(Number::from(v as i64));
    }
    Number::from_f64(v).map_or(Value::Null, Value::Number)
}

fn locale_map(field: &Map<String, Value>, key: &str) -> Result<Map<String, Value>> {
    let Some(raw) = field.get(key) else {
        return Ok(Map::new());
    };
    let obj = raw.as_object().ok_or_else(schema_err)?;
    let mut out = Map::new();
    for (locale, v) in obj {
        let text = v.as_str().ok_or_else(schema_err)?.trim();
        if !text.is_empty() {
            out.insert(locale.clone(), Value::String(text.to_owned()));
        }
    }
    Ok(out)
}

/// Compiles a pattern; `/…/flags` JavaScript literals are converted (i, m, s honoured; g, u, y ignored).
fn compile_regex(raw: &str) -> Result<Regex> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(schema_err());
    }
    let pattern = match parse_regex_literal(raw) {
        Some((pattern, flags)) => {
            let mut prefix = String::new();
            for flag in flags.chars() {
                match flag {
                    'i' => prefix.push_str("(?i)"),
                    'm' => prefix.push_str("(?m)"),
                    's' => prefix.push_str("(?s)"),
                    'g' | 'u' | 'y' => {}
                    _ => return Err(schema_err()),
                }
            }
            format!("{prefix}{pattern}")
        }
        None => raw.to_owned(),
    };
    Regex::new(&pattern).map_err(|_| schema_err())
}

fn parse_regex_literal(raw: &str) -> Option<(&str, &str)> {
    let bytes = raw.as_bytes();
    if bytes.len() < 2 || bytes[0] != b'/' {
        return None;
    }
    let mut i = bytes.len() - 1;
    while i > 0 {
        if bytes[i] == b'/' {
            let backslashes = bytes[..i].iter().rev().take_while(|b| **b == b'\\').count();
            if backslashes % 2 == 0 {
                return Some((&raw[1..i], &raw[i + 1..]));
            }
        }
        i -= 1;
    }
    None
}

fn parse_schema(schema: &Map<String, Value>) -> Result<(Vec<Field>, Value)> {
    if schema.is_empty() {
        return Ok((Vec::new(), json!({"fields": []})));
    }
    let list = schema
        .get("fields")
        .and_then(Value::as_array)
        .ok_or_else(schema_err)?;
    let mut keys = HashSet::new();
    let mut fields = Vec::with_capacity(list.len());
    let mut normalized = Vec::with_capacity(list.len());
    for raw in list {
        let map = raw.as_object().ok_or_else(schema_err)?;
        let key = map
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_owned();
        if key.is_empty() || !keys.insert(key.clone()) || !valid_key(&key) {
            return Err(schema_err());
        }
        let kind = map
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .ok_or_else(schema_err)?
            .to_owned();
        if !TYPES.contains(&kind.as_str()) {
            return Err(schema_err());
        }
        let label = locale_map(map, "label")?;
        let placeholder = locale_map(map, "placeholder")?;
        // LQA-I2: the supplier's own message for a value its pattern refuses (acg-faka
        // widget `error`), shown by the storefront instead of the generic one.
        let error_message = locale_map(map, "error_message")?;
        let required = match map.get("required") {
            None => false,
            Some(Value::Bool(b)) => *b,
            Some(_) => return Err(schema_err()),
        };
        let regex = match map.get("regex") {
            None => String::new(),
            Some(Value::String(s)) => s.trim().to_owned(),
            Some(_) => return Err(schema_err()),
        };
        if !regex.is_empty() {
            compile_regex(&regex)?;
        }
        let min = map
            .get("min")
            .map(|v| number_of(v).ok_or_else(schema_err))
            .transpose()?;
        let max = map
            .get("max")
            .map(|v| number_of(v).ok_or_else(schema_err))
            .transpose()?;
        if let (Some(a), Some(b)) = (min, max)
            && a > b
        {
            return Err(schema_err());
        }
        let max_len = match map.get("max_len") {
            None => None,
            Some(v) => {
                let n = number_of(v).ok_or_else(schema_err)?;
                if n.fract() != 0.0 || n <= 0.0 {
                    return Err(schema_err());
                }
                #[expect(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    reason = "validated positive integral value"
                )]
                Some(n as usize)
            }
        };
        let options = match map.get("options") {
            None => Vec::new(),
            Some(Value::Array(items)) => {
                let mut set = BTreeSet::new();
                for item in items {
                    let text = item.as_str().ok_or_else(schema_err)?.trim();
                    if text.is_empty() {
                        return Err(schema_err());
                    }
                    set.insert(text.to_owned());
                }
                set.into_iter().collect()
            }
            Some(_) => return Err(schema_err()),
        };
        if matches!(kind.as_str(), "select" | "radio" | "checkbox") && options.is_empty() {
            return Err(schema_err());
        }

        let mut out = Map::new();
        out.insert("key".into(), Value::String(key.clone()));
        out.insert("type".into(), Value::String(kind.clone()));
        out.insert("required".into(), Value::Bool(required));
        if !label.is_empty() {
            out.insert("label".into(), Value::Object(label));
        }
        if !placeholder.is_empty() {
            out.insert("placeholder".into(), Value::Object(placeholder));
        }
        if !error_message.is_empty() {
            out.insert("error_message".into(), Value::Object(error_message));
        }
        if !regex.is_empty() {
            out.insert("regex".into(), Value::String(regex.clone()));
        }
        if let Some(v) = min {
            out.insert("min".into(), number_value(v));
        }
        if let Some(v) = max {
            out.insert("max".into(), number_value(v));
        }
        if let Some(v) = max_len {
            out.insert("max_len".into(), Value::from(v));
        }
        if !options.is_empty() {
            out.insert("options".into(), json!(options));
        }
        normalized.push(Value::Object(out));
        fields.push(Field {
            key,
            kind,
            required,
            regex,
            min,
            max,
            max_len,
            options,
        });
    }
    Ok((fields, json!({ "fields": normalized })))
}

/// Validates and normalises a schema (used on product create/update).
pub fn normalize_schema(schema: &Map<String, Value>) -> Result<Map<String, Value>> {
    let (_, normalized) = parse_schema(schema)?;
    Ok(match normalized {
        Value::Object(m) => m,
        _ => Map::new(),
    })
}

/// Validates a buyer submission against the schema; returns `(normalized_schema, normalized_submission)`.
pub fn validate_and_normalize(
    schema: &Map<String, Value>,
    submission: &Map<String, Value>,
) -> Result<(Map<String, Value>, Map<String, Value>)> {
    let (fields, normalized_schema) = parse_schema(schema)?;
    let normalized_schema = match normalized_schema {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    if fields.is_empty() {
        if !submission.is_empty() {
            return Err(Error::bad_request(FIELD_INVALID));
        }
        return Ok((normalized_schema, Map::new()));
    }
    if submission.is_empty() {
        if fields.iter().any(|f| f.required) {
            return Err(Error::bad_request(REQUIRED_MISSING));
        }
        return Ok((normalized_schema, Map::new()));
    }
    if submission
        .keys()
        .any(|k| !fields.iter().any(|f| &f.key == k))
    {
        return Err(Error::bad_request(FIELD_INVALID));
    }
    let mut out = Map::new();
    for field in &fields {
        let Some(raw) = submission.get(&field.key) else {
            if field.required {
                return Err(Error::bad_request(REQUIRED_MISSING));
            }
            continue;
        };
        match normalize_value(field, raw)? {
            Some(v) => {
                out.insert(field.key.clone(), v);
            }
            None if field.required => return Err(Error::bad_request(REQUIRED_MISSING)),
            None => {}
        }
    }
    Ok((normalized_schema, out))
}

fn check_text(field: &Field, text: &str) -> Result<()> {
    if field.max_len.is_some_and(|max| text.chars().count() > max) {
        return Err(Error::bad_request(FIELD_INVALID));
    }
    if !field.regex.is_empty() && !compile_regex(&field.regex)?.is_match(text) {
        return Err(Error::bad_request(FIELD_INVALID));
    }
    Ok(())
}

fn is_phone(text: &str) -> bool {
    let body = text.strip_prefix('+').unwrap_or(text);
    let len = body.chars().count();
    (6..=20).contains(&len)
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '-' | '(' | ')') || c.is_whitespace())
}

fn is_email(text: &str) -> bool {
    let addr = match (text.rfind('<'), text.ends_with('>')) {
        (Some(start), true) => &text[start + 1..text.len() - 1],
        _ => text,
    };
    let mut parts = addr.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty()
                && !domain.is_empty()
                && !addr.chars().any(char::is_whitespace)
                && !domain.starts_with('.')
                && !domain.ends_with('.')
        }
        _ => false,
    }
}

/// HTML-escapes like Go's `html.EscapeString`.
pub fn escape_html(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&#34;"),
            _ => out.push(c),
        }
    }
    out
}

fn normalize_value(field: &Field, raw: &Value) -> Result<Option<Value>> {
    let type_err = || Error::bad_request(TYPE_INVALID);
    match field.kind.as_str() {
        "text" | "textarea" | "phone" | "email" => {
            let text = raw.as_str().ok_or_else(type_err)?.trim();
            if text.is_empty() {
                return Ok(None);
            }
            if field.max_len.is_some_and(|max| text.chars().count() > max) {
                return Err(Error::bad_request(FIELD_INVALID));
            }
            if field.kind == "phone" && !is_phone(text) {
                return Err(Error::bad_request(FIELD_INVALID));
            }
            if field.kind == "email" && !is_email(text) {
                return Err(Error::bad_request(FIELD_INVALID));
            }
            check_text(field, text)?;
            Ok(Some(Value::String(escape_html(text))))
        }
        "number" => {
            let n = number_of(raw).ok_or_else(type_err)?;
            if field.min.is_some_and(|m| n < m) || field.max.is_some_and(|m| n > m) {
                return Err(Error::bad_request(FIELD_INVALID));
            }
            Ok(Some(number_value(n)))
        }
        "select" | "radio" => {
            let text = raw.as_str().ok_or_else(type_err)?.trim();
            if text.is_empty() {
                return Ok(None);
            }
            if field.options.iter().any(|o| o == text) {
                Ok(Some(Value::String(text.to_owned())))
            } else {
                Err(Error::bad_request(OPTION_INVALID))
            }
        }
        "checkbox" => {
            let items = raw.as_array().ok_or_else(type_err)?;
            let mut values = BTreeSet::new();
            for item in items {
                let text = item.as_str().ok_or_else(type_err)?.trim();
                if text.is_empty() {
                    continue;
                }
                if !field.options.iter().any(|o| o == text) {
                    return Err(Error::bad_request(OPTION_INVALID));
                }
                values.insert(text.to_owned());
            }
            if values.is_empty() {
                return Ok(None);
            }
            Ok(Some(json!(values.into_iter().collect::<Vec<_>>())))
        }
        _ => Err(schema_err()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obj(v: Value) -> Map<String, Value> {
        match v {
            Value::Object(m) => m,
            _ => Map::new(),
        }
    }

    fn key_of(r: Result<(Map<String, Value>, Map<String, Value>)>) -> String {
        r.unwrap_err().key().to_owned()
    }

    #[test]
    fn required_missing() {
        let schema = obj(json!({"fields": [{"key": "name", "type": "text", "required": true}]}));
        assert_eq!(
            key_of(validate_and_normalize(&schema, &Map::new())),
            REQUIRED_MISSING
        );
    }

    #[test]
    fn type_and_option_errors() {
        let schema = obj(json!({"fields": [{"key": "age", "type": "number", "required": true}]}));
        let sub = obj(json!({"age": {"value": 18}}));
        assert_eq!(key_of(validate_and_normalize(&schema, &sub)), TYPE_INVALID);

        let schema = obj(
            json!({"fields": [{"key": "province", "type": "select", "required": true, "options": ["GD", "BJ"]}]}),
        );
        let sub = obj(json!({"province": "SH"}));
        assert_eq!(
            key_of(validate_and_normalize(&schema, &sub)),
            OPTION_INVALID
        );
    }

    #[test]
    fn success_normalizes_values() {
        let schema = obj(json!({"fields": [
            {"key": "name", "type": "text", "required": true, "max_len": 20},
            {"key": "phone", "type": "text", "required": true, "regex": "^1[0-9]{10}$"},
            {"key": "city", "type": "select", "options": ["shanghai", "beijing"]},
            {"key": "count", "type": "number", "required": true, "min": 1, "max": 99}
        ]}));
        let sub =
            obj(json!({"name": " Alice ", "phone": "13800138000", "city": "beijing", "count": 2}));
        let (normalized, out) = validate_and_normalize(&schema, &sub).unwrap();
        assert!(normalized.contains_key("fields"));
        assert_eq!(out["name"], "Alice");
        assert_eq!(out["phone"], "13800138000");
        assert_eq!(out["city"], "beijing");
        assert_eq!(out["count"], 2);
        let unknown = obj(json!({"name": "a", "phone": "13800138000", "count": 1, "x": 1}));
        assert_eq!(
            key_of(validate_and_normalize(&schema, &unknown)),
            FIELD_INVALID
        );
        let out_of_range = obj(json!({"name": "a", "phone": "13800138000", "count": 100}));
        assert_eq!(
            key_of(validate_and_normalize(&schema, &out_of_range)),
            FIELD_INVALID
        );
    }

    #[test]
    fn schema_rejections() {
        for bad in [
            json!({"fields": [{"key": "phone", "type": "text", "regex": "["}]}),
            json!({"fields": [{"key": "Receiver-Name", "type": "text"}]}),
            json!({"fields": [{"key": "a", "type": "unknown"}]}),
            json!({"fields": [{"key": "a", "type": "text"}, {"key": "a", "type": "text"}]}),
            json!({"fields": [{"key": "a", "type": "select"}]}),
            json!({"fields": [{"key": "a", "type": "number", "min": 5, "max": 1}]}),
            json!({"fields": [{"key": "a", "type": "text", "max_len": 0}]}),
            json!({"fields": [{"key": "a", "type": "text", "required": "yes"}]}),
            json!({"other": []}),
        ] {
            assert_eq!(
                normalize_schema(&obj(bad)).unwrap_err().key(),
                SCHEMA_INVALID
            );
        }
        assert_eq!(
            Value::Object(normalize_schema(&Map::new()).unwrap()),
            json!({"fields": []})
        );
    }

    #[test]
    fn email_phone_checkbox_and_sanitize() {
        let schema = obj(json!({"fields": [
            {"key": "contact_phone", "type": "phone", "required": true},
            {"key": "contact_email", "type": "email", "required": true},
            {"key": "tags", "type": "checkbox", "options": ["A", "B", "C"]},
            {"key": "memo", "type": "textarea"}
        ]}));
        let sub = obj(json!({
            "contact_phone": "+86 13800138000",
            "contact_email": "test@example.com",
            "tags": ["B", "A", "A"],
            "memo": "<script>alert(1)</script>"
        }));
        let (_, out) = validate_and_normalize(&schema, &sub).unwrap();
        assert_eq!(out["tags"], json!(["A", "B"]));
        assert_eq!(out["memo"], "&lt;script&gt;alert(1)&lt;/script&gt;");
        let bad_email = obj(json!({"contact_phone": "13800138000", "contact_email": "nope"}));
        assert_eq!(
            key_of(validate_and_normalize(&schema, &bad_email)),
            FIELD_INVALID
        );
        let bad_phone = obj(json!({"contact_phone": "12ab", "contact_email": "a@b.c"}));
        assert_eq!(
            key_of(validate_and_normalize(&schema, &bad_phone)),
            FIELD_INVALID
        );
    }

    #[test]
    fn keeps_i18n_meta_and_regex_literals() {
        let schema = obj(json!({"fields": [
            {"key": "receiver_name", "type": "text", "required": true,
             "label": {"zh-CN": "收件人", "en-US": "Receiver"}, "placeholder": {"zh-CN": "请输入收件人"}},
            {"key": "contact_phone", "type": "phone", "regex": "/^1[0-9]{10}$/"},
            {"key": "code", "type": "text", "regex": "/^abc$/i"}
        ]}));
        let sub =
            obj(json!({"receiver_name": "Alice", "contact_phone": "13277745648", "code": "ABC"}));
        let (normalized, out) = validate_and_normalize(&schema, &sub).unwrap();
        assert_eq!(normalized["fields"][0]["label"]["zh-CN"], "收件人");
        assert_eq!(out["contact_phone"], "13277745648");
        assert_eq!(out["code"], "ABC");
    }
}
