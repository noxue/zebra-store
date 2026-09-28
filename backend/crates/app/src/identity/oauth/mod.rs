//! Telegram and Google login + account binding (original `userauth`
//! `telegram_*.go` / `google*.go`, `telegramauth`, `googleauth`).
//!
//! Shared here: settings, account provisioning, the external-login exit that
//! applies 2FA (AUTH-03), login logs and the unbind rules.

pub mod google;
pub mod store;
pub mod telegram;

use std::sync::{Arc, PoisonError, RwLock};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::{DateTime, Utc};
use rand::RngCore;
use serde::Serialize;
use zs_domain::dashboard::users::{
    AdminUserRepo, LoginProviders, UnbindOutcome, UsableProviders, keeps_usable_login,
};
use zs_domain::identity::login_log;
use zs_domain::identity::oauth::{
    ExternalIdentity, ExternalIdentityRepo, JwtVerifier, OAuthEndpoints, OidcTokenClient, keys,
};
use zs_domain::identity::registration::{self, RegistrationConfig};
use zs_domain::identity::user::{DefaultMemberLevel, User, UserRepo};
use zs_domain::settings::schema::login::{GoogleAuthSetting, TelegramAuthSetting};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;

use self::google::{RedirectHandoff, RedirectIntent};
use self::store::Expiring;
use self::telegram::OidcState;
use super::admin_auth::ClientInfo;
use super::audit::AuditService;
use super::user_account::{Session, UserLoginOutcome};
use super::user_auth::UserAuthService;

/// Dependencies of [`OAuthService`].
#[derive(Clone)]
pub struct OAuthDeps {
    pub users: Arc<dyn UserRepo>,
    pub identities: Arc<dyn ExternalIdentityRepo>,
    /// Identity listing and the atomic unbind shared with the admin user directory.
    pub directory: Arc<dyn AdminUserRepo>,
    pub providers: Arc<dyn LoginProviders>,
    pub levels: Arc<dyn DefaultMemberLevel>,
    pub settings: Arc<dyn SettingsStore>,
    /// `config.yml` values used until the settings are saved.
    pub telegram_default: TelegramAuthSetting,
    pub google_default: GoogleAuthSetting,
    pub auth: UserAuthService,
    pub audit: AuditService,
    pub jwt: Arc<dyn JwtVerifier>,
    pub tokens: Arc<dyn OidcTokenClient>,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for OAuthDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OAuthDeps")
    }
}

struct Inner {
    d: OAuthDeps,
    endpoints: RwLock<OAuthEndpoints>,
    /// Telegram replay markers (`telegram:auth:replay:{uid}:{hash}`).
    replays: Expiring<()>,
    oidc_states: Expiring<OidcState>,
    intents: Expiring<RedirectIntent>,
    handoffs: Expiring<RedirectHandoff>,
}

/// Third-party login service.
#[derive(Clone)]
pub struct OAuthService {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for OAuthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OAuthService")
    }
}

/// Keeps `err` when its key is one the endpoint reports; anything else becomes
/// the endpoint's generic internal error (like the original handlers' default branch).
pub(crate) fn restrict(err: Error, allowed: &[&str], fallback: &'static str) -> Error {
    if allowed.contains(&err.key()) {
        err
    } else {
        Error::internal(err).or_internal(fallback)
    }
}

/// `n` random bytes as unpadded base64url.
pub(crate) fn random_token(n: usize) -> String {
    let mut bytes = vec![0u8; n];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Linked Telegram account (original `TelegramBindingResp`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct TelegramBinding {
    pub bound: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider_user_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub avatar_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_at: Option<DateTime<Utc>>,
    pub can_unbind: bool,
}

impl TelegramBinding {
    pub fn of(identity: Option<&ExternalIdentity>, can_unbind: bool) -> Self {
        match identity {
            None => Self::default(),
            Some(i) => Self {
                bound: true,
                provider: i.provider.clone(),
                provider_user_id: i.provider_user_id.clone(),
                username: i.username.clone(),
                avatar_url: i.avatar_url.clone(),
                auth_at: i.auth_at,
                can_unbind,
            },
        }
    }
}

/// Linked Google account (original `GoogleBindingResp`).
#[derive(Debug, Clone, Default, Serialize)]
pub struct GoogleBinding {
    pub bound: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider_user_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub username: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub avatar_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_at: Option<DateTime<Utc>>,
    pub can_unbind: bool,
}

impl OAuthService {
    pub fn new(deps: OAuthDeps) -> Self {
        let clock = deps.clock.clone();
        Self {
            inner: Arc::new(Inner {
                d: deps,
                endpoints: RwLock::new(OAuthEndpoints::default()),
                replays: Expiring::new(clock.clone()),
                oidc_states: Expiring::new(clock.clone()),
                intents: Expiring::new(clock.clone()),
                handoffs: Expiring::new(clock),
            }),
        }
    }

    fn d(&self) -> &OAuthDeps {
        &self.inner.d
    }

    fn now(&self) -> DateTime<Utc> {
        self.d().clock.now()
    }

    /// Current upstream endpoints.
    pub fn endpoints(&self) -> OAuthEndpoints {
        self.inner
            .endpoints
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Points the Telegram/Google calls at other endpoints (mock servers in tests).
    pub fn set_endpoints(&self, endpoints: OAuthEndpoints) {
        *self
            .inner
            .endpoints
            .write()
            .unwrap_or_else(PoisonError::into_inner) = endpoints;
    }

    /// Effective `telegram_auth_config` (normalised).
    pub async fn telegram_setting(&self) -> Result<TelegramAuthSetting> {
        let raw = self
            .d()
            .settings
            .get(setting_keys::TELEGRAM_AUTH_CONFIG)
            .await?;
        let fallback = self.d().telegram_default.clone();
        Ok(match raw {
            Some(v) => TelegramAuthSetting::decode(Some(&v), fallback),
            None => fallback,
        }
        .normalized())
    }

    /// Effective `google_auth_config` (normalised).
    pub async fn google_setting(&self) -> Result<GoogleAuthSetting> {
        let raw = self
            .d()
            .settings
            .get(setting_keys::GOOGLE_AUTH_CONFIG)
            .await?;
        let fallback = self.d().google_default.clone();
        Ok(match raw {
            Some(v) => GoogleAuthSetting::decode(Some(&v), fallback),
            None => fallback,
        }
        .normalized())
    }

    async fn registration(&self) -> Result<RegistrationConfig> {
        let raw = self
            .d()
            .settings
            .get(setting_keys::REGISTRATION_CONFIG)
            .await?;
        Ok(RegistrationConfig::from_value(raw.as_ref()))
    }

    /// AUTH-05: every path creating a user honours the registration switch.
    async fn ensure_registration_allowed(&self) -> Result<RegistrationConfig> {
        let reg = self.registration().await?;
        if !reg.registration_enabled {
            return Err(Error::forbidden(registration::KEY_REGISTRATION_DISABLED));
        }
        Ok(reg)
    }

    /// Default member level of new accounts (0 when none, a551e8f8).
    async fn default_level(&self) -> Id {
        match self.d().levels.default_level_id().await {
            Ok(level) => level.unwrap_or(0),
            Err(error) => {
                tracing::warn!(%error, "failed to resolve the default member level");
                0
            }
        }
    }

    /// An existing, enabled user.
    async fn active_user(&self, id: Id) -> Result<User> {
        let user = self
            .d()
            .users
            .get(id)
            .await?
            .ok_or_else(|| Error::not_found(keys::USER_NOT_FOUND))?;
        if !user.is_active() {
            return Err(Error::unauthorized(keys::USER_DISABLED));
        }
        Ok(user)
    }

    /// Common exit of every third-party login: a 2FA challenge when the account
    /// has TOTP enabled (AUTH-03), otherwise an access token.
    async fn complete_login(&self, mut user: User, source: &str) -> Result<UserLoginOutcome> {
        if user.totp_enabled_at.is_some() {
            let c = self.d().auth.issue_challenge(user.id, false, source)?;
            return Ok(UserLoginOutcome::Challenge {
                token: c.token,
                expires_at: c.expires_at,
            });
        }
        let (token, expires_at) = self.d().auth.issue_token(&user, false)?;
        user.last_login_at = Some(self.now());
        self.d().users.save(&user).await?;
        Ok(UserLoginOutcome::Token(Session {
            user,
            token,
            expires_at,
        }))
    }

    async fn record(
        &self,
        outcome: std::result::Result<&User, &str>,
        source: &str,
        client: &ClientInfo,
    ) {
        let (email, user_id, status, reason) = match outcome {
            Ok(u) => (u.email.as_str(), u.id, login_log::STATUS_SUCCESS, ""),
            Err(reason) => ("", 0, login_log::STATUS_FAILED, reason),
        };
        self.d()
            .audit
            .record_user_login_from(email, user_id, status, reason, source, client)
            .await;
    }

    /// Records a request rejected before the service ran (malformed body).
    pub async fn record_bad_request(&self, source: &str, client: &ClientInfo) {
        self.record(Err(login_log::reasons::BAD_REQUEST), source, client)
            .await;
    }

    async fn usable(&self) -> Result<UsableProviders> {
        self.d().providers.usable().await
    }

    /// Whether removing `identity_id` keeps a usable login method.
    async fn can_unbind(&self, user: &User, identity_id: Id) -> Result<bool> {
        let usable = self.usable().await?;
        let identities = self.d().directory.identities(user.id).await?;
        Ok(keeps_usable_login(user, identity_id, &identities, usable))
    }

    /// Removes the `provider` identity unless it is the last usable login method.
    async fn unbind(
        &self,
        user_id: Id,
        provider: &str,
        locked_key: &'static str,
        not_bound_key: &'static str,
    ) -> Result<()> {
        let fail = |e: Error| Error::internal(e).or_internal(keys::USER_UPDATE_FAILED);
        let usable = self.usable().await.map_err(fail)?;
        match self
            .d()
            .directory
            .unbind(user_id, provider, usable)
            .await
            .map_err(fail)?
        {
            UnbindOutcome::Unbound => Ok(()),
            UnbindOutcome::NotBound => Err(Error::bad_request(not_bound_key)),
            UnbindOutcome::Locked => Err(Error::bad_request(locked_key)),
            other @ (UnbindOutcome::UserNotFound | UnbindOutcome::UserDisabled) => {
                Err(Error::internal_msg(format!("unbind rejected: {other:?}"))
                    .or_internal(keys::USER_UPDATE_FAILED))
            }
        }
    }

    /// Returns `user` after a lookup error typed like the original
    /// `GetTelegramBinding`/`GetGoogleBinding` (404 or `error.user_fetch_failed`).
    async fn binding_user(&self, user_id: Id) -> Result<User> {
        match self.active_user(user_id).await {
            Ok(u) => Ok(u),
            Err(e) if e.is_not_found() => Err(e),
            Err(e) => Err(Error::internal(e).or_internal(keys::USER_FETCH_FAILED)),
        }
    }
}
