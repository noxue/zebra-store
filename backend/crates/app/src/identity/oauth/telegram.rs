//! Telegram login and binding: Login Widget, Mini App and OIDC
//! (original `telegram_login.go`, `telegram_identity.go`, `telegram_binding.go`,
//! `telegram_oidc.go` and `telegramauth`).

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Duration;
use zs_domain::identity::login_log::reasons as log_reasons;
use zs_domain::identity::oauth::{
    self as oauth, ExternalIdentity, JwksProfile, NewExternalIdentity, PROVIDER_TELEGRAM,
    SOURCE_TELEGRAM, keys, reasons, telegram_config_invalid,
};
use zs_domain::identity::registration;
use zs_domain::identity::telegram::{
    self as tg, TelegramIdentity, WidgetPayload, check_auth_time, check_hash,
};
use zs_domain::identity::user::{NewUser, User};
use zs_domain::notify::channel::{placeholder_email, telegram_display_name};
use zs_domain::settings::schema::login::{TelegramAuthSetting, TelegramLoginMode, telegram_bot_id};
use zs_domain::{Error, Id, Result};

use super::{OAuthService, TelegramBinding, random_token, restrict};
use crate::identity::admin_auth::ClientInfo;
use crate::identity::password;
use crate::identity::user_account::UserLoginOutcome;

/// Lifetime of an OIDC state (original `telegramOIDCStateTTL`, 600 s).
const OIDC_STATE_TTL_SECONDS: i64 = 600;
/// Random bytes of an OIDC state (base64url, original 32).
const OIDC_STATE_BYTES: usize = 32;
/// Random bytes of a PKCE verifier (48 → 64 base64url chars, within 43..=128).
const PKCE_VERIFIER_BYTES: usize = 48;
/// Digits of the random password seed of placeholder accounts.
const PASSWORD_SEED_DIGITS: usize = 16;
/// Bytes of the `id_token` fingerprint used as replay marker.
const ID_TOKEN_FINGERPRINT_BYTES: usize = 8;

/// Keys reported by the widget / Mini App login endpoints (`telegramLoginErrorRules`).
const LOGIN_KEYS: &[&str] = &[
    keys::TELEGRAM_DISABLED,
    keys::TELEGRAM_CONFIG_INVALID,
    keys::TELEGRAM_PAYLOAD_INVALID,
    keys::TELEGRAM_SIGNATURE_INVALID,
    keys::TELEGRAM_EXPIRED,
    keys::TELEGRAM_REPLAYED,
    keys::USER_DISABLED,
    registration::KEY_REGISTRATION_DISABLED,
];

/// Keys reported by the widget / Mini App bind endpoints (`respondTelegramBindError`).
const BIND_KEYS: &[&str] = &[
    keys::TELEGRAM_DISABLED,
    keys::TELEGRAM_CONFIG_INVALID,
    keys::TELEGRAM_PAYLOAD_INVALID,
    keys::TELEGRAM_SIGNATURE_INVALID,
    keys::TELEGRAM_EXPIRED,
    keys::TELEGRAM_REPLAYED,
    keys::TELEGRAM_BIND_CONFLICT,
    keys::TELEGRAM_ALREADY_BOUND,
];

/// Keys reported by the OIDC endpoints (`respondTelegramOIDCError`).
const OIDC_KEYS: &[&str] = &[
    keys::TELEGRAM_DISABLED,
    keys::TELEGRAM_CONFIG_INVALID,
    keys::TELEGRAM_OIDC_STATE_INVALID,
    keys::TELEGRAM_OIDC_EXCHANGE_FAILED,
    keys::TELEGRAM_OIDC_ID_TOKEN_INVALID,
    keys::TELEGRAM_PAYLOAD_INVALID,
    keys::TELEGRAM_EXPIRED,
    keys::TELEGRAM_REPLAYED,
    keys::TELEGRAM_BIND_CONFLICT,
    keys::TELEGRAM_ALREADY_BOUND,
    keys::USER_DISABLED,
    registration::KEY_REGISTRATION_DISABLED,
];

/// What an OIDC flow was started for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OidcIntent {
    Login,
    Bind,
}

/// Server-side OIDC state (PKCE verifier, intent and binding user).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcState {
    verifier: String,
    intent: OidcIntent,
    user_id: Id,
}

/// Login-log reason of a widget / Mini App failure.
fn login_reason(err: &Error) -> &'static str {
    match err.key() {
        keys::TELEGRAM_DISABLED | keys::TELEGRAM_CONFIG_INVALID => reasons::TELEGRAM_CONFIG,
        keys::TELEGRAM_PAYLOAD_INVALID | keys::TELEGRAM_SIGNATURE_INVALID => {
            reasons::TELEGRAM_INVALID
        }
        keys::TELEGRAM_EXPIRED => reasons::TELEGRAM_EXPIRED,
        keys::TELEGRAM_REPLAYED => reasons::TELEGRAM_REPLAYED,
        keys::USER_DISABLED => log_reasons::USER_DISABLED,
        registration::KEY_REGISTRATION_DISABLED => log_reasons::BAD_REQUEST,
        _ => log_reasons::INTERNAL_ERROR,
    }
}

/// Go `url.QueryEscape`.
fn query_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(b));
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Copies the verified profile onto `identity`; true when something changed.
fn apply_profile(v: &TelegramIdentity, identity: &mut ExternalIdentity) -> bool {
    let mut changed = false;
    if identity.provider.is_empty() {
        identity.provider = PROVIDER_TELEGRAM.into();
        changed = true;
    }
    if identity.provider_user_id.is_empty() {
        identity.provider_user_id.clone_from(&v.provider_user_id);
        changed = true;
    }
    if identity.username != v.username {
        identity.username.clone_from(&v.username);
        changed = true;
    }
    if identity.avatar_url != v.avatar_url {
        identity.avatar_url.clone_from(&v.avatar_url);
        changed = true;
    }
    if identity.auth_at != Some(v.auth_at) {
        identity.auth_at = Some(v.auth_at);
        changed = true;
    }
    changed
}

fn new_identity(user_id: Id, v: &TelegramIdentity) -> NewExternalIdentity {
    NewExternalIdentity {
        user_id,
        provider: PROVIDER_TELEGRAM.into(),
        provider_user_id: v.provider_user_id.clone(),
        username: v.username.clone(),
        avatar_url: v.avatar_url.clone(),
        auth_at: Some(v.auth_at),
    }
}

fn random_digits(n: usize) -> String {
    use rand::Rng as _;
    let mut rng = rand::rng();
    (0..n)
        .map(|_| char::from(b'0' + rng.random_range(0..10u8)))
        .collect()
}

impl OAuthService {
    /// Enabled setting with a bot token (`error.telegram_auth_disabled` /
    /// `error.telegram_auth_config_invalid`).
    async fn telegram_ready(&self) -> Result<TelegramAuthSetting> {
        let cfg = self.telegram_setting().await?;
        if !cfg.enabled {
            return Err(Error::bad_request(keys::TELEGRAM_DISABLED));
        }
        if cfg.bot_token.is_empty() {
            return Err(telegram_config_invalid("bot token missing"));
        }
        Ok(cfg)
    }

    /// Marks `(telegram id, hash)` as used for `ttl_seconds` (AUTH-04 replay protection).
    fn mark_replay(&self, telegram_id: i64, hash: &str, ttl_seconds: i64) -> Result<()> {
        let key = format!("telegram:auth:replay:{telegram_id}:{hash}");
        if self
            .inner
            .replays
            .insert_new(&key, (), Duration::seconds(ttl_seconds))
        {
            Ok(())
        } else {
            Err(Error::bad_request(keys::TELEGRAM_REPLAYED))
        }
    }

    /// Verifies a Login Widget payload.
    pub async fn verify_telegram_widget(
        &self,
        payload: &WidgetPayload,
    ) -> Result<TelegramIdentity> {
        let cfg = self.telegram_ready().await?;
        let p = payload.normalized()?;
        let auth_at = tg::auth_time(p.auth_date)?;
        check_auth_time(self.now(), auth_at, cfg.login_expire_seconds)?;
        check_hash(
            &tg::widget_hash(&cfg.bot_token, &p.data_check_string()),
            &p.hash,
        )?;
        self.mark_replay(p.id, &p.hash, cfg.replay_ttl_seconds)?;
        Ok(TelegramIdentity {
            provider_user_id: p.id.to_string(),
            aliases: Vec::new(),
            username: p.username,
            avatar_url: p.photo_url,
            first_name: p.first_name,
            last_name: p.last_name,
            auth_at,
        })
    }

    /// Verifies Mini App `initData` (AUTH-04).
    pub async fn verify_telegram_miniapp(&self, init_data: &str) -> Result<TelegramIdentity> {
        let cfg = self.telegram_ready().await?;
        let parsed = tg::parse_miniapp_init_data(init_data)?;
        let auth_at = tg::auth_time(parsed.auth_date)?;
        check_auth_time(self.now(), auth_at, cfg.login_expire_seconds)?;
        check_hash(
            &tg::miniapp_hash_bytes(&cfg.bot_token, &parsed.data_check_string),
            &parsed.hash,
        )?;
        self.mark_replay(parsed.user.id, &parsed.hash, cfg.replay_ttl_seconds)?;
        let u = parsed.user;
        Ok(TelegramIdentity {
            provider_user_id: u.id.to_string(),
            aliases: Vec::new(),
            username: u.username,
            avatar_url: u.photo_url,
            first_name: u.first_name,
            last_name: u.last_name,
            auth_at,
        })
    }

    /// Binding by numeric id, then by historic aliases (AUTH-02).
    async fn telegram_identity_of(&self, v: &TelegramIdentity) -> Result<Option<ExternalIdentity>> {
        let repo = &self.d().identities;
        if let Some(found) = repo
            .by_provider_user(PROVIDER_TELEGRAM, &v.provider_user_id)
            .await?
        {
            return Ok(Some(found));
        }
        for alias in &v.aliases {
            let alias = alias.trim();
            if alias.is_empty() || alias == v.provider_user_id {
                continue;
            }
            if let Some(found) = repo.by_provider_user(PROVIDER_TELEGRAM, alias).await? {
                return Ok(Some(found));
            }
        }
        Ok(None)
    }

    /// Migrates an alias binding to the numeric id; fails when the numeric id is
    /// held by another binding (AUTH-02).
    async fn canonicalize(
        &self,
        v: &TelegramIdentity,
        identity: &mut ExternalIdentity,
    ) -> Result<bool> {
        if identity.provider_user_id == v.provider_user_id {
            return Ok(false);
        }
        if let Some(occupied) = self
            .d()
            .identities
            .by_provider_user(PROVIDER_TELEGRAM, &v.provider_user_id)
            .await?
            && occupied.id != identity.id
        {
            return Err(Error::bad_request(keys::TELEGRAM_BIND_CONFLICT));
        }
        identity.provider_user_id.clone_from(&v.provider_user_id);
        Ok(true)
    }

    /// Placeholder account of a Telegram id (created when registration is open).
    async fn provision_telegram(&self, v: &TelegramIdentity) -> Result<User> {
        let email = placeholder_email(&v.provider_user_id);
        let repo = &self.d().identities;
        let recover = |err: Error| async move {
            // A concurrent login may have created the binding first.
            match repo
                .by_provider_user(PROVIDER_TELEGRAM, &v.provider_user_id)
                .await
            {
                Ok(Some(existing)) => self.active_user(existing.user_id).await,
                _ => Err(err),
            }
        };
        if let Some(user) = self.d().users.get_by_email(&email).await? {
            if !user.is_active() {
                return Err(Error::unauthorized(keys::USER_DISABLED));
            }
            return match repo.create(&new_identity(user.id, v)).await {
                Ok(_) => Ok(user),
                Err(e) => recover(e).await,
            };
        }
        // AUTH-05: already-bound users log in above; only sign-ups are blocked.
        self.ensure_registration_allowed().await?;
        let seed = format!(
            "tg_{}_{}",
            v.provider_user_id,
            random_digits(PASSWORD_SEED_DIGITS)
        );
        let new_user = NewUser {
            email,
            password_hash: password::hash(&seed).await?,
            display_name: telegram_display_name(
                &v.provider_user_id,
                &v.username,
                &v.first_name,
                &v.last_name,
            ),
            locale: zs_shared::i18n::DEFAULT_LOCALE.into(),
            member_level_id: self.default_level().await,
            email_verified_at: None,
            password_setup_required: true,
        };
        match repo
            .create_user_with_identity(&new_user, &new_identity(0, v))
            .await
        {
            Ok((user, _)) => Ok(user),
            Err(e) => recover(e).await,
        }
    }

    /// Logs in (or signs up) a verified Telegram identity.
    async fn login_verified_telegram(&self, v: &TelegramIdentity) -> Result<User> {
        match self.telegram_identity_of(v).await? {
            Some(mut identity) => {
                let user = self.active_user(identity.user_id).await?;
                let migrated = self.canonicalize(v, &mut identity).await?;
                if apply_profile(v, &mut identity) || migrated {
                    self.d().identities.update(&identity).await?;
                }
                Ok(user)
            }
            None => self.provision_telegram(v).await,
        }
    }

    async fn finish_telegram_login(
        &self,
        verified: Result<TelegramIdentity>,
        oidc: bool,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        let result = match verified {
            Ok(v) => match self.login_verified_telegram(&v).await {
                Ok(user) => self
                    .complete_login(user.clone(), SOURCE_TELEGRAM)
                    .await
                    .map(|o| (user, o)),
                Err(e) => Err(e),
            },
            Err(e) => Err(e),
        };
        match result {
            Ok((user, outcome)) => {
                self.record(Ok(&user), SOURCE_TELEGRAM, client).await;
                Ok(outcome)
            }
            Err(e) => {
                let (reason, allowed) = if oidc {
                    (reasons::TELEGRAM_INVALID, OIDC_KEYS)
                } else {
                    (login_reason(&e), LOGIN_KEYS)
                };
                self.record(Err(reason), SOURCE_TELEGRAM, client).await;
                Err(restrict(e, allowed, keys::LOGIN_FAILED))
            }
        }
    }

    /// `POST /auth/telegram/login`.
    pub async fn telegram_login(
        &self,
        payload: &WidgetPayload,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        let verified = self.verify_telegram_widget(payload).await;
        self.finish_telegram_login(verified, false, client).await
    }

    /// `POST /auth/telegram/miniapp/login`.
    pub async fn telegram_miniapp_login(
        &self,
        init_data: &str,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        let verified = self.verify_telegram_miniapp(init_data).await;
        self.finish_telegram_login(verified, false, client).await
    }

    // --- binding -----------------------------------------------------------

    async fn bind_verified_telegram(
        &self,
        user_id: Id,
        v: &TelegramIdentity,
    ) -> Result<ExternalIdentity> {
        self.active_user(user_id).await?;
        if let Some(occupied) = self.telegram_identity_of(v).await?
            && occupied.user_id != user_id
        {
            return Err(Error::bad_request(keys::TELEGRAM_BIND_CONFLICT));
        }
        let repo = &self.d().identities;
        let current = repo.by_user_provider(user_id, PROVIDER_TELEGRAM).await?;
        match current {
            Some(current) if !v.matches(&current.provider_user_id) => {
                Err(Error::bad_request(keys::TELEGRAM_ALREADY_BOUND))
            }
            Some(mut current) => {
                let migrated = self.canonicalize(v, &mut current).await?;
                if apply_profile(v, &mut current) || migrated {
                    repo.update(&current).await?;
                }
                Ok(current)
            }
            None => match repo.create(&new_identity(user_id, v)).await {
                Ok(created) => Ok(created),
                Err(e) => {
                    // Translate a uniqueness race into the stable errors.
                    if let Ok(Some(occupied)) = self.telegram_identity_of(v).await
                        && occupied.user_id != user_id
                    {
                        return Err(Error::bad_request(keys::TELEGRAM_BIND_CONFLICT));
                    }
                    match repo.by_user_provider(user_id, PROVIDER_TELEGRAM).await {
                        Ok(Some(latest)) if !v.matches(&latest.provider_user_id) => {
                            Err(Error::bad_request(keys::TELEGRAM_ALREADY_BOUND))
                        }
                        Ok(Some(latest)) => Ok(latest),
                        _ => Err(e),
                    }
                }
            },
        }
    }

    /// `GET /me/telegram`.
    pub async fn telegram_binding(&self, user_id: Id) -> Result<TelegramBinding> {
        let user = self.binding_user(user_id).await?;
        let fail = |e: Error| Error::internal(e).or_internal(keys::USER_FETCH_FAILED);
        let identity = self
            .d()
            .identities
            .by_user_provider(user_id, PROVIDER_TELEGRAM)
            .await
            .map_err(fail)?;
        let Some(identity) = identity else {
            return Ok(TelegramBinding::default());
        };
        let can_unbind = self.can_unbind(&user, identity.id).await.map_err(fail)?;
        Ok(TelegramBinding::of(Some(&identity), can_unbind))
    }

    /// View returned after a successful bind (falls back to the bare identity).
    async fn binding_after(&self, user_id: Id, identity: &ExternalIdentity) -> TelegramBinding {
        match self.telegram_binding(user_id).await {
            Ok(b) => b,
            Err(_) => TelegramBinding::of(Some(identity), false),
        }
    }

    /// `POST /me/telegram/bind`.
    pub async fn telegram_bind(
        &self,
        user_id: Id,
        payload: &WidgetPayload,
    ) -> Result<TelegramBinding> {
        let result = match self.verify_telegram_widget(payload).await {
            Ok(v) => self.bind_verified_telegram(user_id, &v).await,
            Err(e) => Err(e),
        };
        let identity = result.map_err(|e| restrict(e, BIND_KEYS, keys::USER_UPDATE_FAILED))?;
        Ok(self.binding_after(user_id, &identity).await)
    }

    /// `POST /me/telegram/miniapp/bind`.
    pub async fn telegram_miniapp_bind(
        &self,
        user_id: Id,
        init_data: &str,
    ) -> Result<TelegramBinding> {
        let result = match self.verify_telegram_miniapp(init_data).await {
            Ok(v) => self.bind_verified_telegram(user_id, &v).await,
            Err(e) => Err(e),
        };
        let identity = result.map_err(|e| restrict(e, BIND_KEYS, keys::USER_UPDATE_FAILED))?;
        Ok(self.binding_after(user_id, &identity).await)
    }

    /// `DELETE /me/telegram/unbind`.
    pub async fn telegram_unbind(&self, user_id: Id) -> Result<()> {
        self.unbind(
            user_id,
            PROVIDER_TELEGRAM,
            keys::TELEGRAM_UNBIND_REQUIRES_EMAIL,
            keys::TELEGRAM_NOT_BOUND,
        )
        .await
    }

    // --- OIDC --------------------------------------------------------------

    /// OIDC-mode setting and its client id (the bot id).
    async fn telegram_oidc_ready(&self) -> Result<(TelegramAuthSetting, String)> {
        let cfg = self.telegram_setting().await?;
        match cfg.login_mode() {
            TelegramLoginMode::Disabled => return Err(Error::bad_request(keys::TELEGRAM_DISABLED)),
            TelegramLoginMode::Widget => {
                return Err(telegram_config_invalid("oidc not configured"));
            }
            TelegramLoginMode::Oidc => {}
        }
        let client_id = telegram_bot_id(&cfg.bot_token)
            .ok_or_else(|| telegram_config_invalid("bot id"))?
            .to_owned();
        Ok((cfg, client_id))
    }

    /// `GET /auth/telegram/oidc/start` and `GET /me/telegram/oidc/start`:
    /// stores a single-use state (PKCE verifier, intent, user) for 10 minutes
    /// and returns the authorization URL.
    pub async fn telegram_oidc_start(&self, intent: OidcIntent, user_id: Id) -> Result<String> {
        let run = async {
            let (cfg, client_id) = self.telegram_oidc_ready().await?;
            let state = random_token(OIDC_STATE_BYTES);
            let verifier = random_token(PKCE_VERIFIER_BYTES);
            let challenge = URL_SAFE_NO_PAD.encode(oauth::sha256(verifier.as_bytes()));
            let stored = OidcState {
                verifier,
                intent,
                user_id,
            };
            if !self.inner.oidc_states.insert_new(
                &format!("telegram:oidc:state:{state}"),
                stored,
                Duration::seconds(OIDC_STATE_TTL_SECONDS),
            ) {
                return Err(Error::bad_request(keys::TELEGRAM_OIDC_STATE_INVALID));
            }
            // Sorted like Go's `url.Values.Encode`.
            let params = [
                ("client_id", client_id.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                ("redirect_uri", cfg.oidc_redirect_uri.as_str()),
                ("response_type", "code"),
                ("scope", "openid profile"),
                ("state", state.as_str()),
            ];
            let query = params
                .iter()
                .map(|(k, v)| format!("{k}={}", query_escape(v)))
                .collect::<Vec<_>>()
                .join("&");
            Ok(format!("{}?{query}", self.endpoints().telegram_auth))
        };
        run.await
            .map_err(|e| restrict(e, OIDC_KEYS, keys::LOGIN_FAILED))
    }

    /// Consumes the state, exchanges the code and verifies the `id_token`.
    async fn complete_telegram_oidc(
        &self,
        code: &str,
        state: &str,
    ) -> Result<(TelegramIdentity, OidcState)> {
        let (code, state) = (code.trim(), state.trim());
        if code.is_empty() || state.is_empty() {
            return Err(Error::bad_request(keys::TELEGRAM_PAYLOAD_INVALID));
        }
        let (cfg, client_id) = self.telegram_oidc_ready().await?;
        // AUTH-02: single use; a replayed state finds nothing.
        let stored = self
            .inner
            .oidc_states
            .take(&format!("telegram:oidc:state:{state}"))
            .filter(|s| !s.verifier.is_empty())
            .ok_or_else(|| Error::bad_request(keys::TELEGRAM_OIDC_STATE_INVALID))?;
        let endpoints = self.endpoints();
        let form = vec![
            ("grant_type".to_owned(), "authorization_code".to_owned()),
            ("code".to_owned(), code.to_owned()),
            ("redirect_uri".to_owned(), cfg.oidc_redirect_uri.clone()),
            ("client_id".to_owned(), client_id.clone()),
            ("code_verifier".to_owned(), stored.verifier.clone()),
        ];
        let id_token = self
            .d()
            .tokens
            .exchange(
                &endpoints.telegram_token,
                &form,
                &client_id,
                &cfg.client_secret,
            )
            .await?;
        let claims = self
            .d()
            .jwt
            .verify_rs256(&id_token, &endpoints.telegram_jwks, JwksProfile::Telegram)
            .await
            .map_err(|e| {
                tracing::debug!(error = %e, "telegram id_token rejected");
                Error::bad_request(keys::TELEGRAM_OIDC_ID_TOKEN_INVALID)
            })?;
        let verified = tg::validate_oidc_claims(&claims, &client_id, self.now())?;
        let digest = oauth::sha256(id_token.as_bytes());
        let fingerprint: String = digest
            .iter()
            .take(ID_TOKEN_FINGERPRINT_BYTES)
            .map(|b| format!("{b:02x}"))
            .collect();
        self.mark_replay(verified.id, &fingerprint, cfg.replay_ttl_seconds)?;
        Ok((verified.identity, stored))
    }

    /// `POST /auth/telegram/oidc/callback`.
    pub async fn telegram_oidc_login(
        &self,
        code: &str,
        state: &str,
        client: &ClientInfo,
    ) -> Result<UserLoginOutcome> {
        let verified = match self.complete_telegram_oidc(code, state).await {
            Ok((_, st)) if st.intent != OidcIntent::Login => {
                Err(Error::bad_request(keys::TELEGRAM_PAYLOAD_INVALID))
            }
            Ok((v, _)) => Ok(v),
            Err(e) => Err(e),
        };
        self.finish_telegram_login(verified, true, client).await
    }

    /// `POST /me/telegram/oidc/callback`: the state must be a bind state created
    /// by the same user (AUTH-02). The response carries `can_unbind: false`
    /// like the original.
    pub async fn telegram_oidc_bind(
        &self,
        user_id: Id,
        code: &str,
        state: &str,
    ) -> Result<TelegramBinding> {
        let run = async {
            let (v, st) = self.complete_telegram_oidc(code, state).await?;
            if st.intent != OidcIntent::Bind || st.user_id != user_id {
                return Err(Error::bad_request(keys::TELEGRAM_PAYLOAD_INVALID));
            }
            self.bind_verified_telegram(user_id, &v).await
        };
        let identity = run
            .await
            .map_err(|e| restrict(e, OIDC_KEYS, keys::LOGIN_FAILED))?;
        Ok(TelegramBinding::of(Some(&identity), false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_escape_like_go() {
        assert_eq!(query_escape("openid profile"), "openid+profile");
        assert_eq!(
            query_escape("https://shop.example/auth/cb"),
            "https%3A%2F%2Fshop.example%2Fauth%2Fcb"
        );
        assert_eq!(query_escape("a-b_c.d~e"), "a-b_c.d~e");
    }

    #[test]
    fn pkce_challenge_matches_go_vector() {
        // `s256Challenge` of the original for this verifier.
        let challenge = URL_SAFE_NO_PAD.encode(oauth::sha256(
            b"dBjftJeZ4CVP-mJ0G0X4IZT3aGhVKY5hXhE7s2UEYtQ",
        ));
        assert_eq!(challenge, "gEnhm3PimybWzhuMgCkSNHqANhHGGiYap9vf8PVtAq4");
    }

    #[test]
    fn login_reasons() {
        assert_eq!(
            login_reason(&Error::bad_request(keys::TELEGRAM_REPLAYED)),
            reasons::TELEGRAM_REPLAYED
        );
        assert_eq!(
            login_reason(&telegram_config_invalid("x")),
            reasons::TELEGRAM_CONFIG
        );
        assert_eq!(
            login_reason(&Error::forbidden(registration::KEY_REGISTRATION_DISABLED)),
            log_reasons::BAD_REQUEST
        );
    }
}
