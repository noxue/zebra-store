//! One hashed automation credential per account and authentication realm.
use sea_orm::entity::prelude::*;
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "personal_api_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(
        column_type = "String(StringLen::N(16))",
        unique_key = "idx_personal_api_key_owner"
    )]
    pub realm: String,
    #[sea_orm(unique_key = "idx_personal_api_key_owner")]
    pub owner_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))", unique)]
    pub key_hash: String,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub prefix: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}
#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
