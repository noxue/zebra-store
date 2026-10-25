//! Personal automation credentials, isolated from supplier protocol credentials.
use crate::{Id, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiKeyRealm {
    Admin,
    User,
}
impl ApiKeyRealm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::User => "user",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct PersonalApiKey {
    pub realm: ApiKeyRealm,
    pub owner_id: Id,
    #[serde(skip)]
    pub key_hash: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[async_trait]
pub trait PersonalApiKeyRepo: Send + Sync {
    async fn get(&self, realm: ApiKeyRealm, owner_id: Id) -> Result<Option<PersonalApiKey>>;
    async fn find_hash(&self, hash: &str) -> Result<Option<PersonalApiKey>>;
    /// Create never overwrites; rotation only changes an existing credential.
    async fn save(&self, key: &PersonalApiKey, rotate: bool) -> Result<bool>;
    async fn delete(&self, realm: ApiKeyRealm, owner_id: Id) -> Result<()>;
}
pub trait ApiKeyEntropy: Send + Sync {
    fn generate(&self) -> [u8; 32];
}
