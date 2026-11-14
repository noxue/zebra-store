//! Versioned optional browse attributes; independent of purchase and fulfillment data.
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "product_catalog_documents")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub product_id: i64,
    #[sea_orm(default_value = 1)]
    pub schema_version: i32,
    pub revision: i64,
    #[sea_orm(column_type = "Json")]
    pub facets_json: Json,
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
