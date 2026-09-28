//! [`DefaultMemberLevel`] over the `member_levels` table (consumer-side port).

use async_trait::async_trait;
use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder};
use zs_domain::identity::user::DefaultMemberLevel;
use zs_domain::{Id, Result};

use crate::db::entity::member_levels as levels;
use crate::db::repo::support::DbResultExt;

/// Reads the active default member level.
#[derive(Debug, Clone)]
pub struct SeaDefaultMemberLevel {
    db: DatabaseConnection,
}

impl SeaDefaultMemberLevel {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl DefaultMemberLevel for SeaDefaultMemberLevel {
    async fn default_level_id(&self) -> Result<Option<Id>> {
        Ok(levels::Entity::find()
            .filter(levels::Column::IsDefault.eq(true))
            .filter(levels::Column::IsActive.eq(true))
            .filter(levels::Column::DeletedAt.is_null())
            .order_by_asc(levels::Column::SortOrder)
            .order_by_asc(levels::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(|l| l.id))
    }
}
