//! Optional product/SKU to converter mapping.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "card_converter_bindings")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique_key = "idx_converter_binding_product_sku", indexed)]
    pub product_id: i64,
    #[sea_orm(unique_key = "idx_converter_binding_product_sku", indexed)]
    pub sku_id: i64,
    #[sea_orm(indexed)]
    pub converter_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(100))")]
    pub type_id: String,
    #[sea_orm(column_type = "Json", nullable)]
    pub fields_json: Option<Json>,
    #[sea_orm(column_type = "Json", nullable)]
    pub extra_template_json: Option<Json>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
impl ActiveModelBehavior for ActiveModel {}
