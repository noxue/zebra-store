//! [`CouponRepo`] and [`CouponLedger`] backed by `coupons` / `coupon_usages`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::marketing::coupon::{
    Coupon, CouponClaim, CouponFields, CouponFilter, CouponLedger, CouponRepo, CouponUsage,
    SCOPE_PRODUCT, keys, scope_id_patterns,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::{coupon_usages, coupons};
use crate::db::repo::catalog::sql::tombstone;
use crate::db::repo::support::{DbResultExt, from_json, to_json};

/// SeaORM implementation of the coupon ports.
#[derive(Debug, Clone)]
pub struct SeaCouponRepo {
    db: DatabaseConnection,
}

impl SeaCouponRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: coupons::Model) -> Coupon {
    Coupon {
        id: m.id,
        code: m.code,
        kind: m.type_,
        value: Amount::new(m.value),
        min_amount: Amount::new(m.min_amount),
        max_discount: Amount::new(m.max_discount),
        usage_limit: m.usage_limit,
        used_count: m.used_count,
        per_user_limit: m.per_user_limit,
        disabled_wholesale_price: m.disabled_wholesale_price,
        per_item_discount: m.per_item_discount,
        payment_roles: from_json(m.payment_roles),
        member_levels: from_json(m.member_levels),
        scope_type: m.scope_type,
        scope_ref_ids: m.scope_ref_ids,
        starts_at: m.starts_at,
        ends_at: m.ends_at,
        is_active: m.is_active,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn usage_to_domain(m: coupon_usages::Model) -> CouponUsage {
    CouponUsage {
        id: m.id,
        coupon_id: m.coupon_id,
        user_id: m.user_id,
        order_id: m.order_id,
        discount_amount: Amount::new(m.discount_amount),
        created_at: m.created_at,
    }
}

fn apply_fields(
    model: &mut coupons::ActiveModel,
    f: &CouponFields,
    now: DateTime<Utc>,
) -> Result<()> {
    model.code = Set(f.code.clone());
    model.type_ = Set(f.kind.as_str().to_owned());
    model.value = Set(f.value.decimal());
    model.min_amount = Set(f.min_amount.decimal());
    model.max_discount = Set(f.max_discount.decimal());
    model.usage_limit = Set(f.usage_limit);
    model.per_user_limit = Set(f.per_user_limit);
    model.disabled_wholesale_price = Set(f.disabled_wholesale_price);
    model.per_item_discount = Set(f.per_item_discount);
    model.payment_roles = Set(to_json(&f.payment_roles)?);
    model.member_levels = Set(to_json(&f.member_levels)?);
    model.scope_type = Set(SCOPE_PRODUCT.to_owned());
    model.scope_ref_ids = Set(f.scope_ref_ids.clone());
    model.starts_at = Set(f.starts_at);
    model.ends_at = Set(f.ends_at);
    model.is_active = Set(f.is_active);
    model.updated_at = Set(now);
    Ok(())
}

async fn count_user_usages_in<C: ConnectionTrait>(
    conn: &C,
    coupon_id: Id,
    user_id: Id,
) -> Result<u64> {
    coupon_usages::Entity::find()
        .filter(coupon_usages::Column::CouponId.eq(coupon_id))
        .filter(coupon_usages::Column::UserId.eq(user_id))
        .filter(coupon_usages::Column::DeletedAt.is_null())
        .count(conn)
        .await
        .dom()
}

/// Claims one coupon use inside the caller's transaction (PRC-01): conditional
/// `used_count + 1 WHERE usage_limit = 0 OR used_count < usage_limit`, per-user re-check,
/// usage row insert. Any failure must roll the caller's transaction back.
pub async fn claim_in<C: ConnectionTrait>(
    conn: &C,
    claim: &CouponClaim,
    now: DateTime<Utc>,
) -> Result<()> {
    let coupon = coupons::Entity::find_by_id(claim.coupon_id)
        .filter(coupons::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
    if coupon.per_user_limit > 0 && claim.user_id != 0 {
        let used = count_user_usages_in(conn, coupon.id, claim.user_id).await?;
        if used >= u64::try_from(coupon.per_user_limit).unwrap_or(0) {
            return Err(Error::bad_request(keys::PER_USER_LIMIT));
        }
    }
    let affected = coupons::Entity::update_many()
        .col_expr(
            coupons::Column::UsedCount,
            Expr::col(coupons::Column::UsedCount).add(1),
        )
        .col_expr(coupons::Column::UpdatedAt, Expr::value(now))
        .filter(coupons::Column::Id.eq(coupon.id))
        .filter(coupons::Column::DeletedAt.is_null())
        .filter(
            Condition::any()
                .add(coupons::Column::UsageLimit.lte(0))
                .add(
                    Expr::col(coupons::Column::UsedCount)
                        .lt(Expr::col(coupons::Column::UsageLimit)),
                ),
        )
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if affected != 1 {
        return Err(Error::bad_request(keys::USAGE_LIMIT));
    }
    coupon_usages::ActiveModel {
        coupon_id: Set(coupon.id),
        user_id: Set(claim.user_id),
        order_id: Set(claim.order_id),
        discount_amount: Set(claim.discount_amount.decimal()),
        created_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    Ok(())
}

/// Releases every usage of an order and decrements the counters (never below zero).
pub async fn release_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<u64> {
    let usages = coupon_usages::Entity::find()
        .filter(coupon_usages::Column::OrderId.eq(order_id))
        .filter(coupon_usages::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?;
    for usage in &usages {
        coupons::Entity::update_many()
            .col_expr(
                coupons::Column::UsedCount,
                Expr::col(coupons::Column::UsedCount).sub(1),
            )
            .col_expr(coupons::Column::UpdatedAt, Expr::value(now))
            .filter(coupons::Column::Id.eq(usage.coupon_id))
            .filter(coupons::Column::UsedCount.gte(1))
            .exec(conn)
            .await
            .dom()?;
    }
    coupon_usages::Entity::update_many()
        .col_expr(coupon_usages::Column::DeletedAt, Expr::value(now))
        .filter(coupon_usages::Column::OrderId.eq(order_id))
        .filter(coupon_usages::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?;
    Ok(usages.len() as u64)
}

#[async_trait]
impl CouponRepo for SeaCouponRepo {
    async fn get(&self, id: Id) -> Result<Option<Coupon>> {
        Ok(coupons::Entity::find_by_id(id)
            .filter(coupons::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_by_code(&self, code: &str) -> Result<Option<Coupon>> {
        Ok(coupons::Entity::find()
            .filter(coupons::Column::DeletedAt.is_null())
            .filter(coupons::Column::Code.eq(code))
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn list(&self, filter: &CouponFilter, page: PageRequest) -> Result<Page<Coupon>> {
        let mut cond = Condition::all().add(coupons::Column::DeletedAt.is_null());
        if filter.id > 0 {
            cond = cond.add(coupons::Column::Id.eq(filter.id));
        }
        if !filter.code.is_empty() {
            cond = cond.add(coupons::Column::Code.eq(filter.code.clone()));
        }
        if filter.scope_ref_id > 0 {
            // PRC-15: whole-element match inside the compact JSON array.
            let [exact, prefix, middle, suffix] = scope_id_patterns(filter.scope_ref_id);
            cond = cond.add(
                Condition::any()
                    .add(coupons::Column::ScopeRefIds.eq(exact))
                    .add(coupons::Column::ScopeRefIds.like(prefix))
                    .add(coupons::Column::ScopeRefIds.like(middle))
                    .add(coupons::Column::ScopeRefIds.like(suffix)),
            );
        }
        if let Some(active) = filter.is_active {
            cond = cond.add(coupons::Column::IsActive.eq(active));
        }
        let q = coupons::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let items = q
            .order_by_desc(coupons::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(to_domain)
            .collect();
        Ok(Page { items, total })
    }

    async fn create(&self, fields: &CouponFields, now: DateTime<Utc>) -> Result<Coupon> {
        let mut model = coupons::ActiveModel {
            used_count: Set(0),
            created_at: Set(now),
            ..Default::default()
        };
        apply_fields(&mut model, fields, now)?;
        Ok(to_domain(model.insert(&self.db).await.dom()?))
    }

    async fn update(&self, id: Id, fields: &CouponFields, now: DateTime<Utc>) -> Result<()> {
        let mut model = coupons::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        apply_fields(&mut model, fields, now)?;
        model.update(&self.db).await.dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()> {
        let code: Option<String> = coupons::Entity::find_by_id(id)
            .select_only()
            .column(coupons::Column::Code)
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        let mut q = coupons::Entity::update_many()
            .col_expr(coupons::Column::DeletedAt, Expr::value(now))
            .filter(coupons::Column::Id.eq(id))
            .filter(coupons::Column::DeletedAt.is_null());
        if let Some(code) = code {
            // Frees the unique code for reuse (ORD-11 principle).
            q = q.col_expr(coupons::Column::Code, Expr::value(tombstone(&code, id)));
        }
        q.exec(&self.db).await.dom()?;
        Ok(())
    }

    async fn count_user_usages(&self, coupon_id: Id, user_id: Id) -> Result<u64> {
        count_user_usages_in(&self.db, coupon_id, user_id).await
    }

    async fn list_user_usages(&self, user_id: Id, page: PageRequest) -> Result<Page<CouponUsage>> {
        let q = coupon_usages::Entity::find()
            .filter(coupon_usages::Column::UserId.eq(user_id))
            .filter(coupon_usages::Column::DeletedAt.is_null());
        let total = q.clone().count(&self.db).await.dom()?;
        let items = q
            .order_by_desc(coupon_usages::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(usage_to_domain)
            .collect();
        Ok(Page { items, total })
    }
}

#[async_trait]
impl CouponLedger for SeaCouponRepo {
    async fn claim(&self, claim: &CouponClaim, now: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        claim_in(&txn, claim, now).await?;
        txn.commit().await.dom()?;
        Ok(())
    }

    async fn release(&self, order_id: Id, now: DateTime<Utc>) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let released = release_in(&txn, order_id, now).await?;
        txn.commit().await.dom()?;
        Ok(released)
    }
}
