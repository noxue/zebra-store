//! Third-party card converter configurations.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "card_converters")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(100))")]
    pub name: String,
    #[sea_orm(column_type = "String(StringLen::N(2048))")]
    pub base_url: String,
    #[sea_orm(column_type = "Text")]
    pub token_enc: String,
    #[sea_orm(default_value = false, indexed)]
    pub enabled: bool,
    #[sea_orm(column_type = "Json", nullable)]
    pub types_json: Option<Json>,
    #[sea_orm(
        column_type = "String(StringLen::N(16))",
        default_value = "unknown",
        indexed
    )]
    pub health: String,
    #[sea_orm(default_value = 0)]
    pub consecutive_failures: i32,
    pub last_checked_at: Option<DateTimeUtc>,
    #[sea_orm(column_type = "String(StringLen::N(191))", default_value = "")]
    pub last_error: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
