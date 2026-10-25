//! Opt-in personal API keys exchange for the existing bearer token format.
use super::{admin_auth::AdminAuthService, user_auth::UserAuthService};

/// Four realm-prefix bytes plus 43 unpadded base64url bytes (256 random bits).
const ENCODED_KEY_LENGTH: usize = 47;
/// Display the prefix and only eight characters of the random secret.
const DISPLAY_PREFIX_LENGTH: usize = 12;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::sync::Arc;
use zs_domain::identity::api_key::{
    ApiKeyEntropy, ApiKeyRealm, PersonalApiKey, PersonalApiKeyRepo,
};
use zs_domain::{Error, Id, Result};
use zs_shared::{clock::Clock, crypto::sha256_hex};

#[derive(Clone)]
pub struct PersonalApiKeyService {
    repo: Arc<dyn PersonalApiKeyRepo>,
    entropy: Arc<dyn ApiKeyEntropy>,
    clock: Arc<dyn Clock>,
    admin: AdminAuthService,
    user: UserAuthService,
}
impl std::fmt::Debug for PersonalApiKeyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalApiKeyService")
    }
}
#[derive(Serialize)]
pub struct IssuedApiKey {
    pub api_key: String,
    pub prefix: String,
}
#[derive(Serialize)]
pub struct ApiKeySession {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}
impl std::fmt::Debug for IssuedApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedApiKey")
            .field("prefix", &self.prefix)
            .finish_non_exhaustive()
    }
}
impl std::fmt::Debug for ApiKeySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApiKeySession")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}
impl PersonalApiKeyService {
    pub fn new(
        repo: Arc<dyn PersonalApiKeyRepo>,
        entropy: Arc<dyn ApiKeyEntropy>,
        clock: Arc<dyn Clock>,
        admin: AdminAuthService,
        user: UserAuthService,
    ) -> Self {
        Self {
            repo,
            entropy,
            clock,
            admin,
            user,
        }
    }
    pub async fn status(&self, realm: ApiKeyRealm, owner: Id) -> Result<Option<PersonalApiKey>> {
        self.repo.get(realm, owner).await
    }
    pub async fn generate(
        &self,
        realm: ApiKeyRealm,
        owner: Id,
        rotate: bool,
    ) -> Result<IssuedApiKey> {
        let api_key = format!(
            "zs{}_{}",
            if realm == ApiKeyRealm::Admin {
                "a"
            } else {
                "u"
            },
            URL_SAFE_NO_PAD.encode(self.entropy.generate())
        );
        let prefix = api_key
            .chars()
            .take(DISPLAY_PREFIX_LENGTH)
            .collect::<String>();
        let now = self.clock.now();
        let key = PersonalApiKey {
            realm,
            owner_id: owner,
            key_hash: sha256_hex(api_key.as_bytes()),
            prefix: prefix.clone(),
            created_at: now,
            updated_at: now,
        };
        if !self.repo.save(&key, rotate).await? {
            return Err(Error::bad_request(if rotate {
                "error.api_key_missing"
            } else {
                "error.api_key_exists"
            }));
        }
        Ok(IssuedApiKey { api_key, prefix })
    }
    pub async fn delete(&self, realm: ApiKeyRealm, owner: Id) -> Result<()> {
        self.repo.delete(realm, owner).await
    }
    pub async fn exchange(&self, realm: ApiKeyRealm, raw: &str) -> Result<ApiKeySession> {
        let invalid = || Error::unauthorized("error.api_key_invalid");
        // 256 random bits encoded as 43 base64url characters, plus a realm prefix.
        if raw.len() != ENCODED_KEY_LENGTH
            || !raw.starts_with(if realm == ApiKeyRealm::Admin {
                "zsa_"
            } else {
                "zsu_"
            })
        {
            return Err(invalid());
        }
        let key = self
            .repo
            .find_hash(&sha256_hex(raw.as_bytes()))
            .await?
            .filter(|k| k.realm == realm)
            .ok_or_else(invalid)?;
        let (token, expires_at) = match realm {
            ApiKeyRealm::Admin => {
                let account = self
                    .admin
                    .repo()
                    .get(key.owner_id)
                    .await?
                    .ok_or_else(invalid)?;
                self.admin.issue_api_token(&account, &key.key_hash)?
            }
            ApiKeyRealm::User => {
                let account = self
                    .user
                    .repo()
                    .get(key.owner_id)
                    .await?
                    .filter(|u| u.is_active())
                    .ok_or_else(invalid)?;
                self.user.issue_api_token(&account, &key.key_hash)?
            }
        };
        Ok(ApiKeySession { token, expires_at })
    }
}
/// Bound tokens are revoked as soon as their source key is rotated/deleted.
pub async fn validate_binding(
    repo: Option<&Arc<dyn PersonalApiKeyRepo>>,
    realm: ApiKeyRealm,
    owner: Id,
    hash: &str,
) -> Result<()> {
    if hash.is_empty() {
        return Ok(());
    }
    let key = match repo {
        Some(repo) => repo.get(realm, owner).await?,
        None => None,
    };
    if key.is_some_and(|k| k.key_hash == hash) {
        Ok(())
    } else {
        Err(Error::unauthorized("error.token_revoked"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    struct FakeKeys(Option<PersonalApiKey>);
    #[async_trait]
    impl PersonalApiKeyRepo for FakeKeys {
        async fn get(&self, realm: ApiKeyRealm, owner: Id) -> Result<Option<PersonalApiKey>> {
            Ok(self
                .0
                .clone()
                .filter(|k| k.realm == realm && k.owner_id == owner))
        }
        async fn find_hash(&self, hash: &str) -> Result<Option<PersonalApiKey>> {
            Ok(self.0.clone().filter(|k| k.key_hash == hash))
        }
        async fn save(&self, _: &PersonalApiKey, _: bool) -> Result<bool> {
            Ok(false)
        }
        async fn delete(&self, _: ApiKeyRealm, _: Id) -> Result<()> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn auth03_key_binding_requires_same_owner_realm_and_current_generation() {
        let repo: Arc<dyn PersonalApiKeyRepo> = Arc::new(FakeKeys(Some(PersonalApiKey {
            realm: ApiKeyRealm::User,
            owner_id: 7,
            key_hash: "current".into(),
            prefix: "zsu_example".into(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })));
        assert!(
            validate_binding(Some(&repo), ApiKeyRealm::User, 7, "current")
                .await
                .is_ok()
        );
        assert!(
            validate_binding(Some(&repo), ApiKeyRealm::User, 7, "old")
                .await
                .is_err()
        );
        assert!(
            validate_binding(Some(&repo), ApiKeyRealm::Admin, 7, "current")
                .await
                .is_err()
        );
        assert!(
            validate_binding(Some(&repo), ApiKeyRealm::User, 8, "current")
                .await
                .is_err()
        );
        assert!(
            validate_binding(None, ApiKeyRealm::User, 7, "current")
                .await
                .is_err()
        );
        assert!(
            validate_binding(None, ApiKeyRealm::User, 7, "")
                .await
                .is_ok()
        );
    }
}
