//! Database-backed job queue (replaces the original asynq/Redis queue).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "jobs")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// Task type, e.g. `order:timeout_cancel`.
    #[sea_orm(column_type = "String(StringLen::N(64))", indexed)]
    pub kind: String,
    #[sea_orm(column_type = "Text")]
    pub payload: String,
    /// `pending`, `running`, `done` or `failed`.
    #[sea_orm(column_type = "String(StringLen::N(16))", indexed)]
    pub status: String,
    #[sea_orm(indexed)]
    pub run_at: DateTimeUtc,
    pub attempts: i32,
    pub max_attempts: i32,
    #[sea_orm(nullable)]
    pub locked_until: Option<DateTimeUtc>,
    #[sea_orm(column_type = "Text")]
    pub last_error: String,
    /// Optional de-duplication key; pending jobs with the same key are not enqueued twice.
    #[sea_orm(column_type = "String(StringLen::N(191))", indexed)]
    pub unique_key: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
