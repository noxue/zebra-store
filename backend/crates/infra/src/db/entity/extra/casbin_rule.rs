//! Casbin policy table (same layout as the gorm-adapter used by the original).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "casbin_rule")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub ptype: String,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub v0: String,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub v1: String,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub v2: String,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub v3: String,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub v4: String,
    #[sea_orm(
        column_type = "String(StringLen::N(100))",
        unique_key = "idx_casbin_rule"
    )]
    pub v5: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
