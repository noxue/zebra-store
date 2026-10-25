//! Personal API key persistence and operating-system-seeded entropy.
use crate::db::entity::extra::personal_api_keys as keys;
use crate::db::repo::support::{DbResultExt, insert_if_absent};
use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use zs_domain::identity::api_key::{
    ApiKeyEntropy, ApiKeyRealm, PersonalApiKey, PersonalApiKeyRepo,
};
use zs_domain::{Error, Id, Result};
#[derive(Debug, Clone)]
pub struct SeaPersonalApiKeyRepo {
    db: DatabaseConnection,
}
impl SeaPersonalApiKeyRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}
fn domain(row: keys::Model) -> Result<PersonalApiKey> {
    let realm = match row.realm.as_str() {
        "admin" => ApiKeyRealm::Admin,
        "user" => ApiKeyRealm::User,
        _ => return Err(Error::internal_msg("invalid API key realm")),
    };
    Ok(PersonalApiKey {
        realm,
        owner_id: row.owner_id,
        key_hash: row.key_hash,
        prefix: row.prefix,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
#[async_trait]
impl PersonalApiKeyRepo for SeaPersonalApiKeyRepo {
    async fn get(&self, realm: ApiKeyRealm, owner_id: Id) -> Result<Option<PersonalApiKey>> {
        keys::Entity::find()
            .filter(keys::Column::Realm.eq(realm.as_str()))
            .filter(keys::Column::OwnerId.eq(owner_id))
            .one(&self.db)
            .await
            .dom()?
            .map(domain)
            .transpose()
    }
    async fn find_hash(&self, hash: &str) -> Result<Option<PersonalApiKey>> {
        keys::Entity::find()
            .filter(keys::Column::KeyHash.eq(hash))
            .one(&self.db)
            .await
            .dom()?
            .map(domain)
            .transpose()
    }
    async fn save(&self, key: &PersonalApiKey, rotate: bool) -> Result<bool> {
        if rotate {
            let result = keys::Entity::update_many()
                .col_expr(keys::Column::KeyHash, Expr::value(key.key_hash.clone()))
                .col_expr(keys::Column::Prefix, Expr::value(key.prefix.clone()))
                .col_expr(keys::Column::UpdatedAt, Expr::value(key.updated_at))
                .filter(keys::Column::Realm.eq(key.realm.as_str()))
                .filter(keys::Column::OwnerId.eq(key.owner_id))
                .exec(&self.db)
                .await
                .dom()?;
            return Ok(result.rows_affected == 1);
        }
        insert_if_absent(
            &self.db,
            keys::ActiveModel {
                realm: Set(key.realm.as_str().to_owned()),
                owner_id: Set(key.owner_id),
                key_hash: Set(key.key_hash.clone()),
                prefix: Set(key.prefix.clone()),
                created_at: Set(key.created_at),
                updated_at: Set(key.updated_at),
                ..Default::default()
            },
            keys::Column::Id,
        )
        .await
    }
    async fn delete(&self, realm: ApiKeyRealm, owner_id: Id) -> Result<()> {
        keys::Entity::delete_many()
            .filter(keys::Column::Realm.eq(realm.as_str()))
            .filter(keys::Column::OwnerId.eq(owner_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
#[derive(Debug)]
pub struct SystemApiKeyEntropy;
impl ApiKeyEntropy for SystemApiKeyEntropy {
    fn generate(&self) -> [u8; 32] {
        rand::random()
    }
}
