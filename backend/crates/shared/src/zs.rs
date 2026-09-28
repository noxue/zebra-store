//! Zebra Store site-to-site protocol v1 (`zebra-store`) primitives, shared by the
//! supplier side we serve (`/api/v1/zs/*`) and the buyer-side adapter:
//!
//! - canonical request signing (spec §2): `HMAC-SHA256` over
//!   `"ZS1\n" METHOD \n PATH \n CANONICAL_QUERY \n TIMESTAMP \n NONCE \n hex(SHA256(body))`;
//! - delivery encryption (spec §7): AES-256-GCM with `SHA256("zs-delivery:" + secret)`;
//! - connection codes (spec §3): `zsc1_<base64url(JSON)>`.

use aes_gcm::aead::{Aead, AeadCore, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Header carrying the API key.
pub const HEADER_KEY: &str = "ZS-Key";
/// Header carrying the unix timestamp (seconds).
pub const HEADER_TIMESTAMP: &str = "ZS-Timestamp";
/// Header carrying the replay-protection nonce.
pub const HEADER_NONCE: &str = "ZS-Nonce";
/// Header carrying `v1=<hex>`.
pub const HEADER_SIGNATURE: &str = "ZS-Signature";
/// Header carrying the event id of a pushed event.
pub const HEADER_EVENT_ID: &str = "ZS-Event-Id";
/// Header carrying the idempotency key of `POST /orders`.
pub const HEADER_IDEMPOTENCY_KEY: &str = "Idempotency-Key";
/// Maximum accepted clock skew (spec §2: 300 s).
pub const MAX_SKEW_SECS: i64 = 300;
/// A nonce may not be reused by the same key within this window (spec §2: 10 min).
pub const NONCE_TTL_SECS: i64 = 600;
/// Prefix of a connection code (spec §3).
pub const CODE_PREFIX: &str = "zsc1_";
/// Signature scheme prefix (`v1=`).
const SIGNATURE_PREFIX: &str = "v1=";
/// Nonce length bounds (spec §2: 16–64 characters).
const NONCE_MIN: usize = 16;
const NONCE_MAX: usize = 64;
/// GCM standard nonce length.
const GCM_NONCE_LEN: usize = 12;
/// Algorithm name of an encrypted delivery.
pub const DELIVERY_ALG: &str = "A256GCM";

/// True for a 16–64 character `[A-Za-z0-9_-]` nonce.
pub fn valid_nonce(nonce: &str) -> bool {
    (NONCE_MIN..=NONCE_MAX).contains(&nonce.len())
        && nonce
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// True when `timestamp` is within [`MAX_SKEW_SECS`] of `now`.
pub fn timestamp_valid(timestamp: i64, now: i64) -> bool {
    (now - timestamp).abs() <= MAX_SKEW_SECS
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

/// RFC 3986 percent-encoding (unreserved characters kept, uppercase hex).
pub fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        if is_unreserved(b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Decodes `%XX` sequences (`+` is kept literally); invalid sequences stay as-is.
pub fn percent_decode(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let (Some(h), Some(l)) = (
                bytes.get(i + 1).copied().and_then(hex_val),
                bytes.get(i + 2).copied().and_then(hex_val),
            )
        {
            out.push(h * 16 + l);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Canonical query (spec §2): pairs decoded, re-encoded with RFC 3986, sorted by key
/// then value and joined with `&`; empty for no query.
pub fn canonical_query(raw: &str) -> String {
    let mut pairs: Vec<(String, String)> = raw
        .trim_start_matches('?')
        .split('&')
        .filter(|p| !p.is_empty())
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap_or((p, ""));
            (
                percent_encode(&percent_decode(k)),
                percent_encode(&percent_decode(v)),
            )
        })
        .collect();
    pairs.sort();
    pairs
        .into_iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("&")
}

/// Hex SHA-256.
pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// A request to sign or verify.
#[derive(Debug, Clone, Copy)]
pub struct Signed<'a> {
    pub method: &'a str,
    /// Path without the query string.
    pub path: &'a str,
    /// Raw query string (with or without the leading `?`).
    pub query: &'a str,
    pub timestamp: i64,
    pub nonce: &'a str,
    pub body: &'a [u8],
}

impl Signed<'_> {
    /// The canonical string of spec §2.
    pub fn canonical(&self) -> String {
        format!(
            "ZS1\n{}\n{}\n{}\n{}\n{}\n{}",
            self.method.to_ascii_uppercase(),
            self.path,
            canonical_query(self.query),
            self.timestamp,
            self.nonce,
            sha256_hex(self.body)
        )
    }

    /// `v1=<hex(HMAC_SHA256(secret, canonical))>`.
    pub fn sign(&self, secret: &str) -> String {
        let mac = crate::sign::hmac_sha256(secret.as_bytes(), self.canonical().as_bytes());
        format!("{SIGNATURE_PREFIX}{}", hex::encode(mac))
    }

    /// Constant-time verification; empty secrets never verify.
    pub fn verify(&self, secret: &str, signature: &str) -> bool {
        if secret.is_empty() {
            return false;
        }
        let Some(hex_sig) = signature.trim().strip_prefix(SIGNATURE_PREFIX) else {
            return false;
        };
        let Ok(expected) = hex::decode(hex_sig) else {
            return false;
        };
        let Ok(mut mac) = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()) else {
            return false;
        };
        mac.update(self.canonical().as_bytes());
        mac.verify_slice(&expected).is_ok()
    }
}

/// Errors of delivery encryption and connection codes.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ZsError {
    #[error("encryption failed")]
    Encrypt,
    #[error("decryption failed")]
    Decrypt,
    #[error("invalid connection code")]
    InvalidCode,
}

fn delivery_cipher(secret: &str) -> Aes256Gcm {
    let key = Sha256::digest(format!("zs-delivery:{secret}").as_bytes());
    Aes256Gcm::new(&key)
}

/// An encrypted delivery (`{"encrypted", "alg", "nonce", "ciphertext"}`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sealed {
    pub encrypted: bool,
    pub alg: String,
    pub nonce: String,
    pub ciphertext: String,
}

/// Encrypts a delivery plaintext with the credential secret (spec §7).
pub fn seal_delivery(secret: &str, plaintext: &[u8]) -> Result<Sealed, ZsError> {
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = delivery_cipher(secret)
        .encrypt(&nonce, plaintext)
        .map_err(|_| ZsError::Encrypt)?;
    Ok(Sealed {
        encrypted: true,
        alg: DELIVERY_ALG.to_owned(),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ct),
    })
}

/// Decrypts a sealed delivery.
pub fn open_delivery(secret: &str, sealed: &Sealed) -> Result<Vec<u8>, ZsError> {
    if sealed.alg != DELIVERY_ALG {
        return Err(ZsError::Decrypt);
    }
    let nonce = STANDARD
        .decode(sealed.nonce.trim())
        .map_err(|_| ZsError::Decrypt)?;
    if nonce.len() != GCM_NONCE_LEN {
        return Err(ZsError::Decrypt);
    }
    let ct = STANDARD
        .decode(sealed.ciphertext.trim())
        .map_err(|_| ZsError::Decrypt)?;
    delivery_cipher(secret)
        .decrypt(Nonce::from_slice(&nonce), ct.as_ref())
        .map_err(|_| ZsError::Decrypt)
}

/// Payload of a connection code (spec §3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionCode {
    pub v: u32,
    pub url: String,
    pub key: String,
    pub secret: String,
    #[serde(default)]
    pub name: String,
}

/// `zsc1_<base64url(JSON)>`.
pub fn encode_connection_code(code: &ConnectionCode) -> String {
    let json = serde_json::to_vec(code).unwrap_or_default();
    format!("{CODE_PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
}

/// Parses a connection code; version, url, key and secret are required.
pub fn decode_connection_code(raw: &str) -> Result<ConnectionCode, ZsError> {
    let body = raw
        .trim()
        .strip_prefix(CODE_PREFIX)
        .ok_or(ZsError::InvalidCode)?;
    let bytes = URL_SAFE_NO_PAD
        .decode(body.trim_end_matches('='))
        .map_err(|_| ZsError::InvalidCode)?;
    let code: ConnectionCode = serde_json::from_slice(&bytes).map_err(|_| ZsError::InvalidCode)?;
    let url_ok = code.url.starts_with("https://") || code.url.starts_with("http://");
    if code.v != 1 || !url_ok || code.key.trim().is_empty() || code.secret.trim().is_empty() {
        return Err(ZsError::InvalidCode);
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req<'a>(query: &'a str, body: &'a [u8]) -> Signed<'a> {
        Signed {
            method: "get",
            path: "/api/v1/zs/catalog/products",
            query,
            timestamp: 1_700_000_000,
            nonce: "abcdefghijklmnop",
            body,
        }
    }

    // Fixed vector: canonical string layout and HMAC (computed independently with
    // `printf 'ZS1\nGET\n...' | openssl dgst -sha256 -hmac secret`).
    #[test]
    fn canonical_string_and_signature_vector() {
        let r = req("limit=50&cursor=", b"");
        assert_eq!(
            r.canonical(),
            "ZS1\nGET\n/api/v1/zs/catalog/products\ncursor=&limit=50\n1700000000\nabcdefghijklmnop\n\
             e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(r.sign("secret"), SIGNATURE_VECTOR);
        assert!(r.verify("secret", SIGNATURE_VECTOR));
        assert!(!r.verify("other", SIGNATURE_VECTOR));
        assert!(!r.verify("", SIGNATURE_VECTOR));
        assert!(
            !r.verify("secret", &SIGNATURE_VECTOR[3..]),
            "prefix required"
        );
        // body participates through its SHA-256
        assert!(!req("limit=50&cursor=", b"{}").verify("secret", SIGNATURE_VECTOR));
    }

    const SIGNATURE_VECTOR: &str =
        "v1=9638c9cb873bafcbfc2e29e60877db6955f1680a0f97ae08f2f682bcdecddd9d";

    #[test]
    fn query_canonicalization() {
        assert_eq!(canonical_query(""), "");
        assert_eq!(canonical_query("?b=2&a=1&a=0"), "a=0&a=1&b=2");
        // decoded then re-encoded with RFC 3986: equivalent encodings sign the same
        assert_eq!(
            canonical_query("q=a%20b&t=2026-09-25T06:00:00%2B08:00"),
            canonical_query("t=2026-09-25T06%3A00%3A00%2B08%3A00&q=a b")
        );
        assert_eq!(canonical_query("x=1+2"), "x=1%2B2");
        assert_eq!(canonical_query("flag"), "flag=");
    }

    #[test]
    fn nonce_and_timestamp_rules() {
        assert!(valid_nonce("abcdefghij_-1234"));
        assert!(!valid_nonce("short"));
        assert!(!valid_nonce("abcdefghijklmnop!"));
        assert!(!valid_nonce(&"a".repeat(65)));
        assert!(timestamp_valid(1000, 1300));
        assert!(!timestamp_valid(1000, 1301));
    }

    #[test]
    fn delivery_round_trip() {
        let sealed = seal_delivery("s3cret", br#"{"payload":"CARD-1"}"#).unwrap();
        assert!(sealed.encrypted);
        assert_eq!(sealed.alg, "A256GCM");
        assert_eq!(
            open_delivery("s3cret", &sealed).unwrap(),
            br#"{"payload":"CARD-1"}"#
        );
        assert_eq!(open_delivery("wrong", &sealed), Err(ZsError::Decrypt));
        let mut tampered = sealed.clone();
        tampered.ciphertext = STANDARD.encode(b"garbage-garbage-garbage");
        assert_eq!(open_delivery("s3cret", &tampered), Err(ZsError::Decrypt));
        let other = seal_delivery("s3cret", b"x").unwrap();
        assert_ne!(other.nonce, sealed.nonce, "fresh nonce per delivery");
    }

    #[test]
    fn connection_code_round_trip() {
        let code = ConnectionCode {
            v: 1,
            url: "https://supplier.example.com".into(),
            key: "k".into(),
            secret: "s".into(),
            name: "供货站".into(),
        };
        let text = encode_connection_code(&code);
        assert!(text.starts_with("zsc1_"));
        assert!(!text.contains('='), "unpadded base64url");
        assert_eq!(decode_connection_code(&format!("  {text} ")).unwrap(), code);
        for bad in [
            "zsc2_abc",
            "zsc1_!!!",
            "zsc1_",
            &encode_connection_code(&ConnectionCode {
                v: 2,
                ..code.clone()
            }),
            &encode_connection_code(&ConnectionCode {
                secret: String::new(),
                ..code.clone()
            }),
            &encode_connection_code(&ConnectionCode {
                url: "ftp://x".into(),
                ..code.clone()
            }),
        ] {
            assert_eq!(
                decode_connection_code(bad),
                Err(ZsError::InvalidCode),
                "{bad}"
            );
        }
    }
}
