//! Immutable deduplication/audit links for late upstream cards recovered to stock.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "procurement_stock_recoveries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub procurement_order_id: i64,
    #[sea_orm(indexed)]
    pub order_id: i64,
    #[sea_orm(indexed)]
    pub batch_id: i64,
    #[sea_orm(unique)]
    pub card_secret_id: i64,
    /// Global recovery deduplication, including simultaneous different-SKU results.
    #[sea_orm(column_type = "String(StringLen::N(64))", unique)]
    pub credential_hash: String,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
