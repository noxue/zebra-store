//! Storefront accounts: registration, login (with 2FA), password reset,
//! profile, email change and self-service 2FA (original `userauth`).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use zs_domain::identity::captcha::{self, CaptchaPayload, scenes};
use zs_domain::identity::email;
use zs_domain::identity::login_log::{self, reasons};
use zs_domain::identity::mailer::{self, BrandScope};
use zs_domain::identity::password::PasswordPolicy;
use zs_domain::identity::registration::{self, RegistrationConfig};
use zs_domain::identity::totp::{self, TotpError, TotpStatus};
use zs_domain::identity::user::{DefaultMemberLevel, NewUser, User, UserRepo};
use zs_domain::identity::verify_code::{self, Purpose};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, ErrorKind, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;

use super::admin_2fa::SecondFactor;
use super::admin_auth::ClientInfo;
use super::audit::AuditService;
use super::captcha::CaptchaService;
use super::challenge::ChallengeStore;
use super::password;
use super::totp::{SetupResult, TotpService, UserAccounts};
use super::user_auth::UserAuthService;
use super::verify_code::VerifyCodeService;

/// TOTP service over user accounts.
pub type UserTotp = TotpService<UserAccounts>;

/// Only a new-email code is needed (Telegram placeholder accounts).
pub const EMAIL_CHANGE_BIND_ONLY: &str = "bind_only";
/// Codes for the old and the new address are needed.
pub const EMAIL_CHANGE_WITH_OLD_AND_NEW: &str = "change_with_old_and_new";
/// First password: no old password required.
pub const PASSWORD_SET_WITHOUT_OLD: &str = "set_without_old";
/// Regular change: the old password is required.
pub const PASSWORD_CHANGE_WITH_OLD: &str = "change_with_old";

/// Login/registration user summary (original `UserAuthBriefResp`).
#[derive(Debug, Clone, Serialize)]
pub struct UserBrief {
    pub id: Id,
    pub email: String,
    pub nickname: String,
    pub email_verified_at: Option<DateTime<Utc>>,
}

impl From<&User> for UserBrief {
    fn from(u: &User) -> Self {
        Self {
            id: u.id,
            email: u.email.clone(),
            nickname: u.display_name.clone(),
            email_verified_at: u.email_verified_at,
        }
    }
}

/// `GET /me` (original `UserProfileResp`).
#[derive(Debug, Clone, Serialize)]
pub struct UserProfile {
    pub id: Id,
    pub email: String,
    pub nickname: String,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub locale: String,
    pub member_level_id: Id,
    pub total_recharged: Amount,
    pub total_spent: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email_change_mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub password_change_mode: String,
}

/// Access token issued to a user.
#[derive(Debug, Clone)]
pub struct Session {
    pub user: User,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// Result of `/auth/login`.
#[derive(Debug, Clone)]
pub enum UserLoginOutcome {
    Token(Session),
    Challenge {
        token: String,
        expires_at: DateTime<Utc>,
    },
}

/// `/me/2fa/enable` response (a fresh token replaces the revoked session).
#[derive(Debug, Clone, Serialize)]
pub struct UserEnableResult {
    pub enabled_at: DateTime<Utc>,
    pub recovery_codes: Vec<String>,
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

/// `/auth/register` input.
#[derive(Debug, Clone, Default)]
pub struct RegisterInput {
    pub email: String,
    pub password: String,
    pub code: String,
    pub agreement_accepted: bool,
}

/// `/auth/login` input.
#[derive(Debug, Clone, Default)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
    pub remember_me: bool,
    pub captcha: CaptchaPayload,
}

/// Dependencies of [`UserAccountService`].
#[derive(Clone)]
pub struct UserAccountDeps {
    pub users: Arc<dyn UserRepo>,
    pub auth: UserAuthService,
    pub totp: UserTotp,
    pub challenges: ChallengeStore,
    pub codes: VerifyCodeService,
    pub captcha: CaptchaService,
    pub audit: AuditService,
    pub settings: Arc<dyn SettingsStore>,
    pub levels: Arc<dyn DefaultMemberLevel>,
    pub policy: PasswordPolicy,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for UserAccountDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserAccountDeps")
    }
}

/// User account service.
#[derive(Clone)]
pub struct UserAccountService {
    d: UserAccountDeps,
}

impl std::fmt::Debug for UserAccountService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserAccountService")
    }
}

fn not_found() -> Error {
    Error::not_found("error.user_not_found")
}

/// Maps a captcha failure to its login-log reason.
fn captcha_reason(err: &Error) -> &'static str {
    match err.key() {
        captcha::keys::REQUIRED => reasons::CAPTCHA_REQUIRED,
        captcha::keys::INVALID => reasons::CAPTCHA_INVALID,
        captcha::keys::CONFIG_INVALID => reasons::CAPTCHA_CONFIG_INVALID,
        _ => reasons::CAPTCHA_VERIFY_FAILED,
    }
}

/// Replaces unexpected internal errors with `key`, keeping the SMTP configuration error.
fn or_fail(err: Error, key: &'static str) -> Error {
    if err.key() == mailer::KEY_NOT_CONFIGURED {
        err
    } else {
        err.or_internal(key)
    }
}

/// Captcha failures keep their keys; anything else is `error.captcha_verify_failed`.
fn captcha_error(err: Error) -> Error {
    match err.key() {
        captcha::keys::REQUIRED | captcha::keys::INVALID | captcha::keys::CONFIG_INVALID => err,
        _ => Error::internal(err).or_internal(captcha::keys::VERIFY_FAILED),
    }
}

impl UserAccountService {
    pub fn new(deps: UserAccountDeps) -> Self {
        Self { d: deps }
    }

    pub fn totp(&self) -> &UserTotp {
        &self.d.totp
    }

    /// The `registration_config` setting.
    pub async fn registration(&self) -> Result<RegistrationConfig> {
        let raw = self
            .d
            .settings
            .get(setting_keys::REGISTRATION_CONFIG)
            .await?;
        Ok(RegistrationConfig::from_value(raw.as_ref()))
    }

    async fn user(&self, id: Id) -> Result<User> {
        self.d.users.get(id).await?.ok_or_else(not_found)
    }

    /// Telegram placeholder accounts are never email-verified and must set a password.
    async fn ensure_placeholder_state(&self, user: &mut User) -> Result<()> {
        if !email::is_placeholder(&user.email) {
            return Ok(());
        }
        let mut changed = false;
        if user.email_verified_at.is_some() {
            user.email_verified_at = None;
            changed = true;
        }
        if !user.password_setup_required {
            user.password_setup_required = true;
            changed = true;
        }
        if changed {
            self.d.users.save(user).await?;
        }
        Ok(())
    }

    fn email_change_mode(user: &User) -> &'static str {
        if email::is_placeholder(&user.email) {
            EMAIL_CHANGE_BIND_ONLY
        } else {
            EMAIL_CHANGE_WITH_OLD_AND_NEW
        }
    }

    fn password_change_mode(user: &User) -> &'static str {
        if user.password_setup_required {
            PASSWORD_SET_WITHOUT_OLD
        } else {
            PASSWORD_CHANGE_WITH_OLD
        }
    }

    fn profile_of(user: &User) -> UserProfile {
        UserProfile {
            id: user.id,
            email: user.email.clone(),
            nickname: user.display_name.clone(),
            email_verified_at: user.email_verified_at,
            locale: user.locale.clone(),
            member_level_id: user.member_level_id,
            total_recharged: user.total_recharged,
            total_spent: user.total_spent,
            email_change_mode: Self::email_change_mode(user).into(),
            password_change_mode: Self::password_change_mode(user).into(),
        }
    }

    fn issue(&self, mut user: User, remember_me: bool) -> Result<(User, String, DateTime<Utc>)> {
        let (token, expires_at) = self.d.auth.issue_token(&user, remember_me)?;
        user.last_login_at = Some(self.d.clock.now());
        Ok((user, token, expires_at))
    }

    async fn record(&self, email: &str, user_id: Id, failure: Option<&str>, client: &ClientInfo) {
        self.record_from(email, user_id, failure, "", client).await;
    }

    /// Records a login attempt of `source` (the 2FA step keeps the source of the
    /// first step, e.g. `telegram`).
    async fn record_from(
        &self,
        email: &str,
        user_id: Id,
        failure: Option<&str>,
        source: &str,
        client: &ClientInfo,
    ) {
        let (status, reason) = match failure {
            None => (login_log::STATUS_SUCCESS, ""),
            Some(r) => (login_log::STATUS_FAILED, r),
        };
        self.d
            .audit
            .record_user_login_from(email, user_id, status, reason, source, client)
            .await;
    }

    /// `/auth/register`.
    pub async fn register(&self, input: &RegisterInput) -> Result<Session> {
        let reg = self
            .registration()
            .await
            .map_err(|e| e.or_internal("error.register_failed"))?;
        if !reg.registration_enabled {
            return Err(Error::forbidden(registration::KEY_REGISTRATION_DISABLED));
        }
        if !input.agreement_accepted {
            return Err(Error::bad_request("error.agreement_required"));
        }
        let addr = email::normalize_user_supplied(&input.email)?;
        reg.check_email_domain(&addr)?;
        self.d.policy.validate(&input.password)?;
        if self
            .d
            .users
            .get_by_email(&addr)
            .await
            .map_err(|e| e.or_internal("error.register_failed"))?
            .is_some()
        {
            return Err(Error::bad_request("error.email_exists"));
        }
        if reg.email_verification_enabled {
            self.d
                .codes
                .verify(&addr, Purpose::Register, &input.code)
                .await
                .map_err(|e| e.or_internal("error.register_failed"))?;
        }
        let level = match self.d.levels.default_level_id().await {
            Ok(level) => level.unwrap_or(0),
            Err(error) => {
                tracing::warn!(%error, "failed to resolve the default member level");
                0
            }
        };
        let hash = password::hash(&input.password)
            .await
            .map_err(|e| e.or_internal("error.register_failed"))?;
        let now = self.d.clock.now();
        let user = self
            .d
            .users
            .create(&NewUser {
                email: addr.clone(),
                password_hash: hash,
                display_name: email::nickname_from(&addr),
                locale: zs_shared::i18n::DEFAULT_LOCALE.into(),
                member_level_id: level,
                email_verified_at: Some(now),
                password_setup_required: false,
            })
            .await
            .map_err(|e| e.or_internal("error.register_failed"))?;
        let (user, token, expires_at) = self.issue(user, false)?;
        self.d
            .users
            .save(&user)
            .await
            .map_err(|e| e.or_internal("error.register_failed"))?;
        Ok(Session {
            user,
            token,
            expires_at,
        })
    }

    async fn check_credentials(&self, input: &LoginInput) -> Result<User> {
        let addr = email::normalize(&input.email)?;
        let Some(user) = self.d.users.get_by_email(&addr).await? else {
            // AUTH-09: same bcrypt cost whether or not the account exists.
            password::verify(&input.password, password::DUMMY_HASH).await;
            return Err(Error::unauthorized("error.login_invalid"));
        };
        // The password is checked first so disabled/unverified states are not
        // revealed to someone who does not know it.
        if !password::verify(&input.password, &user.password_hash).await {
            return Err(Error::unauthorized("error.login_invalid"));
        }
        if !user.is_active() {
            return Err(Error::unauthorized("error.user_disabled"));
        }
        if user.email_verified_at.is_none() {
            return Err(Error::unauthorized("error.email_not_verified"));
        }
        Ok(user)
    }

    /// `/auth/login`: captcha, credentials, then either a token or a 2FA challenge.
    pub async fn login(&self, input: &LoginInput, client: &ClientInfo) -> Result<UserLoginOutcome> {
        if let Err(e) = self
            .d
            .captcha
            .verify(scenes::LOGIN, &input.captcha, &client.ip)
            .await
        {
            self.record(&input.email, 0, Some(captcha_reason(&e)), client)
                .await;
            return Err(captcha_error(e));
        }
        let user = match self.check_credentials(input).await {
            Ok(u) => u,
            Err(e) => {
                let reason = match e.key() {
                    email::KEY_EMAIL_INVALID => reasons::INVALID_EMAIL,
                    "error.login_invalid" => reasons::INVALID_CREDENTIALS,
                    "error.email_not_verified" => reasons::EMAIL_NOT_VERIFIED,
                    "error.user_disabled" => reasons::USER_DISABLED,
                    _ => reasons::INTERNAL_ERROR,
                };
                self.record(&input.email, 0, Some(reason), client).await;
                return Err(e.or_internal("error.login_failed"));
            }
        };
        if user.totp_enabled_at.is_some() {
            let challenge = self
                .d
                .auth
                .issue_challenge(user.id, input.remember_me, login_log::SOURCE_WEB)
                .map_err(|e| e.or_internal("error.login_failed"))?;
            self.record(&user.email, user.id, None, client).await;
            return Ok(UserLoginOutcome::Challenge {
                token: challenge.token,
                expires_at: challenge.expires_at,
            });
        }
        let (user, token, expires_at) = self
            .issue(user, input.remember_me)
            .map_err(|e| e.or_internal("error.login_failed"))?;
        self.d
            .users
            .save(&user)
            .await
            .map_err(|e| e.or_internal("error.login_failed"))?;
        self.record(&user.email, user.id, None, client).await;
        Ok(UserLoginOutcome::Token(Session {
            user,
            token,
            expires_at,
        }))
    }

    /// Records a request rejected before reaching [`Self::login`] (malformed body).
    pub async fn record_bad_login_request(&self, email_addr: &str, client: &ClientInfo) {
        self.record(email_addr, 0, Some(reasons::BAD_REQUEST), client)
            .await;
    }

    async fn complete_login(&self, user_id: Id, remember_me: bool) -> Result<Session> {
        let user = self.user(user_id).await?;
        let (user, token, expires_at) = self.issue(user, remember_me)?;
        self.d.users.save(&user).await?;
        Ok(Session {
            user,
            token,
            expires_at,
        })
    }

    /// `/auth/login/verify-2fa`.
    pub async fn verify_login(
        &self,
        challenge_token: &str,
        factor: SecondFactor<'_>,
        client: &ClientInfo,
    ) -> Result<Session> {
        if factor.code.is_empty() && factor.recovery_code.is_empty() {
            return Err(Error::bad_request(totp::keys::CODE_REQUIRED));
        }
        let claims = match self.d.auth.parse_challenge(challenge_token) {
            Ok(c) => c,
            Err(e) => {
                self.record("", 0, Some(reasons::CHALLENGE_INVALID), client)
                    .await;
                return Err(e);
            }
        };
        if self.d.challenges.is_revoked(&claims.jti) {
            self.record_from(
                "",
                claims.user_id,
                Some(reasons::CHALLENGE_INVALID),
                &claims.login_source,
                client,
            )
            .await;
            return Err(Error::unauthorized(totp::keys::CHALLENGE_INVALID));
        }
        let is_recovery = !factor.recovery_code.is_empty();
        let verified = if is_recovery {
            self.d
                .totp
                .verify_challenge_recovery(claims.user_id, factor.recovery_code)
                .await
        } else {
            self.d
                .totp
                .verify_challenge_code(claims.user_id, factor.code)
                .await
        };
        if let Err(e) = verified {
            let email_addr = match self.d.users.get(claims.user_id).await {
                Ok(Some(u)) => u.email,
                _ => String::new(),
            };
            let fails = self.d.challenges.bump_fails(&claims.jti);
            let reason = match TotpError::of(&e) {
                Some(TotpError::RecoveryInvalid) => reasons::INVALID_RECOVERY_CODE,
                Some(TotpError::CodeInvalid) => reasons::INVALID_TOTP_CODE,
                _ => reasons::INTERNAL_ERROR,
            };
            self.record_from(
                &email_addr,
                claims.user_id,
                Some(reason),
                &claims.login_source,
                client,
            )
            .await;
            if fails >= totp::CHALLENGE_MAX_FAILURES {
                self.d.challenges.revoke(&claims.jti);
                return Err(Error::unauthorized(totp::keys::TOO_MANY_ATTEMPTS));
            }
            return Err(Error::unauthorized(match TotpError::of(&e) {
                Some(TotpError::CodeInvalid) => totp::keys::CODE_INVALID,
                Some(TotpError::RecoveryInvalid) => totp::keys::RECOVERY_INVALID,
                _ => totp::keys::CHALLENGE_INVALID,
            }));
        }
        self.d.challenges.revoke(&claims.jti);
        let session = self
            .complete_login(claims.user_id, claims.remember_me)
            .await
            .map_err(|e| Error::internal(e).or_internal("error.login_failed"))?;
        self.record_from(
            &session.user.email,
            session.user.id,
            None,
            &claims.login_source,
            client,
        )
        .await;
        Ok(session)
    }

    /// `/auth/send-verify-code` (switches, captcha, then the purpose rules).
    pub async fn send_verify_code(
        &self,
        email_addr: &str,
        purpose_raw: &str,
        captcha_payload: &CaptchaPayload,
        locale: &str,
        client_ip: &str,
        scope: &BrandScope,
    ) -> Result<()> {
        let fail = "error.send_verify_code_failed";
        let reg = self.registration().await.map_err(|e| e.or_internal(fail))?;
        if !reg.email_verification_enabled {
            return Err(Error::forbidden(registration::KEY_VERIFICATION_DISABLED));
        }
        let purpose_lower = purpose_raw.trim().to_lowercase();
        if purpose_lower == Purpose::Register.as_str() && !reg.registration_enabled {
            return Err(Error::forbidden(registration::KEY_REGISTRATION_DISABLED));
        }
        let scene = match purpose_lower.as_str() {
            "register" => Some(scenes::REGISTER_SEND_CODE),
            "reset" => Some(scenes::RESET_SEND_CODE),
            _ => None,
        };
        if let Some(scene) = scene {
            self.d
                .captcha
                .verify(scene, captcha_payload, client_ip)
                .await
                .map_err(captcha_error)?;
        }
        let purpose = Purpose::parse(&purpose_lower);
        let addr = if purpose == Some(Purpose::Register) {
            email::normalize_user_supplied(email_addr)?
        } else {
            email::normalize(email_addr)?
        };
        let purpose =
            purpose.ok_or_else(|| Error::bad_request(verify_code::keys::PURPOSE_INVALID))?;
        let mut locale = locale.to_owned();
        match purpose {
            Purpose::Register => {
                reg.check_email_domain(&addr)?;
                if self
                    .d
                    .users
                    .get_by_email(&addr)
                    .await
                    .map_err(|e| e.or_internal(fail))?
                    .is_some()
                {
                    return Err(Error::bad_request("error.email_exists"));
                }
            }
            Purpose::Reset | Purpose::TelegramBind => {
                let user = self
                    .d
                    .users
                    .get_by_email(&addr)
                    .await
                    .map_err(|e| e.or_internal(fail))?
                    .ok_or_else(not_found)?;
                if !user.locale.trim().is_empty() {
                    locale = user.locale;
                }
            }
            Purpose::ChangeEmailOld | Purpose::ChangeEmailNew => {}
        }
        self.d
            .codes
            .send_for(&addr, purpose, &locale, scope)
            .await
            .map_err(|e| or_fail(e, fail))
    }

    /// `/auth/forgot-password`.
    pub async fn forgot_password(
        &self,
        email_addr: &str,
        code: &str,
        new_password: &str,
    ) -> Result<()> {
        let fail = "error.reset_failed";
        let reg = self.registration().await.map_err(|e| e.or_internal(fail))?;
        if !reg.email_verification_enabled {
            return Err(Error::forbidden(registration::KEY_PASSWORD_RESET_DISABLED));
        }
        let addr = email::normalize(email_addr)?;
        self.d.policy.validate(new_password)?;
        let mut user = self
            .d
            .users
            .get_by_email(&addr)
            .await
            .map_err(|e| e.or_internal(fail))?
            .ok_or_else(not_found)?;
        self.d
            .codes
            .verify(&addr, Purpose::Reset, code)
            .await
            .map_err(|e| e.or_internal(fail))?;
        user.password_hash = password::hash(new_password)
            .await
            .map_err(|e| e.or_internal(fail))?;
        user.password_setup_required = false;
        let now = self.d.clock.now();
        user.token_version += 1;
        user.token_invalid_before = Some(now);
        self.d
            .users
            .save(&user)
            .await
            .map_err(|e| e.or_internal(fail))
    }

    /// `PUT /me/password`; revokes every session of the user.
    pub async fn change_password(&self, user_id: Id, old: &str, new: &str) -> Result<()> {
        let fail = "error.save_failed";
        let mut user = self.user(user_id).await.map_err(|e| e.or_internal(fail))?;
        self.ensure_placeholder_state(&mut user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        if Self::password_change_mode(&user) == PASSWORD_CHANGE_WITH_OLD
            && !password::verify(old, &user.password_hash).await
        {
            return Err(Error::bad_request("error.password_old_invalid"));
        }
        self.d.policy.validate(new)?;
        user.password_hash = password::hash(new).await.map_err(|e| e.or_internal(fail))?;
        user.password_setup_required = false;
        let now = self.d.clock.now();
        user.token_version += 1;
        user.token_invalid_before = Some(now);
        self.d
            .users
            .save(&user)
            .await
            .map_err(|e| e.or_internal(fail))
    }

    /// `GET /me`.
    pub async fn profile(&self, user_id: Id) -> Result<UserProfile> {
        let fail = "error.user_fetch_failed";
        let mut user = self.user(user_id).await.map_err(|e| e.or_internal(fail))?;
        self.ensure_placeholder_state(&mut user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        Ok(Self::profile_of(&user))
    }

    /// `PUT /me/profile`.
    pub async fn update_profile(
        &self,
        user_id: Id,
        nickname: Option<&str>,
        locale: Option<&str>,
    ) -> Result<UserProfile> {
        let fail = "error.user_update_failed";
        let mut user = self.user(user_id).await.map_err(|e| e.or_internal(fail))?;
        let mut updated = false;
        if let Some(n) = nickname.map(str::trim).filter(|n| !n.is_empty()) {
            user.display_name = n.to_owned();
            updated = true;
        }
        if let Some(l) = locale.map(str::trim).filter(|l| !l.is_empty()) {
            user.locale = l.to_owned();
            updated = true;
        }
        if !updated {
            return Err(Error::bad_request("error.profile_empty"));
        }
        self.d
            .users
            .save(&user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        self.ensure_placeholder_state(&mut user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        Ok(Self::profile_of(&user))
    }

    /// `POST /me/email/send-verify-code` (`kind` = `old` | `new`).
    pub async fn send_change_email_code(
        &self,
        user_id: Id,
        kind: &str,
        new_email: &str,
        locale: &str,
        scope: &BrandScope,
    ) -> Result<()> {
        let fail = "error.send_verify_code_failed";
        let mut user = self.user(user_id).await.map_err(|e| e.or_internal(fail))?;
        let locale = if user.locale.trim().is_empty() {
            locale.to_owned()
        } else {
            user.locale.clone()
        };
        self.ensure_placeholder_state(&mut user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        let invalid = || Error::bad_request("error.email_change_invalid");
        match kind.trim().to_lowercase().as_str() {
            "old" => {
                if Self::email_change_mode(&user) == EMAIL_CHANGE_BIND_ONLY {
                    return Err(invalid());
                }
                self.d
                    .codes
                    .send_for(&user.email, Purpose::ChangeEmailOld, &locale, scope)
                    .await
                    .map_err(|e| or_fail(e, fail))
            }
            "new" => {
                let addr = email::normalize_user_supplied(new_email)?;
                if addr.eq_ignore_ascii_case(&user.email) {
                    return Err(invalid());
                }
                if self
                    .d
                    .users
                    .get_by_email(&addr)
                    .await
                    .map_err(|e| e.or_internal(fail))?
                    .is_some()
                {
                    return Err(Error::bad_request("error.email_change_exists"));
                }
                self.d
                    .codes
                    .send_for(&addr, Purpose::ChangeEmailNew, &locale, scope)
                    .await
                    .map_err(|e| or_fail(e, fail))
            }
            _ => Err(invalid()),
        }
    }

    /// `POST /me/email/change`.
    pub async fn change_email(
        &self,
        user_id: Id,
        new_email: &str,
        old_code: &str,
        new_code: &str,
    ) -> Result<UserProfile> {
        let fail = "error.email_change_failed";
        let mut user = self.user(user_id).await.map_err(|e| e.or_internal(fail))?;
        self.ensure_placeholder_state(&mut user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        let mode = Self::email_change_mode(&user);
        let addr = email::normalize_user_supplied(new_email)?;
        if addr.eq_ignore_ascii_case(&user.email) {
            return Err(Error::bad_request("error.email_change_invalid"));
        }
        if self
            .d
            .users
            .get_by_email(&addr)
            .await
            .map_err(|e| e.or_internal(fail))?
            .is_some()
        {
            return Err(Error::bad_request("error.email_change_exists"));
        }
        if mode != EMAIL_CHANGE_BIND_ONLY {
            self.d
                .codes
                .verify(&user.email, Purpose::ChangeEmailOld, old_code)
                .await
                .map_err(|e| e.or_internal(fail))?;
        }
        self.d
            .codes
            .verify(&addr, Purpose::ChangeEmailNew, new_code)
            .await
            .map_err(|e| e.or_internal(fail))?;
        user.email = addr;
        user.email_verified_at = Some(self.d.clock.now());
        self.d
            .users
            .save(&user)
            .await
            .map_err(|e| e.or_internal(fail))?;
        Ok(Self::profile_of(&user))
    }

    /// `/me/2fa/status`.
    pub async fn totp_status(&self, user_id: Id) -> Result<TotpStatus> {
        self.d.totp.status(user_id).await.map_err(Error::internal)
    }

    /// `/me/2fa/setup`.
    pub async fn totp_setup(&self, user_id: Id) -> Result<SetupResult> {
        self.d
            .totp
            .setup(user_id)
            .await
            .map_err(|e| e.or_internal("error.internal_error"))
    }

    /// `/me/2fa/enable`: enabling revokes other sessions, so a new token is issued.
    pub async fn totp_enable(&self, user_id: Id, code: &str) -> Result<UserEnableResult> {
        let res = self
            .d
            .totp
            .enable(user_id, code)
            .await
            .map_err(|e| e.or_internal("error.internal_error"))?;
        let (token, expires_at) = match self.complete_login(user_id, false).await {
            Ok(s) => (s.token, s.expires_at),
            Err(error) => {
                tracing::warn!(%error, user_id, "failed to re-issue token after enabling 2FA");
                (String::new(), res.enabled_at)
            }
        };
        Ok(UserEnableResult {
            enabled_at: res.enabled_at,
            recovery_codes: res.recovery_codes,
            token,
            expires_at,
        })
    }

    /// `/me/2fa/disable`.
    pub async fn totp_disable(&self, user_id: Id, factor: SecondFactor<'_>) -> Result<()> {
        if factor.code.is_empty() && factor.recovery_code.is_empty() {
            return Err(Error::bad_request(totp::keys::CODE_REQUIRED));
        }
        let (code, is_recovery) = if factor.recovery_code.is_empty() {
            (factor.code, false)
        } else {
            (factor.recovery_code, true)
        };
        self.d
            .totp
            .disable(user_id, code, is_recovery)
            .await
            .map_err(|e| e.or_internal("error.internal_error"))
    }

    /// `/me/2fa/recovery-codes/regenerate`.
    pub async fn totp_regenerate(&self, user_id: Id, code: &str) -> Result<Vec<String>> {
        self.d
            .totp
            .regenerate_recovery_codes(user_id, code)
            .await
            .map_err(|e| e.or_internal("error.internal_error"))
    }

    /// `DELETE /admin/users/:id/2fa`: clears the user's 2FA and revokes their sessions.
    pub async fn admin_reset_totp(
        &self,
        operator_id: Id,
        user_id: Id,
        client: &ClientInfo,
    ) -> Result<User> {
        if operator_id <= 0 {
            return Err(Error::new(ErrorKind::Internal, "error.internal_error"));
        }
        let user = self
            .d
            .users
            .get(user_id)
            .await
            .map_err(|e| e.or_internal("error.internal_error"))?
            .ok_or_else(not_found)?;
        if user.totp_enabled_at.is_none() {
            return Err(TotpError::NotEnabled.into());
        }
        let target = self
            .d
            .totp
            .clear(user_id)
            .await
            .map_err(|e| e.or_internal("error.internal_error"))?;
        tracing::warn!(
            operator_admin_id = operator_id,
            target_user_id = target.id,
            target_email = %target.email,
            client_ip = %client.ip,
            user_agent = %client.user_agent,
            request_id = %client.request_id,
            "admin_reset_user_2fa"
        );
        Ok(target)
    }
}
