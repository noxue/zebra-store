//! [`ProfileRepo`]: `reseller_profiles` and `reseller_domains`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::reseller::ports::{
    DomainFilter, DomainPlan, ProfileFilter, ProfilePlan, ProfileRepo,
};
use zs_domain::reseller::rules::{SystemDomainPlan, plan_system_domain};
use zs_domain::reseller::{
    DomainStatus, DomainType, Profile, ProfileStatus, ResellerDomain, SettlementStatus,
    VerificationStatus, keys,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::{
    SeaResellerStore, domain_conflict, domain_model, keyword_profile_ids, load_profiles,
    normalize_domain, with_users,
};
use crate::db::entity::{reseller_domains as domains, reseller_profiles as profiles};
use crate::db::repo::catalog::sql::ilike_sql;
use crate::db::repo::support::DbResultExt;

async fn profile_in<C: ConnectionTrait>(
    conn: &C,
    cond: Condition,
    lock: bool,
) -> Result<Option<Profile>> {
    let mut q = profiles::Entity::find()
        .filter(profiles::Column::DeletedAt.is_null())
        .filter(cond);
    if lock {
        q = q.lock_exclusive();
    }
    let Some(row) = q.one(conn).await.dom()? else {
        return Ok(None);
    };
    Ok(with_users(conn, vec![row]).await?.pop())
}

/// Live domains of a reseller (`is_primary DESC, id ASC`).
pub(crate) async fn domains_of_in<C: ConnectionTrait>(
    conn: &C,
    reseller_id: Id,
) -> Result<Vec<ResellerDomain>> {
    Ok(domains::Entity::find()
        .filter(domains::Column::ResellerId.eq(reseller_id))
        .filter(domains::Column::DeletedAt.is_null())
        .order_by_desc(domains::Column::IsPrimary)
        .order_by_asc(domains::Column::Id)
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(domain_model)
        .collect())
}

async fn domain_in<C: ConnectionTrait>(
    conn: &C,
    id: Id,
    lock: bool,
) -> Result<Option<ResellerDomain>> {
    let mut q = domains::Entity::find_by_id(id).filter(domains::Column::DeletedAt.is_null());
    if lock {
        q = q.lock_exclusive();
    }
    Ok(q.one(conn).await.dom()?.map(domain_model))
}

/// Writes every mutable column of a domain row.
async fn save_domain_in<C: ConnectionTrait>(
    conn: &C,
    d: &ResellerDomain,
    now: DateTime<Utc>,
) -> Result<()> {
    domains::ActiveModel {
        id: Set(d.id),
        reseller_id: Set(d.reseller_id),
        domain: Set(normalize_domain(&d.domain)),
        type_: Set(d.kind.as_str().to_owned()),
        verification_token: Set(d.verification_token.clone()),
        verification_status: Set(d.verification_status.as_str().to_owned()),
        status: Set(d.status.as_str().to_owned()),
        is_primary: Set(d.is_primary),
        verified_at: Set(d.verified_at),
        updated_at: Set(now),
        ..Default::default()
    }
    .update(conn)
    .await
    .map_err(domain_conflict)?;
    Ok(())
}

fn hosts(domains: &[ResellerDomain]) -> Vec<String> {
    domains
        .iter()
        .map(|d| d.domain.clone())
        .filter(|d| !d.is_empty())
        .collect()
}

#[async_trait]
impl ProfileRepo for SeaResellerStore {
    async fn profile_by_id(&self, id: Id) -> Result<Option<Profile>> {
        profile_in(
            &self.db,
            Condition::all().add(profiles::Column::Id.eq(id)),
            false,
        )
        .await
    }

    async fn profile_by_user(&self, user_id: Id) -> Result<Option<Profile>> {
        profile_in(
            &self.db,
            Condition::all().add(profiles::Column::UserId.eq(user_id)),
            false,
        )
        .await
    }

    async fn create_profile(
        &self,
        user_id: Id,
        apply_reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Profile> {
        let row = profiles::ActiveModel {
            user_id: Set(user_id),
            status: Set(ProfileStatus::PendingReview.as_str().to_owned()),
            apply_reason: Set(apply_reason.to_owned()),
            reject_reason: Set(String::new()),
            default_markup_percent: Set(Amount::ZERO.decimal()),
            max_markup_percent: Set(Amount::ZERO.decimal()),
            settlement_status: Set(SettlementStatus::Normal.as_str().to_owned()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(with_users(&self.db, vec![row])
            .await?
            .pop()
            .ok_or_else(zs_domain::reseller::not_found)?)
    }

    async fn update_profile(
        &self,
        id: Id,
        plan: &ProfilePlan<'_>,
        now: DateTime<Utc>,
    ) -> Result<Option<(Profile, Vec<String>)>> {
        let txn = self.db.begin().await.dom()?;
        let Some(current) = profile_in(
            &txn,
            Condition::all().add(profiles::Column::Id.eq(id)),
            true,
        )
        .await?
        else {
            return Ok(None);
        };
        let mut next = plan(current)?;
        next.updated_at = now;
        profiles::ActiveModel {
            id: Set(id),
            status: Set(next.status.as_str().to_owned()),
            apply_reason: Set(next.apply_reason.clone()),
            reject_reason: Set(next.reject_reason.clone()),
            default_markup_percent: Set(next.default_markup_percent.decimal()),
            max_markup_percent: Set(next.max_markup_percent.decimal()),
            settlement_status: Set(next.settlement_status.as_str().to_owned()),
            reviewed_by: Set(next.reviewed_by),
            reviewed_at: Set(next.reviewed_at),
            updated_at: Set(now),
            ..Default::default()
        }
        .update(&txn)
        .await
        .dom()?;
        let hosts = hosts(&domains_of_in(&txn, id).await?);
        txn.commit().await.dom()?;
        Ok(Some((next, hosts)))
    }

    async fn list_profiles(&self, f: &ProfileFilter, page: PageRequest) -> Result<Page<Profile>> {
        let mut cond = Condition::all().add(profiles::Column::DeletedAt.is_null());
        if let Some(uid) = f.user_id {
            cond = cond.add(profiles::Column::UserId.eq(uid));
        }
        if !f.status.trim().is_empty() {
            cond = cond.add(profiles::Column::Status.eq(f.status.trim()));
        }
        if !f.settlement_status.trim().is_empty() {
            cond = cond.add(profiles::Column::SettlementStatus.eq(f.settlement_status.trim()));
        }
        if !f.keyword.trim().is_empty() {
            cond = cond
                .add(profiles::Column::Id.is_in(keyword_profile_ids(&self.db, &f.keyword).await?));
        }
        if let Some(from) = f.created_from {
            cond = cond.add(profiles::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(profiles::Column::CreatedAt.lte(to));
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
            items: with_users(&self.db, rows).await?,
            total,
        })
    }

    async fn domains_of(&self, reseller_id: Id) -> Result<Vec<ResellerDomain>> {
        domains_of_in(&self.db, reseller_id).await
    }

    async fn domain_by_id(&self, id: Id) -> Result<Option<ResellerDomain>> {
        let Some(mut d) = domain_in(&self.db, id, false).await? else {
            return Ok(None);
        };
        d.profile = load_profiles(&self.db, &[d.reseller_id])
            .await?
            .remove(&d.reseller_id)
            .map(Box::new);
        Ok(Some(d))
    }

    async fn live_domain(&self, host: &str) -> Result<Option<ResellerDomain>> {
        let host = normalize_domain(host);
        if host.is_empty() {
            return Ok(None);
        }
        let Some(row) = domains::Entity::find()
            .filter(domains::Column::Domain.eq(host))
            .filter(domains::Column::Status.eq(DomainStatus::Active.as_str()))
            .filter(domains::Column::VerificationStatus.eq(VerificationStatus::Verified.as_str()))
            .filter(domains::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let mut d = domain_model(row);
        match self.profile_by_id(d.reseller_id).await? {
            Some(p) if p.status == ProfileStatus::Active => {
                d.profile = Some(Box::new(p));
                Ok(Some(d))
            }
            _ => Ok(None),
        }
    }

    async fn add_custom_domain(
        &self,
        reseller_id: Id,
        domain: &str,
        now: DateTime<Utc>,
    ) -> Result<ResellerDomain> {
        let domain = normalize_domain(domain);
        let txn = self.db.begin().await.dom()?;
        let existing = domains::Entity::find()
            .filter(domains::Column::Domain.eq(domain.as_str()))
            .order_by_desc(domains::Column::Id)
            .all(&txn)
            .await
            .dom()?;
        if existing.iter().any(|d| d.deleted_at.is_none()) {
            return Err(Error::bad_request(keys::DOMAIN_CONFLICT));
        }
        let row = if let Some(dead) = existing.into_iter().next() {
            // Revive the soft-deleted row of the same name (original `UpsertDomain`).
            domains::ActiveModel {
                id: Set(dead.id),
                reseller_id: Set(reseller_id),
                type_: Set(DomainType::Custom.as_str().to_owned()),
                verification_token: Set(String::new()),
                verification_status: Set(VerificationStatus::Pending.as_str().to_owned()),
                status: Set(DomainStatus::PendingReview.as_str().to_owned()),
                is_primary: Set(false),
                verified_at: Set(None),
                deleted_at: Set(None),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(&txn)
            .await
            .map_err(domain_conflict)?
        } else {
            domains::ActiveModel {
                reseller_id: Set(reseller_id),
                domain: Set(domain),
                type_: Set(DomainType::Custom.as_str().to_owned()),
                verification_token: Set(String::new()),
                verification_status: Set(VerificationStatus::Pending.as_str().to_owned()),
                status: Set(DomainStatus::PendingReview.as_str().to_owned()),
                is_primary: Set(false),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .map_err(domain_conflict)?
        };
        txn.commit().await.dom()?;
        Ok(domain_model(row))
    }

    async fn update_domains(
        &self,
        domain_id: Id,
        plan: &DomainPlan<'_>,
        now: DateTime<Utc>,
    ) -> Result<Option<(ResellerDomain, Vec<String>)>> {
        let txn = self.db.begin().await.dom()?;
        let Some(target) = domain_in(&txn, domain_id, true).await? else {
            return Ok(None);
        };
        let all = domains_of_in(&txn, target.reseller_id).await?;
        let mut touched = hosts(&all);
        for change in plan(&target, &all)? {
            save_domain_in(&txn, &change, now).await?;
            touched.push(change.domain);
        }
        let row = domain_in(&txn, domain_id, false)
            .await?
            .ok_or_else(zs_domain::reseller::not_found)?;
        txn.commit().await.dom()?;
        Ok(Some((row, touched)))
    }

    async fn assign_system_domain(
        &self,
        reseller_id: Id,
        domain: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<(ResellerDomain, Vec<String>)>> {
        let domain = normalize_domain(domain);
        let txn = self.db.begin().await.dom()?;
        if profile_in(
            &txn,
            Condition::all().add(profiles::Column::Id.eq(reseller_id)),
            true,
        )
        .await?
        .is_none()
        {
            return Ok(None);
        }
        let existing = domains::Entity::find()
            .filter(domains::Column::Domain.eq(domain.as_str()))
            .filter(domains::Column::DeletedAt.is_null())
            .one(&txn)
            .await
            .dom()?
            .map(domain_model);
        let all = domains_of_in(&txn, reseller_id).await?;
        let mut touched = hosts(&all);
        let id = match plan_system_domain(reseller_id, &domain, existing.as_ref(), &all, now)? {
            SystemDomainPlan::Insert { domain, is_primary } => {
                let inserted = domains::ActiveModel {
                    reseller_id: Set(reseller_id),
                    domain: Set(domain.clone()),
                    type_: Set(DomainType::Subdomain.as_str().to_owned()),
                    verification_token: Set(String::new()),
                    verification_status: Set(VerificationStatus::Verified.as_str().to_owned()),
                    status: Set(DomainStatus::Active.as_str().to_owned()),
                    is_primary: Set(is_primary),
                    verified_at: Set(Some(now)),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                }
                .insert(&txn)
                .await
                .map_err(domain_conflict)?;
                touched.push(domain);
                inserted.id
            }
            SystemDomainPlan::Update(row) => {
                save_domain_in(&txn, &row, now).await?;
                touched.push(row.domain);
                row.id
            }
        };
        let row = domain_in(&txn, id, false)
            .await?
            .ok_or_else(zs_domain::reseller::not_found)?;
        txn.commit().await.dom()?;
        Ok(Some((row, touched)))
    }

    async fn list_domains(
        &self,
        f: &DomainFilter,
        page: PageRequest,
    ) -> Result<Page<ResellerDomain>> {
        let mut cond = Condition::all().add(domains::Column::DeletedAt.is_null());
        if let Some(rid) = f.reseller_id {
            cond = cond.add(domains::Column::ResellerId.eq(rid));
        }
        if let Some(uid) = f.user_id {
            cond = cond.add(
                domains::Column::ResellerId.is_in(super::user_profile_ids(&self.db, uid).await?),
            );
        }
        if !f.domain.trim().is_empty() {
            cond = cond.add(domains::Column::Domain.eq(normalize_domain(&f.domain)));
        }
        for (col, v) in [
            (domains::Column::Type, &f.kind),
            (domains::Column::Status, &f.status),
            (domains::Column::VerificationStatus, &f.verification_status),
        ] {
            if !v.trim().is_empty() {
                cond = cond.add(col.eq(v.trim()));
            }
        }
        let keyword = f.keyword.trim();
        if !keyword.is_empty() {
            cond = cond.add(Condition::any().add(ilike_sql("domain", keyword)).add(
                domains::Column::ResellerId.is_in(keyword_profile_ids(&self.db, keyword).await?),
            ));
        }
        if let Some(from) = f.created_from {
            cond = cond.add(domains::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(domains::Column::CreatedAt.lte(to));
        }
        let q = domains::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows: Vec<ResellerDomain> = q
            .order_by_desc(domains::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(domain_model)
            .collect();
        let ids: Vec<Id> = rows.iter().map(|d| d.reseller_id).collect();
        let profiles = load_profiles(&self.db, &ids).await?;
        let items = rows
            .into_iter()
            .map(|mut d| {
                d.profile = profiles.get(&d.reseller_id).cloned().map(Box::new);
                d
            })
            .collect();
        Ok(Page { items, total })
    }
}
