//! Manual delivery data (`normalizeManualDeliveryData` / `buildManualDeliveryPayload`).

use serde_json::{Map, Value};

use super::model::JsonMap;

fn to_text(raw: &Value) -> String {
    match raw {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    }
}

fn normalize_entries(raw: &Value) -> Vec<Value> {
    let Value::Array(items) = raw else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Value::as_object)
        .filter_map(|row| {
            let key = row.get("key").map(to_text).unwrap_or_default();
            let value = row.get("value").map(to_text).unwrap_or_default();
            let (key, value) = (key.trim(), value.trim());
            if key.is_empty() && value.is_empty() {
                return None;
            }
            let mut entry = Map::new();
            if !key.is_empty() {
                entry.insert("key".into(), Value::String(key.to_owned()));
            }
            if !value.is_empty() {
                entry.insert("value".into(), Value::String(value.to_owned()));
            }
            Some(Value::Object(entry))
        })
        .collect()
}

fn normalize_primitive(raw: &Value) -> Option<Value> {
    match raw {
        Value::String(s) => {
            let t = s.trim();
            (!t.is_empty()).then(|| Value::String(t.to_owned()))
        }
        Value::Bool(_) | Value::Number(_) => Some(raw.clone()),
        _ => None,
    }
}

/// Keeps `note`, non-empty `entries[{key,value}]` and scalar extra keys (sorted).
pub fn normalize_delivery_data(raw: &JsonMap) -> JsonMap {
    let mut out = Map::new();
    if raw.is_empty() {
        return out;
    }
    let note = raw.get("note").map(to_text).unwrap_or_default();
    if !note.trim().is_empty() {
        out.insert("note".into(), Value::String(note.trim().to_owned()));
    }
    if let Some(entries) = raw.get("entries") {
        let entries = normalize_entries(entries);
        if !entries.is_empty() {
            out.insert("entries".into(), Value::Array(entries));
        }
    }
    let mut keys: Vec<&String> = raw
        .keys()
        .filter(|k| k.as_str() != "note" && k.as_str() != "entries")
        .collect();
    keys.sort();
    for key in keys {
        if let Some(v) = raw.get(key).and_then(normalize_primitive) {
            out.insert(key.clone(), v);
        }
    }
    out
}

/// Plain-text payload of normalized delivery data: note, `key: value` entries, extras.
pub fn delivery_payload(data: &JsonMap) -> String {
    let mut lines = Vec::new();
    if let Some(Value::String(note)) = data.get("note")
        && !note.trim().is_empty()
    {
        lines.push(note.clone());
    }
    if let Some(Value::Array(entries)) = data.get("entries") {
        for e in entries.iter().filter_map(Value::as_object) {
            let key = e.get("key").map(to_text).unwrap_or_default();
            let value = e.get("value").map(to_text).unwrap_or_default();
            let (key, value) = (key.trim(), value.trim());
            match (key.is_empty(), value.is_empty()) {
                (true, true) => {}
                (true, false) => lines.push(value.to_owned()),
                (false, true) => lines.push(key.to_owned()),
                (false, false) => lines.push(format!("{key}: {value}")),
            }
        }
    }
    let mut extra: Vec<&String> = data
        .keys()
        .filter(|k| k.as_str() != "note" && k.as_str() != "entries")
        .collect();
    extra.sort();
    for key in extra {
        let value = data.get(key).map(to_text).unwrap_or_default();
        if !value.trim().is_empty() {
            lines.push(format!("{key}: {}", value.trim()));
        }
    }
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_payload_from_structured_data() {
        let raw = json!({
            "note": " shipped ",
            "entries": [{"key": "Tracking", "value": "SF123"}, {"key": "", "value": ""}, {"value": "only"}],
            "carrier": "SF",
            "nested": {"x": 1},
            "blank": " ",
        });
        let data = normalize_delivery_data(raw.as_object().unwrap_or(&Map::new()));
        assert_eq!(data["note"], "shipped");
        assert_eq!(data["entries"].as_array().map(Vec::len), Some(2));
        assert!(data.get("nested").is_none());
        assert!(data.get("blank").is_none());
        assert_eq!(
            delivery_payload(&data),
            "shipped\nTracking: SF123\nonly\ncarrier: SF"
        );
    }
}
