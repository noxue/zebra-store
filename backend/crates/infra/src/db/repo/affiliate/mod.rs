//! Affiliate persistence ([`AffiliateRepo`]) and the refund clawback used inside the
//! order group's refund transaction.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::affiliate::rules::{clawback, plan_withdraw, refund_window, split_type};
use zs_domain::affiliate::{
    AffiliateRepo, Commission, CommissionFilter, CommissionItem, CommissionOrder, NewClick,
    NewCommission, OrderRef, Processor, Profile, ProfileFilter, ProfileStats, WithdrawFilter,
    WithdrawRequest, status,
};
use zs_domain::identity::user::User;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::{
    admins, affiliate_clicks as clicks, affiliate_commissions as commissions,
    affiliate_profiles as profiles, affiliate_withdraw_requests as withdraws, order_items, orders,
    products, users,
};
use crate::db::repo::support::DbResultExt;
use crate::db::repo::wallet::repo::{contains, ilike};

/// Rows per `IN (…)` chunk.
const CHUNK: usize = 500;

/// SeaORM implementation of [`AffiliateRepo`].
#[derive(Debug, Clone)]
pub struct SeaAffiliateRepo {
    db: DatabaseConnection,
}

impl SeaAffiliateRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn profile_to_domain(m: profiles::Model, user: Option<User>) -> Profile {
    Profile {
        id: m.id,
        user_id: m.user_id,
        affiliate_code: m.affiliate_code,
        status: m.status,
        created_at: m.created_at,
        updated_at: m.updated_at,
        user,
    }
}

fn commission_to_domain(m: commissions::Model) -> Commission {
    Commission {
        id: m.id,
        affiliate_profile_id: m.affiliate_profile_id,
        order_id: m.order_id,
        order_item_id: m.order_item_id,
        commission_type: m.commission_type,
        base_amount: Amount::new(m.base_amount),
        rate_percent: Amount::new(m.rate_percent),
        commission_amount: Amount::new(m.commission_amount),
        status: m.status,
        confirm_at: m.confirm_at,
        available_at: m.available_at,
        withdraw_request_id: m.withdraw_request_id,
        invalid_reason: m.invalid_reason,
        created_at: m.created_at,
        updated_at: m.updated_at,
        affiliate_profile: None,
        order: None,
    }
}

fn withdraw_to_domain(m: withdraws::Model) -> WithdrawRequest {
    WithdrawRequest {
        id: m.id,
        affiliate_profile_id: m.affiliate_profile_id,
        amount: Amount::new(m.amount),
        channel: m.channel,
        account: m.account,
        status: m.status,
        reject_reason: m.reject_reason,
        processed_by: m.processed_by,
        processed_at: m.processed_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        affiliate_profile: None,
        processor: None,
    }
}

fn unique(mut ids: Vec<Id>) -> Vec<Id> {
    ids.retain(|id| *id > 0);
    ids.sort_unstable();
    ids.dedup();
    ids
}

/// Live users by id (soft-deleted users are not preloaded, DB-03).
async fn users_by_ids<C: ConnectionTrait>(conn: &C, ids: &[Id]) -> Result<HashMap<Id, User>> {
    let mut out = HashMap::new();
    for chunk in unique(ids.to_vec()).chunks(CHUNK) {
        let rows = users::Entity::find()
            .filter(users::Column::Id.is_in(chunk.to_vec()))
            .filter(users::Column::DeletedAt.is_null())
            .all(conn)
            .await
            .dom()?;
        out.extend(
            rows.into_iter()
                .map(|u| (u.id, crate::db::repo::identity::user::to_domain(u))),
        );
    }
    Ok(out)
}

/// Profiles with their users.
async fn attach_users<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<profiles::Model>,
) -> Result<Vec<Profile>> {
    let users = users_by_ids(conn, &rows.iter().map(|p| p.user_id).collect::<Vec<_>>()).await?;
    Ok(rows
        .into_iter()
        .map(|p| {
            let user = users.get(&p.user_id).cloned();
            profile_to_domain(p, user)
        })
        .collect())
}

/// Live profiles (with users) by id.
async fn profiles_by_ids<C: ConnectionTrait>(conn: &C, ids: &[Id]) -> Result<HashMap<Id, Profile>> {
    let mut rows = Vec::new();
    for chunk in unique(ids.to_vec()).chunks(CHUNK) {
        rows.extend(
            profiles::Entity::find()
                .filter(profiles::Column::Id.is_in(chunk.to_vec()))
                .filter(profiles::Column::DeletedAt.is_null())
                .all(conn)
                .await
                .dom()?,
        );
    }
    Ok(attach_users(conn, rows)
        .await?
        .into_iter()
        .map(|p| (p.id, p))
        .collect())
}

async fn find_profile<C: ConnectionTrait>(conn: &C, cond: Condition) -> Result<Option<Profile>> {
    let row = profiles::Entity::find()
        .filter(cond)
        .filter(profiles::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?;
    Ok(match row {
        Some(r) => attach_users(conn, vec![r]).await?.pop(),
        None => None,
    })
}

/// Profiles whose user or code matches `keyword` (subquery for list filters).
fn keyword_profiles(keyword: &str) -> sea_orm::sea_query::SelectStatement {
    let users = Query::select()
        .column(users::Column::Id)
        .from(users::Entity)
        .cond_where(
            Condition::any()
                .add(ilike((users::Entity, users::Column::Email), keyword))
                .add(ilike((users::Entity, users::Column::DisplayName), keyword)),
        )
        .to_owned();
    Query::select()
        .column(profiles::Column::Id)
        .from(profiles::Entity)
        .cond_where(
            Condition::any()
                .add(profiles::Column::UserId.in_subquery(users))
                .add(ilike(
                    (profiles::Entity, profiles::Column::AffiliateCode),
                    keyword,
                )),
        )
        .to_owned()
}

fn commission_condition(f: &CommissionFilter) -> Condition {
    let mut cond = Condition::all().add(commissions::Column::DeletedAt.is_null());
    if f.profile_id > 0 {
        cond = cond.add(commissions::Column::AffiliateProfileId.eq(f.profile_id));
    }
    if f.order_id > 0 {
        cond = cond.add(commissions::Column::OrderId.eq(f.order_id));
    }
    if !f.order_no.trim().is_empty() {
        let orders = Query::select()
            .column(orders::Column::Id)
            .from(orders::Entity)
            .and_where(Expr::col(orders::Column::DeletedAt).is_null())
            .and_where(Expr::col(orders::Column::OrderNo).like(contains(f.order_no.trim())))
            .to_owned();
        cond = cond.add(commissions::Column::OrderId.in_subquery(orders));
    }
    if !f.status.trim().is_empty() {
        cond = cond.add(commissions::Column::Status.eq(f.status.trim()));
    }
    if !f.keyword.trim().is_empty() {
        cond = cond.add(
            commissions::Column::AffiliateProfileId.in_subquery(keyword_profiles(f.keyword.trim())),
        );
    }
    cond
}

fn withdraw_condition(f: &WithdrawFilter) -> Condition {
    let mut cond = Condition::all().add(withdraws::Column::DeletedAt.is_null());
    if f.profile_id > 0 {
        cond = cond.add(withdraws::Column::AffiliateProfileId.eq(f.profile_id));
    }
    if !f.status.trim().is_empty() {
        cond = cond.add(withdraws::Column::Status.eq(f.status.trim()));
    }
    if !f.keyword.trim().is_empty() {
        let keyword = f.keyword.trim();
        cond = cond.add(
            Condition::any()
                .add(withdraws::Column::AffiliateProfileId.in_subquery(keyword_profiles(keyword)))
                .add(ilike(
                    (withdraws::Entity, withdraws::Column::Account),
                    keyword,
                )),
        );
    }
    cond
}

/// Commissions with their profile and (live) order.
async fn attach_commission_refs<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<commissions::Model>,
) -> Result<Vec<Commission>> {
    let profiles = profiles_by_ids(
        conn,
        &rows
            .iter()
            .map(|c| c.affiliate_profile_id)
            .collect::<Vec<_>>(),
    )
    .await?;
    let mut order_nos: HashMap<Id, String> = HashMap::new();
    for chunk in unique(rows.iter().map(|c| c.order_id).collect()).chunks(CHUNK) {
        let found: Vec<(Id, String)> = orders::Entity::find()
            .select_only()
            .columns([orders::Column::Id, orders::Column::OrderNo])
            .filter(orders::Column::Id.is_in(chunk.to_vec()))
            .filter(orders::Column::DeletedAt.is_null())
            .into_tuple()
            .all(conn)
            .await
            .dom()?;
        order_nos.extend(found);
    }
    Ok(rows
        .into_iter()
        .map(|m| {
            let profile = profiles.get(&m.affiliate_profile_id).cloned();
            let order = order_nos.get(&m.order_id).map(|no| OrderRef {
                id: m.order_id,
                order_no: no.clone(),
            });
            Commission {
                affiliate_profile: profile,
                order,
                ..commission_to_domain(m)
            }
        })
        .collect())
}

/// Withdrawals with their profile and processor.
async fn attach_withdraw_refs<C: ConnectionTrait>(
    conn: &C,
    rows: Vec<withdraws::Model>,
) -> Result<Vec<WithdrawRequest>> {
    let profiles = profiles_by_ids(
        conn,
        &rows
            .iter()
            .map(|w| w.affiliate_profile_id)
            .collect::<Vec<_>>(),
    )
    .await?;
    let admin_ids = unique(rows.iter().filter_map(|w| w.processed_by).collect());
    let mut admins: HashMap<Id, String> = HashMap::new();
    for chunk in admin_ids.chunks(CHUNK) {
        let found: Vec<(Id, String)> = admins::Entity::find()
            .select_only()
            .columns([admins::Column::Id, admins::Column::Username])
            .filter(admins::Column::Id.is_in(chunk.to_vec()))
            .filter(admins::Column::DeletedAt.is_null())
            .into_tuple()
            .all(conn)
            .await
            .dom()?;
        admins.extend(found);
    }
    Ok(rows
        .into_iter()
        .map(|m| {
            let profile = profiles.get(&m.affiliate_profile_id).cloned();
            let processor = m.processed_by.and_then(|id| {
                admins.get(&id).map(|u| Processor {
                    id,
                    username: u.clone(),
                })
            });
            WithdrawRequest {
                affiliate_profile: profile,
                processor,
                ..withdraw_to_domain(m)
            }
        })
        .collect())
}

async fn withdraw_in<C: ConnectionTrait>(conn: &C, id: Id) -> Result<Option<WithdrawRequest>> {
    let row = withdraws::Entity::find_by_id(id)
        .filter(withdraws::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?;
    Ok(match row {
        Some(r) => attach_withdraw_refs(conn, vec![r]).await?.pop(),
        None => None,
    })
}

fn open_statuses() -> [&'static str; 2] {
    [
        status::COMMISSION_PENDING_CONFIRM,
        status::COMMISSION_AVAILABLE,
    ]
}

/// Claws back the order's unbound commissions after a refund (`HandleOrderRefunded`),
/// inside the caller's refund transaction: each commission (and its base) shrinks by
/// `refund_delta / remaining`; commissions reaching zero are rejected. Commissions already
/// bound to a withdrawal are left untouched.
pub async fn clawback_on_refund<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    order_total: Amount,
    refund_delta: Amount,
    refunded_before: Amount,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<u64> {
    let Some((delta, remaining)) = refund_window(order_total, refunded_before, refund_delta) else {
        return Ok(0);
    };
    let reason = match reason.trim() {
        "" => "order_refunded",
        r => r,
    };
    let rows = commissions::Entity::find()
        .filter(commissions::Column::OrderId.eq(order_id))
        .filter(commissions::Column::Status.is_in(open_statuses()))
        .filter(commissions::Column::DeletedAt.is_null())
        .order_by_asc(commissions::Column::Id)
        .lock_exclusive()
        .all(conn)
        .await
        .dom()?;
    let mut changed = 0;
    for row in rows {
        if row.withdraw_request_id.is_some() {
            continue;
        }
        let (next, next_base, rejected) = clawback(
            Amount::new(row.commission_amount),
            Amount::new(row.base_amount),
            delta,
            remaining,
        );
        let mut q = commissions::Entity::update_many()
            .col_expr(
                commissions::Column::CommissionAmount,
                Expr::value(next.decimal()),
            )
            .col_expr(
                commissions::Column::BaseAmount,
                Expr::value(next_base.decimal()),
            )
            .col_expr(commissions::Column::UpdatedAt, Expr::value(now));
        if rejected {
            q = q
                .col_expr(
                    commissions::Column::Status,
                    Expr::value(status::COMMISSION_REJECTED),
                )
                .col_expr(commissions::Column::InvalidReason, Expr::value(reason))
                .col_expr(
                    commissions::Column::ConfirmAt,
                    Expr::value(Option::<DateTime<Utc>>::None),
                )
                .col_expr(
                    commissions::Column::AvailableAt,
                    Expr::value(Option::<DateTime<Utc>>::None),
                );
        }
        changed += q
            .filter(commissions::Column::Id.eq(row.id))
            .filter(commissions::Column::Status.eq(row.status.clone()))
            .filter(commissions::Column::WithdrawRequestId.is_null())
            .exec(conn)
            .await
            .dom()?
            .rows_affected;
    }
    Ok(changed)
}

#[async_trait]
impl AffiliateRepo for SeaAffiliateRepo {
    async fn user_status(&self, user_id: Id) -> Result<Option<String>> {
        if user_id <= 0 {
            return Ok(None);
        }
        users::Entity::find_by_id(user_id)
            .select_only()
            .column(users::Column::Status)
            .filter(users::Column::DeletedAt.is_null())
            .into_tuple()
            .one(&self.db)
            .await
            .dom()
    }

    async fn profile(&self, id: Id) -> Result<Option<Profile>> {
        if id <= 0 {
            return Ok(None);
        }
        find_profile(&self.db, Condition::all().add(profiles::Column::Id.eq(id))).await
    }

    async fn profile_by_user(&self, user_id: Id) -> Result<Option<Profile>> {
        if user_id <= 0 {
            return Ok(None);
        }
        find_profile(
            &self.db,
            Condition::all().add(profiles::Column::UserId.eq(user_id)),
        )
        .await
    }

    async fn profile_by_code(&self, code: &str) -> Result<Option<Profile>> {
        let code = code.trim().to_uppercase();
        if code.is_empty() {
            return Ok(None);
        }
        find_profile(
            &self.db,
            Condition::all().add(profiles::Column::AffiliateCode.eq(code)),
        )
        .await
    }

    async fn create_profile(
        &self,
        user_id: Id,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<Profile>> {
        let row = profiles::ActiveModel {
            user_id: Set(user_id),
            affiliate_code: Set(code.to_owned()),
            status: Set(status::PROFILE_ACTIVE.to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        };
        if !crate::db::repo::support::insert_if_absent(&self.db, row, profiles::Column::Id).await? {
            return Ok(None);
        }
        let created = self.profile_by_user(user_id).await?;
        Ok(created.filter(|p| p.affiliate_code == code))
    }

    async fn set_profile_status(&self, id: Id, state: &str, now: DateTime<Utc>) -> Result<()> {
        profiles::Entity::update_many()
            .col_expr(profiles::Column::Status, Expr::value(state))
            .col_expr(profiles::Column::UpdatedAt, Expr::value(now))
            .filter(profiles::Column::Id.eq(id))
            .filter(profiles::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn batch_profile_status(
        &self,
        ids: &[Id],
        state: &str,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        let mut total = 0;
        for chunk in ids.chunks(CHUNK) {
            total += profiles::Entity::update_many()
                .col_expr(profiles::Column::Status, Expr::value(state))
                .col_expr(profiles::Column::UpdatedAt, Expr::value(now))
                .filter(profiles::Column::Id.is_in(chunk.to_vec()))
                .filter(profiles::Column::DeletedAt.is_null())
                .exec(&self.db)
                .await
                .dom()?
                .rows_affected;
        }
        Ok(total)
    }

    async fn list_profiles(&self, f: &ProfileFilter, page: PageRequest) -> Result<Page<Profile>> {
        let mut cond = Condition::all().add(profiles::Column::DeletedAt.is_null());
        if f.user_id > 0 {
            cond = cond.add(profiles::Column::UserId.eq(f.user_id));
        }
        if !f.code.trim().is_empty() {
            cond = cond.add(profiles::Column::AffiliateCode.eq(f.code.trim().to_uppercase()));
        }
        if !f.status.trim().is_empty() {
            cond = cond.add(profiles::Column::Status.eq(f.status.trim()));
        }
        if !f.keyword.trim().is_empty() {
            cond = cond.add(profiles::Column::Id.in_subquery(keyword_profiles(f.keyword.trim())));
        }
        let q = profiles::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(profiles::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: attach_users(&self.db, rows).await?,
            total,
        })
    }

    async fn profile_stats(&self, ids: &[Id]) -> Result<HashMap<Id, ProfileStats>> {
        let ids = unique(ids.to_vec());
        let mut out: HashMap<Id, ProfileStats> = ids
            .iter()
            .map(|id| (*id, ProfileStats::default()))
            .collect();
        for chunk in ids.chunks(CHUNK) {
            let click_rows: Vec<(Id, i64)> = clicks::Entity::find()
                .select_only()
                .column(clicks::Column::AffiliateProfileId)
                .column_as(clicks::Column::Id.count(), "total")
                .filter(clicks::Column::AffiliateProfileId.is_in(chunk.to_vec()))
                .group_by(clicks::Column::AffiliateProfileId)
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            for (id, total) in click_rows {
                out.entry(id).or_default().click_count = total;
            }
            // Exact decimal sums are computed here rather than in SQL (SQLite REAL sums).
            let rows: Vec<(Id, Id, String, Decimal, Option<Id>)> = commissions::Entity::find()
                .select_only()
                .columns([
                    commissions::Column::AffiliateProfileId,
                    commissions::Column::OrderId,
                    commissions::Column::Status,
                    commissions::Column::CommissionAmount,
                    commissions::Column::WithdrawRequestId,
                ])
                .filter(commissions::Column::AffiliateProfileId.is_in(chunk.to_vec()))
                .filter(commissions::Column::DeletedAt.is_null())
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            let mut valid: HashMap<Id, HashSet<Id>> = HashMap::new();
            for (profile_id, order_id, state, amount, withdraw_id) in rows {
                let s = out.entry(profile_id).or_default();
                let amount = Amount::new(amount);
                match state.as_str() {
                    status::COMMISSION_PENDING_CONFIRM => s.pending += amount,
                    status::COMMISSION_AVAILABLE if withdraw_id.is_none() => s.available += amount,
                    status::COMMISSION_WITHDRAWN => s.withdrawn += amount,
                    _ => {}
                }
                if state != status::COMMISSION_REJECTED {
                    valid.entry(profile_id).or_default().insert(order_id);
                }
            }
            for (profile_id, orders) in valid {
                out.entry(profile_id).or_default().valid_order_count =
                    i64::try_from(orders.len()).unwrap_or(i64::MAX);
            }
        }
        Ok(out)
    }

    async fn has_recent_click(
        &self,
        profile_id: Id,
        visitor_key: &str,
        landing_path: &str,
        since: DateTime<Utc>,
    ) -> Result<bool> {
        let mut q = clicks::Entity::find()
            .filter(clicks::Column::AffiliateProfileId.eq(profile_id))
            .filter(clicks::Column::VisitorKey.eq(visitor_key.trim()))
            .filter(clicks::Column::CreatedAt.gte(since));
        if !landing_path.trim().is_empty() {
            q = q.filter(clicks::Column::LandingPath.eq(landing_path.trim()));
        }
        Ok(q.count(&self.db).await.dom()? > 0)
    }

    async fn create_click(&self, c: &NewClick, now: DateTime<Utc>) -> Result<()> {
        clicks::ActiveModel {
            affiliate_profile_id: Set(c.profile_id),
            visitor_key: Set(c.visitor_key.clone()),
            landing_path: Set(c.landing_path.clone()),
            referrer: Set(c.referrer.clone()),
            client_ip: Set(c.client_ip.clone()),
            user_agent: Set(c.user_agent.clone()),
            created_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn latest_profile_by_visitor(
        &self,
        visitor_key: &str,
        since: DateTime<Utc>,
    ) -> Result<Option<Profile>> {
        let key = visitor_key.trim();
        if key.is_empty() {
            return Ok(None);
        }
        let active = Query::select()
            .column(profiles::Column::Id)
            .from(profiles::Entity)
            .and_where(Expr::col(profiles::Column::Status).eq(status::PROFILE_ACTIVE))
            .and_where(Expr::col(profiles::Column::DeletedAt).is_null())
            .to_owned();
        let click = clicks::Entity::find()
            .filter(clicks::Column::VisitorKey.eq(key))
            .filter(clicks::Column::CreatedAt.gte(since))
            .filter(clicks::Column::AffiliateProfileId.in_subquery(active))
            .order_by_desc(clicks::Column::CreatedAt)
            .order_by_desc(clicks::Column::Id)
            .one(&self.db)
            .await
            .dom()?;
        match click {
            Some(c) => self.profile(c.affiliate_profile_id).await,
            None => Ok(None),
        }
    }

    async fn list_commissions(
        &self,
        f: &CommissionFilter,
        page: PageRequest,
    ) -> Result<Page<Commission>> {
        let q = commissions::Entity::find().filter(commission_condition(f));
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(commissions::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: attach_commission_refs(&self.db, rows).await?,
            total,
        })
    }

    async fn list_withdraws(
        &self,
        f: &WithdrawFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        let q = withdraws::Entity::find().filter(withdraw_condition(f));
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(withdraws::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: attach_withdraw_refs(&self.db, rows).await?,
            total,
        })
    }

    async fn withdraw(&self, id: Id) -> Result<Option<WithdrawRequest>> {
        withdraw_in(&self.db, id).await
    }

    async fn confirm_due(&self, now: DateTime<Utc>) -> Result<u64> {
        Ok(commissions::Entity::update_many()
            .col_expr(
                commissions::Column::Status,
                Expr::value(status::COMMISSION_AVAILABLE),
            )
            .col_expr(commissions::Column::AvailableAt, Expr::value(Some(now)))
            .col_expr(commissions::Column::UpdatedAt, Expr::value(now))
            .filter(commissions::Column::Status.eq(status::COMMISSION_PENDING_CONFIRM))
            .filter(commissions::Column::ConfirmAt.is_not_null())
            .filter(commissions::Column::ConfirmAt.lte(now))
            .filter(commissions::Column::WithdrawRequestId.is_null())
            .filter(commissions::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected)
    }

    async fn apply_withdraw(
        &self,
        user_id: Id,
        amount: Amount,
        channel: &str,
        account: &str,
        now: DateTime<Utc>,
    ) -> Result<WithdrawRequest> {
        let txn = self.db.begin().await.dom()?;
        let profile = profiles::Entity::find()
            .filter(profiles::Column::UserId.eq(user_id))
            .filter(profiles::Column::DeletedAt.is_null())
            .one(&txn)
            .await
            .dom()?
            .filter(|p| p.status.trim() == status::PROFILE_ACTIVE)
            .ok_or_else(Error::invalid)?;
        let rows = commissions::Entity::find()
            .filter(commissions::Column::AffiliateProfileId.eq(profile.id))
            .filter(commissions::Column::Status.eq(status::COMMISSION_AVAILABLE))
            .filter(commissions::Column::WithdrawRequestId.is_null())
            .filter(commissions::Column::DeletedAt.is_null())
            .order_by_asc(commissions::Column::Id)
            .lock_exclusive()
            .all(&txn)
            .await
            .dom()?;
        let amounts: Vec<(Id, Amount)> = rows
            .iter()
            .map(|r| (r.id, Amount::new(r.commission_amount)))
            .collect();
        let plan = plan_withdraw(&amounts, amount).ok_or_else(Error::invalid)?;
        if let Some((split_id, bound, remainder)) = plan.split {
            let source = rows
                .iter()
                .find(|r| r.id == split_id)
                .ok_or_else(|| Error::internal_msg("split commission missing"))?;
            let shrunk = commissions::Entity::update_many()
                .col_expr(
                    commissions::Column::CommissionAmount,
                    Expr::value(bound.decimal()),
                )
                .col_expr(commissions::Column::UpdatedAt, Expr::value(now))
                .filter(commissions::Column::Id.eq(split_id))
                .filter(commissions::Column::WithdrawRequestId.is_null())
                .filter(commissions::Column::Status.eq(status::COMMISSION_AVAILABLE))
                .exec(&txn)
                .await
                .dom()?
                .rows_affected;
            if shrunk != 1 {
                return Err(Error::internal_msg("commission changed concurrently"));
            }
            commissions::ActiveModel {
                affiliate_profile_id: Set(source.affiliate_profile_id),
                order_id: Set(source.order_id),
                order_item_id: Set(source.order_item_id),
                commission_type: Set(split_type(source.id, now)),
                base_amount: Set(source.base_amount),
                rate_percent: Set(source.rate_percent),
                commission_amount: Set(remainder.decimal()),
                status: Set(status::COMMISSION_AVAILABLE.to_owned()),
                confirm_at: Set(source.confirm_at),
                available_at: Set(source.available_at),
                withdraw_request_id: Set(None),
                invalid_reason: Set(String::new()),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .dom()?;
        }
        let request = withdraws::ActiveModel {
            affiliate_profile_id: Set(profile.id),
            amount: Set(amount.decimal()),
            channel: Set(channel.to_owned()),
            account: Set(account.to_owned()),
            status: Set(status::WITHDRAW_PENDING_REVIEW.to_owned()),
            reject_reason: Set(String::new()),
            processed_by: Set(None),
            processed_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        let bound = commissions::Entity::update_many()
            .col_expr(
                commissions::Column::WithdrawRequestId,
                Expr::value(Some(request.id)),
            )
            .col_expr(commissions::Column::UpdatedAt, Expr::value(now))
            .filter(commissions::Column::Id.is_in(plan.selected.clone()))
            .filter(commissions::Column::WithdrawRequestId.is_null())
            .filter(commissions::Column::Status.eq(status::COMMISSION_AVAILABLE))
            .exec(&txn)
            .await
            .dom()?
            .rows_affected;
        if bound != plan.selected.len() as u64 {
            // Another request froze some of these commissions first.
            return Err(Error::internal_msg("commission changed concurrently"));
        }
        txn.commit().await.dom()?;
        withdraw_in(&self.db, request.id)
            .await?
            .ok_or_else(|| Error::internal_msg("withdraw request missing"))
    }

    async fn review_withdraw(
        &self,
        id: Id,
        admin_id: Id,
        pay: bool,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<WithdrawRequest> {
        let txn = self.db.begin().await.dom()?;
        let request = withdraws::Entity::find_by_id(id)
            .filter(withdraws::Column::DeletedAt.is_null())
            .lock_exclusive()
            .one(&txn)
            .await
            .dom()?
            .ok_or_else(|| Error::not_found(zs_domain::error::keys::BAD_REQUEST))?;
        if request.status != status::WITHDRAW_PENDING_REVIEW {
            return Err(Error::invalid());
        }
        let ids: Vec<Id> = commissions::Entity::find()
            .select_only()
            .column(commissions::Column::Id)
            .filter(commissions::Column::WithdrawRequestId.eq(id))
            .filter(commissions::Column::DeletedAt.is_null())
            .lock_exclusive()
            .into_tuple()
            .all(&txn)
            .await
            .dom()?;
        if !ids.is_empty() {
            let q = commissions::Entity::update_many()
                .col_expr(commissions::Column::UpdatedAt, Expr::value(now));
            let q = if pay {
                q.col_expr(
                    commissions::Column::Status,
                    Expr::value(status::COMMISSION_WITHDRAWN),
                )
            } else {
                q.col_expr(
                    commissions::Column::WithdrawRequestId,
                    Expr::value(Option::<Id>::None),
                )
            };
            q.filter(commissions::Column::Id.is_in(ids))
                .exec(&txn)
                .await
                .dom()?;
        }
        let (next, reject_reason) = if pay {
            (status::WITHDRAW_PAID, String::new())
        } else {
            (status::WITHDRAW_REJECTED, reason.to_owned())
        };
        let updated = withdraws::Entity::update_many()
            .col_expr(withdraws::Column::Status, Expr::value(next))
            .col_expr(withdraws::Column::RejectReason, Expr::value(reject_reason))
            .col_expr(withdraws::Column::ProcessedBy, Expr::value(Some(admin_id)))
            .col_expr(withdraws::Column::ProcessedAt, Expr::value(Some(now)))
            .col_expr(withdraws::Column::UpdatedAt, Expr::value(now))
            .filter(withdraws::Column::Id.eq(id))
            .filter(withdraws::Column::Status.eq(status::WITHDRAW_PENDING_REVIEW))
            .exec(&txn)
            .await
            .dom()?
            .rows_affected;
        if updated != 1 {
            return Err(Error::invalid());
        }
        txn.commit().await.dom()?;
        withdraw_in(&self.db, id)
            .await?
            .ok_or_else(|| Error::internal_msg("withdraw request missing"))
    }

    async fn commission_order(&self, order_id: Id) -> Result<Option<CommissionOrder>> {
        let Some(order) = orders::Entity::find_by_id(order_id)
            .filter(orders::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let children: Vec<Id> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .filter(orders::Column::ParentId.eq(order.id))
            .filter(orders::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        let targets = if children.is_empty() {
            vec![order.id]
        } else {
            children
        };
        let items = order_items::Entity::find()
            .filter(order_items::Column::OrderId.is_in(targets))
            .filter(order_items::Column::DeletedAt.is_null())
            .order_by_asc(order_items::Column::Id)
            .all(&self.db)
            .await
            .dom()?;
        let product_ids = unique(items.iter().map(|i| i.product_id).collect());
        let mut enabled: HashSet<Id> = HashSet::new();
        for chunk in product_ids.chunks(CHUNK) {
            let found: Vec<Id> = products::Entity::find()
                .select_only()
                .column(products::Column::Id)
                .filter(products::Column::Id.is_in(chunk.to_vec()))
                .filter(products::Column::IsAffiliateEnabled.eq(true))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            enabled.extend(found);
        }
        Ok(Some(CommissionOrder {
            id: order.id,
            user_id: order.user_id,
            paid_at: order.paid_at,
            affiliate_profile_id: order.affiliate_profile_id,
            affiliate_code: order.affiliate_code,
            items: items
                .into_iter()
                .map(|i| CommissionItem {
                    product_id: i.product_id,
                    total_price: Amount::new(i.total_price),
                    coupon_discount: Amount::new(i.coupon_discount),
                    affiliate_enabled: enabled.contains(&i.product_id),
                })
                .collect(),
        }))
    }

    async fn create_commission(&self, c: &NewCommission) -> Result<bool> {
        let row = commissions::ActiveModel {
            affiliate_profile_id: Set(c.profile_id),
            order_id: Set(c.order_id),
            order_item_id: Set(None),
            commission_type: Set(c.commission_type.clone()),
            base_amount: Set(c.base_amount.decimal()),
            rate_percent: Set(c.rate_percent.decimal()),
            commission_amount: Set(c.commission_amount.decimal()),
            status: Set(c.status.clone()),
            confirm_at: Set(c.confirm_at),
            available_at: Set(c.available_at),
            withdraw_request_id: Set(None),
            invalid_reason: Set(String::new()),
            created_at: Set(c.now),
            updated_at: Set(c.now),
            ..Default::default()
        };
        crate::db::repo::support::insert_if_absent(&self.db, row, commissions::Column::Id).await
    }

    async fn reject_order_commissions(
        &self,
        order_id: Id,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        Ok(commissions::Entity::update_many()
            .col_expr(
                commissions::Column::Status,
                Expr::value(status::COMMISSION_REJECTED),
            )
            .col_expr(commissions::Column::InvalidReason, Expr::value(reason))
            .col_expr(commissions::Column::UpdatedAt, Expr::value(now))
            .filter(commissions::Column::OrderId.eq(order_id))
            .filter(commissions::Column::Status.is_in(open_statuses()))
            .filter(commissions::Column::WithdrawRequestId.is_null())
            .filter(commissions::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected)
    }
}
