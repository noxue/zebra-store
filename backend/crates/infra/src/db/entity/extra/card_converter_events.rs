//! Redacted converter health and order-exchange audit events.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "card_converter_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub converter_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(32))", indexed)]
    pub event_type: String,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub status: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub error_code: String,
    #[sea_orm(default_value = 0)]
    pub duration_ms: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))", default_value = "")]
    pub order_no: String,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
