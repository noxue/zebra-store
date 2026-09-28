//! Administrator 2FA: self-service management, the login challenge and resets,
//! each recorded in `admin_login_logs` like the original handlers.

use chrono::{DateTime, Utc};
use zs_domain::identity::admin::{Admin, events};
use zs_domain::identity::totp::{self, TotpError, TotpStatus};
use zs_domain::{Error, Id, Result};

use super::admin_auth::{AdminAuthService, AdminPrincipal, ClientInfo, cli_client};
use super::audit::{AdminEvent, AuditService};
use super::challenge::ChallengeStore;
use super::totp::{AdminAccounts, SetupResult, TotpService};

/// TOTP service over administrator accounts.
pub type AdminTotp = TotpService<AdminAccounts>;

/// `/2fa/enable` response.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AdminEnableResult {
    pub enabled_at: DateTime<Utc>,
    pub recovery_codes: Vec<String>,
}

/// A code submitted to verify a challenge or confirm an action.
#[derive(Debug, Clone, Copy)]
pub struct SecondFactor<'a> {
    pub code: &'a str,
    pub recovery_code: &'a str,
}

impl SecondFactor<'_> {
    fn is_empty(&self) -> bool {
        self.code.is_empty() && self.recovery_code.is_empty()
    }

    fn is_recovery(&self) -> bool {
        !self.recovery_code.is_empty()
    }
}

/// Administrator 2FA service.
#[derive(Debug, Clone)]
pub struct AdminTwoFactorService {
    auth: AdminAuthService,
    totp: AdminTotp,
    challenges: ChallengeStore,
    audit: AuditService,
}

fn fail_reason(err: &Error) -> &'static str {
    match TotpError::of(err) {
        Some(TotpError::AlreadyEnabled) => events::FAIL_ALREADY_ENABLED,
        Some(TotpError::PendingExpired) => events::FAIL_PENDING_EXPIRED,
        Some(TotpError::CodeInvalid) => events::FAIL_INVALID_TOTP_CODE,
        Some(TotpError::TooManyAttempts) => events::FAIL_TOO_MANY_ATTEMPTS,
        Some(TotpError::RecoveryInvalid) => events::FAIL_INVALID_RECOVERY_CODE,
        _ => events::FAIL_INTERNAL,
    }
}

/// Unexpected failures surface as `error.internal_error` (original handlers).
fn internal(err: Error) -> Error {
    if TotpError::of(&err).is_some() {
        err
    } else {
        err.or_internal("error.internal_error")
    }
}

impl AdminTwoFactorService {
    pub fn new(
        auth: AdminAuthService,
        totp: AdminTotp,
        challenges: ChallengeStore,
        audit: AuditService,
    ) -> Self {
        Self {
            auth,
            totp,
            challenges,
            audit,
        }
    }

    pub fn totp(&self) -> &AdminTotp {
        &self.totp
    }

    async fn log(
        &self,
        admin_id: Id,
        username: &str,
        event_type: &str,
        result: std::result::Result<(), &str>,
        operator_id: Option<Id>,
        client: &ClientInfo,
    ) {
        let (status, reason) = match result {
            Ok(()) => (events::STATUS_SUCCESS, ""),
            Err(reason) => (events::STATUS_FAILED, reason),
        };
        self.audit
            .record_admin(
                AdminEvent {
                    admin_id,
                    username,
                    event_type,
                    status,
                    fail_reason: reason,
                    operator_id,
                },
                client,
            )
            .await;
    }

    pub async fn status(&self, admin_id: Id) -> Result<TotpStatus> {
        self.totp.status(admin_id).await.map_err(Error::internal)
    }

    pub async fn setup(&self, me: &AdminPrincipal, client: &ClientInfo) -> Result<SetupResult> {
        let res = self.totp.setup(me.id).await.map_err(internal)?;
        self.log(
            me.id,
            &me.username,
            events::TOTP_SETUP,
            Ok(()),
            None,
            client,
        )
        .await;
        Ok(res)
    }

    pub async fn enable(
        &self,
        me: &AdminPrincipal,
        code: &str,
        client: &ClientInfo,
    ) -> Result<AdminEnableResult> {
        match self.totp.enable(me.id, code).await {
            Ok(res) => {
                self.log(
                    me.id,
                    &me.username,
                    events::TOTP_ENABLED,
                    Ok(()),
                    None,
                    client,
                )
                .await;
                Ok(AdminEnableResult {
                    enabled_at: res.enabled_at,
                    recovery_codes: res.recovery_codes,
                })
            }
            Err(e) => {
                if TotpError::of(&e).is_some() {
                    self.log(
                        me.id,
                        &me.username,
                        events::TOTP_ENABLED,
                        Err(fail_reason(&e)),
                        None,
                        client,
                    )
                    .await;
                }
                Err(internal(e))
            }
        }
    }

    pub async fn disable(
        &self,
        me: &AdminPrincipal,
        factor: SecondFactor<'_>,
        client: &ClientInfo,
    ) -> Result<()> {
        if factor.is_empty() {
            return Err(Error::bad_request(totp::keys::CODE_REQUIRED));
        }
        let code = if factor.is_recovery() {
            factor.recovery_code
        } else {
            factor.code
        };
        match self.totp.disable(me.id, code, factor.is_recovery()).await {
            Ok(()) => {
                self.log(
                    me.id,
                    &me.username,
                    events::TOTP_DISABLED,
                    Ok(()),
                    None,
                    client,
                )
                .await;
                Ok(())
            }
            Err(e) => {
                if matches!(
                    TotpError::of(&e),
                    Some(TotpError::CodeInvalid | TotpError::RecoveryInvalid)
                ) {
                    self.log(
                        me.id,
                        &me.username,
                        events::TOTP_DISABLED,
                        Err(fail_reason(&e)),
                        None,
                        client,
                    )
                    .await;
                }
                Err(internal(e))
            }
        }
    }

    pub async fn regenerate(
        &self,
        me: &AdminPrincipal,
        code: &str,
        client: &ClientInfo,
    ) -> Result<Vec<String>> {
        match self.totp.regenerate_recovery_codes(me.id, code).await {
            Ok(codes) => {
                self.log(
                    me.id,
                    &me.username,
                    events::RECOVERY_REGENERATED,
                    Ok(()),
                    None,
                    client,
                )
                .await;
                Ok(codes)
            }
            Err(e) => {
                if TotpError::of(&e) == Some(TotpError::CodeInvalid) {
                    self.log(
                        me.id,
                        &me.username,
                        events::RECOVERY_REGENERATED,
                        Err(events::FAIL_INVALID_TOTP_CODE),
                        None,
                        client,
                    )
                    .await;
                }
                Err(internal(e))
            }
        }
    }

    /// Login step two (`/admin/login/verify-2fa`).
    pub async fn verify_login(
        &self,
        challenge_token: &str,
        factor: SecondFactor<'_>,
        client: &ClientInfo,
    ) -> Result<(Admin, String, DateTime<Utc>)> {
        if factor.is_empty() {
            return Err(Error::bad_request(totp::keys::CODE_REQUIRED));
        }
        let (admin_id, jti) = self.auth.parse_challenge(challenge_token)?;
        if self.challenges.is_revoked(&jti) {
            return Err(Error::unauthorized(totp::keys::CHALLENGE_INVALID));
        }
        let verified = if factor.is_recovery() {
            self.totp
                .verify_challenge_recovery(admin_id, factor.recovery_code)
                .await
        } else {
            self.totp.verify_challenge_code(admin_id, factor.code).await
        };
        let username = self.auth.username_of(admin_id).await;
        let event = if factor.is_recovery() {
            events::LOGIN_RECOVERY_CODE
        } else {
            events::LOGIN_2FA_VERIFY
        };
        if let Err(e) = verified {
            let fails = self.challenges.bump_fails(&jti);
            let reason = if factor.is_recovery() {
                events::FAIL_INVALID_RECOVERY_CODE
            } else {
                events::FAIL_INVALID_TOTP_CODE
            };
            self.log(admin_id, &username, event, Err(reason), None, client)
                .await;
            if fails >= totp::CHALLENGE_MAX_FAILURES {
                self.challenges.revoke(&jti);
                return Err(Error::unauthorized(totp::keys::TOO_MANY_ATTEMPTS));
            }
            return Err(Error::unauthorized(match TotpError::of(&e) {
                Some(TotpError::CodeInvalid) => totp::keys::CODE_INVALID,
                Some(TotpError::RecoveryInvalid) => totp::keys::RECOVERY_INVALID,
                _ => totp::keys::CHALLENGE_INVALID,
            }));
        }
        // One-shot: the challenge cannot be reused after success.
        self.challenges.revoke(&jti);
        let (admin, token, expires_at) = self
            .auth
            .complete_2fa_login(admin_id)
            .await
            .map_err(|e| Error::internal(e).or_internal("error.login_failed"))?;
        self.log(admin_id, &username, event, Ok(()), None, client)
            .await;
        Ok((admin, token, expires_at))
    }

    /// Super admin resets another administrator's 2FA.
    pub async fn reset_admin(
        &self,
        operator: &AdminPrincipal,
        target_id: Id,
        client: &ClientInfo,
    ) -> Result<()> {
        if !operator.is_super {
            return Err(Error::forbidden("error.forbidden"));
        }
        if operator.id == target_id {
            return Err(Error::bad_request(totp::keys::CANNOT_RESET_SELF));
        }
        let target = self.totp.clear(target_id).await.map_err(|e| {
            if e.is_not_found() {
                e
            } else {
                e.or_internal("error.internal_error")
            }
        })?;
        self.log(
            target_id,
            &target.username,
            events::TOTP_RESET_BY_ADMIN,
            Ok(()),
            Some(operator.id),
            client,
        )
        .await;
        Ok(())
    }

    /// Operator CLI (`admin reset-2fa`).
    pub async fn cli_reset(&self, username: &str) -> Result<Admin> {
        let admin = self
            .auth
            .repo()
            .get_by_username(username)
            .await?
            .ok_or_else(|| Error::not_found("error.user_not_found"))?;
        self.totp.clear(admin.id).await?;
        let client = cli_client();
        self.log(
            admin.id,
            &admin.username,
            events::TOTP_RESET_BY_ADMIN,
            Ok(()),
            None,
            &client,
        )
        .await;
        Ok(admin)
    }
}
