//! Order-preserving JSON parser that keeps the literal text of numbers
//! (Go `json.Decoder` with `UseNumber`). Gateways sign the original number text
//! (`10.50` must stay `10.50`), which `serde_json::Value` cannot preserve.

use super::common::go_json_string;

/// A JSON value with raw number text and ordered object members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RawJson {
    Null,
    Bool(bool),
    /// Literal number text as received.
    Number(String),
    String(String),
    Array(Vec<RawJson>),
    /// Members in document order (duplicates kept).
    Object(Vec<(String, RawJson)>),
}

impl RawJson {
    /// Parses the first JSON value of `input`; trailing data is ignored like `Decoder.Decode`.
    pub fn parse(input: &[u8]) -> Option<Self> {
        let mut p = Parser { s: input, i: 0 };
        p.ws();
        p.value(0)
    }

    /// Object members, `None` for non-objects.
    pub fn members(&self) -> Option<&[(String, RawJson)]> {
        match self {
            Self::Object(m) => Some(m),
            _ => None,
        }
    }

    /// Last member named `key` (Go maps keep the last duplicate).
    pub fn get(&self, key: &str) -> Option<&RawJson> {
        self.members()?
            .iter()
            .rev()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// Scalar text: strings as-is, numbers literal, booleans `true`/`false`, null empty.
    pub fn scalar_text(&self) -> Option<String> {
        match self {
            Self::Null => Some(String::new()),
            Self::Bool(b) => Some(b.to_string()),
            Self::Number(n) => Some(n.clone()),
            Self::String(s) => Some(s.clone()),
            _ => None,
        }
    }

    /// `json.Marshal` of the Go value decoded with `UseNumber` (maps sorted, last duplicate wins).
    pub fn go_marshal(&self) -> String {
        let mut out = String::new();
        self.write_go(&mut out);
        out
    }

    fn write_go(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            Self::Number(n) => out.push_str(n),
            Self::String(s) => go_json_string(out, s),
            Self::Array(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_go(out);
                }
                out.push(']');
            }
            Self::Object(members) => {
                let mut map: std::collections::BTreeMap<&str, &RawJson> =
                    std::collections::BTreeMap::new();
                for (k, v) in members {
                    map.insert(k, v);
                }
                out.push('{');
                for (i, (k, v)) in map.into_iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    go_json_string(out, k);
                    out.push(':');
                    v.write_go(out);
                }
                out.push('}');
            }
        }
    }
}

/// Nesting limit guarding against stack exhaustion.
const MAX_DEPTH: usize = 128;

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.i += 1;
        }
    }

    fn eat(&mut self, lit: &[u8]) -> bool {
        if self.s[self.i..].starts_with(lit) {
            self.i += lit.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self, depth: usize) -> Option<RawJson> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.peek()? {
            b'{' => self.object(depth),
            b'[' => self.array(depth),
            b'"' => self.string().map(RawJson::String),
            b't' => self.eat(b"true").then_some(RawJson::Bool(true)),
            b'f' => self.eat(b"false").then_some(RawJson::Bool(false)),
            b'n' => self.eat(b"null").then_some(RawJson::Null),
            b'-' | b'0'..=b'9' => self.number().map(RawJson::Number),
            _ => None,
        }
    }

    fn object(&mut self, depth: usize) -> Option<RawJson> {
        self.i += 1;
        let mut members = Vec::new();
        self.ws();
        if self.peek()? == b'}' {
            self.i += 1;
            return Some(RawJson::Object(members));
        }
        loop {
            self.ws();
            if self.peek()? != b'"' {
                return None;
            }
            let key = self.string()?;
            self.ws();
            if self.peek()? != b':' {
                return None;
            }
            self.i += 1;
            self.ws();
            let value = self.value(depth + 1)?;
            members.push((key, value));
            self.ws();
            match self.peek()? {
                b',' => self.i += 1,
                b'}' => {
                    self.i += 1;
                    return Some(RawJson::Object(members));
                }
                _ => return None,
            }
        }
    }

    fn array(&mut self, depth: usize) -> Option<RawJson> {
        self.i += 1;
        let mut items = Vec::new();
        self.ws();
        if self.peek()? == b']' {
            self.i += 1;
            return Some(RawJson::Array(items));
        }
        loop {
            self.ws();
            items.push(self.value(depth + 1)?);
            self.ws();
            match self.peek()? {
                b',' => self.i += 1,
                b']' => {
                    self.i += 1;
                    return Some(RawJson::Array(items));
                }
                _ => return None,
            }
        }
    }

    fn number(&mut self) -> Option<String> {
        let start = self.i;
        if self.peek() == Some(b'-') {
            self.i += 1;
        }
        match self.peek()? {
            b'0' => self.i += 1,
            b'1'..=b'9' => {
                while matches!(self.peek(), Some(b'0'..=b'9')) {
                    self.i += 1;
                }
            }
            _ => return None,
        }
        if self.peek() == Some(b'.') {
            self.i += 1;
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return None;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.i += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if !matches!(self.peek(), Some(b'0'..=b'9')) {
                return None;
            }
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.i += 1;
            }
        }
        std::str::from_utf8(&self.s[start..self.i])
            .ok()
            .map(str::to_owned)
    }

    fn hex4(&mut self) -> Option<u32> {
        let h = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
        let v = u32::from_str_radix(h, 16).ok()?;
        self.i += 4;
        Some(v)
    }

    fn string(&mut self) -> Option<String> {
        self.i += 1;
        let mut out: Vec<u8> = Vec::new();
        loop {
            let b = self.peek()?;
            self.i += 1;
            match b {
                b'"' => return Some(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let esc = self.peek()?;
                    self.i += 1;
                    let c = match esc {
                        b'"' => '"',
                        b'\\' => '\\',
                        b'/' => '/',
                        b'b' => '\u{8}',
                        b'f' => '\u{c}',
                        b'n' => '\n',
                        b'r' => '\r',
                        b't' => '\t',
                        b'u' => {
                            let hi = self.hex4()?;
                            if (0xD800..0xDC00).contains(&hi)
                                && self.s[self.i..].starts_with(b"\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                let lo = self.hex4()?;
                                if (0xDC00..0xE000).contains(&lo) {
                                    char::from_u32(0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00))
                                        .unwrap_or('\u{FFFD}')
                                } else {
                                    self.i = save;
                                    '\u{FFFD}'
                                }
                            } else {
                                char::from_u32(hi).unwrap_or('\u{FFFD}')
                            }
                        }
                        _ => return None,
                    };
                    let mut buf = [0u8; 4];
                    out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                }
                b if b < 0x20 => return None,
                b => out.push(b),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_number_text_and_order() {
        let v = RawJson::parse(
            r#" {"b":10.50,"a":[1,{"x":"中\"<"}],"n":null,"t":true} trailing"#.as_bytes(),
        );
        let v = v.unwrap_or(RawJson::Null);
        let members = v.members().unwrap_or_default();
        assert_eq!(members[0], ("b".into(), RawJson::Number("10.50".into())));
        assert_eq!(members[1].0, "a");
        assert_eq!(
            v.go_marshal(),
            r#"{"a":[1,{"x":"中\"\u003c"}],"b":10.50,"n":null,"t":true}"#
        );
        assert!(RawJson::parse(b"{\"a\":}").is_none());
        assert!(RawJson::parse(b"code=200&sign=x").is_none());
    }
}
