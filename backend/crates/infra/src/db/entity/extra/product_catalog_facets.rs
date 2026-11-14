//! Indexed product-level browse values. No order or resource references these row IDs.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "product_catalog_facets")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique_key = "idx_product_catalog_dimension", indexed)]
    pub product_id: i64,
    #[sea_orm(
        column_type = "String(StringLen::N(20))",
        unique_key = "idx_product_catalog_dimension",
        indexed
    )]
    pub dimension: String,
    #[sea_orm(column_type = "String(StringLen::N(128))", indexed)]
    pub value_id: String,
    #[sea_orm(column_type = "Json")]
    pub label_json: Json,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::super::products::Entity",
        from = "Column::ProductId",
        to = "super::super::products::Column::Id",
        on_update = "Cascade",
        on_delete = "Cascade"
    )]
    Product,
}
impl ActiveModelBehavior for ActiveModel {}
