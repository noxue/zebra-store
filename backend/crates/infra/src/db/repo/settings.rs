//! [`SettingsStore`] backed by the `settings` table.

use async_trait::async_trait;
use sea_orm::sea_query::OnConflict;
use sea_orm::{DatabaseConnection, EntityTrait, QuerySelect, Set};
use zs_domain::Result;
use zs_domain::settings::SettingsStore;

use crate::db::entity::settings;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`SettingsStore`].
#[derive(Debug, Clone)]
pub struct SeaSettingsStore {
    db: DatabaseConnection,
}

impl SeaSettingsStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    /// Every stored key (for the operator `admin prune-settings` command).
    pub async fn keys(&self) -> Result<Vec<String>> {
        settings::Entity::find()
            .select_only()
            .column(settings::Column::Key)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()
    }

    /// Deletes one key; returns whether a row was removed.
    pub async fn delete(&self, key: &str) -> Result<bool> {
        let res = settings::Entity::delete_by_id(key.to_owned())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }
}

#[async_trait]
impl SettingsStore for SeaSettingsStore {
    async fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
        let row = settings::Entity::find_by_id(key.to_owned())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.and_then(|r| r.value_json))
    }

    async fn set(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let model = settings::ActiveModel {
            key: Set(key.to_owned()),
            value_json: Set(Some(value.clone())),
        };
        settings::Entity::insert(model)
            .on_conflict(
                OnConflict::column(settings::Column::Key)
                    .update_column(settings::Column::ValueJson)
                    .to_owned(),
            )
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
