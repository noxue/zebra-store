//! Pending API secret rotations (zebra-store spec §2): `secret_next` verifies next to
//! the current secret until first use or expiry, then replaces it.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_credential_rotations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub credential_id: i64,
    /// AES-GCM encrypted new secret.
    #[sea_orm(column_type = "String(StringLen::N(512))")]
    pub secret_next: String,
    pub expires_at: DateTimeUtc,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
