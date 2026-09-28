//! Deliveries a supplier handed over synchronously with the order call (acg-faka
//! `trade.secret`, mcy-shop `trade.contents`), stored in the same transaction that
//! records the supplier order so a restart before the local delivery never loses them.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "procurement_deliveries")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub procurement_order_id: i64,
    /// `RemoteFulfillment.kind` (`auto` / `manual` …).
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub kind: String,
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub status: String,
    #[sea_orm(column_type = "Text")]
    pub payload: String,
    #[sea_orm(column_type = "Json", nullable)]
    pub delivery_data: Option<Json>,
    #[sea_orm(nullable)]
    pub delivered_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
