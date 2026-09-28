//! Google login, binding and the redirect (`form_post`) flow
//! (original `google.go`, `google_redirect.go`, `googleauth`; AUTH-01).

use chrono::Duration;
use zs_domain::identity::google::{self as g, GoogleIdentity, MAX_CREDENTIAL_BYTES};
use zs_domain::identity::login_log::reasons as log_reasons;
use zs_domain::identity::oauth::{
    ExternalIdentity, JwksProfile, JwtFailure, NewExternalIdentity, PROVIDER_GOOGLE,
    RedirectTenant, SOURCE_GOOGLE, google_config_invalid, google_unavailable, keys, reasons,
};
use zs_domain::identity::registration;
use zs_domain::identity::user::{NewUser, User};
use zs_domain::{Error, Id, Result};

use super::{GoogleBinding, OAuthService, random_token, restrict};
use crate::identity::admin_auth::ClientInfo;
use crate::identity::password;
use crate::identity::user_account::UserLoginOutcome;

/// Lifetime of a redirect intent (original `GoogleRedirectIntentTTL`, 10 min).
pub const INTENT_TTL_SECONDS: i64 = 10 * 60;
/// Lifetime of a verified-claims handoff (original `GoogleRedirectHandoffTTL`, 2 min).
pub const HANDOFF_TTL_SECONDS: i64 = 2 * 60;
/// Random bytes of a redirect state / handoff handle (base64url, 43 chars).
const HANDLE_BYTES: usize = 32;
/// Length of a canonical handle.
const HANDLE_LEN: usize = 43;
/// Random bytes of the unusable password of accounts created from Google.
const PASSWORD_SEED_BYTES: usize = 32;

/// Keys reported by the Google login endpoints (`googleLoginErrorRules`).
const LOGIN_KEYS: &[&str] = &[
    keys::GOOGLE_DISABLED,
    keys::GOOGLE_CONFIG_INVALID,
    keys::GOOGLE_UNAVAILABLE,
    keys::GOOGLE_CREDENTIAL_INVALID,
    keys::GOOGLE_CREDENTIAL_EXPIRED,
    keys::GOOGLE_EMAIL_UNVERIFIED,
    keys::GOOGLE_AUTO_LINK_FORBIDDEN,
    keys::GOOGLE_ALREADY_BOUND,
    registration::KEY_DOMAIN_NOT_ALLOWED,
    keys::USER_DISABLED,
    registration::KEY_REGISTRATION_DISABLED,
];

/// Keys reported by the Google bind endpoints (`respondGoogleBindError`).
const BIND_KEYS: &[&str] = &[
    keys::GOOGLE_DISABLED,
    keys::GOOGLE_CONFIG_INVALID,
    keys::GOOGLE_UNAVAILABLE,
    keys::GOOGLE_CREDENTIAL_INVALID,
    keys::GOOGLE_CREDENTIAL_EXPIRED,
    keys::GOOGLE_EMAIL_UNVERIFIED,
    keys::GOOGLE_BIND_CONFLICT,
    keys::GOOGLE_ALREADY_BOUND,
    keys::USER_DISABLED,
];

/// Redirect-state errors (answered by the redirect error mapping, not the
/// login/bind one).
pub const REDIRECT_STATE_KEYS: &[&str] = &[
    keys::GOOGLE_REDIRECT_SESSION_EXPIRED,
    keys::GOOGLE_REDIRECT_CONTEXT_MISMATCH,
];

/// Flow of a redirect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedirectFlow {
    Login,
    Bind,
}

impl RedirectFlow {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Bind => "bind",
        }
    }
}

/// Server-side redirect intent bound to flow, user and tenant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectIntent {
    flow: RedirectFlow,
    user_id: Id,
    tenant: RedirectTenant,
}

/// Verified claims waiting to be exchanged (never the raw credential).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RedirectHandoff {
    flow: RedirectFlow,
    user_id: Id,
    tenant: RedirectTenant,
    identity: GoogleIdentity,
}

/// Result of the `form_post` callback: the flow of the consumed intent (known
/// even when a later step fails) and the handoff handle or the error.
#[derive(Debug)]
pub struct RedirectCompletion {
    pub flow: RedirectFlow,
    pub handoff: Result<String>,
}

fn session_expired() -> Error {
    Error::bad_request(keys::GOOGLE_REDIRECT_SESSION_EXPIRED)
}

fn context_mismatch() -> Error {
    Error::forbidden(keys::GOOGLE_REDIRECT_CONTEXT_MISMATCH)
}

/// Canonical handle format: 32 random bytes as unpadded base64url.
pub fn valid_handle(value: &str) -> bool {
    use base64::Engine as _;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    value.len() == HANDLE_LEN
        && URL_SAFE_NO_PAD
            .decode(value)
            .is_ok_and(|d| d.len() == HANDLE_BYTES && URL_SAFE_NO_PAD.encode(&d) == value)
}

/// Login-log reason of a Google login failure.
fn login_reason(err: &Error) -> &'static str {
    match err.key() {
        keys::GOOGLE_DISABLED | keys::GOOGLE_CONFIG_INVALID | keys::GOOGLE_UNAVAILABLE => {
            reasons::GOOGLE_CONFIG
        }
        keys::GOOGLE_CREDENTIAL_INVALID
        | keys::GOOGLE_CREDENTIAL_EXPIRED
        | keys::GOOGLE_EMAIL_UNVERIFIED
        | keys::GOOGLE_AUTO_LINK_FORBIDDEN
        | keys::GOOGLE_ALREADY_BOUND
        | registration::KEY_DOMAIN_NOT_ALLOWED => reasons::GOOGLE_INVALID,
        keys::USER_DISABLED => log_reasons::USER_DISABLED,
        registration::KEY_REGISTRATION_DISABLED => log_reasons::BAD_REQUEST,
        _ => log_reasons::INTERNAL_ERROR,
    }
}

/// Error code appended to the storefront callback URL (`googleRedirectCallbackError`).
pub fn callback_error_code(err: &Error) -> &'static str {
    match err.key() {
        keys::GOOGLE_REDIRECT_SESSION_EXPIRED => "session_expired",
        keys::GOOGLE_REDIRECT_CONTEXT_MISMATCH => "tenant_mismatch",
        keys::GOOGLE_DISABLED => "auth_disabled",
        keys::GOOGLE_CONFIG_INVALID => "configuration_error",
        keys::GOOGLE_CREDENTIAL_EXPIRED => "credential_expired",
        keys::GOOGLE_EMAIL_UNVERIFIED => "email_unverified",
        keys::GOOGLE_CREDENTIAL_INVALID => "credential_invalid",
        keys::GOOGLE_UNAVAILABLE => "service_unavailable",
        _ => "internal_error",
    }
}

/// Copies the verified profile onto `identity`; true when something changed.
fn apply_profile(v: &GoogleIdentity, identity: &mut ExternalIdentity) -> bool {
    let mut changed = false;
    for (field, value) in [
        (&mut identity.provider, PROVIDER_GOOGLE),
        (&mut identity.provider_user_id, v.sub.as_str()),
        (&mut identity.username, v.email.as_str()),
        (&mut identity.avatar_url, v.picture.as_str()),
    ] {
        if field != value {
            *field = value.to_owned();
            changed = true;
        }
    }
    if identity.auth_at != Some(v.auth_at) {
        identity.auth_at = Some(v.auth_at);
        changed = true;
    }
    changed
}

fn new_identity(user_id: Id, v: &GoogleIdentity) -> NewExternalIdentity {
    NewExternalIdentity {
        user_id,
        provider: PROVIDER_GOOGLE.into(),
        provider_user_id: v.sub.clone(),
        username: v.email.clone(),
        avatar_url: v.picture.clone(),
        auth_at: Some(v.auth_at),
    }
}

impl OAuthService {
    /// Enabled Google login and its client id.
    async fn google_client_id(&self) -> Result<String> {
        let cfg = self.google_setting().await?;
        if !cfg.enabled {
            return Err(Error::bad_request(keys::GOOGLE_DISABLED));
        }
        if cfg.client_id.is_empty() {
            return Err(google_config_invalid("client id missing"));
        }
        Ok(cfg.client_id)
    }

    /// Verifies a Google Identity Services credential (AUTH-01).
    pub async fn verify_google_credential(&self, credential: &str) -> Result<GoogleIdentity> {
        let client_id = self.google_client_id().await?;
        if credential.len() > MAX_CREDENTIAL_BYTES {
            return Err(Error::bad_request(keys::GOOGLE_CREDENTIAL_INVALID));
        }
        let credential = credential.trim();
        if credential.is_empty() {
            return Err(Error::bad_request(keys::GOOGLE_CREDENTIAL_INVALID));
        }
        let jwks = self.endpoints().google_jwks;
        let claims = self
            .d()
            .jwt
            .verify_rs256(credential, &jwks, JwksProfile::Google)
            .await
            .map_err(|failure| match failure {
                JwtFailure::KeyUnavailable(detail) => google_unavailable(&detail),
                other => {
                    tracing::debug!(error = %other, "google credential rejected");
                    Error::bad_request(keys::GOOGLE_CREDENTIAL_INVALID)
                }
            })?;
        let identity = g::validate_claims(&claims, &client_id, self.now())?;
        g::normalize_identity(&identity)
    }

    /// Links or creates the account of a verified identity (one retry after a
    /// uniqueness race, like the original).
    async fn login_verified_google(&self, v: &GoogleIdentity) -> Result<User> {
        let v = g::normalize_identity(v)?;
        match self.login_google_once(&v).await {
            Ok(user) => Ok(user),
            Err(e) if e.kind() == zs_domain::ErrorKind::Internal => {
                // A concurrent login/registration may have committed first.
                let identities = &self.d().identities;
                if let Ok(Some(_)) = identities.by_provider_user(PROVIDER_GOOGLE, &v.sub).await {
                    return self.login_google_once(&v).await;
                }
                if let Ok(Some(user)) = self.d().users.get_by_email(&v.email).await {
                    match identities.by_user_provider(user.id, PROVIDER_GOOGLE).await {
                        Ok(Some(current)) if current.provider_user_id != v.sub => {
                            return Err(Error::bad_request(keys::GOOGLE_ALREADY_BOUND));
                        }
                        Ok(_) => return self.login_google_once(&v).await,
                        Err(_) => {}
                    }
                }
                Err(e)
            }
            Err(e) => Err(e),
        }
    }

    async fn login_google_once(&self, v: &GoogleIdentity) -> Result<User> {
        let now = self.now();
        let identities = &self.d().identities;
        if let Some(mut identity) = identities.by_provider_user(PROVIDER_GOOGLE, &v.sub).await? {
            let user = self.active_user(identity.user_id).await?;
            if apply_profile(v, &mut identity) {
                identities.update(&identity).await?;
            }
            return Ok(user);
        }
        match self.d().users.get_by_email(&v.email).await? {
            Some(mut user) => {
                // AUTH-01: only an authoritative address may take over a local account.
                if !v.email_authoritative {
                    return Err(Error::forbidden(keys::GOOGLE_AUTO_LINK_FORBIDDEN));
                }
                if !user.is_active() {
                    return Err(Error::unauthorized(keys::USER_DISABLED));
                }
                match identities
                    .by_user_provider(user.id, PROVIDER_GOOGLE)
                    .await?
                {
                    Some(current) if current.provider_user_id != v.sub => {
                        return Err(Error::bad_request(keys::GOOGLE_ALREADY_BOUND));
                    }
                    Some(mut current) => {
                        if apply_profile(v, &mut current) {
                            identities.update(&current).await?;
                        }
                    }
                    None => {
                        identities.create(&new_identity(user.id, v)).await?;
                    }
                }
                if user.email_verified_at.is_none() {
                    user.email_verified_at = Some(now);
                    self.d().users.save(&user).await?;
                }
                Ok(user)
            }
            None => {
                // A custom-domain address without `hd` never creates an account.
                if !v.email_authoritative {
                    return Err(Error::forbidden(keys::GOOGLE_AUTO_LINK_FORBIDDEN));
                }
                let reg = self.ensure_registration_allowed().await?;
                reg.check_email_domain(&v.email)?;
                let new_user = NewUser {
                    email: v.email.clone(),
                    password_hash: password::hash(&random_token(PASSWORD_SEED_BYTES)).await?,
                    display_name: g::display_name(v),
                    locale: zs_shared::i18n::DEFAULT_LOCALE.into(),
                    member_level_id: self.default_level().await,
                    email_verified_at: Some(now),
                    password_setup_required: true,
                };
                let (user, _) = identities
                    .create_user_with_identity(&new_user, &new_identity(0, v))
                    .await?;
                Ok(user)
            }
        }
    }

    async fn finish_google_login(
        &self,
        verified: Result<GoogleIdentity>,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        let result = match verified {
            Ok(v) => match self.login_verified_google(&v).await {
                Ok(user) => self
                    .complete_login(user.clone(), SOURCE_GOOGLE)
                    .await
                    .map(|o| (user, o)),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        };
        match result {
            Ok((user, outcome)) => {
                self.record(Ok(&user), SOURCE_GOOGLE, client).await;
                Ok(outcome)
            }
            Err(e) => {
                self.record(Err(login_reason(&e)), SOURCE_GOOGLE, client)
                    .await;
                Err(restrict(e, LOGIN_KEYS, keys::LOGIN_FAILED))
            }
        }
    }

    /// `POST /auth/google/login`.
    pub async fn google_login(
        &self,
        credential: &str,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        let verified = self.verify_google_credential(credential).await;
        self.finish_google_login(verified, client).await
    }

    // --- binding -----------------------------------------------------------

    async fn bind_verified_google(
        &self,
        user_id: Id,
        v: &GoogleIdentity,
    ) -> Result<(User, ExternalIdentity)> {
        let v = g::normalize_identity(v)?;
        let user = self.active_user(user_id).await?;
        let identities = &self.d().identities;
        if let Some(occupied) = identities.by_provider_user(PROVIDER_GOOGLE, &v.sub).await?
            && occupied.user_id != user_id
        {
            return Err(Error::bad_request(keys::GOOGLE_BIND_CONFLICT));
        }
        let result = match identities
            .by_user_provider(user_id, PROVIDER_GOOGLE)
            .await?
        {
            Some(current) if current.provider_user_id != v.sub => {
                return Err(Error::bad_request(keys::GOOGLE_ALREADY_BOUND));
            }
            Some(mut current) => {
                if apply_profile(&v, &mut current) {
                    identities.update(&current).await.map(|()| current)
                } else {
                    Ok(current)
                }
            }
            None => identities.create(&new_identity(user_id, &v)).await,
        };
        match result {
            Ok(identity) => Ok((user, identity)),
            Err(e) => {
                // Translate uniqueness races into stable errors.
                if let Ok(Some(occupied)) =
                    identities.by_provider_user(PROVIDER_GOOGLE, &v.sub).await
                    && occupied.user_id != user_id
                {
                    return Err(Error::bad_request(keys::GOOGLE_BIND_CONFLICT));
                }
                if let Ok(Some(latest)) =
                    identities.by_user_provider(user_id, PROVIDER_GOOGLE).await
                    && latest.provider_user_id != v.sub
                {
                    return Err(Error::bad_request(keys::GOOGLE_ALREADY_BOUND));
                }
                Err(e)
            }
        }
    }

    async fn google_binding_of(
        &self,
        user: &User,
        identity: Option<&ExternalIdentity>,
    ) -> Result<GoogleBinding> {
        let Some(identity) = identity else {
            return Ok(GoogleBinding::default());
        };
        let email_addr = if identity.username.trim().is_empty() {
            user.email.clone()
        } else {
            identity.username.trim().to_owned()
        };
        Ok(GoogleBinding {
            bound: true,
            provider: identity.provider.clone(),
            provider_user_id: identity.provider_user_id.clone(),
            username: identity.username.clone(),
            email: email_addr,
            display_name: user.display_name.clone(),
            avatar_url: identity.avatar_url.clone(),
            auth_at: identity.auth_at,
            can_unbind: self.can_unbind(user, identity.id).await?,
        })
    }

    /// `GET /me/google`.
    pub async fn google_binding(&self, user_id: Id) -> Result<GoogleBinding> {
        let user = self.binding_user(user_id).await?;
        let fail = |e: Error| Error::internal(e).or_internal(keys::USER_FETCH_FAILED);
        let identity = self
            .d()
            .identities
            .by_user_provider(user_id, PROVIDER_GOOGLE)
            .await
            .map_err(fail)?;
        self.google_binding_of(&user, identity.as_ref())
            .await
            .map_err(fail)
    }

    async fn bind_google_view(
        &self,
        user_id: Id,
        verified: Result<GoogleIdentity>,
    ) -> Result<GoogleBinding> {
        let run = async {
            let (user, identity) = self.bind_verified_google(user_id, &verified?).await?;
            self.google_binding_of(&user, Some(&identity)).await
        };
        run.await
            .map_err(|e| restrict(e, BIND_KEYS, keys::USER_UPDATE_FAILED))
    }

    /// `POST /me/google/bind`.
    pub async fn google_bind(&self, user_id: Id, credential: &str) -> Result<GoogleBinding> {
        let verified = self.verify_google_credential(credential).await;
        self.bind_google_view(user_id, verified).await
    }

    /// `DELETE /me/google/unbind`: keeps a local password or another usable identity.
    pub async fn google_unbind(&self, user_id: Id) -> Result<()> {
        self.unbind(
            user_id,
            PROVIDER_GOOGLE,
            keys::GOOGLE_UNBIND_LOCKED,
            keys::GOOGLE_NOT_BOUND,
        )
        .await
    }

    // --- redirect flow -----------------------------------------------------

    /// `POST /auth/google/redirect/intent` and `/me/google/redirect/intent`:
    /// a tenant (and, for binding, user) bound single-use state.
    pub async fn google_redirect_intent(
        &self,
        flow: RedirectFlow,
        user_id: Id,
        tenant: &RedirectTenant,
    ) -> Result<String> {
        self.google_client_id().await?;
        if flow == RedirectFlow::Bind && user_id <= 0 {
            return Err(session_expired());
        }
        let user_id = if flow == RedirectFlow::Login {
            0
        } else {
            user_id
        };
        let tenant = tenant.normalized().ok_or_else(context_mismatch)?;
        let state = random_token(HANDLE_BYTES);
        let intent = RedirectIntent {
            flow,
            user_id,
            tenant,
        };
        if !self
            .inner
            .intents
            .insert_new(&state, intent, Duration::seconds(INTENT_TTL_SECONDS))
        {
            return Err(google_unavailable("state collision"));
        }
        Ok(state)
    }

    /// `form_post` callback: consumes the intent *before* verifying the
    /// credential (a failure cannot be replayed), then stores verified claims
    /// under a 2-minute handoff handle.
    pub async fn google_redirect_complete(
        &self,
        state: &str,
        credential: &str,
        tenant: &RedirectTenant,
        client: &ClientInfo,
    ) -> RedirectCompletion {
        let fail = |e: Error| RedirectCompletion {
            flow: RedirectFlow::Login,
            handoff: Err(e),
        };
        if !valid_handle(state) {
            return fail(session_expired());
        }
        let Some(intent) = self.inner.intents.take(state) else {
            return fail(session_expired());
        };
        let flow = intent.flow;
        let handoff = async {
            let current = tenant.normalized().ok_or_else(context_mismatch)?;
            if !intent.tenant.same(&current) {
                return Err(context_mismatch());
            }
            let identity = match self.verify_google_credential(credential).await {
                Ok(identity) => identity,
                Err(e) => {
                    if flow == RedirectFlow::Login {
                        self.record(Err(login_reason(&e)), SOURCE_GOOGLE, client)
                            .await;
                    }
                    return Err(e);
                }
            };
            let handle = random_token(HANDLE_BYTES);
            let stored = RedirectHandoff {
                flow,
                user_id: intent.user_id,
                tenant: intent.tenant.clone(),
                identity,
            };
            if !self.inner.handoffs.insert_new(
                &handle,
                stored,
                Duration::seconds(HANDOFF_TTL_SECONDS),
            ) {
                return Err(google_unavailable("handoff collision"));
            }
            Ok(handle)
        };
        RedirectCompletion {
            flow,
            handoff: handoff.await,
        }
    }

    /// Consumes a handoff and checks flow, tenant, user and the current client id.
    async fn take_handoff(
        &self,
        handle: &str,
        flow: RedirectFlow,
        user_id: Id,
        tenant: &RedirectTenant,
    ) -> Result<GoogleIdentity> {
        if !valid_handle(handle) {
            return Err(session_expired());
        }
        let handoff = self
            .inner
            .handoffs
            .take(handle)
            .ok_or_else(session_expired)?;
        let current = tenant.normalized().ok_or_else(context_mismatch)?;
        if handoff.flow != flow {
            return Err(session_expired());
        }
        if !handoff.tenant.same(&current) {
            return Err(context_mismatch());
        }
        match flow {
            RedirectFlow::Bind if handoff.user_id != user_id => return Err(context_mismatch()),
            RedirectFlow::Login if handoff.user_id != 0 => return Err(session_expired()),
            _ => {}
        }
        // A changed client id changes the accepted audience: invalidate.
        let client_id = self.google_client_id().await?;
        if handoff.identity.client_id.is_empty() || handoff.identity.client_id != client_id {
            return Err(session_expired());
        }
        Ok(handoff.identity)
    }

    /// `POST /auth/google/redirect/exchange`: same response as popup login.
    pub async fn google_redirect_exchange_login(
        &self,
        handle: &str,
        tenant: &RedirectTenant,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        match self
            .take_handoff(handle, RedirectFlow::Login, 0, tenant)
            .await
        {
            Err(e) if REDIRECT_STATE_KEYS.contains(&e.key()) => Err(e),
            verified => self.finish_google_login(verified, client).await,
        }
    }

    /// `POST /me/google/redirect/exchange`: the handoff must belong to `user_id`.
    pub async fn google_redirect_exchange_bind(
        &self,
        handle: &str,
        user_id: Id,
        tenant: &RedirectTenant,
    ) -> Result<GoogleBinding> {
        if user_id <= 0 {
            return Err(context_mismatch());
        }
        match self
            .take_handoff(handle, RedirectFlow::Bind, user_id, tenant)
            .await
        {
            Err(e) if REDIRECT_STATE_KEYS.contains(&e.key()) => Err(e),
            verified => self.bind_google_view(user_id, verified).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handles() {
        let h = random_token(HANDLE_BYTES);
        assert!(valid_handle(&h));
        assert!(!valid_handle(&h[..42]));
        assert!(!valid_handle(&format!("{}=", &h[..42])));
        assert!(!valid_handle("x"));
    }

    #[test]
    fn callback_codes() {
        assert_eq!(callback_error_code(&session_expired()), "session_expired");
        assert_eq!(callback_error_code(&context_mismatch()), "tenant_mismatch");
        assert_eq!(
            callback_error_code(&google_unavailable("x")),
            "service_unavailable"
        );
        assert_eq!(
            callback_error_code(&Error::bad_request(keys::GOOGLE_CREDENTIAL_EXPIRED)),
            "credential_expired"
        );
        assert_eq!(callback_error_code(&Error::invalid()), "internal_error");
    }
}
