//! Last seen state of every product for the change-feed snapshot diff.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "zs_catalog_snapshots")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub product_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub version: String,
    /// `[[sku_id, "price", stock], …]`.
    #[sea_orm(column_type = "Json", nullable)]
    pub skus: Option<Json>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
