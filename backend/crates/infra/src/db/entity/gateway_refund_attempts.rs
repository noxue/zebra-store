//! Durable gateway original-route refund attempts.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "gateway_refund_attempts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub order_id: i64,
    #[sea_orm(indexed)]
    pub payment_id: i64,
    #[sea_orm(indexed)]
    pub channel_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub request_date: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", unique)]
    pub refund_no: String,
    #[sea_orm(column_type = "String(StringLen::N(255))", indexed)]
    pub provider_ref: String,
    #[sea_orm(column_type = "Decimal(Some((20, 2)))")]
    pub amount: Decimal,
    #[sea_orm(column_type = "String(StringLen::N(16))", indexed)]
    pub status: String,
    #[sea_orm(column_type = "Text")]
    pub remark: String,
    #[sea_orm(default_value = false)]
    pub payment_fee_refunded: bool,
    #[sea_orm(column_type = "Json", nullable)]
    pub payload: Option<Json>,
    #[sea_orm(column_type = "Text")]
    pub error: String,
    #[sea_orm(nullable)]
    pub refund_record_id: Option<i64>,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
    #[sea_orm(indexed)]
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
