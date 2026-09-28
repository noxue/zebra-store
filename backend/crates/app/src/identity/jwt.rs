//! HS256 JWT signing and verification shared by the admin and user realms.

use chrono::{DateTime, Duration, Utc};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::Serialize;
use serde::de::DeserializeOwned;
use zs_domain::{Error, Result};

/// `typ` of access tokens.
pub const TYP_ACCESS: &str = "access";
/// `typ` of 2FA challenge tokens.
pub const TYP_2FA_CHALLENGE: &str = "2fa_challenge";

/// Registered claims embedded in every token.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct Registered {
    pub exp: i64,
    pub iat: i64,
    pub nbf: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub jti: String,
}

impl Registered {
    pub fn new(now: DateTime<Utc>, ttl: Duration) -> Self {
        Self {
            exp: (now + ttl).timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
            jti: String::new(),
        }
    }
}

/// HS256 signer bound to one secret.
#[derive(Clone)]
pub struct Signer {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl std::fmt::Debug for Signer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Signer(..)")
    }
}

impl Signer {
    pub fn new(secret: &str) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        }
    }

    pub fn sign<C: Serialize>(&self, claims: &C) -> Result<String> {
        jsonwebtoken::encode(&Header::new(Algorithm::HS256), claims, &self.encoding)
            .map_err(Error::internal)
    }

    /// Verifies signature (HS256 only), `exp` and `nbf`.
    pub fn verify<C: DeserializeOwned>(&self, token: &str) -> Option<C> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_nbf = true;
        validation.leeway = 0;
        validation.required_spec_claims.clear();
        jsonwebtoken::decode::<C>(token, &self.decoding, &validation)
            .ok()
            .map(|d| d.claims)
    }
}

/// Signer of 2FA challenge tokens: a key derived from the realm secret, so a
/// challenge token can never verify as an access token (AUTH-03).
pub fn challenge_signer(secret: &str) -> Signer {
    Signer::new(&format!("{secret}\u{0}2fa_challenge"))
}

/// True for access tokens (`typ` empty is accepted for tokens issued before `typ` existed).
pub fn is_access_type(typ: &str) -> bool {
    typ.is_empty() || typ == TYP_ACCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, serde::Deserialize)]
    struct Claims {
        sub: i64,
        #[serde(flatten)]
        reg: Registered,
    }

    #[test]
    fn roundtrip_and_expiry() {
        let s = Signer::new("secret");
        let now = Utc::now();
        let t = s
            .sign(&Claims {
                sub: 7,
                reg: Registered::new(now, Duration::hours(1)),
            })
            .unwrap();
        assert_eq!(s.verify::<Claims>(&t).unwrap().sub, 7);
        assert!(Signer::new("other").verify::<Claims>(&t).is_none());
        let expired = s
            .sign(&Claims {
                sub: 7,
                reg: Registered::new(now - Duration::hours(2), Duration::hours(1)),
            })
            .unwrap();
        assert!(s.verify::<Claims>(&expired).is_none());
    }

    // RISK-06 / AUTH-03: only HS256 is accepted and challenge tokens use a derived key.
    #[test]
    fn rejects_other_algorithms_and_realms() {
        let now = Utc::now();
        let claims = Claims {
            sub: 1,
            reg: Registered::new(now, Duration::hours(1)),
        };
        let hs512 = jsonwebtoken::encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        assert!(Signer::new("secret").verify::<Claims>(&hs512).is_none());
        let unsigned = format!(
            "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.{}.",
            hs512.split('.').nth(1).unwrap()
        );
        assert!(Signer::new("secret").verify::<Claims>(&unsigned).is_none());
        let challenge = challenge_signer("secret").sign(&claims).unwrap();
        assert!(Signer::new("secret").verify::<Claims>(&challenge).is_none());
        assert!(is_access_type("") && is_access_type("access"));
        assert!(!is_access_type("refresh") && !is_access_type(TYP_2FA_CHALLENGE));
    }
}
