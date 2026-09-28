//! Inbound supplier event ids already applied (per connection deduplication).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "integration_processed_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// `"{connection_id}:{event_id}"`.
    #[sea_orm(column_type = "String(StringLen::N(191))", unique)]
    pub event_key: String,
    #[sea_orm(indexed)]
    pub connection_id: i64,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
