//! Reseller persistence (port of `reseller/infrastructure/gormstore`).
//!
//! One store implements every reseller port. Amount sums are computed in Rust from
//! the selected rows (SQLite has no exact decimal `SUM`). Functions ending in `_in`
//! take any connection so other groups (order) can run them inside their own
//! transaction (DB-01).

pub mod ledger;
pub mod operations;
pub mod orders;
pub mod profiles;
pub mod settings;
pub mod sites;

use std::collections::{HashMap, HashSet};

use sea_orm::sea_query::Query;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, DbErr, EntityTrait, QueryFilter,
    SqlErr,
};
use serde_json::{Value, json};
use zs_domain::reseller::{
    BalanceAccount, BalanceStatus, DomainStatus, DomainType, LedgerEntry, LedgerStatus, LedgerType,
    OrderSnapshot, PricingMode, ProductSetting, Profile, ProfileStatus, ResellerDomain,
    SettlementStatus, SiteConfig, UserRef, VerificationStatus, WithdrawRequest, WithdrawStatus,
    keys,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{
    reseller_balance_accounts as balances, reseller_domains as domains,
    reseller_ledger_entries as ledger_t, reseller_order_snapshots as snapshots,
    reseller_product_settings as psettings, reseller_profiles as profiles_t,
    reseller_site_configs as sites_t, reseller_withdraw_requests as withdraws, users,
};
use crate::db::repo::catalog::sql::ilike_sql;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of every reseller port.
#[derive(Debug, Clone)]
pub struct SeaResellerStore {
    db: DatabaseConnection,
}

impl SeaResellerStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

/// `true` when the error is a unique-constraint violation.
pub(crate) fn is_unique_violation(e: &DbErr) -> bool {
    matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_)))
}

/// Maps a unique violation on a domain name to `error.reseller_domain_conflict`.
pub(crate) fn domain_conflict(e: DbErr) -> Error {
    if is_unique_violation(&e) {
        Error::bad_request(keys::DOMAIN_CONFLICT)
    } else {
        Error::internal(e)
    }
}

pub(crate) fn json_or(v: Option<Value>, default: Value) -> Value {
    v.filter(|v| !v.is_null()).unwrap_or(default)
}

pub(crate) fn normalize_domain(raw: &str) -> String {
    let d = raw.trim().to_lowercase();
    d.strip_suffix('.').map(str::to_owned).unwrap_or(d)
}

pub(crate) fn profile_model(m: profiles_t::Model, user: Option<UserRef>) -> Profile {
    Profile {
        id: m.id,
        user_id: m.user_id,
        status: ProfileStatus::from_db(&m.status),
        apply_reason: m.apply_reason,
        reject_reason: m.reject_reason,
        default_markup_percent: Amount::new(m.default_markup_percent),
        max_markup_percent: Amount::new(m.max_markup_percent),
        settlement_status: SettlementStatus::from_db(&m.settlement_status),
        reviewed_by: m.reviewed_by,
        reviewed_at: m.reviewed_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        user,
    }
}

pub(crate) fn domain_model(m: domains::Model) -> ResellerDomain {
    ResellerDomain {
        id: m.id,
        reseller_id: m.reseller_id,
        domain: m.domain,
        kind: DomainType::from_db(&m.type_),
        verification_token: m.verification_token,
        verification_status: VerificationStatus::from_db(&m.verification_status),
        status: DomainStatus::from_db(&m.status),
        is_primary: m.is_primary,
        verified_at: m.verified_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        profile: None,
    }
}

pub(crate) fn site_model(m: sites_t::Model, profile: Option<Profile>) -> SiteConfig {
    SiteConfig {
        id: m.id,
        reseller_id: m.reseller_id,
        site_name: m.site_name,
        logo: m.logo,
        favicon: m.favicon,
        announcement: json_or(m.announcement_json, json!({})),
        support: json_or(m.support_json, json!({})),
        seo: json_or(m.seo_json, json!({})),
        footer_links: json_or(m.footer_links_json, json!({})),
        nav_config: json_or(m.nav_config_json, json!({})),
        theme: json_or(m.theme_json, json!({})),
        created_at: m.created_at,
        updated_at: m.updated_at,
        profile,
    }
}

pub(crate) fn setting_model(m: psettings::Model) -> ProductSetting {
    ProductSetting {
        id: m.id,
        reseller_id: m.reseller_id,
        product_id: m.product_id,
        sku_id: m.sku_id,
        is_listed: m.is_listed,
        pricing_mode: PricingMode::from_db(&m.pricing_mode),
        markup_percent: Amount::new(m.markup_percent),
        fixed_markup_amount: Amount::new(m.fixed_markup_amount),
        fixed_price_amount: Amount::new(m.fixed_price_amount),
        sort_order: m.sort_order,
        created_at: m.created_at,
        updated_at: m.updated_at,
        profile: None,
        product: None,
    }
}

pub(crate) fn snapshot_model(m: snapshots::Model) -> OrderSnapshot {
    OrderSnapshot {
        id: m.id,
        order_id: m.order_id,
        reseller_id: m.reseller_id,
        domain: m.domain,
        currency: m.currency,
        reseller_user_id: m.reseller_user_id,
        buyer_user_id: m.buyer_user_id,
        base_amount: Amount::new(m.base_amount),
        reseller_amount: Amount::new(m.reseller_amount),
        profit_amount: Amount::new(m.profit_amount),
        profit_eligible: m.profit_eligible,
        profit_block_reason: m.profit_block_reason,
        pricing_snapshot_json: json_or(m.pricing_snapshot_json, json!({})),
        risk_snapshot_json: json_or(m.risk_snapshot_json, json!({})),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub(crate) fn ledger_model(m: ledger_t::Model) -> LedgerEntry {
    LedgerEntry {
        id: m.id,
        reseller_id: m.reseller_id,
        order_id: m.order_id,
        kind: LedgerType::from_db(&m.type_),
        amount: Amount::new(m.amount),
        currency: m.currency,
        idempotency_key: m.idempotency_key,
        metadata_json: m.metadata_json.unwrap_or(Value::Null),
        status: LedgerStatus::from_db(&m.status),
        available_at: m.available_at,
        withdraw_request_id: m.withdraw_request_id,
        remark: m.remark,
        created_at: m.created_at,
        updated_at: m.updated_at,
        profile: None,
        order: None,
    }
}

pub(crate) fn withdraw_model(m: withdraws::Model) -> WithdrawRequest {
    WithdrawRequest {
        id: m.id,
        reseller_id: m.reseller_id,
        amount: Amount::new(m.amount),
        currency: m.currency,
        channel: m.channel,
        account: m.account,
        status: WithdrawStatus::from_db(&m.status),
        reject_reason: m.reject_reason,
        processed_by: m.processed_by,
        processed_at: m.processed_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        profile: None,
        processor: None,
    }
}

pub(crate) fn balance_model(m: balances::Model) -> BalanceAccount {
    BalanceAccount {
        id: m.id,
        reseller_id: m.reseller_id,
        currency: m.currency,
        status: BalanceStatus::from_db(&m.status),
        available_amount_cache: Amount::new(m.available_amount_cache),
        locked_amount_cache: Amount::new(m.locked_amount_cache),
        negative_amount_cache: Amount::new(m.negative_amount_cache),
        last_ledger_entry_id: m.last_ledger_entry_id,
        risk_note: m.risk_note,
        created_at: m.created_at,
        updated_at: m.updated_at,
        profile: None,
    }
}

/// Live users by id.
pub(crate) async fn load_users<C: ConnectionTrait>(
    conn: &C,
    ids: &[Id],
) -> Result<HashMap<Id, UserRef>> {
    let ids: Vec<Id> = ids
        .iter()
        .copied()
        .filter(|i| *i > 0)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = users::Entity::find()
        .filter(users::Column::Id.is_in(ids))
        .filter(users::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?;
    Ok(rows
        .into_iter()
        .map(|u| {
            (
                u.id,
                UserRef {
                    id: u.id,
                    email: u.email,
                    display_name: u.display_name,
                },
            )
        })
        .collect())
}

/// Attaches users to profile rows.
pub(crate) async fn with_users<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<profiles_t::Model>,
) -> Result<Vec<Profile>> {
    let ids: Vec<Id> = rows.iter().map(|p| p.user_id).collect();
    let users = load_users(conn, &ids).await?;
    Ok(rows
        .into_iter()
        .map(|m| {
            let user = users.get(&m.user_id).cloned();
            profile_model(m, user)
        })
        .collect())
}

/// Live profiles (with users) by id.
pub(crate) async fn load_profiles<C: ConnectionTrait>(
    conn: &C,
    ids: &[Id],
) -> Result<HashMap<Id, Profile>> {
    let ids: Vec<Id> = ids
        .iter()
        .copied()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = profiles_t::Entity::find()
        .filter(profiles_t::Column::Id.is_in(ids))
        .filter(profiles_t::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?;
    Ok(with_users(conn, rows)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect())
}

/// Profiles matching a keyword on the owner's e-mail / display name (case-insensitive)
/// or a numeric profile id — the admin `keyword` filter.
pub(crate) async fn keyword_profile_ids<C: ConnectionTrait>(
    conn: &C,
    keyword: &str,
) -> Result<Vec<Id>> {
    let user_ids = Query::select()
        .column(users::Column::Id)
        .from(users::Entity)
        .cond_where(
            Condition::any()
                .add(ilike_sql("email", keyword))
                .add(ilike_sql("display_name", keyword)),
        )
        .to_owned();
    let mut cond = Condition::any().add(profiles_t::Column::UserId.in_subquery(user_ids));
    if let Ok(id) = keyword.trim().parse::<Id>() {
        cond = cond.add(profiles_t::Column::Id.eq(id));
    }
    let rows = profiles_t::Entity::find()
        .filter(profiles_t::Column::DeletedAt.is_null())
        .filter(cond)
        .all(conn)
        .await
        .dom()?;
    Ok(rows.into_iter().map(|p| p.id).collect())
}

/// Profile ids owned by a user.
pub(crate) async fn user_profile_ids<C: ConnectionTrait>(conn: &C, user_id: Id) -> Result<Vec<Id>> {
    let rows = profiles_t::Entity::find()
        .filter(profiles_t::Column::UserId.eq(user_id))
        .filter(profiles_t::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?;
    Ok(rows.into_iter().map(|p| p.id).collect())
}
