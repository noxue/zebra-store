//! HMAC-SHA256 request signing shared by the upstream and channel APIs.
//!
//! `signature = hex(HMAC_SHA256(secret, "{METHOD}\n{path}\n{timestamp}\n{md5hex(body)}"))`

use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use sha2::Sha256;

/// Header carrying the upstream API key.
pub const HEADER_API_KEY: &str = "Dujiao-Next-Api-Key";
/// Header carrying the upstream request timestamp (unix seconds).
pub const HEADER_TIMESTAMP: &str = "Dujiao-Next-Timestamp";
/// Header carrying the upstream signature.
pub const HEADER_SIGNATURE: &str = "Dujiao-Next-Signature";
/// Header carrying the channel client key.
pub const HEADER_CHANNEL_KEY: &str = "Dujiao-Next-Channel-Key";
/// Header carrying the channel request timestamp.
pub const HEADER_CHANNEL_TIMESTAMP: &str = "Dujiao-Next-Channel-Timestamp";
/// Header carrying the channel signature.
pub const HEADER_CHANNEL_SIGNATURE: &str = "Dujiao-Next-Channel-Signature";
/// Maximum accepted clock skew in seconds.
pub const MAX_TIMESTAMP_SKEW: i64 = 60;

/// Computes the request signature.
pub fn sign(secret: &str, method: &str, path: &str, timestamp: i64, body: &[u8]) -> String {
    let body_md5 = md5_hex(body);
    let payload = format!("{method}\n{path}\n{timestamp}\n{body_md5}");
    hex::encode(hmac_sha256(secret.as_bytes(), payload.as_bytes()))
}

/// Verifies a signature in constant time.
pub fn verify(
    secret: &str,
    method: &str,
    path: &str,
    signature: &str,
    timestamp: i64,
    body: &[u8],
) -> bool {
    let Ok(expected) = hex::decode(signature) else {
        return false;
    };
    let body_md5 = md5_hex(body);
    let payload = format!("{method}\n{path}\n{timestamp}\n{body_md5}");
    let Ok(mut mac) = Hmac::<Sha256>::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload.as_bytes());
    mac.verify_slice(&expected).is_ok()
}

/// True when `timestamp` is within [`MAX_TIMESTAMP_SKEW`] of `now`.
pub fn timestamp_valid(timestamp: i64, now: i64) -> bool {
    (now - timestamp).abs() <= MAX_TIMESTAMP_SKEW
}

/// Returns raw HMAC-SHA256 bytes.
pub fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    // HMAC accepts keys of any length, so construction cannot fail.
    match Hmac::<Sha256>::new_from_slice(key) {
        Ok(mut mac) => {
            mac.update(data);
            mac.finalize().into_bytes().to_vec()
        }
        Err(_) => Vec::new(),
    }
}

/// Returns lowercase hex MD5 of `data`.
pub fn md5_hex(data: &[u8]) -> String {
    hex::encode(Md5::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_of_empty_body() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn known_vector() {
        // HMAC-SHA256("key", "GET\n/api/v1/upstream/ping\n1700000000\nd41d8cd98f00b204e9800998ecf8427e")
        let sig = sign("key", "GET", "/api/v1/upstream/ping", 1_700_000_000, b"");
        assert_eq!(sig.len(), 64);
        assert!(verify(
            "key",
            "GET",
            "/api/v1/upstream/ping",
            &sig,
            1_700_000_000,
            b""
        ));
        assert!(!verify(
            "key2",
            "GET",
            "/api/v1/upstream/ping",
            &sig,
            1_700_000_000,
            b""
        ));
    }

    #[test]
    fn timestamp_window() {
        assert!(timestamp_valid(100, 160));
        assert!(!timestamp_valid(100, 161));
    }
}

#[cfg(test)]
mod compat_tests {
    /// Vector produced by the original Go `upstream.Sign`.
    #[test]
    fn matches_original_go_signer() {
        let sig = super::sign(
            "key",
            "POST",
            "/api/v1/upstream/orders",
            1_700_000_000,
            br#"{"a":1}"#,
        );
        assert_eq!(sig, GO_VECTOR);
    }

    const GO_VECTOR: &str = "8823443bc9bed23ffabce7009855fd329aa0c9a242fc18e07f4573d2eebcf3e3";
}
