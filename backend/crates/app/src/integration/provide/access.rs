//! Compat keys: the `app_key` the PHP-born protocols (acg-faka `sign`, mcy
//! `Api-Signature`) sign with, and the authentication of their callers.
//!
//! - `app_id` / `Api-Id` is the caller's **user id** (the convention of both systems);
//!   the user must own an approved + enabled credential and be active (UPS-18).
//! - The `app_key` is a separate random secret per credential, never the HMAC secret
//!   of the legacy / zebra-store protocols: the compatibility protocols sign with plain
//!   md5 and acg-faka even posts the key in clear, so a leaked `app_key` must not
//!   compromise the stronger protocols. The owner issues / resets it, switches the
//!   compat access on or off and may restrict it to an IP allowlist.
//! - A re-approval of the credential (new `api_key`) invalidates the compat key.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rand::Rng;
use zs_domain::integration::credential::CredentialStatus;
use zs_domain::integration::keys;
use zs_domain::integration::provide::{
    APP_KEY_LEN, CompatKey, CompatKeyRepo, ip_allowed, normalize_allowlist,
};
use zs_domain::integration::supplier::TOUCH_THROTTLE_SECS;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;

use crate::integration::credential::{Caller, CredentialService};

/// Alphabet of generated app keys (acg-faka / mcy keys are uppercase alphanumerics).
const APP_KEY_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

/// Random uppercase alphanumeric key.
fn random_app_key() -> String {
    let mut rng = rand::rng();
    (0..APP_KEY_LEN)
        .map(|_| char::from(APP_KEY_ALPHABET[rng.random_range(0..APP_KEY_ALPHABET.len())]))
        .collect()
}

/// Why a compat request was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessDenied {
    /// Missing / malformed `app_id`, no usable credential, or no compat key.
    UnknownApp,
    /// The owner switched compat access off.
    Disabled,
    /// Client address outside the owner's allowlist.
    IpDenied,
    BadSignature,
    Internal,
}

/// The owner's view of the compat key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatView {
    /// `app_id` / `Api-Id` to enter on the downstream site (the user id).
    pub app_id: Id,
    /// Plain key; empty until issued (or after a re-approval invalidated it).
    pub app_key: String,
    pub is_active: bool,
    pub ip_allowlist: String,
    pub last_used_at: Option<DateTime<Utc>>,
}

/// Compat key management and caller authentication.
#[derive(Clone)]
pub struct CompatAccess {
    keys: Arc<dyn CompatKeyRepo>,
    credentials: CredentialService,
    cipher: Cipher,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for CompatAccess {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CompatAccess")
    }
}

impl CompatAccess {
    pub fn new(
        keys: Arc<dyn CompatKeyRepo>,
        credentials: CredentialService,
        cipher: Cipher,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            keys,
            credentials,
            cipher,
            clock,
        }
    }

    /// Root order id of an order number (order lookups of the facades).
    pub async fn order_id_by_no(&self, order_no: &str) -> Result<Option<Id>> {
        self.keys.order_id_by_no(order_no.trim()).await
    }

    // ------------------------------------------------------------------ owner

    /// The owner's approved credential (not approved → the credential page's
    /// "not approved" message).
    async fn approved(
        &self,
        user_id: Id,
    ) -> Result<zs_domain::integration::credential::ApiCredential> {
        let own = self
            .credentials
            .mine(user_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::CREDENTIAL_NOT_FOUND))?;
        if own.credential.status != CredentialStatus::Approved {
            return Err(Error::bad_request(keys::MSG_CREDENTIAL_NOT_APPROVED));
        }
        Ok(own.credential)
    }

    fn view(&self, user_id: Id, api_key: &str, key: Option<CompatKey>) -> Result<CompatView> {
        let key = key.filter(|k| k.bound_api_key == api_key);
        let app_key = match &key {
            Some(k) => self.cipher.decrypt(&k.app_key).map_err(Error::internal)?,
            None => String::new(),
        };
        Ok(CompatView {
            app_id: user_id,
            app_key,
            is_active: key.as_ref().is_some_and(|k| k.is_active),
            ip_allowlist: key
                .as_ref()
                .map(|k| k.ip_allowlist.clone())
                .unwrap_or_default(),
            last_used_at: key.and_then(|k| k.last_used_at),
        })
    }

    /// The owner's compat key (`app_key` empty until issued).
    pub async fn mine(&self, user_id: Id) -> Result<CompatView> {
        let c = self.approved(user_id).await?;
        let key = self
            .keys
            .get(c.id)
            .await
            .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))?;
        self.view(user_id, &c.api_key, key)
            .map_err(|e| e.or_internal(keys::CREDENTIAL_FETCH_FAILED))
    }

    /// Issues a new `app_key` (the previous one stops working at once) and enables
    /// compat access; the allowlist is kept.
    pub async fn issue(&self, user_id: Id) -> Result<CompatView> {
        let wrap = |e: Error| e.or_internal(keys::CREDENTIAL_REGENERATE_FAILED);
        let c = self.approved(user_id).await?;
        let now = self.clock.now();
        let previous = self.keys.get(c.id).await.map_err(wrap)?;
        let plain = random_app_key();
        let key = CompatKey {
            credential_id: c.id,
            user_id,
            bound_api_key: c.api_key.clone(),
            app_key: self
                .cipher
                .encrypt(&plain)
                .map_err(Error::internal)
                .map_err(wrap)?,
            is_active: true,
            ip_allowlist: previous
                .as_ref()
                .map(|k| k.ip_allowlist.clone())
                .unwrap_or_default(),
            last_used_at: None,
            created_at: previous.as_ref().map_or(now, |k| k.created_at),
            updated_at: now,
        };
        self.keys.put(&key, now).await.map_err(wrap)?;
        self.view(user_id, &c.api_key, Some(key)).map_err(wrap)
    }

    /// Switches compat access and / or replaces the IP allowlist.
    pub async fn update(
        &self,
        user_id: Id,
        is_active: Option<bool>,
        ip_allowlist: Option<&str>,
    ) -> Result<CompatView> {
        let wrap = |e: Error| e.or_internal(keys::CREDENTIAL_UPDATE_FAILED);
        let c = self.approved(user_id).await?;
        let mut key = self
            .keys
            .get(c.id)
            .await
            .map_err(wrap)?
            .filter(|k| k.bound_api_key == c.api_key)
            .ok_or_else(|| Error::bad_request(keys::CREDENTIAL_NOT_FOUND))?;
        if let Some(active) = is_active {
            key.is_active = active;
        }
        if let Some(raw) = ip_allowlist {
            key.ip_allowlist = normalize_allowlist(raw).map_err(|_| Error::invalid())?;
        }
        let now = self.clock.now();
        key.updated_at = now;
        self.keys.put(&key, now).await.map_err(wrap)?;
        self.view(user_id, &c.api_key, Some(key)).map_err(wrap)
    }

    // ------------------------------------------------------------------ callers

    /// Authenticates a compat caller: `app_id` (user id) → usable credential of an
    /// active user → active compat key bound to the current `api_key` → IP allowlist →
    /// `verify(app_key)` (the facade's signature check).
    pub async fn authenticate(
        &self,
        app_id: &str,
        client_ip: &str,
        verify: impl Fn(&str) -> bool + Send,
    ) -> std::result::Result<Caller, AccessDenied> {
        let internal = |error: Error| {
            tracing::error!(%error, "compat auth failed");
            AccessDenied::Internal
        };
        let app_id = app_id.trim();
        let Some(user_id) = app_id
            .parse::<Id>()
            .ok()
            .filter(|id| *id > 0 && id.to_string() == app_id)
        else {
            return Err(AccessDenied::UnknownApp);
        };
        let Some(credential) = self
            .credentials
            .usable_for_user(user_id)
            .await
            .map_err(internal)?
        else {
            return Err(AccessDenied::UnknownApp);
        };
        let Some(key) = self
            .keys
            .get(credential.id)
            .await
            .map_err(internal)?
            .filter(|k| k.bound_api_key == credential.api_key)
        else {
            return Err(AccessDenied::UnknownApp);
        };
        if !key.is_active {
            return Err(AccessDenied::Disabled);
        }
        if !ip_allowed(&key.ip_allowlist, client_ip) {
            tracing::warn!(
                credential_id = credential.id,
                client_ip,
                "compat call from a non-allowlisted address"
            );
            return Err(AccessDenied::IpDenied);
        }
        let app_key = self
            .cipher
            .decrypt(&key.app_key)
            .map_err(|e| internal(Error::internal(e)))?;
        if app_key.is_empty() || !verify(&app_key) {
            return Err(AccessDenied::BadSignature);
        }
        let now = self.clock.now();
        if key
            .last_used_at
            .is_none_or(|t| (now - t).num_seconds() > TOUCH_THROTTLE_SECS)
            && let Err(error) = self.keys.touch(credential.id, now).await
        {
            tracing::warn!(%error, "compat key touch failed");
        }
        self.credentials.touch_used(&credential).await;
        Ok(Caller {
            credential_id: credential.id,
            user_id: credential.user_id,
            api_key: credential.api_key,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use zs_shared::clock::FixedClock;

    use super::*;
    use crate::integration::credential::tests::MemCredentials;

    #[derive(Default)]
    struct MemKeys(Mutex<Vec<CompatKey>>);

    #[async_trait]
    impl CompatKeyRepo for MemKeys {
        async fn get(&self, credential_id: Id) -> Result<Option<CompatKey>> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .iter()
                .find(|k| k.credential_id == credential_id)
                .cloned())
        }
        async fn put(&self, key: &CompatKey, _now: DateTime<Utc>) -> Result<()> {
            let mut rows = self.0.lock().unwrap();
            rows.retain(|k| k.credential_id != key.credential_id);
            rows.push(key.clone());
            Ok(())
        }
        async fn touch(&self, credential_id: Id, at: DateTime<Utc>) -> Result<()> {
            if let Some(k) = self
                .0
                .lock()
                .unwrap()
                .iter_mut()
                .find(|k| k.credential_id == credential_id)
            {
                k.last_used_at = Some(at);
            }
            Ok(())
        }
        async fn order_id_by_no(&self, _order_no: &str) -> Result<Option<Id>> {
            Ok(None)
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    async fn setup() -> (CompatAccess, CredentialService, Arc<MemCredentials>) {
        let repo = Arc::new(MemCredentials::default());
        *repo.user_status.lock().unwrap() = "active".into();
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(now()));
        let cipher = Cipher::from_secret("test-secret");
        let credentials = CredentialService::new(repo.clone(), cipher.clone(), clock.clone());
        let access = CompatAccess::new(
            Arc::new(MemKeys::default()),
            credentials.clone(),
            cipher,
            clock,
        );
        (access, credentials, repo)
    }

    #[tokio::test]
    async fn issue_and_authenticate() {
        let (access, credentials, repo) = setup().await;
        let c = credentials.apply(7).await.unwrap();
        // pending credential: no compat key can be issued (UPS-18)
        assert_eq!(
            access.issue(7).await.unwrap_err().key(),
            keys::MSG_CREDENTIAL_NOT_APPROVED
        );
        credentials.approve(c.id).await.unwrap();
        assert_eq!(access.mine(7).await.unwrap().app_key, "");
        let v = access.issue(7).await.unwrap();
        assert_eq!(v.app_id, 7);
        assert_eq!(v.app_key.len(), APP_KEY_LEN);
        assert!(
            v.app_key
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
        );
        let key = v.app_key.clone();
        let ok = |k: &str| k == key;
        assert_eq!(
            access
                .authenticate("7", "1.1.1.1", ok)
                .await
                .unwrap()
                .user_id,
            7
        );
        assert_eq!(
            access.authenticate("7", "1.1.1.1", |_| false).await,
            Err(AccessDenied::BadSignature)
        );
        for bad in ["", "07", "abc", "-7", "8"] {
            assert_eq!(
                access.authenticate(bad, "1.1.1.1", ok).await,
                Err(AccessDenied::UnknownApp),
                "{bad}"
            );
        }
        // allowlist
        access.update(7, None, Some("10.0.0.0/8")).await.unwrap();
        assert_eq!(
            access.authenticate("7", "1.1.1.1", ok).await,
            Err(AccessDenied::IpDenied)
        );
        assert!(access.authenticate("7", "10.1.2.3", ok).await.is_ok());
        assert!(access.update(7, None, Some("nope")).await.is_err());
        // owner switch
        access.update(7, Some(false), None).await.unwrap();
        assert_eq!(
            access.authenticate("7", "10.1.2.3", ok).await,
            Err(AccessDenied::Disabled)
        );
        // reissue: old key dies, access re-enabled, allowlist kept
        let v2 = access.issue(7).await.unwrap();
        assert_ne!(v2.app_key, key);
        assert_eq!(v2.ip_allowlist, "10.0.0.0/8");
        assert_eq!(
            access.authenticate("7", "10.1.2.3", ok).await,
            Err(AccessDenied::BadSignature)
        );
        let key2 = v2.app_key.clone();
        assert!(
            access
                .authenticate("7", "10.1.2.3", |k| k == key2)
                .await
                .is_ok()
        );
        // disabled user / credential
        *repo.user_status.lock().unwrap() = "disabled".into();
        assert_eq!(
            access.authenticate("7", "10.1.2.3", |k| k == key2).await,
            Err(AccessDenied::UnknownApp)
        );
        *repo.user_status.lock().unwrap() = "active".into();
        credentials.set_active(c.id, false).await.unwrap();
        assert_eq!(
            access.authenticate("7", "10.1.2.3", |k| k == key2).await,
            Err(AccessDenied::UnknownApp)
        );
        // re-approval (new api_key) invalidates the compat key
        credentials.approve(c.id).await.unwrap();
        assert_eq!(
            access.authenticate("7", "10.1.2.3", |k| k == key2).await,
            Err(AccessDenied::UnknownApp)
        );
        assert_eq!(access.mine(7).await.unwrap().app_key, "");
    }
}
