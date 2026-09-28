//! API credentials: user application, admin review, request authentication.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rand::RngCore;
use zs_domain::integration::credential::{
    ApiCredential, ApplyDecision, CredentialFilter, CredentialRepo, CredentialSecurityRepo,
    CredentialStatus, ROTATION_DAYS, Rotation, apply_decision,
};
use zs_domain::integration::keys;
use zs_domain::integration::supplier::TOUCH_THROTTLE_SECS;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;
use zs_shared::page::Page;
use zs_shared::sign;

/// Random bytes of an API key (hex-encoded → 64 chars, fits `varchar(64)`).
const KEY_BYTES: usize = 32;
/// Random bytes of an API secret (hex → 64 chars; encrypted it stays below the
/// `varchar(256)` column, UPS-18).
const SECRET_BYTES: usize = 32;

/// Random lowercase hex string of `bytes` random bytes.
pub fn random_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut buf);
    hex_encode(&buf)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from(HEX[usize::from(b >> 4)]));
        s.push(char::from(HEX[usize::from(b & 0x0f)]));
    }
    s
}

/// Why an upstream API request was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailure {
    MissingHeaders,
    InvalidTimestamp,
    TimestampExpired,
    /// Unknown key, or credential not approved / disabled / deleted.
    InvalidKey,
    UserDisabled,
    InvalidSignature,
    Internal,
}

/// A signed request to authenticate.
#[derive(Debug, Clone, Copy)]
pub struct SignedRequest<'a> {
    pub api_key: &'a str,
    pub timestamp: &'a str,
    pub signature: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub body: &'a [u8],
}

/// An authenticated API buyer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Caller {
    pub credential_id: Id,
    pub user_id: Id,
    pub api_key: String,
}

/// Plain credential view for its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnCredential {
    pub credential: ApiCredential,
    /// Last 4 characters of the plain secret (approved only).
    pub secret_tail: String,
    /// Expiry of a pending secret rotation (`None` = no rotation pending).
    pub rotation_expires_at: Option<DateTime<Utc>>,
}

/// A zebra-store signed request (spec §2).
#[derive(Debug, Clone, Copy)]
pub struct ZsSignedRequest<'a> {
    pub api_key: &'a str,
    pub timestamp: &'a str,
    pub nonce: &'a str,
    pub signature: &'a str,
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub body: &'a [u8],
}

/// Why a zebra-store request was refused (spec §8: 401 without details, 403 for
/// unusable credentials / owners).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZsAuthFailure {
    Unauthorized,
    Forbidden,
    Internal,
}

/// Secrets a request may be signed with.
#[derive(Debug, Clone)]
struct Secrets {
    current: String,
    /// Pending rotation secret (plain), still within its window.
    next: Option<String>,
}

/// API credential use cases.
#[derive(Clone)]
pub struct CredentialService {
    repo: Arc<dyn CredentialRepo>,
    security: Option<Arc<dyn CredentialSecurityRepo>>,
    cipher: Cipher,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for CredentialService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CredentialService")
    }
}

fn not_found() -> Error {
    Error::not_found(keys::CREDENTIAL_NOT_FOUND)
}

impl CredentialService {
    pub fn new(repo: Arc<dyn CredentialRepo>, cipher: Cipher, clock: Arc<dyn Clock>) -> Self {
        Self {
            repo,
            security: None,
            cipher,
            clock,
        }
    }

    /// Enables secret rotation and nonce replay protection.
    #[must_use]
    pub fn with_security(mut self, security: Arc<dyn CredentialSecurityRepo>) -> Self {
        self.security = Some(security);
        self
    }

    /// Current secret plus a pending rotation secret; an expired rotation is promoted
    /// first (spec §2: automatic promotion after 7 days).
    async fn secrets(&self, c: &ApiCredential) -> Result<Secrets> {
        let mut current = self.plain_secret(c)?;
        let mut next = None;
        if let Some(security) = &self.security
            && let Some(r) = security.rotation(c.id).await?
        {
            let plain = self
                .cipher
                .decrypt(&r.secret_next)
                .map_err(Error::internal)?;
            if r.expires_at <= self.now() {
                if security.promote_rotation(c.id, self.now()).await? {
                    tracing::info!(
                        credential_id = c.id,
                        "api secret rotation promoted on expiry"
                    );
                }
                current = plain;
            } else {
                next = Some(plain);
            }
        }
        Ok(Secrets { current, next })
    }

    /// Starts a secret rotation: a new secret is kept as `secret_next` for
    /// [`ROTATION_DAYS`]; both verify until the buyer first signs with the new one.
    /// Returns the plain new secret (shown once) and the rotation expiry.
    pub async fn rotate_mine(&self, user_id: Id) -> Result<(ApiCredential, String, DateTime<Utc>)> {
        let wrap = |e: Error| e.or_internal(keys::CREDENTIAL_REGENERATE_FAILED);
        let c = self
            .repo
            .get_by_user(user_id)
            .await
            .map_err(wrap)?
            .ok_or_else(not_found)?;
        if c.status != CredentialStatus::Approved {
            return Err(Error::bad_request(keys::MSG_CREDENTIAL_NOT_APPROVED));
        }
        let security = self
            .security
            .as_ref()
            .ok_or_else(|| Error::internal_msg("secret rotation not configured"))
            .map_err(wrap)?;
        let secret = random_hex(SECRET_BYTES);
        let expires_at = self.now() + chrono::Duration::days(ROTATION_DAYS);
        let rotation = Rotation {
            credential_id: c.id,
            secret_next: self
                .cipher
                .encrypt(&secret)
                .map_err(Error::internal)
                .map_err(wrap)?,
            expires_at,
        };
        security
            .put_rotation(&rotation, self.now())
            .await
            .map_err(wrap)?;
        Ok((c, secret, expires_at))
    }

    /// Authenticates a zebra-store request (spec §2): ±300 s, nonce syntax, approved +
    /// active credential of an active user, HMAC over the canonical string with the
    /// current or pending secret (first use of the pending one promotes it), then the
    /// nonce is claimed (replays within 10 min are refused).
    pub async fn authenticate_zs(
        &self,
        req: ZsSignedRequest<'_>,
    ) -> std::result::Result<Caller, ZsAuthFailure> {
        use zs_shared::zs;
        let unauthorized = Err(ZsAuthFailure::Unauthorized);
        if req.api_key.is_empty() || req.signature.is_empty() || !zs::valid_nonce(req.nonce) {
            return unauthorized;
        }
        let Ok(ts) = req.timestamp.trim().parse::<i64>() else {
            return unauthorized;
        };
        let now = self.now();
        if !zs::timestamp_valid(ts, now.timestamp()) {
            return unauthorized;
        }
        let internal = |error: Error| {
            tracing::error!(%error, "zs auth failed");
            ZsAuthFailure::Internal
        };
        let Some(found) = self
            .repo
            .find_for_auth(req.api_key)
            .await
            .map_err(internal)?
        else {
            return unauthorized;
        };
        if !found.credential.usable() || found.user_status.as_deref() != Some("active") {
            return Err(ZsAuthFailure::Forbidden);
        }
        let c = found.credential;
        let secrets = self.secrets(&c).await.map_err(internal)?;
        let signed = zs::Signed {
            method: req.method,
            path: req.path,
            query: req.query,
            timestamp: ts,
            nonce: req.nonce,
            body: req.body,
        };
        if !signed.verify(&secrets.current, req.signature) {
            let with_next = secrets
                .next
                .as_deref()
                .is_some_and(|next| signed.verify(next, req.signature));
            if !with_next {
                return unauthorized;
            }
            if let Some(security) = &self.security
                && security
                    .promote_rotation(c.id, now)
                    .await
                    .map_err(internal)?
            {
                tracing::info!(
                    credential_id = c.id,
                    "api secret rotation promoted on first use"
                );
            }
        }
        if let Some(security) = &self.security
            && !security
                .claim_nonce(c.id, req.nonce, now)
                .await
                .map_err(internal)?
        {
            tracing::warn!(credential_id = c.id, "zs nonce replay refused");
            return unauthorized;
        }
        self.touch(&c, now).await;
        Ok(Caller {
            credential_id: c.id,
            user_id: c.user_id,
            api_key: c.api_key,
        })
    }

    async fn touch(&self, c: &ApiCredential, now: DateTime<Utc>) {
        let stale = c
            .last_used_at
            .is_none_or(|t| (now - t).num_seconds() > TOUCH_THROTTLE_SECS);
        if stale && let Err(error) = self.repo.touch(c.id, now).await {
            tracing::warn!(%error, "api auth touch failed");
        }
    }

    /// The current plain secret of a credential (after any due promotion).
    pub async fn current_secret(&self, c: &ApiCredential) -> Result<String> {
        Ok(self.secrets(c).await?.current)
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Decrypted secret of a credential (empty when none was issued).
    pub fn plain_secret(&self, c: &ApiCredential) -> Result<String> {
        if c.api_secret.is_empty() {
            return Ok(String::new());
        }
        self.cipher.decrypt(&c.api_secret).map_err(Error::internal)
    }

    /// The caller's live credential (`None` → `{"status":"none"}`).
    pub async fn mine(&self, user_id: Id) -> Result<Option<OwnCredential>> {
        let Some(c) = self
            .repo
            .get_by_user(user_id)
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))?
        else {
            return Ok(None);
        };
        let secret = if c.status == CredentialStatus::Approved {
            self.plain_secret(&c)
                .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))?
        } else {
            String::new()
        };
        let tail = if secret.len() >= 4 {
            secret[secret.len() - 4..].to_owned()
        } else {
            String::new()
        };
        let rotation_expires_at = match &self.security {
            Some(security) if c.status == CredentialStatus::Approved => security
                .rotation(c.id)
                .await
                .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))?
                .map(|r| r.expires_at)
                .filter(|t| *t > self.now()),
            _ => None,
        };
        Ok(Some(OwnCredential {
            credential: c,
            secret_tail: tail,
            rotation_expires_at,
        }))
    }

    /// Applies for API access (UPS-18: unique random key even while pending; deleted or
    /// rejected rows are reset in place).
    pub async fn apply(&self, user_id: Id) -> Result<ApiCredential> {
        let wrap = |e: Error| e.or_internal(keys::CREDENTIAL_APPLY_FAILED);
        let existing = self.repo.get_any_by_user(user_id).await.map_err(wrap)?;
        match apply_decision(existing.as_ref()) {
            ApplyDecision::Pending => Err(Error::bad_request(keys::MSG_CREDENTIAL_PENDING)),
            ApplyDecision::Exists => Err(Error::bad_request(keys::MSG_CREDENTIAL_EXISTS)),
            ApplyDecision::Create => self
                .repo
                .create(user_id, &random_hex(KEY_BYTES), self.now())
                .await
                .map_err(wrap),
            ApplyDecision::Reset => {
                let mut c = existing.ok_or_else(|| Error::internal_msg("missing credential"))?;
                c.reset_for_reapply(random_hex(KEY_BYTES));
                self.repo.save(&c, self.now()).await.map_err(wrap)?;
                Ok(c)
            }
        }
    }

    /// New secret for an approved credential; returns the plain secret (shown once).
    async fn regenerate(&self, mut c: ApiCredential) -> Result<String> {
        if c.status != CredentialStatus::Approved {
            return Err(Error::bad_request(keys::MSG_CREDENTIAL_NOT_APPROVED));
        }
        let secret = random_hex(SECRET_BYTES);
        c.api_secret = self.cipher.encrypt(&secret).map_err(Error::internal)?;
        self.repo.save(&c, self.now()).await?;
        if let Some(security) = &self.security {
            security.clear_rotation(c.id).await?;
        }
        Ok(secret)
    }

    pub async fn regenerate_mine(&self, user_id: Id) -> Result<String> {
        let c = self
            .repo
            .get_by_user(user_id)
            .await?
            .ok_or_else(not_found)?;
        self.regenerate(c)
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_REGENERATE_FAILED))
    }

    pub async fn set_active_mine(&self, user_id: Id, active: bool) -> Result<()> {
        let c = self
            .repo
            .get_by_user(user_id)
            .await?
            .ok_or_else(not_found)?;
        if c.status != CredentialStatus::Approved {
            return Err(Error::bad_request(keys::MSG_CREDENTIAL_NOT_APPROVED));
        }
        self.save_active(c, active).await
    }

    async fn save_active(&self, mut c: ApiCredential, active: bool) -> Result<()> {
        c.is_active = active;
        self.repo
            .save(&c, self.now())
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_UPDATE_FAILED))
    }

    // ------------------------------------------------------------------ admin

    pub async fn list(&self, filter: &CredentialFilter) -> Result<Page<ApiCredential>> {
        self.repo
            .list(filter)
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))
    }

    pub async fn get(&self, id: Id) -> Result<ApiCredential> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))?
            .ok_or_else(not_found)
    }

    /// Approves: a fresh key and secret are generated; neither is returned to the
    /// admin (UPS-10), the owner fetches the key and regenerates the secret.
    pub async fn approve(&self, id: Id) -> Result<ApiCredential> {
        let wrap = |e: Error| e.or_internal(keys::CREDENTIAL_APPROVE_FAILED);
        let mut c = self
            .repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(not_found)?;
        let secret = random_hex(SECRET_BYTES);
        c.api_key = random_hex(KEY_BYTES);
        c.api_secret = self
            .cipher
            .encrypt(&secret)
            .map_err(Error::internal)
            .map_err(wrap)?;
        c.status = CredentialStatus::Approved;
        c.approved_at = Some(self.now());
        c.is_active = true;
        c.reject_reason.clear();
        self.repo.save(&c, self.now()).await.map_err(wrap)?;
        Ok(c)
    }

    pub async fn reject(&self, id: Id, reason: &str) -> Result<()> {
        let wrap = |e: Error| e.or_internal(keys::CREDENTIAL_REJECT_FAILED);
        let mut c = self
            .repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(not_found)?;
        c.status = CredentialStatus::Rejected;
        c.reject_reason = reason.to_owned();
        c.is_active = false;
        self.repo.save(&c, self.now()).await.map_err(wrap)
    }

    pub async fn set_active(&self, id: Id, active: bool) -> Result<()> {
        let c = self
            .repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_UPDATE_FAILED))?
            .ok_or_else(not_found)?;
        if c.status != CredentialStatus::Approved {
            return Err(Error::bad_request(keys::CREDENTIAL_NOT_APPROVED));
        }
        self.save_active(c, active).await
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        self.repo
            .delete(id, self.now())
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_DELETE_FAILED))
    }

    /// Credential by id (downstream callbacks).
    pub async fn find(&self, id: Id) -> Result<Option<ApiCredential>> {
        self.repo.get(id).await
    }

    /// The user's credential when it may call the API: approved + enabled (UPS-18)
    /// and owned by an active user. Used by the provider-compat protocols, whose
    /// callers identify themselves by user id (`app_id` / `Api-Id`).
    pub async fn usable_for_user(&self, user_id: Id) -> Result<Option<ApiCredential>> {
        if user_id <= 0 {
            return Ok(None);
        }
        let Some(c) = self.repo.get_by_user(user_id).await? else {
            return Ok(None);
        };
        if !c.usable() {
            return Ok(None);
        }
        let owner_active = self
            .repo
            .find_for_auth(&c.api_key)
            .await?
            .is_some_and(|f| f.user_status.as_deref() == Some("active"));
        Ok(owner_active.then_some(c))
    }

    /// Records a successful compat call on the credential (throttled like the others).
    pub async fn touch_used(&self, c: &ApiCredential) {
        self.touch(c, self.now()).await;
    }

    // ------------------------------------------------------------------ auth

    /// Authenticates an upstream API request (original `UpstreamAPIAuthMiddleware`):
    /// headers, ±60 s timestamp, approved + active credential, active owner, then the
    /// HMAC signature in constant time; `last_used_at` is touched at most once a minute.
    pub async fn authenticate(
        &self,
        req: SignedRequest<'_>,
    ) -> std::result::Result<Caller, AuthFailure> {
        if req.api_key.is_empty() || req.timestamp.is_empty() || req.signature.is_empty() {
            return Err(AuthFailure::MissingHeaders);
        }
        let ts: i64 = req
            .timestamp
            .trim()
            .parse()
            .map_err(|_| AuthFailure::InvalidTimestamp)?;
        let now = self.now();
        if !sign::timestamp_valid(ts, now.timestamp()) {
            return Err(AuthFailure::TimestampExpired);
        }
        let found = self
            .repo
            .find_for_auth(req.api_key)
            .await
            .map_err(|error| {
                tracing::error!(%error, "upstream auth lookup failed");
                AuthFailure::Internal
            })?;
        let Some(found) = found.filter(|f| f.credential.usable()) else {
            return Err(AuthFailure::InvalidKey);
        };
        if found.user_status.as_deref() != Some("active") {
            return Err(AuthFailure::UserDisabled);
        }
        let c = found.credential;
        let secrets = self.secrets(&c).await.map_err(|error| {
            tracing::error!(%error, credential_id = c.id, "credential secret decrypt failed");
            AuthFailure::Internal
        })?;
        let verify = |secret: &str| {
            !secret.is_empty()
                && sign::verify(secret, req.method, req.path, req.signature, ts, req.body)
        };
        if !verify(&secrets.current) {
            // A pending rotation secret also verifies; its first use promotes it.
            if !secrets.next.as_deref().is_some_and(verify) {
                return Err(AuthFailure::InvalidSignature);
            }
            if let Some(security) = &self.security
                && let Err(error) = security.promote_rotation(c.id, now).await
            {
                tracing::error!(%error, credential_id = c.id, "rotation promotion failed");
                return Err(AuthFailure::Internal);
            }
        }
        self.touch(&c, now).await;
        Ok(Caller {
            credential_id: c.id,
            user_id: c.user_id,
            api_key: c.api_key,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use zs_domain::integration::credential::AuthCredential;
    use zs_shared::clock::FixedClock;

    use super::*;

    #[derive(Default)]
    pub struct MemCredentials {
        pub rows: Mutex<Vec<ApiCredential>>,
        pub user_status: Mutex<String>,
        pub touched: Mutex<u32>,
    }

    #[async_trait]
    impl CredentialRepo for MemCredentials {
        async fn get(&self, id: Id) -> Result<Option<ApiCredential>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id && !c.deleted)
                .cloned())
        }
        async fn get_by_user(&self, user_id: Id) -> Result<Option<ApiCredential>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.user_id == user_id && !c.deleted)
                .cloned())
        }
        async fn get_any_by_user(&self, user_id: Id) -> Result<Option<ApiCredential>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.user_id == user_id)
                .cloned())
        }
        async fn find_for_auth(&self, api_key: &str) -> Result<Option<AuthCredential>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.api_key == api_key && !c.deleted)
                .cloned()
                .map(|credential| AuthCredential {
                    credential,
                    user_status: Some(self.user_status.lock().unwrap().clone()),
                }))
        }
        async fn create(
            &self,
            user_id: Id,
            api_key: &str,
            now: DateTime<Utc>,
        ) -> Result<ApiCredential> {
            let mut rows = self.rows.lock().unwrap();
            let c = ApiCredential {
                id: i64::try_from(rows.len()).unwrap() + 1,
                user_id,
                api_key: api_key.into(),
                api_secret: String::new(),
                status: CredentialStatus::PendingReview,
                reject_reason: String::new(),
                approved_at: None,
                is_active: false,
                last_used_at: None,
                created_at: now,
                updated_at: now,
                user: None,
                deleted: false,
            };
            rows.push(c.clone());
            Ok(c)
        }
        async fn save(&self, c: &ApiCredential, _now: DateTime<Utc>) -> Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(r) = rows.iter_mut().find(|r| r.id == c.id) {
                *r = c.clone();
            }
            Ok(())
        }
        async fn touch(&self, id: Id, at: DateTime<Utc>) -> Result<()> {
            *self.touched.lock().unwrap() += 1;
            if let Some(r) = self.rows.lock().unwrap().iter_mut().find(|r| r.id == id) {
                r.last_used_at = Some(at);
            }
            Ok(())
        }
        async fn delete(&self, id: Id, _at: DateTime<Utc>) -> Result<()> {
            if let Some(r) = self.rows.lock().unwrap().iter_mut().find(|r| r.id == id) {
                r.deleted = true;
            }
            Ok(())
        }
        async fn list(&self, _filter: &CredentialFilter) -> Result<Page<ApiCredential>> {
            let items = self.rows.lock().unwrap().clone();
            Ok(Page {
                total: items.len() as u64,
                items,
            })
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn service() -> (CredentialService, Arc<MemCredentials>) {
        let repo = Arc::new(MemCredentials::default());
        *repo.user_status.lock().unwrap() = "active".into();
        (
            CredentialService::new(
                repo.clone(),
                Cipher::from_secret("test-secret"),
                Arc::new(FixedClock(now())),
            ),
            repo,
        )
    }

    // UPS-18 (1): two pending applications get distinct non-empty keys.
    #[tokio::test]
    async fn ups18_pending_keys_are_unique() {
        let (s, _) = service();
        let a = s.apply(1).await.unwrap();
        let b = s.apply(2).await.unwrap();
        assert_eq!(a.api_key.len(), 64);
        assert_ne!(a.api_key, b.api_key);
        assert_eq!(
            s.apply(1).await.unwrap_err().key(),
            keys::MSG_CREDENTIAL_PENDING
        );
    }

    // UPS-18 (2) / UPS-10: approve → no secret returned; reject → reapply resets; delete → restored.
    #[tokio::test]
    async fn ups18_review_and_reapply() {
        let (s, repo) = service();
        let c = s.apply(1).await.unwrap();
        s.reject(c.id, "no").await.unwrap();
        let again = s.apply(1).await.unwrap();
        assert_eq!(again.id, c.id);
        assert_ne!(again.api_key, c.api_key);
        assert_eq!(again.status, CredentialStatus::PendingReview);
        assert!(again.reject_reason.is_empty());

        let approved = s.approve(c.id).await.unwrap();
        assert!(approved.usable());
        let secret = s.regenerate_mine(1).await.unwrap();
        assert_eq!(secret.len(), 64);
        // stored encrypted, fits the column
        let stored = repo.rows.lock().unwrap()[0].api_secret.clone();
        assert_ne!(stored, secret);
        assert!(stored.len() <= 256);
        assert_eq!(
            s.apply(1).await.unwrap_err().key(),
            keys::MSG_CREDENTIAL_EXISTS
        );

        s.delete(c.id).await.unwrap();
        let restored = s.apply(1).await.unwrap();
        assert_eq!(restored.id, c.id);
        assert_eq!(restored.status, CredentialStatus::PendingReview);
        assert!(!restored.is_active);
    }

    // UPS-10: signature, timestamp window, credential and owner status.
    #[tokio::test]
    async fn ups10_authentication_rules() {
        let (s, repo) = service();
        let c = s.apply(1).await.unwrap();
        let pending_key = c.api_key.clone();
        let ts = now().timestamp().to_string();
        let auth = |key: String, ts: String, sig: String| {
            let s = s.clone();
            async move {
                s.authenticate(SignedRequest {
                    api_key: &key,
                    timestamp: &ts,
                    signature: &sig,
                    method: "POST",
                    path: "/api/v1/upstream/ping",
                    body: b"",
                })
                .await
            }
        };
        // UPS-18: a pending credential cannot call the API
        assert_eq!(
            auth(pending_key, ts.clone(), "00".into()).await,
            Err(AuthFailure::InvalidKey)
        );
        let approved = s.approve(c.id).await.unwrap();
        let secret = s.regenerate_mine(1).await.unwrap();
        let sig = |t: i64| sign::sign(&secret, "POST", "/api/v1/upstream/ping", t, b"");
        let t0 = now().timestamp();
        let ok = auth(approved.api_key.clone(), ts.clone(), sig(t0))
            .await
            .unwrap();
        assert_eq!(ok.user_id, 1);
        assert_eq!(*repo.touched.lock().unwrap(), 1);
        // throttled touch
        auth(approved.api_key.clone(), ts.clone(), sig(t0))
            .await
            .unwrap();
        assert_eq!(*repo.touched.lock().unwrap(), 1);
        // 61 s skew
        assert_eq!(
            auth(
                approved.api_key.clone(),
                (t0 - 61).to_string(),
                sig(t0 - 61)
            )
            .await,
            Err(AuthFailure::TimestampExpired)
        );
        assert_eq!(
            auth(approved.api_key.clone(), "x".into(), sig(t0)).await,
            Err(AuthFailure::InvalidTimestamp)
        );
        assert_eq!(
            auth(approved.api_key.clone(), ts.clone(), "deadbeef".into()).await,
            Err(AuthFailure::InvalidSignature)
        );
        assert_eq!(
            auth(String::new(), ts.clone(), sig(t0)).await,
            Err(AuthFailure::MissingHeaders)
        );
        *repo.user_status.lock().unwrap() = "disabled".into();
        assert_eq!(
            auth(approved.api_key.clone(), ts.clone(), sig(t0)).await,
            Err(AuthFailure::UserDisabled)
        );
        *repo.user_status.lock().unwrap() = "active".into();
        s.set_active(c.id, false).await.unwrap();
        assert_eq!(
            auth(approved.api_key, ts, sig(t0)).await,
            Err(AuthFailure::InvalidKey)
        );
    }
}
