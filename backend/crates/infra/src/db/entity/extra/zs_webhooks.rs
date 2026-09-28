//! Push endpoints registered by zebra-store buyers (one per credential).

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "zs_webhooks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub credential_id: i64,
    #[sea_orm(column_type = "String(StringLen::N(500))")]
    pub url: String,
    #[sea_orm(column_type = "Json", nullable)]
    pub events: Option<Json>,
    #[sea_orm(column_type = "Decimal(Some((20, 2)))", nullable)]
    pub balance_low_threshold: Option<Decimal>,
    #[sea_orm(nullable)]
    pub last_balance_low_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
