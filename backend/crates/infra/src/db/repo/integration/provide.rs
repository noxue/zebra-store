//! [`CompatKeyRepo`]: compat keys of the provider-compat facades (acg-faka, mcy) and
//! the order-number lookup they need.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};
use zs_domain::integration::provide::{CompatKey, CompatKeyRepo};
use zs_domain::{Id, Result};

use crate::db::entity::{api_compat_keys as keys, orders};
use crate::db::repo::support::{DbResultExt, insert_if_absent};

/// SeaORM implementation of [`CompatKeyRepo`].
#[derive(Debug, Clone)]
pub struct SeaCompatKeyRepo {
    db: DatabaseConnection,
}

impl SeaCompatKeyRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: keys::Model) -> CompatKey {
    CompatKey {
        credential_id: m.credential_id,
        user_id: m.user_id,
        bound_api_key: m.bound_api_key,
        app_key: m.app_key,
        is_active: m.is_active,
        ip_allowlist: m.ip_allowlist,
        last_used_at: m.last_used_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

#[async_trait]
impl CompatKeyRepo for SeaCompatKeyRepo {
    async fn get(&self, credential_id: Id) -> Result<Option<CompatKey>> {
        Ok(keys::Entity::find()
            .filter(keys::Column::CredentialId.eq(credential_id))
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn put(&self, key: &CompatKey, now: DateTime<Utc>) -> Result<()> {
        let row = keys::ActiveModel {
            credential_id: Set(key.credential_id),
            user_id: Set(key.user_id),
            bound_api_key: Set(key.bound_api_key.clone()),
            app_key: Set(key.app_key.clone()),
            is_active: Set(key.is_active),
            ip_allowlist: Set(key.ip_allowlist.clone()),
            last_used_at: Set(key.last_used_at),
            created_at: Set(key.created_at),
            updated_at: Set(now),
            ..Default::default()
        };
        // Unique `credential_id`: insert when absent, otherwise update in place.
        if insert_if_absent(&self.db, row, keys::Column::CredentialId).await? {
            return Ok(());
        }
        keys::Entity::update_many()
            .col_expr(keys::Column::UserId, key.user_id.into())
            .col_expr(keys::Column::BoundApiKey, key.bound_api_key.clone().into())
            .col_expr(keys::Column::AppKey, key.app_key.clone().into())
            .col_expr(keys::Column::IsActive, key.is_active.into())
            .col_expr(keys::Column::IpAllowlist, key.ip_allowlist.clone().into())
            .col_expr(keys::Column::LastUsedAt, key.last_used_at.into())
            .col_expr(keys::Column::UpdatedAt, now.into())
            .filter(keys::Column::CredentialId.eq(key.credential_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn touch(&self, credential_id: Id, at: DateTime<Utc>) -> Result<()> {
        keys::Entity::update_many()
            .col_expr(keys::Column::LastUsedAt, Some(at).into())
            .filter(keys::Column::CredentialId.eq(credential_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn order_id_by_no(&self, order_no: &str) -> Result<Option<Id>> {
        if order_no.is_empty() {
            return Ok(None);
        }
        Ok(orders::Entity::find()
            .filter(orders::Column::OrderNo.eq(order_no))
            .filter(orders::Column::ParentId.is_null())
            .filter(orders::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(|o| o.id))
    }
}

#[cfg(test)]
mod tests {
    use sea_orm::Database;

    use super::*;

    // Upsert keeps one row per credential (insert, then update in place).
    #[tokio::test]
    async fn put_upserts_per_credential() {
        let db = Database::connect("sqlite::memory:").await.unwrap();
        db.get_schema_registry("zs_infra::db::entity::*")
            .sync(&db)
            .await
            .unwrap();
        let repo = SeaCompatKeyRepo::new(db.clone());
        let now = Utc::now();
        let mut key = CompatKey {
            credential_id: 5,
            user_id: 9,
            bound_api_key: "k".into(),
            app_key: "enc".into(),
            is_active: true,
            ip_allowlist: String::new(),
            last_used_at: None,
            created_at: now,
            updated_at: now,
        };
        repo.put(&key, now).await.unwrap();
        key.app_key = "enc2".into();
        key.is_active = false;
        repo.put(&key, now).await.unwrap();
        repo.touch(5, now).await.unwrap();
        let got = repo.get(5).await.unwrap().unwrap();
        assert_eq!(got.app_key, "enc2");
        assert!(!got.is_active);
        assert!(got.last_used_at.is_some());
        assert_eq!(keys::Entity::find().all(&db).await.unwrap().len(), 1);
    }
}
