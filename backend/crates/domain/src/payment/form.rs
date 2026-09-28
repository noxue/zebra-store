//! URL query / form encoding compatible with Go's `net/url`.
//!
//! Gateway signatures and redirect URLs are compared byte-for-byte with the
//! original implementation, so escaping rules must match `url.QueryEscape`,
//! `url.PathEscape`, `url.Values.Encode` and `url.ParseQuery` exactly.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// Parsed form values (`url.Values`): key → values in arrival order.
pub type FormMap = BTreeMap<String, Vec<String>>;

/// Returns the first value of `key`, trimmed (`common.FormValue`).
pub fn form_value(form: &FormMap, key: &str) -> String {
    form.get(key)
        .and_then(|v| v.first())
        .map(|v| v.trim().to_owned())
        .unwrap_or_default()
}

/// Returns the first raw (untrimmed) value of `key`.
pub fn form_raw(form: &FormMap, key: &str) -> String {
    form.get(key)
        .and_then(|v| v.first())
        .cloned()
        .unwrap_or_default()
}

/// Converts a form into a JSON object holding the first value of each key (`common.FormToJSON`).
pub fn form_to_json(form: &FormMap) -> serde_json::Map<String, serde_json::Value> {
    form.iter()
        .filter_map(|(k, v)| {
            v.first()
                .map(|first| (k.clone(), serde_json::Value::String(first.clone())))
        })
        .collect()
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~')
}

fn push_escaped(out: &mut String, b: u8) {
    // Writing into a String cannot fail.
    let _ = write!(out, "%{b:02X}");
}

/// `url.QueryEscape`: space → `+`, everything but `[A-Za-z0-9-_.~]` percent-encoded.
pub fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unreserved(b) {
            out.push(char::from(b));
        } else if b == b' ' {
            out.push('+');
        } else {
            push_escaped(&mut out, b);
        }
    }
    out
}

/// `url.PathEscape`: like a path segment (`/ ; , ?` escaped, `$ & + : = @` kept, space → `%20`).
pub fn path_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        if is_unreserved(b) || matches!(b, b'$' | b'&' | b'+' | b':' | b'=' | b'@') {
            out.push(char::from(b));
        } else {
            push_escaped(&mut out, b);
        }
    }
    out
}

/// `url.Values.Encode`: keys sorted (stable), `k=v` joined with `&`.
pub fn encode_pairs(pairs: &[(String, String)]) -> String {
    let mut sorted: Vec<&(String, String)> = pairs.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    sorted
        .iter()
        .map(|(k, v)| format!("{}={}", query_escape(k), query_escape(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Encodes a form map (`url.Values.Encode`).
pub fn encode_form(form: &FormMap) -> String {
    let pairs: Vec<(String, String)> = form
        .iter()
        .flat_map(|(k, vs)| vs.iter().map(move |v| (k.clone(), v.clone())))
        .collect();
    encode_pairs(&pairs)
}

/// Error returned by [`parse_query`] (the first problem encountered).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("invalid semicolon separator in query")]
    Semicolon,
    #[error("invalid URL escape")]
    Escape,
}

/// `url.QueryUnescape`: `+` → space, `%XX` decoded; malformed escapes are errors.
pub fn query_unescape(s: &str) -> Result<String, QueryError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                let hi = bytes.get(i + 1).and_then(|b| char::from(*b).to_digit(16));
                let lo = bytes.get(i + 2).and_then(|b| char::from(*b).to_digit(16));
                match (hi, lo) {
                    (Some(h), Some(l)) => {
                        // Two hex digits always fit in a byte.
                        out.push(u8::try_from(h * 16 + l).map_err(|_| QueryError::Escape)?);
                        i += 3;
                    }
                    _ => return Err(QueryError::Escape),
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    Ok(String::from_utf8_lossy(&out).into_owned())
}

/// `url.ParseQuery`: returns every well-formed pair plus the first error (if any).
pub fn parse_query(raw: &str) -> (Vec<(String, String)>, Option<QueryError>) {
    let mut pairs = Vec::new();
    let mut first_err = None;
    for part in raw.split('&') {
        if part.contains(';') {
            first_err.get_or_insert(QueryError::Semicolon);
            continue;
        }
        if part.is_empty() {
            continue;
        }
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        match (query_unescape(k), query_unescape(v)) {
            (Ok(k), Ok(v)) => pairs.push((k, v)),
            _ => {
                first_err.get_or_insert(QueryError::Escape);
            }
        }
    }
    (pairs, first_err)
}

/// Collects pairs into a [`FormMap`].
pub fn pairs_to_form(pairs: Vec<(String, String)>) -> FormMap {
    let mut form = FormMap::new();
    for (k, v) in pairs {
        form.entry(k).or_default().push(v);
    }
    form
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vector from Go `url.QueryEscape`.
    #[test]
    fn query_escape_matches_go() {
        assert_eq!(
            query_escape("a b+c&d=e/f?g~h*i'j(k)l!m@n:o;p,q$r中"),
            "a+b%2Bc%26d%3De%2Ff%3Fg~h%2Ai%27j%28k%29l%21m%40n%3Ao%3Bp%2Cq%24r%E4%B8%AD"
        );
        assert_eq!(path_escape("a b/c:d"), "a%20b%2Fc:d");
    }

    #[test]
    fn parses_query_like_go() {
        let (pairs, err) = parse_query("a=1&b=x+y%21&&c");
        assert_eq!(err, None);
        assert_eq!(
            pairs,
            vec![
                ("a".into(), "1".into()),
                ("b".into(), "x y!".into()),
                ("c".into(), String::new())
            ]
        );
        let (pairs, err) = parse_query("a=1;b=2&c=%zz&d=4");
        assert_eq!(err, Some(QueryError::Semicolon));
        assert_eq!(pairs, vec![("d".into(), "4".into())]);
    }

    #[test]
    fn encodes_sorted() {
        let pairs = vec![
            ("b".to_owned(), "2".to_owned()),
            ("a".to_owned(), "x y".to_owned()),
            ("b".to_owned(), "1".to_owned()),
        ];
        assert_eq!(encode_pairs(&pairs), "a=x+y&b=2&b=1");
    }
}
