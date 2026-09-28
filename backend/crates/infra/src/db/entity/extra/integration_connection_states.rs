//! Adapter state negotiated per site connection (handshake features, capabilities,
//! change cursor, push-event registration, adapter-specific JSON configuration).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "integration_connection_states")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub connection_id: i64,
    #[sea_orm(column_type = "Json", nullable)]
    pub features: Option<Json>,
    #[sea_orm(column_type = "Json", nullable)]
    pub capabilities: Option<Json>,
    pub negotiated: bool,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub supplier_currency: String,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub sync_mode: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub change_cursor: String,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub webhook_status: String,
    #[sea_orm(column_type = "Json", nullable)]
    pub extra: Option<Json>,
    #[sea_orm(nullable)]
    pub last_handshake_at: Option<DateTimeUtc>,
    #[sea_orm(column_type = "Text")]
    pub handshake_error: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
