//! Compat keys of API credentials: the `app_key` the acg-faka / mcy OpenApi provider
//! facades verify md5 signatures with (one per credential, separate from the HMAC
//! secret), the owner's on/off switch and optional IP allowlist.

use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "api_compat_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub credential_id: i64,
    #[sea_orm(indexed)]
    pub user_id: i64,
    /// The credential's `api_key` at issue time (a re-approval invalidates the key).
    #[sea_orm(column_type = "String(StringLen::N(64))")]
    pub bound_api_key: String,
    /// AES-GCM encrypted app key.
    #[sea_orm(column_type = "String(StringLen::N(256))")]
    pub app_key: String,
    pub is_active: bool,
    /// Comma separated IPs / CIDRs (empty = any).
    #[sea_orm(column_type = "String(StringLen::N(1000))")]
    pub ip_allowlist: String,
    #[sea_orm(nullable)]
    pub last_used_at: Option<DateTimeUtc>,
    pub created_at: DateTimeUtc,
    pub updated_at: DateTimeUtc,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
