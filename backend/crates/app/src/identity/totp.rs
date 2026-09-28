//! TOTP 2FA shared by administrators and users: secret generation, code
//! verification, recovery codes and the enable/disable workflow.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use serde::Serialize;
use totp_rs::{Algorithm, Secret, TOTP};
use zs_domain::identity::admin::{Admin, AdminRepo};
use zs_domain::identity::totp::{
    self, RecoveryCode, TotpAccount, TotpError, TotpState, TotpStatus, decode_recovery_codes,
    normalize_recovery_code,
};
use zs_domain::identity::user::{User, UserRepo};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;

use super::challenge::FailureCounter;
use super::password;

/// Loads and persists accounts carrying TOTP columns.
#[async_trait]
pub trait TotpAccounts: Send + Sync {
    type Account: TotpAccount;
    async fn load(&self, id: Id) -> Result<Option<Self::Account>>;
    async fn store(&self, account: &Self::Account) -> Result<()>;
    /// Compare-and-swap of the recovery code JSON.
    async fn swap_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool>;
}

/// Administrator accounts as a [`TotpAccounts`] store.
#[derive(Clone)]
pub struct AdminAccounts(pub Arc<dyn AdminRepo>);

impl std::fmt::Debug for AdminAccounts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AdminAccounts")
    }
}

#[async_trait]
impl TotpAccounts for AdminAccounts {
    type Account = Admin;

    async fn load(&self, id: Id) -> Result<Option<Admin>> {
        self.0.get(id).await
    }

    async fn store(&self, account: &Admin) -> Result<()> {
        self.0.save(account).await
    }

    async fn swap_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool> {
        self.0.replace_recovery_codes(id, expected, codes).await
    }
}

/// User accounts as a [`TotpAccounts`] store.
#[derive(Clone)]
pub struct UserAccounts(pub Arc<dyn UserRepo>);

impl std::fmt::Debug for UserAccounts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UserAccounts")
    }
}

#[async_trait]
impl TotpAccounts for UserAccounts {
    type Account = User;

    async fn load(&self, id: Id) -> Result<Option<User>> {
        self.0.get(id).await
    }

    async fn store(&self, account: &User) -> Result<()> {
        self.0.save(account).await
    }

    async fn swap_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool> {
        self.0.replace_recovery_codes(id, expected, codes).await
    }
}

fn build_totp(secret_b32: &str, issuer: &str, label: &str) -> Result<TOTP> {
    let bytes = Secret::Encoded(secret_b32.to_owned())
        .to_bytes()
        .map_err(|e| Error::internal_msg(format!("invalid totp secret: {e:?}")))?;
    Ok(TOTP::new_unchecked(
        Algorithm::SHA1,
        totp::DIGITS,
        totp::SKEW,
        totp::PERIOD_SECONDS,
        bytes,
        (!issuer.is_empty()).then(|| issuer.to_owned()),
        label.to_owned(),
    ))
}

fn unix(now: DateTime<Utc>) -> u64 {
    u64::try_from(now.timestamp()).unwrap_or(0)
}

/// Generates a random base32 secret (160 bits, like `pquerna/otp`).
pub fn generate_secret() -> String {
    match Secret::generate_secret().to_encoded() {
        Secret::Encoded(s) => s,
        Secret::Raw(_) => String::new(),
    }
}

/// The code valid at `now` for `secret_b32` (used by tests and tooling).
pub fn code_at(secret_b32: &str, now: DateTime<Utc>) -> Result<String> {
    Ok(build_totp(secret_b32, "", "")?.generate(unix(now)))
}

/// Checks `code` with ±[`totp::SKEW`] periods of drift.
pub fn verify_code(secret_b32: &str, code: &str, now: DateTime<Utc>) -> bool {
    let code = code.trim();
    if code.is_empty() {
        return false;
    }
    build_totp(secret_b32, "", "").is_ok_and(|t| t.check(code, unix(now)))
}

/// `otpauth://` URL for authenticator apps.
pub fn otpauth_url(secret_b32: &str, issuer: &str, label: &str) -> Result<String> {
    Ok(build_totp(secret_b32, issuer, label)?.get_url())
}

/// Generates `count` recovery codes (5 random bytes as hex, split `xxxx-xxxxxx`
/// exactly like the original) and
/// their bcrypt-hashed JSON form.
pub async fn generate_recovery_codes(count: usize) -> Result<(Vec<String>, String)> {
    let plain: Vec<String> = (0..count)
        .map(|_| {
            let raw: [u8; 5] = rand::rng().random();
            let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
            format!("{}-{}", &hex[..4], &hex[4..])
        })
        .collect();
    let mut entries = Vec::with_capacity(count);
    for code in &plain {
        entries.push(RecoveryCode {
            hash: password::hash(code).await?,
            used_at: None,
        });
    }
    Ok((plain, serde_json::to_string(&entries)?))
}

/// Marks the first unused recovery code matching `code` as used; returns the
/// updated JSON, or `None` when nothing matched.
pub async fn consume_recovery_code(
    json: &str,
    code: &str,
    now: DateTime<Utc>,
) -> Result<Option<String>> {
    let code = normalize_recovery_code(code);
    if code.is_empty() {
        return Ok(None);
    }
    let mut entries = decode_recovery_codes(json).map_err(Error::internal)?;
    for i in 0..entries.len() {
        if entries[i].used_at.is_none() && password::verify(&code, &entries[i].hash).await {
            entries[i].used_at = Some(now);
            return Ok(Some(serde_json::to_string(&entries)?));
        }
    }
    Ok(None)
}

/// `/2fa/setup` response.
#[derive(Debug, Clone, Serialize)]
pub struct SetupResult {
    pub secret: String,
    pub otpauth_url: String,
    pub expires_at: DateTime<Utc>,
}

/// Result of a successful enable.
#[derive(Debug, Clone)]
pub struct EnableResult<A> {
    pub enabled_at: DateTime<Utc>,
    pub recovery_codes: Vec<String>,
    pub account: A,
}

/// Whether enabling 2FA revokes the account's other sessions (users: yes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnEnable {
    KeepSessions,
    RevokeSessions,
}

/// The 2FA workflow for one kind of account.
pub struct TotpService<S: TotpAccounts> {
    accounts: Arc<S>,
    cipher: Cipher,
    clock: Arc<dyn Clock>,
    issuer: String,
    on_enable: OnEnable,
    enable_failures: FailureCounter,
}

impl<S: TotpAccounts> Clone for TotpService<S> {
    fn clone(&self) -> Self {
        Self {
            accounts: self.accounts.clone(),
            cipher: self.cipher.clone(),
            clock: self.clock.clone(),
            issuer: self.issuer.clone(),
            on_enable: self.on_enable,
            enable_failures: self.enable_failures.clone(),
        }
    }
}

impl<S: TotpAccounts> std::fmt::Debug for TotpService<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TotpService")
            .field("issuer", &self.issuer)
            .finish()
    }
}

fn not_found() -> Error {
    Error::not_found("error.user_not_found")
}

impl<S: TotpAccounts> TotpService<S> {
    pub fn new(
        accounts: Arc<S>,
        cipher: Cipher,
        clock: Arc<dyn Clock>,
        issuer: &str,
        on_enable: OnEnable,
    ) -> Self {
        Self {
            accounts,
            cipher,
            clock: clock.clone(),
            issuer: issuer.trim().to_owned(),
            on_enable,
            enable_failures: FailureCounter::new(
                clock,
                Duration::minutes(totp::PENDING_TTL_MINUTES),
            ),
        }
    }

    pub fn accounts(&self) -> &Arc<S> {
        &self.accounts
    }

    async fn load(&self, id: Id) -> Result<S::Account> {
        self.accounts.load(id).await?.ok_or_else(not_found)
    }

    fn decrypt(&self, encrypted: &str) -> Result<String> {
        self.cipher.decrypt(encrypted).map_err(Error::internal)
    }

    fn encrypt(&self, plain: &str) -> Result<String> {
        self.cipher.encrypt(plain).map_err(Error::internal)
    }

    pub async fn status(&self, id: Id) -> Result<TotpStatus> {
        Ok(TotpStatus::of(&self.load(id).await?.totp_state()))
    }

    /// Generates a pending secret valid for [`totp::PENDING_TTL_MINUTES`].
    pub async fn setup(&self, id: Id) -> Result<SetupResult> {
        let mut account = self.load(id).await?;
        let mut state = account.totp_state();
        if state.enabled() {
            return Err(TotpError::AlreadyEnabled.into());
        }
        let secret = generate_secret();
        let url = otpauth_url(&secret, &self.issuer, &account.totp_label())?;
        let expires_at = self.clock.now() + Duration::minutes(totp::PENDING_TTL_MINUTES);
        state.pending_secret = self.encrypt(&secret)?;
        state.pending_expires_at = Some(expires_at);
        account.set_totp_state(state);
        self.accounts.store(&account).await?;
        self.enable_failures.clear(id);
        Ok(SetupResult {
            secret,
            otpauth_url: url,
            expires_at,
        })
    }

    /// Confirms the pending secret (AUTH-08 order: existence, already enabled,
    /// pending expiry, failure limit, code check).
    pub async fn enable(&self, id: Id, code: &str) -> Result<EnableResult<S::Account>> {
        let mut account = self.load(id).await?;
        let mut state = account.totp_state();
        if state.enabled() {
            return Err(TotpError::AlreadyEnabled.into());
        }
        let now = self.clock.now();
        let pending_valid = !state.pending_secret.is_empty()
            && state.pending_expires_at.is_some_and(|exp| now <= exp);
        if !pending_valid {
            return Err(TotpError::PendingExpired.into());
        }
        if self.enable_failures.count(id) >= totp::ENABLE_MAX_FAILURES {
            state.pending_secret.clear();
            state.pending_expires_at = None;
            account.set_totp_state(state);
            self.accounts.store(&account).await?;
            self.enable_failures.clear(id);
            return Err(TotpError::TooManyAttempts.into());
        }
        let secret = self.decrypt(&state.pending_secret)?;
        if !verify_code(&secret, code, now) {
            self.enable_failures.bump(id);
            return Err(TotpError::CodeInvalid.into());
        }
        let (codes, codes_json) = generate_recovery_codes(totp::RECOVERY_CODE_COUNT).await?;
        let enabled_at = self.clock.now();
        account.set_totp_state(TotpState {
            secret: self.encrypt(&secret)?,
            enabled_at: Some(enabled_at),
            pending_secret: String::new(),
            pending_expires_at: None,
            recovery_codes: codes_json,
        });
        if self.on_enable == OnEnable::RevokeSessions {
            account.revoke_tokens(enabled_at);
        }
        self.accounts.store(&account).await?;
        self.enable_failures.clear(id);
        Ok(EnableResult {
            enabled_at,
            recovery_codes: codes,
            account,
        })
    }

    fn check_current_code(&self, state: &TotpState, code: &str) -> Result<()> {
        let secret = self.decrypt(&state.secret)?;
        if verify_code(&secret, code, self.clock.now()) {
            Ok(())
        } else {
            Err(TotpError::CodeInvalid.into())
        }
    }

    async fn consume(&self, account: &S::Account, code: &str) -> Result<()> {
        let current = account.totp_state().recovery_codes;
        let Some(updated) = consume_recovery_code(&current, code, self.clock.now()).await? else {
            return Err(TotpError::RecoveryInvalid.into());
        };
        if self
            .accounts
            .swap_recovery_codes(account.id(), &current, &updated)
            .await?
        {
            Ok(())
        } else {
            // Another request consumed a code concurrently; treat as invalid.
            Err(TotpError::RecoveryInvalid.into())
        }
    }

    /// Turns 2FA off after a TOTP or recovery code; revokes every session.
    pub async fn disable(&self, id: Id, code: &str, is_recovery: bool) -> Result<()> {
        let account = self.load(id).await?;
        let state = account.totp_state();
        if !state.enabled() {
            return Err(TotpError::NotEnabled.into());
        }
        if is_recovery {
            self.consume(&account, code).await?;
        } else {
            self.check_current_code(&state, code)?;
        }
        self.clear(id).await.map(|_| ())
    }

    /// New recovery codes; requires a current TOTP code (recovery codes are not accepted).
    pub async fn regenerate_recovery_codes(&self, id: Id, code: &str) -> Result<Vec<String>> {
        let mut account = self.load(id).await?;
        let mut state = account.totp_state();
        if !state.enabled() {
            return Err(TotpError::NotEnabled.into());
        }
        self.check_current_code(&state, code)?;
        let (codes, json) = generate_recovery_codes(totp::RECOVERY_CODE_COUNT).await?;
        state.recovery_codes = json;
        account.set_totp_state(state);
        self.accounts.store(&account).await?;
        Ok(codes)
    }

    /// Login step two with a TOTP code (no recovery code consumed).
    pub async fn verify_challenge_code(&self, id: Id, code: &str) -> Result<()> {
        let account = self.load(id).await?;
        let state = account.totp_state();
        if !state.enabled() {
            return Err(TotpError::NotEnabled.into());
        }
        self.check_current_code(&state, code)
    }

    /// Login step two with a recovery code (consumes it).
    pub async fn verify_challenge_recovery(&self, id: Id, code: &str) -> Result<()> {
        let account = self.load(id).await?;
        if !account.totp_state().enabled() {
            return Err(TotpError::NotEnabled.into());
        }
        self.consume(&account, code).await
    }

    /// Clears every TOTP column and revokes the account's sessions; returns the
    /// account as it was before clearing.
    pub async fn clear(&self, id: Id) -> Result<S::Account> {
        let mut account = self.load(id).await?;
        let before = account.clone();
        account.set_totp_state(TotpState::default());
        account.revoke_tokens(self.clock.now());
        self.accounts.store(&account).await?;
        self.enable_failures.clear(id);
        Ok(before)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use zs_shared::clock::SystemClock;

    use super::*;

    #[derive(Default)]
    struct MemAdmins(Mutex<HashMap<Id, Admin>>);

    fn admin(id: Id) -> Admin {
        Admin {
            id,
            username: format!("a{id}"),
            password_hash: String::new(),
            token_version: 0,
            token_invalid_before: None,
            is_super: false,
            last_login_at: None,
            totp_secret: String::new(),
            totp_enabled_at: None,
            totp_pending_secret: String::new(),
            totp_pending_expires_at: None,
            recovery_codes: String::new(),
            created_at: Utc::now(),
        }
    }

    #[async_trait]
    impl TotpAccounts for MemAdmins {
        type Account = Admin;
        async fn load(&self, id: Id) -> Result<Option<Admin>> {
            Ok(self.0.lock().unwrap().get(&id).cloned())
        }
        async fn store(&self, a: &Admin) -> Result<()> {
            self.0.lock().unwrap().insert(a.id, a.clone());
            Ok(())
        }
        async fn swap_recovery_codes(&self, id: Id, expected: &str, codes: &str) -> Result<bool> {
            let mut m = self.0.lock().unwrap();
            let a = m.get_mut(&id).unwrap();
            if a.recovery_codes != expected {
                return Ok(false);
            }
            a.recovery_codes = codes.into();
            Ok(true)
        }
    }

    fn service(on_enable: OnEnable) -> (TotpService<MemAdmins>, Arc<MemAdmins>) {
        let store = Arc::new(MemAdmins::default());
        store.0.lock().unwrap().insert(1, admin(1));
        let svc = TotpService::new(
            store.clone(),
            Cipher::from_secret("k"),
            Arc::new(SystemClock),
            "Zebra",
            on_enable,
        );
        (svc, store)
    }

    #[test]
    fn codes_verify_with_skew() {
        let secret = generate_secret();
        assert_eq!(secret.len(), 32);
        let now = Utc::now();
        let code = code_at(&secret, now).unwrap();
        assert_eq!(code.len(), 6);
        assert!(verify_code(&secret, &code, now));
        assert!(verify_code(&secret, &code, now + Duration::seconds(30)));
        assert!(!verify_code(&secret, &code, now + Duration::seconds(95)));
        assert!(!verify_code(&secret, "", now));
        let url = otpauth_url(&secret, "Zebra", "admin").unwrap();
        assert!(url.starts_with("otpauth://totp/Zebra:admin?secret="));
    }

    #[tokio::test]
    async fn recovery_codes_are_one_shot() {
        let (plain, json) = generate_recovery_codes(2).await.unwrap();
        assert!(plain[0].len() == 11 && plain[0].as_bytes()[4] == b'-');
        let now = Utc::now();
        let updated = consume_recovery_code(&json, &plain[0].to_uppercase(), now)
            .await
            .unwrap()
            .unwrap();
        assert!(
            consume_recovery_code(&updated, &plain[0], now)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            consume_recovery_code(&updated, "bogus", now)
                .await
                .unwrap()
                .is_none()
        );
    }

    // AUTH-08: enable order — expired pending does not count, wrong codes count,
    // the limit rejects without checking the code, success clears the counter.
    #[tokio::test]
    async fn enable_follows_check_order() {
        let (svc, store) = service(OnEnable::KeepSessions);
        assert_eq!(
            svc.enable(1, "000000").await.unwrap_err().key(),
            totp::keys::PENDING_EXPIRED
        );
        assert_eq!(svc.enable_failures.count(1), 0);
        let setup = svc.setup(1).await.unwrap();
        // Expire the pending secret.
        {
            let mut m = store.0.lock().unwrap();
            m.get_mut(&1).unwrap().totp_pending_expires_at =
                Some(Utc::now() - Duration::seconds(1));
        }
        assert_eq!(
            svc.enable(1, "000000").await.unwrap_err().key(),
            totp::keys::PENDING_EXPIRED
        );
        assert_eq!(svc.enable_failures.count(1), 0);
        let setup2 = svc.setup(1).await.unwrap();
        assert_ne!(setup.secret, setup2.secret);
        let good = code_at(&setup2.secret, Utc::now()).unwrap();
        let bad = if good == "000000" { "111111" } else { "000000" };
        for i in 1..=totp::ENABLE_MAX_FAILURES {
            assert_eq!(
                svc.enable(1, bad).await.unwrap_err().key(),
                totp::keys::CODE_INVALID
            );
            assert_eq!(svc.enable_failures.count(1), i);
        }
        // Limit reached: rejected even with the right code, pending cleared.
        assert_eq!(
            svc.enable(1, &good).await.unwrap_err().key(),
            totp::keys::TOO_MANY_ATTEMPTS
        );
        assert!(store.0.lock().unwrap()[&1].totp_pending_secret.is_empty());
        let setup3 = svc.setup(1).await.unwrap();
        let code = code_at(&setup3.secret, Utc::now()).unwrap();
        let res = svc.enable(1, &code).await.unwrap();
        assert_eq!(res.recovery_codes.len(), totp::RECOVERY_CODE_COUNT);
        assert_eq!(svc.enable_failures.count(1), 0);
        // Admins keep their sessions on enable.
        assert_eq!(res.account.token_version, 0);
        let status = svc.status(1).await.unwrap();
        assert!(status.enabled);
        assert_eq!(status.recovery_codes_remaining, 10);
        assert_eq!(
            svc.setup(1).await.unwrap_err().key(),
            totp::keys::ALREADY_ENABLED
        );
    }

    #[tokio::test]
    async fn users_revoke_sessions_on_enable_and_disable_clears() {
        let (svc, store) = service(OnEnable::RevokeSessions);
        let s = svc.setup(1).await.unwrap();
        let res = svc
            .enable(1, &code_at(&s.secret, Utc::now()).unwrap())
            .await
            .unwrap();
        assert_eq!(res.account.token_version, 1);
        // Regenerate refuses recovery codes and wrong codes.
        assert_eq!(
            svc.regenerate_recovery_codes(1, &res.recovery_codes[0])
                .await
                .unwrap_err()
                .key(),
            totp::keys::CODE_INVALID
        );
        svc.verify_challenge_recovery(1, &res.recovery_codes[0])
            .await
            .unwrap();
        assert_eq!(
            svc.verify_challenge_recovery(1, &res.recovery_codes[0])
                .await
                .unwrap_err()
                .key(),
            totp::keys::RECOVERY_INVALID
        );
        svc.disable(1, &res.recovery_codes[1], true).await.unwrap();
        let a = store.0.lock().unwrap()[&1].clone();
        assert!(a.totp_enabled_at.is_none() && a.recovery_codes.is_empty());
        assert_eq!(a.token_version, 2);
        assert_eq!(
            svc.disable(1, "123456", false).await.unwrap_err().key(),
            totp::keys::NOT_ENABLED
        );
        assert_eq!(
            svc.status(9).await.unwrap_err().key(),
            "error.user_not_found"
        );
    }
}
