//! PHP form conventions shared by the lizhipay systems (`acg-faka`, `mcy-shop`):
//! `application/x-www-form-urlencoded` bodies built like `http_build_query` and the
//! `Str::generateSignature` md5 signature (docs/protocol/third-party/acg-faka.md §2.2,
//! mcy-shop.md §1).

use md5::{Digest, Md5};

/// Form fields in send order; nested PHP arrays use bracket names (`sku[区服]`).
pub(crate) type Form = Vec<(String, String)>;

/// Which fields `Str::generateSignature` drops besides `sign` and empty flat values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArrayFields {
    /// acg-faka: nested arrays are signed as `name[k]=v` (inner insertion order).
    Signed,
    /// mcy-shop: top-level array values are excluded from the signature.
    Excluded,
}

/// Top-level PHP key of a field name (`sku[区服]` → `sku`).
fn top_key(name: &str) -> &str {
    name.split('[').next().unwrap_or(name)
}

/// `md5(urldecode(http_build_query(ksort(fields minus sign / empty))) . "&key=" . key)`:
/// fields are stably sorted by their top-level key (byte order, nested fields keep
/// their order), flat fields with an empty value are dropped, the joined string is the
/// raw (not percent-encoded) text; lowercase hex.
pub(crate) fn signature(fields: &[(String, String)], key: &str, arrays: ArrayFields) -> String {
    let mut kept: Vec<(&str, &str, &str)> = fields
        .iter()
        .filter_map(|(name, value)| {
            let top = top_key(name);
            let nested = top.len() != name.len();
            let dropped = top == "sign"
                || (!nested && value.is_empty())
                || (nested && arrays == ArrayFields::Excluded);
            (!dropped).then_some((top, name.as_str(), value.as_str()))
        })
        .collect();
    kept.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut raw = kept
        .iter()
        .map(|(_, n, v)| format!("{n}={v}"))
        .collect::<Vec<_>>()
        .join("&");
    raw.push_str("&key=");
    raw.push_str(key);
    md5_hex(raw.as_bytes())
}

/// Lowercase hex md5.
pub(crate) fn md5_hex(bytes: &[u8]) -> String {
    hex::encode(Md5::digest(bytes))
}

/// PHP `urlencode` of one component: unreserved `A-Za-z0-9-_.` kept, space as `+`,
/// everything else `%XX` (upper hex) of the UTF-8 bytes.
fn encode_component(raw: &str, out: &mut String) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' => out.push(char::from(b)),
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(char::from(HEX[usize::from(b >> 4)]));
                out.push(char::from(HEX[usize::from(b & 0x0f)]));
            }
        }
    }
}

/// `application/x-www-form-urlencoded` body of `fields` in order.
pub(crate) fn encode(fields: &[(String, String)]) -> String {
    let mut out = String::new();
    for (i, (name, value)) in fields.iter().enumerate() {
        if i > 0 {
            out.push('&');
        }
        encode_component(name, &mut out);
        out.push('=');
        encode_component(value, &mut out);
    }
    out
}

/// Builds a [`Form`] from `(&str, impl ToString)` pairs.
pub(crate) fn form<const N: usize>(pairs: [(&str, String); N]) -> Form {
    pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &str = "8F3A2C9D1E7B6A54";

    fn f(pairs: &[(&str, &str)]) -> Form {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    // ACG-01 · acg-faka.md §2.4 V1 / V2: connect with and without the (leaked) app_key field.
    #[test]
    fn acg_vectors_v1_v2() {
        let v1 = f(&[("app_id", "1024"), ("app_key", KEY)]);
        assert_eq!(
            signature(&v1, KEY, ArrayFields::Signed),
            "07bf581b48cc04485a688ec49f481368"
        );
        let v2 = f(&[("app_id", "1024")]);
        assert_eq!(
            signature(&v2, KEY, ArrayFields::Signed),
            "1095131eb01a23659cff325ba248290b"
        );
    }

    // ACG-01 · acg-faka.md §2.4 V3: trade with nested sku[..] fields and an empty password that
    // is sent but not signed; exact request body.
    #[test]
    fn acg_vector_v3_trade() {
        let mut v3 = f(&[
            ("shared_code", "ABCDEF1234567890"),
            ("contact", "buyer@example.com"),
            ("num", "2"),
            ("card_id", "0"),
            ("device", "0"),
            ("password", ""),
            ("race", "月卡"),
            ("request_no", "123456789012345678"),
            ("sku[区服]", "亚服"),
            ("sku[版本]", "标准"),
            ("app_id", "1024"),
            ("app_key", KEY),
        ]);
        let sign = signature(&v3, KEY, ArrayFields::Signed);
        assert_eq!(sign, "f7f8a9ce8e0ed44908af0afacb33c593");
        v3.push(("sign".into(), sign));
        assert_eq!(
            encode(&v3),
            "shared_code=ABCDEF1234567890&contact=buyer%40example.com&num=2&card_id=0&device=0&password=&race=%E6%9C%88%E5%8D%A1&request_no=123456789012345678&sku%5B%E5%8C%BA%E6%9C%8D%5D=%E4%BA%9A%E6%9C%8D&sku%5B%E7%89%88%E6%9C%AC%5D=%E6%A0%87%E5%87%86&app_id=1024&app_key=8F3A2C9D1E7B6A54&sign=f7f8a9ce8e0ed44908af0afacb33c593"
        );
        // the `sign` field itself never takes part
        assert_eq!(
            signature(&v3, KEY, ArrayFields::Signed),
            "f7f8a9ce8e0ed44908af0afacb33c593"
        );
    }

    // mcy-shop.md §2.4: Api-Signature of a trade; raw (urldecoded) text is signed, the
    // body encodes space as `+` and `+` as `%2B`.
    #[test]
    fn mcy_vector_trade() {
        let fields = f(&[
            ("sku_id", "17"),
            ("quantity", "1"),
            ("trade_no", "9efebb3d7d059bff092842bf"),
            ("account", "a b+c@x"),
        ]);
        assert_eq!(
            signature(&fields, KEY, ArrayFields::Excluded),
            "2f3078747abcffb750084230a4d60019"
        );
        assert_eq!(
            encode(&fields),
            "sku_id=17&quantity=1&trade_no=9efebb3d7d059bff092842bf&account=a+b%2Bc%40x"
        );
        assert_eq!(
            md5_hex(b"123456789012345678")[..24].to_owned(),
            "9efebb3d7d059bff092842bf"
        );
        // array fields are not signed by mcy-shop; no fields at all signs `&key=…`
        let mut with_array = fields.clone();
        with_array.push(("tags[0]".into(), "x".into()));
        assert_eq!(
            signature(&with_array, KEY, ArrayFields::Excluded),
            "2f3078747abcffb750084230a4d60019"
        );
        assert_eq!(
            signature(&[], KEY, ArrayFields::Excluded),
            md5_hex(format!("&key={KEY}").as_bytes())
        );
    }

    #[test]
    fn empty_nested_values_are_signed_and_zero_is_kept() {
        let fields = f(&[("b", "0"), ("a[x]", ""), ("c", "")]);
        assert_eq!(
            signature(&fields, "k", ArrayFields::Signed),
            md5_hex(b"a[x]=&b=0&key=k")
        );
    }
}
