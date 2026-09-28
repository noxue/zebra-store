//! zebra-store change feed (`/catalog/changes`): `id` is the sequence number.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "zs_change_log")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    /// `product.upserted` / `product.deleted` / `sku.stock` / `sku.price`.
    #[sea_orm(column_type = "String(StringLen::N(32))")]
    pub kind: String,
    #[sea_orm(indexed)]
    pub product_id: i64,
    #[sea_orm(nullable)]
    pub sku_id: Option<i64>,
    #[sea_orm(column_type = "Json", nullable)]
    pub data: Option<Json>,
    #[sea_orm(indexed)]
    pub created_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
