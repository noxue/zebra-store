//! Outbox of signed zebra-store events (delivery status and retries).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "zs_webhook_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))", unique)]
    pub event_id: String,
    #[sea_orm(indexed)]
    pub credential_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub kind: String,
    /// Event JSON exactly as sent.
    #[sea_orm(column_type = "Text")]
    pub body: String,
    /// `pending` / `sent` / `failed`.
    #[sea_orm(column_type = "String(StringLen::N(16))", indexed)]
    pub status: String,
    pub attempts: i32,
    #[sea_orm(nullable)]
    pub next_attempt_at: Option<DateTimeUtc>,
    #[sea_orm(column_type = "Text")]
    pub last_error: String,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
