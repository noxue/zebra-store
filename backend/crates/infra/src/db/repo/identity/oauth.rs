//! [`ExternalIdentityRepo`] backed by `user_oauth_identities` (+ `users` for the
//! transactional sign-up).

use async_trait::async_trait;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
    TransactionTrait,
};
use zs_domain::identity::oauth::{ExternalIdentity, ExternalIdentityRepo, NewExternalIdentity};
use zs_domain::identity::user::{NewUser, User};
use zs_domain::{Id, Result};

use super::user::{new_user_model, to_domain as user_to_domain};
use crate::db::entity::user_oauth_identities as oauth;
use crate::db::repo::support::{DbResultExt, now};

/// SeaORM implementation of [`ExternalIdentityRepo`].
#[derive(Debug, Clone)]
pub struct SeaExternalIdentityRepo {
    db: DatabaseConnection,
}

impl SeaExternalIdentityRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: oauth::Model) -> ExternalIdentity {
    ExternalIdentity {
        id: m.id,
        user_id: m.user_id,
        provider: m.provider,
        provider_user_id: m.provider_user_id,
        username: m.username,
        avatar_url: m.avatar_url,
        auth_at: m.auth_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn new_model(i: &NewExternalIdentity, user_id: Id) -> oauth::ActiveModel {
    let now = now();
    oauth::ActiveModel {
        user_id: Set(user_id),
        provider: Set(i.provider.clone()),
        provider_user_id: Set(i.provider_user_id.clone()),
        username: Set(i.username.clone()),
        avatar_url: Set(i.avatar_url.clone()),
        auth_at: Set(i.auth_at),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

#[async_trait]
impl ExternalIdentityRepo for SeaExternalIdentityRepo {
    async fn by_provider_user(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<ExternalIdentity>> {
        Ok(oauth::Entity::find()
            .filter(oauth::Column::Provider.eq(provider))
            .filter(oauth::Column::ProviderUserId.eq(provider_user_id))
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn by_user_provider(
        &self,
        user_id: Id,
        provider: &str,
    ) -> Result<Option<ExternalIdentity>> {
        Ok(oauth::Entity::find()
            .filter(oauth::Column::UserId.eq(user_id))
            .filter(oauth::Column::Provider.eq(provider))
            .order_by_asc(oauth::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn create(&self, identity: &NewExternalIdentity) -> Result<ExternalIdentity> {
        Ok(to_domain(
            new_model(identity, identity.user_id)
                .insert(&self.db)
                .await
                .dom()?,
        ))
    }

    async fn update(&self, i: &ExternalIdentity) -> Result<()> {
        oauth::ActiveModel {
            id: Set(i.id),
            provider: Set(i.provider.clone()),
            provider_user_id: Set(i.provider_user_id.clone()),
            username: Set(i.username.clone()),
            avatar_url: Set(i.avatar_url.clone()),
            auth_at: Set(i.auth_at),
            updated_at: Set(now()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn create_user_with_identity(
        &self,
        user: &NewUser,
        identity: &NewExternalIdentity,
    ) -> Result<(User, ExternalIdentity)> {
        let txn = self.db.begin().await.dom()?;
        let created = new_user_model(user).insert(&txn).await.dom()?;
        let linked = new_model(identity, created.id).insert(&txn).await.dom()?;
        txn.commit().await.dom()?;
        Ok((user_to_domain(created), to_domain(linked)))
    }
}
