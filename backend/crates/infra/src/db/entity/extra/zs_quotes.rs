//! Price-locking quotes of zebra-store buyers.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "zs_quotes")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(column_type = "String(StringLen::N(64))", unique)]
    pub quote_id: String,
    #[sea_orm(indexed)]
    pub credential_id: i64,
    /// `[{sku_id, product_id, quantity, unit_price}]`.
    #[sea_orm(column_type = "Json", nullable)]
    pub lines: Option<Json>,
    #[sea_orm(column_type = "Decimal(Some((20, 2)))")]
    pub total: Decimal,
    #[sea_orm(column_type = "String(StringLen::N(16))")]
    pub currency: String,
    #[sea_orm(indexed)]
    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
