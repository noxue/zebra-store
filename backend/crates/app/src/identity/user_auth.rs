//! User access tokens: issuing and authenticating.
//!
//! Login/registration/password flows build on [`UserAuthService::issue_token`].

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use zs_domain::identity::totp;
use zs_domain::identity::user::{User, UserRepo};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;

use super::jwt::{self, Registered, Signer};

/// User access-token claims (same JSON keys as the original).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserClaims {
    pub user_id: Id,
    pub email: String,
    pub token_version: i64,
    #[serde(default)]
    pub typ: String,
    #[serde(flatten)]
    pub reg: Registered,
}

/// User 2FA challenge claims (original `UserChallengeClaims`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserChallengeClaims {
    pub user_id: Id,
    pub jti: String,
    pub purpose: String,
    #[serde(default)]
    pub remember_me: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_source: String,
    pub typ: String,
    pub exp: i64,
    pub iat: i64,
    pub nbf: i64,
}

/// An issued challenge.
#[derive(Debug, Clone)]
pub struct IssuedChallenge {
    pub token: String,
    pub jti: String,
    pub expires_at: DateTime<Utc>,
}

/// The authenticated storefront user attached to a request.
#[derive(Debug, Clone)]
pub struct UserPrincipal {
    pub id: Id,
    pub email: String,
}

/// User token service.
#[derive(Clone)]
pub struct UserAuthService {
    repo: Arc<dyn UserRepo>,
    clock: Arc<dyn Clock>,
    signer: Signer,
    challenge_signer: Signer,
    ttl: Duration,
    remember_ttl: Duration,
}

impl std::fmt::Debug for UserAuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserAuthService")
    }
}

impl UserAuthService {
    pub fn new(
        repo: Arc<dyn UserRepo>,
        clock: Arc<dyn Clock>,
        secret: &str,
        expire_hours: i64,
        remember_me_expire_hours: i64,
    ) -> Self {
        Self {
            repo,
            clock,
            signer: Signer::new(secret),
            challenge_signer: jwt::challenge_signer(secret),
            ttl: Duration::hours(expire_hours.max(1)),
            remember_ttl: Duration::hours(remember_me_expire_hours.max(1)),
        }
    }

    pub fn repo(&self) -> &Arc<dyn UserRepo> {
        &self.repo
    }

    pub fn clock(&self) -> &Arc<dyn Clock> {
        &self.clock
    }

    /// Issues an access token (`remember_me` selects the long lifetime).
    pub fn issue_token(&self, user: &User, remember_me: bool) -> Result<(String, DateTime<Utc>)> {
        let now = self.clock.now();
        let ttl = if remember_me {
            self.remember_ttl
        } else {
            self.ttl
        };
        let claims = UserClaims {
            user_id: user.id,
            email: user.email.clone(),
            token_version: user.token_version,
            typ: jwt::TYP_ACCESS.into(),
            reg: Registered::new(now, ttl),
        };
        Ok((self.signer.sign(&claims)?, now + ttl))
    }

    /// Validates a bearer token: signature, `typ`, token version, account status.
    pub async fn authenticate(&self, token: &str) -> Result<UserPrincipal> {
        let claims: UserClaims = self
            .signer
            .verify(token)
            .ok_or_else(|| Error::unauthorized("error.token_invalid"))?;
        if claims.user_id <= 0 || !jwt::is_access_type(&claims.typ) {
            return Err(Error::unauthorized("error.token_invalid"));
        }
        let user = self
            .repo
            .get(claims.user_id)
            .await
            .ok()
            .flatten()
            .ok_or_else(|| Error::unauthorized("error.token_invalid"))?;
        if !user.is_active() {
            return Err(Error::unauthorized("error.user_disabled"));
        }
        if !user.accepts_token(claims.token_version, claims.reg.iat) {
            return Err(Error::unauthorized("error.token_revoked"));
        }
        Ok(UserPrincipal {
            id: user.id,
            email: claims.email,
        })
    }

    /// Signer for derived tokens.
    pub fn signer(&self) -> &Signer {
        &self.signer
    }

    /// Issues a 2FA challenge token (5 minutes) remembering `remember_me` and the login source.
    pub fn issue_challenge(
        &self,
        user_id: Id,
        remember_me: bool,
        login_source: &str,
    ) -> Result<IssuedChallenge> {
        let now = self.clock.now();
        let expires_at = now + Duration::minutes(totp::CHALLENGE_TTL_MINUTES);
        let jti = uuid::Uuid::new_v4().to_string();
        let claims = UserChallengeClaims {
            user_id,
            jti: jti.clone(),
            purpose: totp::CHALLENGE_PURPOSE.into(),
            remember_me,
            login_source: login_source.to_owned(),
            typ: jwt::TYP_2FA_CHALLENGE.into(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
        };
        Ok(IssuedChallenge {
            token: self.challenge_signer.sign(&claims)?,
            jti,
            expires_at,
        })
    }

    /// Verifies a challenge token (`error.totp_challenge_invalid` otherwise).
    pub fn parse_challenge(&self, token: &str) -> Result<UserChallengeClaims> {
        let invalid = || Error::unauthorized(totp::keys::CHALLENGE_INVALID);
        let c: UserChallengeClaims = self.challenge_signer.verify(token).ok_or_else(invalid)?;
        if c.purpose != totp::CHALLENGE_PURPOSE
            || c.typ != jwt::TYP_2FA_CHALLENGE
            || c.user_id <= 0
            || c.jti.is_empty()
        {
            return Err(invalid());
        }
        Ok(c)
    }
}
