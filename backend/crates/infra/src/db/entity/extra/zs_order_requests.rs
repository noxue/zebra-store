//! Idempotency records of zebra-store `POST /orders` (`Idempotency-Key`).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "zs_order_requests")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// `"{credential_id}:{idempotency_key}"`.
    #[sea_orm(column_type = "String(StringLen::N(128))", unique)]
    pub request_key: String,
    #[sea_orm(indexed)]
    pub credential_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub idempotency_key: String,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub request_hash: String,
    /// `0` while the first request is running.
    #[sea_orm(indexed)]
    pub order_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub order_no: String,
    pub callback: bool,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
