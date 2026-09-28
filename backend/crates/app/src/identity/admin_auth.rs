//! Administrator login, token authentication and password changes.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use zs_domain::identity::admin::{
    Admin, AdminLoginLog, AdminLoginLogRepo, AdminRepo, NewAdmin, events,
};
use zs_domain::identity::password::PasswordPolicy;
use zs_domain::identity::totp;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;

use super::jwt::{self, Registered, Signer};
use super::password;

/// Lifetime of a 2FA challenge token (original `challenge.TTL`).
pub const CHALLENGE_TTL_MINUTES: i64 = totp::CHALLENGE_TTL_MINUTES;
/// Minimum length accepted by the CLI password reset (original `minPasswordLength`).
const CLI_MIN_PASSWORD_LENGTH: usize = 8;
/// Username used when `bootstrap.default_admin_username` is empty.
const DEFAULT_BOOTSTRAP_USERNAME: &str = "admin";
/// Password used outside release mode when none is configured (original default).
const DEFAULT_BOOTSTRAP_PASSWORD: &str = "admin123";

/// Access-token claims (same JSON keys as the original).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminClaims {
    pub admin_id: Id,
    pub username: String,
    pub token_version: i64,
    #[serde(default)]
    pub typ: String,
    #[serde(flatten)]
    pub reg: Registered,
}

/// 2FA challenge claims.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeClaims {
    pub admin_id: Id,
    pub jti: String,
    pub purpose: String,
    pub typ: String,
    pub exp: i64,
    pub iat: i64,
    pub nbf: i64,
}

/// The authenticated administrator attached to a request.
#[derive(Debug, Clone)]
pub struct AdminPrincipal {
    pub id: Id,
    pub username: String,
    pub is_super: bool,
}

/// Result of the password step.
#[derive(Debug, Clone)]
pub enum LoginOutcome {
    /// Logged in: access token issued.
    Token {
        admin: Admin,
        token: String,
        expires_at: DateTime<Utc>,
    },
    /// 2FA enabled: a challenge token must be exchanged via verify-2fa.
    Challenge {
        admin: Admin,
        challenge_token: String,
        jti: String,
        expires_at: DateTime<Utc>,
    },
}

/// Request context recorded in login logs.
#[derive(Debug, Clone, Default)]
pub struct ClientInfo {
    pub ip: String,
    pub user_agent: String,
    pub request_id: String,
}

/// Administrator authentication use cases.
#[derive(Clone)]
pub struct AdminAuthService {
    repo: Arc<dyn AdminRepo>,
    logs: Arc<dyn AdminLoginLogRepo>,
    clock: Arc<dyn Clock>,
    signer: Signer,
    challenge_signer: Signer,
    ttl: Duration,
    policy: PasswordPolicy,
}

impl std::fmt::Debug for AdminAuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminAuthService")
    }
}

impl AdminAuthService {
    pub fn new(
        repo: Arc<dyn AdminRepo>,
        logs: Arc<dyn AdminLoginLogRepo>,
        clock: Arc<dyn Clock>,
        jwt_secret: &str,
        expire_hours: i64,
        policy: PasswordPolicy,
    ) -> Self {
        Self {
            repo,
            logs,
            clock,
            signer: Signer::new(jwt_secret),
            challenge_signer: jwt::challenge_signer(jwt_secret),
            ttl: Duration::hours(expire_hours.max(1)),
            policy,
        }
    }

    pub fn repo(&self) -> &Arc<dyn AdminRepo> {
        &self.repo
    }

    pub fn password_policy(&self) -> PasswordPolicy {
        self.policy
    }

    /// Creates the first super administrator when the table is empty; otherwise
    /// makes sure the configured bootstrap administrator is a super admin (ADM-03).
    /// In release mode no administrator is created without a configured password.
    pub async fn bootstrap(
        &self,
        username: &str,
        password: &str,
        release: bool,
    ) -> Result<Option<Admin>> {
        let username = match username.trim() {
            "" => DEFAULT_BOOTSTRAP_USERNAME,
            name => name,
        };
        if self.repo.count().await? > 0 {
            if let Some(mut existing) = self.repo.get_by_username(username).await?
                && !existing.is_super
            {
                existing.is_super = true;
                self.repo.save(&existing).await?;
                tracing::info!(
                    username,
                    "restored super flag of the bootstrap administrator"
                );
            }
            return Ok(None);
        }
        let password = if password.is_empty() {
            if release {
                tracing::warn!(
                    "no administrator exists and bootstrap.default_admin_password is empty; \
                     refusing to create one with a default password in release mode"
                );
                return Ok(None);
            }
            DEFAULT_BOOTSTRAP_PASSWORD
        } else {
            password
        };
        let hash = password::hash(password).await?;
        let admin = self
            .repo
            .create(&NewAdmin {
                username: username.to_owned(),
                password_hash: hash,
                is_super: true,
            })
            .await?;
        tracing::info!(username = %admin.username, "bootstrapped default super administrator");
        Ok(Some(admin))
    }

    /// Password step of the login flow.
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        client: &ClientInfo,
    ) -> Result<LoginOutcome> {
        let admin = self.repo.get_by_username(username).await?;
        let Some(mut admin) = admin else {
            password::verify(password, password::DUMMY_HASH).await;
            self.log(
                0,
                username,
                events::STATUS_FAILED,
                events::FAIL_INVALID_CREDENTIALS,
                client,
            )
            .await;
            return Err(Error::unauthorized("error.admin_login_invalid"));
        };
        if !password::verify(password, &admin.password_hash).await {
            self.log(
                0,
                username,
                events::STATUS_FAILED,
                events::FAIL_INVALID_CREDENTIALS,
                client,
            )
            .await;
            return Err(Error::unauthorized("error.admin_login_invalid"));
        }
        self.log(
            admin.id,
            &admin.username,
            events::STATUS_SUCCESS,
            "",
            client,
        )
        .await;
        if admin.totp_enabled() {
            let (challenge_token, jti, expires_at) = self.issue_challenge(admin.id)?;
            return Ok(LoginOutcome::Challenge {
                admin,
                challenge_token,
                jti,
                expires_at,
            });
        }
        let (token, expires_at) = self.issue_token(&admin)?;
        admin.last_login_at = Some(self.clock.now());
        self.repo
            .save(&admin)
            .await
            .map_err(|e| e.or_internal("error.login_failed"))?;
        Ok(LoginOutcome::Token {
            admin,
            token,
            expires_at,
        })
    }

    /// Issues the access token after a successful 2FA verification.
    pub async fn complete_2fa_login(&self, admin_id: Id) -> Result<(Admin, String, DateTime<Utc>)> {
        let mut admin = self
            .repo
            .get(admin_id)
            .await?
            .ok_or_else(|| Error::not_found("error.user_not_found"))?;
        let (token, expires_at) = self.issue_token(&admin)?;
        admin.last_login_at = Some(self.clock.now());
        self.repo.save(&admin).await?;
        Ok((admin, token, expires_at))
    }

    /// Validates a bearer token and returns the principal.
    pub async fn authenticate(&self, token: &str) -> Result<AdminPrincipal> {
        let claims: AdminClaims = self
            .signer
            .verify(token)
            .ok_or_else(|| Error::unauthorized("error.token_invalid"))?;
        if claims.admin_id <= 0 || !jwt::is_access_type(&claims.typ) {
            return Err(Error::unauthorized("error.token_invalid"));
        }
        let admin = self
            .repo
            .get(claims.admin_id)
            .await
            .ok()
            .flatten()
            .ok_or_else(|| Error::unauthorized("error.token_invalid"))?;
        if !admin.accepts_token(claims.token_version, claims.reg.iat) {
            return Err(Error::unauthorized("error.token_revoked"));
        }
        Ok(AdminPrincipal {
            id: admin.id,
            username: claims.username,
            is_super: admin.is_super,
        })
    }

    /// Verifies a challenge token and returns `(admin_id, jti)`.
    pub fn parse_challenge(&self, token: &str) -> Result<(Id, String)> {
        let invalid = || Error::unauthorized(totp::keys::CHALLENGE_INVALID);
        let c: ChallengeClaims = self.challenge_signer.verify(token).ok_or_else(invalid)?;
        if c.purpose != totp::CHALLENGE_PURPOSE
            || c.typ != jwt::TYP_2FA_CHALLENGE
            || c.admin_id <= 0
            || c.jti.is_empty()
        {
            return Err(invalid());
        }
        Ok((c.admin_id, c.jti))
    }

    /// Username of `admin_id` (empty when unknown).
    pub async fn username_of(&self, admin_id: Id) -> String {
        match self.repo.get(admin_id).await {
            Ok(Some(a)) => a.username,
            _ => String::new(),
        }
    }

    /// Operator password reset (`admin reset-password`): revokes every session
    /// and records `password_reset_by_cli`.
    pub async fn cli_reset_password(&self, username: &str, new_password: &str) -> Result<Admin> {
        if new_password.is_empty() || new_password.chars().count() < CLI_MIN_PASSWORD_LENGTH {
            return Err(
                Error::bad_request("error.password_min_length").arg(CLI_MIN_PASSWORD_LENGTH)
            );
        }
        let mut admin = self
            .repo
            .get_by_username(username)
            .await?
            .ok_or_else(|| Error::not_found("error.user_not_found"))?;
        admin.password_hash = password::hash(new_password).await?;
        self.revoke_tokens(&mut admin);
        self.repo.save(&admin).await?;
        let client = cli_client();
        self.record(
            admin.id,
            &admin.username,
            events::PASSWORD_RESET_BY_CLI,
            events::STATUS_SUCCESS,
            "",
            &client,
        )
        .await;
        Ok(admin)
    }

    pub async fn change_password(&self, admin_id: Id, old: &str, new: &str) -> Result<()> {
        let mut admin = self
            .repo
            .get(admin_id)
            .await?
            .ok_or_else(|| Error::not_found("error.user_not_found"))?;
        if !password::verify(old, &admin.password_hash).await {
            return Err(Error::bad_request("error.password_old_invalid"));
        }
        self.policy.validate(new)?;
        admin.password_hash = password::hash(new).await?;
        self.revoke_tokens(&mut admin);
        self.repo
            .save(&admin)
            .await
            .map_err(|e| e.or_internal("error.save_failed"))
    }

    /// Invalidates every token issued so far for `admin` (caller persists).
    pub fn revoke_tokens(&self, admin: &mut Admin) {
        admin.token_version += 1;
        admin.token_invalid_before = Some(self.clock.now());
    }

    fn issue_token(&self, admin: &Admin) -> Result<(String, DateTime<Utc>)> {
        let now = self.clock.now();
        let claims = AdminClaims {
            admin_id: admin.id,
            username: admin.username.clone(),
            token_version: admin.token_version,
            typ: jwt::TYP_ACCESS.into(),
            reg: Registered::new(now, self.ttl),
        };
        Ok((self.signer.sign(&claims)?, now + self.ttl))
    }

    fn issue_challenge(&self, admin_id: Id) -> Result<(String, String, DateTime<Utc>)> {
        let now = self.clock.now();
        let expires_at = now + Duration::minutes(CHALLENGE_TTL_MINUTES);
        let jti = uuid::Uuid::new_v4().to_string();
        let claims = ChallengeClaims {
            admin_id,
            jti: jti.clone(),
            purpose: totp::CHALLENGE_PURPOSE.into(),
            typ: jwt::TYP_2FA_CHALLENGE.into(),
            exp: expires_at.timestamp(),
            iat: now.timestamp(),
            nbf: now.timestamp(),
        };
        Ok((self.challenge_signer.sign(&claims)?, jti, expires_at))
    }

    async fn log(
        &self,
        admin_id: Id,
        username: &str,
        status: &str,
        reason: &str,
        client: &ClientInfo,
    ) {
        self.record(
            admin_id,
            username,
            events::LOGIN_PASSWORD,
            status,
            reason,
            client,
        )
        .await;
    }

    async fn record(
        &self,
        admin_id: Id,
        username: &str,
        event_type: &str,
        status: &str,
        reason: &str,
        client: &ClientInfo,
    ) {
        let entry = AdminLoginLog {
            admin_id,
            username: username.to_owned(),
            event_type: event_type.into(),
            status: status.into(),
            fail_reason: reason.into(),
            client_ip: client.ip.clone(),
            user_agent: client.user_agent.clone(),
            request_id: client.request_id.clone(),
            operator_id: None,
        };
        if let Err(error) = self.logs.record(entry).await {
            tracing::warn!(%error, "failed to record admin login log");
        }
    }
}

/// Client info recorded for operator CLI actions.
pub fn cli_client() -> ClientInfo {
    ClientInfo {
        ip: "cli".into(),
        user_agent: "admin-tool".into(),
        request_id: format!("cli-{}", uuid::Uuid::new_v4()),
    }
}
