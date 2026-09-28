//! PHP form semantics shared by the PHP-born compat protocols (acg-faka, mcy):
//! `application/x-www-form-urlencoded` bodies decoded the way PHP fills `$_POST`
//! (top-level `.`/space → `_`, `name[a][b]` nesting, `[]` appends, last value wins)
//! and the md5 signature both systems compute over it:
//!
//! ```php
//! unset($data['sign']); ksort($data);
//! foreach ($data as $k => $v) if ($v === '' /* mcy: || is_array($v) */) unset($data[$k]);
//! return md5(urldecode(http_build_query($data) . "&key=" . $appKey));
//! ```
//!
//! The signature must be computed over the *received* fields (the server side of
//! acg-faka signs `$_POST` as is), so the form is kept as parsed, never rebuilt from a
//! typed struct.

use std::collections::HashMap;

/// Most fields accepted in one body (PHP's default `max_input_vars` is 1000).
const MAX_FIELDS: usize = 1000;

/// Which fields take part in the signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignScope {
    /// acg-faka `Str::generateSignature`: nested arrays are signed as `k[a]=v`.
    WithArrays,
    /// mcy-shop `Str::generateSignature`: top-level arrays are left out.
    ScalarsOnly,
}

/// A top-level `$_POST` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Slot {
    Scalar(String),
    /// Leaves as `(path, value)` in first-insertion order, `path` = `[a][b]`.
    Array(Vec<(String, String)>),
}

/// A parsed form body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhpForm {
    /// Top-level keys in first-insertion order.
    entries: Vec<(String, Slot)>,
}

/// Decodes one `application/x-www-form-urlencoded` component (`+` = space; invalid
/// `%` escapes are kept literally, like PHP's `urldecode`).
fn decode(raw: &[u8]) -> String {
    let hex = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        match raw[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < raw.len() => match (hex(raw[i + 1]), hex(raw[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// PHP's top-level variable name normalization (`.` and space become `_`).
fn normalize_top(name: &str) -> String {
    name.chars()
        .map(|c| if c == '.' || c == ' ' { '_' } else { c })
        .collect()
}

/// Splits `name[a][b]` into the normalized top key and bracket segments (PHP
/// `php_register_variable_ex`): an unmatched `[` in the top part becomes `_` and the
/// variable is scalar; anything after the last well-formed segment is ignored.
fn split_name(raw: &str) -> Option<(String, Vec<String>)> {
    let raw = raw.trim_start_matches(' ');
    let Some(open) = raw.find('[') else {
        let top = normalize_top(raw);
        return (!top.is_empty()).then_some((top, Vec::new()));
    };
    let top_raw = &raw[..open];
    let rest = &raw[open..];
    if !rest.contains(']') {
        let top = normalize_top(&raw.replacen('[', "_", 1));
        return (!top.is_empty()).then_some((top, Vec::new()));
    }
    let top = normalize_top(top_raw);
    if top.is_empty() {
        return None;
    }
    let mut segments = Vec::new();
    let mut s = rest;
    while let Some(after_open) = s.strip_prefix('[') {
        let Some(close) = after_open.find(']') else {
            break;
        };
        segments.push(after_open[..close].to_owned());
        s = &after_open[close + 1..];
    }
    Some((top, segments))
}

impl PhpForm {
    /// Parses a raw body.
    pub fn parse(body: &[u8]) -> Self {
        let mut form = Self::default();
        // Next auto index of each array level for `[]` (keyed by the path prefix).
        let mut next_index: HashMap<String, i64> = HashMap::new();
        for pair in body
            .split(|b| *b == b'&')
            .filter(|p| !p.is_empty())
            .take(MAX_FIELDS)
        {
            let (name, value) = match pair.iter().position(|b| *b == b'=') {
                Some(i) => (&pair[..i], &pair[i + 1..]),
                None => (pair, &[][..]),
            };
            let Some((top, segments)) = split_name(&decode(name)) else {
                continue;
            };
            let value = decode(value);
            if segments.is_empty() {
                form.set(top, Slot::Scalar(value));
                continue;
            }
            let mut path = String::new();
            let mut prefix = top.clone();
            for seg in &segments {
                let key = if seg.is_empty() {
                    let n = next_index.entry(prefix.clone()).or_insert(0);
                    let key = n.to_string();
                    *n += 1;
                    key
                } else {
                    if let Ok(n) = seg.parse::<i64>()
                        && n.to_string() == *seg
                    {
                        let next = next_index.entry(prefix.clone()).or_insert(0);
                        *next = (*next).max(n.saturating_add(1));
                    }
                    seg.clone()
                };
                path.push('[');
                path.push_str(&key);
                path.push(']');
                prefix.push('[');
                prefix.push_str(&key);
                prefix.push(']');
            }
            match form.entries.iter_mut().find(|(k, _)| *k == top) {
                Some((_, Slot::Array(leaves))) => {
                    match leaves.iter_mut().find(|(p, _)| *p == path) {
                        Some(leaf) => leaf.1 = value,
                        None => leaves.push((path, value)),
                    }
                }
                _ => form.set(top, Slot::Array(vec![(path, value)])),
            }
        }
        form
    }

    fn set(&mut self, key: String, slot: Slot) {
        match self.entries.iter_mut().find(|(k, _)| *k == key) {
            Some(entry) => entry.1 = slot,
            None => self.entries.push((key, slot)),
        }
    }

    /// A scalar field (`None` when missing or an array: PHP's `is_scalar` check, so
    /// `app_id[]=1` cannot reach a lookup, PRV-01).
    pub fn scalar(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|(k, v)| match v {
            Slot::Scalar(s) if k == key => Some(s.as_str()),
            _ => None,
        })
    }

    /// A scalar field trimmed, empty when missing.
    pub fn text(&self, key: &str) -> &str {
        self.scalar(key).map(str::trim).unwrap_or_default()
    }

    /// True when the field is present (scalar or array).
    pub fn has(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// Top-level scalar fields in arrival order.
    pub fn scalars(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().filter_map(|(k, v)| match v {
            Slot::Scalar(s) => Some((k.as_str(), s.as_str())),
            Slot::Array(_) => None,
        })
    }

    /// The signed string without the `&key=` suffix: fields except `sign`, sorted by
    /// top-level key, top-level empty strings dropped, values not escaped.
    pub fn canonical(&self, scope: SignScope) -> String {
        let mut keys: Vec<&(String, Slot)> =
            self.entries.iter().filter(|(k, _)| k != "sign").collect();
        keys.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let mut parts: Vec<String> = Vec::new();
        for (k, slot) in keys {
            match slot {
                Slot::Scalar(v) if v.is_empty() => {}
                Slot::Scalar(v) => parts.push(format!("{k}={v}")),
                Slot::Array(leaves) => {
                    if scope == SignScope::WithArrays {
                        parts.extend(leaves.iter().map(|(p, v)| format!("{k}{p}={v}")));
                    }
                }
            }
        }
        parts.join("&")
    }

    /// `md5(canonical + "&key=" + app_key)`, lowercase hex.
    pub fn signature(&self, scope: SignScope, app_key: &str) -> String {
        let text = format!("{}&key={app_key}", self.canonical(scope));
        zs_shared::sign::md5_hex(text.as_bytes())
    }
}

/// Constant-time equality of two signatures (PHP `hash_equals`: exact bytes, no
/// numeric juggling, so `0e…` "magic hashes" never compare equal, PRV-01).
pub fn signature_eq(expected: &str, given: &str) -> bool {
    let (a, b) = (expected.as_bytes(), given.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "8F3A2C9D1E7B6A54";

    // acg-faka.md §2.4 V1 / V2.
    #[test]
    fn acg_vectors_v1_v2() {
        let f = PhpForm::parse(b"app_id=1024&app_key=8F3A2C9D1E7B6A54");
        assert_eq!(
            f.canonical(SignScope::WithArrays),
            "app_id=1024&app_key=8F3A2C9D1E7B6A54"
        );
        assert_eq!(
            f.signature(SignScope::WithArrays, KEY),
            "07bf581b48cc04485a688ec49f481368"
        );
        let f = PhpForm::parse(b"app_id=1024");
        assert_eq!(
            f.signature(SignScope::WithArrays, KEY),
            "1095131eb01a23659cff325ba248290b"
        );
    }

    // acg-faka.md §2.4 V3: the exact trade body of the acg-faka client (nested sku,
    // empty password sent but not signed).
    #[test]
    fn acg_vector_v3() {
        let body = "shared_code=ABCDEF1234567890&contact=buyer%40example.com&num=2&card_id=0&device=0&password=&race=%E6%9C%88%E5%8D%A1&request_no=123456789012345678&sku%5B%E5%8C%BA%E6%9C%8D%5D=%E4%BA%9A%E6%9C%8D&sku%5B%E7%89%88%E6%9C%AC%5D=%E6%A0%87%E5%87%86&app_id=1024&app_key=8F3A2C9D1E7B6A54&sign=f7f8a9ce8e0ed44908af0afacb33c593";
        let f = PhpForm::parse(body.as_bytes());
        assert_eq!(
            f.canonical(SignScope::WithArrays),
            "app_id=1024&app_key=8F3A2C9D1E7B6A54&card_id=0&contact=buyer@example.com&device=0&num=2&race=月卡&request_no=123456789012345678&shared_code=ABCDEF1234567890&sku[区服]=亚服&sku[版本]=标准"
        );
        assert_eq!(
            f.signature(SignScope::WithArrays, KEY),
            "f7f8a9ce8e0ed44908af0afacb33c593"
        );
        assert_eq!(f.scalar("sign"), Some("f7f8a9ce8e0ed44908af0afacb33c593"));
        assert_eq!(f.scalar("race"), Some("月卡"));
        assert_eq!(f.scalar("sku"), None);
        assert!(f.has("sku"));
    }

    // mcy-shop.md §2.4: Api-Signature over the flat business fields.
    #[test]
    fn mcy_vector() {
        let body = "sku_id=17&quantity=1&trade_no=9efebb3d7d059bff092842bf&account=a+b%2Bc%40x";
        let f = PhpForm::parse(body.as_bytes());
        assert_eq!(
            f.canonical(SignScope::ScalarsOnly),
            "account=a b+c@x&quantity=1&sku_id=17&trade_no=9efebb3d7d059bff092842bf"
        );
        assert_eq!(
            f.signature(SignScope::ScalarsOnly, KEY),
            "2f3078747abcffb750084230a4d60019"
        );
        // no fields: "&key=<app_key>"
        assert_eq!(
            PhpForm::parse(b"").signature(SignScope::ScalarsOnly, KEY),
            zs_shared::sign::md5_hex(format!("&key={KEY}").as_bytes())
        );
    }

    #[test]
    fn php_name_rules() {
        let f = PhpForm::parse(b"a.b=1&c+d=2&x[=3&arr[]=p&arr[]=q&arr[5]=r&arr[]=s&dup=1&dup=2&n[k]=1&n[j]=2&n[k]=3&[bad]=1&e=");
        assert_eq!(f.scalar("a_b"), Some("1"));
        assert_eq!(f.scalar("c_d"), Some("2"));
        assert_eq!(f.scalar("x_"), Some("3"));
        assert_eq!(f.scalar("dup"), Some("2"));
        assert_eq!(
            f.canonical(SignScope::WithArrays),
            "a_b=1&arr[0]=p&arr[1]=q&arr[5]=r&arr[6]=s&c_d=2&dup=2&n[k]=3&n[j]=2&x_=3"
        );
        assert_eq!(
            f.canonical(SignScope::ScalarsOnly),
            "a_b=1&c_d=2&dup=2&x_=3"
        );
        // "0" is kept, only "" is dropped
        assert_eq!(
            PhpForm::parse(b"z=0&y=").canonical(SignScope::WithArrays),
            "z=0"
        );
        // a later scalar replaces an array and vice versa
        assert_eq!(PhpForm::parse(b"k[a]=1&k=2").scalar("k"), Some("2"));
        assert_eq!(PhpForm::parse(b"k=2&k[a]=1").scalar("k"), None);
    }

    #[test]
    fn decoding() {
        assert_eq!(decode(b"a+b%20c%2"), "a b c%2");
        assert_eq!(decode(b"%zz%4"), "%zz%4");
        assert_eq!(decode(b"%E6%9C%88"), "月");
        assert_eq!(decode(b"100%"), "100%");
    }

    // PRV-01: exact, constant-time comparison.
    #[test]
    fn prv01_signature_comparison() {
        assert!(signature_eq("abc", "abc"));
        assert!(!signature_eq("0e462097431906509019562988736854", "0"));
        assert!(!signature_eq(
            "0e462097431906509019562988736854",
            "0e830400451993494058024219903391"
        ));
        assert!(!signature_eq("abc", "ABC"));
        assert!(!signature_eq("abc", ""));
    }
}
