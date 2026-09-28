//! Request nonces of signed API calls (replay protection shared by every instance).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_request_nonces")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// `"{credential_id}:{nonce}"`.
    #[sea_orm(column_type = "String(StringLen::N(128))", unique)]
    pub nonce_key: String,
    #[sea_orm(indexed)]
    pub credential_id: i64,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
