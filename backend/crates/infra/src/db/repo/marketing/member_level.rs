//! [`MemberLevelRepo`] (`member_levels`, `member_level_prices`) and [`MemberUserRepo`] (`users`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::marketing::member_level::{
    LevelPriceInput, MemberLevel, MemberLevelFields, MemberLevelPrice, MemberLevelRepo,
    MemberProgress, MemberUserRepo,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::{member_level_prices, member_levels, users};
use crate::db::repo::catalog::sql::tombstone;
use crate::db::repo::support::{DbResultExt, from_json, to_json};

/// SeaORM implementation of the member-level ports.
#[derive(Debug, Clone)]
pub struct SeaMemberLevelRepo {
    db: DatabaseConnection,
}

impl SeaMemberLevelRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_domain(m: member_levels::Model) -> MemberLevel {
    MemberLevel {
        id: m.id,
        name: from_json(m.name_json),
        slug: m.slug,
        icon: m.icon,
        discount_rate: Amount::new(m.discount_rate),
        recharge_threshold: Amount::new(m.recharge_threshold),
        spend_threshold: Amount::new(m.spend_threshold),
        is_default: m.is_default,
        sort_order: m.sort_order,
        is_active: m.is_active,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn price_to_domain(m: member_level_prices::Model) -> MemberLevelPrice {
    MemberLevelPrice {
        id: m.id,
        member_level_id: m.member_level_id,
        product_id: m.product_id,
        sku_id: m.sku_id,
        price_amount: Amount::new(m.price_amount),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn apply_fields(
    model: &mut member_levels::ActiveModel,
    f: &MemberLevelFields,
    now: DateTime<Utc>,
) -> Result<()> {
    model.name_json = Set(to_json(&f.name)?);
    model.slug = Set(f.slug.clone());
    model.icon = Set(f.icon.clone());
    model.discount_rate = Set(f.discount_rate.decimal());
    model.recharge_threshold = Set(f.recharge_threshold.decimal());
    model.spend_threshold = Set(f.spend_threshold.decimal());
    model.is_default = Set(f.is_default);
    model.sort_order = Set(f.sort_order);
    model.is_active = Set(f.is_active);
    model.updated_at = Set(now);
    Ok(())
}

async fn clear_default<C: ConnectionTrait>(conn: &C, exclude: Id) -> Result<()> {
    let mut q = member_levels::Entity::update_many()
        .col_expr(member_levels::Column::IsDefault, Expr::value(false))
        .filter(member_levels::Column::DeletedAt.is_null())
        .filter(member_levels::Column::IsDefault.eq(true));
    if exclude > 0 {
        q = q.filter(member_levels::Column::Id.ne(exclude));
    }
    q.exec(conn).await.dom()?;
    Ok(())
}

fn alive() -> Condition {
    Condition::all().add(member_levels::Column::DeletedAt.is_null())
}

#[async_trait]
impl MemberLevelRepo for SeaMemberLevelRepo {
    async fn get(&self, id: Id) -> Result<Option<MemberLevel>> {
        Ok(member_levels::Entity::find_by_id(id)
            .filter(alive())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_by_slug(&self, slug: &str) -> Result<Option<MemberLevel>> {
        Ok(member_levels::Entity::find()
            .filter(alive())
            .filter(member_levels::Column::Slug.eq(slug))
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_default(&self) -> Result<Option<MemberLevel>> {
        Ok(member_levels::Entity::find()
            .filter(alive())
            .filter(member_levels::Column::IsDefault.eq(true))
            .filter(member_levels::Column::IsActive.eq(true))
            .order_by_asc(member_levels::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn find_active_by_sort_order(
        &self,
        sort_order: i32,
        exclude: Id,
    ) -> Result<Option<MemberLevel>> {
        let mut q = member_levels::Entity::find()
            .filter(alive())
            .filter(member_levels::Column::IsActive.eq(true))
            .filter(member_levels::Column::SortOrder.eq(sort_order));
        if exclude > 0 {
            q = q.filter(member_levels::Column::Id.ne(exclude));
        }
        Ok(q.one(&self.db).await.dom()?.map(to_domain))
    }

    async fn list_active(&self) -> Result<Vec<MemberLevel>> {
        Ok(member_levels::Entity::find()
            .filter(alive())
            .filter(member_levels::Column::IsActive.eq(true))
            .order_by_desc(member_levels::Column::SortOrder)
            .order_by_asc(member_levels::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(to_domain)
            .collect())
    }

    async fn list(&self, is_active: Option<bool>, page: PageRequest) -> Result<Page<MemberLevel>> {
        let mut q = member_levels::Entity::find().filter(alive());
        if let Some(active) = is_active {
            q = q.filter(member_levels::Column::IsActive.eq(active));
        }
        let total = q.clone().count(&self.db).await.dom()?;
        let items = q
            .order_by_desc(member_levels::Column::SortOrder)
            .order_by_asc(member_levels::Column::Id)
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

    async fn create(&self, fields: &MemberLevelFields, now: DateTime<Utc>) -> Result<MemberLevel> {
        let txn = self.db.begin().await.dom()?;
        if fields.is_default {
            clear_default(&txn, 0).await?;
        }
        let mut model = member_levels::ActiveModel {
            created_at: Set(now),
            ..Default::default()
        };
        apply_fields(&mut model, fields, now)?;
        let created = model.insert(&txn).await.dom()?;
        txn.commit().await.dom()?;
        Ok(to_domain(created))
    }

    async fn update(&self, id: Id, fields: &MemberLevelFields, now: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        if fields.is_default {
            clear_default(&txn, id).await?;
        }
        let mut model = member_levels::ActiveModel {
            id: Set(id),
            ..Default::default()
        };
        apply_fields(&mut model, fields, now)?;
        model.update(&txn).await.dom()?;
        txn.commit().await.dom()?;
        Ok(())
    }

    async fn delete(&self, id: Id, now: DateTime<Utc>) -> Result<()> {
        let slug: Option<String> = member_levels::Entity::find_by_id(id)
            .select_only()
            .column(member_levels::Column::Slug)
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        let mut q = member_levels::Entity::update_many()
            .col_expr(member_levels::Column::DeletedAt, Expr::value(now))
            .filter(member_levels::Column::Id.eq(id))
            .filter(member_levels::Column::DeletedAt.is_null());
        if let Some(slug) = slug {
            q = q.col_expr(
                member_levels::Column::Slug,
                Expr::value(tombstone(&slug, id)),
            );
        }
        q.exec(&self.db).await.dom()?;
        Ok(())
    }

    async fn get_price(&self, id: Id) -> Result<Option<MemberLevelPrice>> {
        Ok(member_level_prices::Entity::find_by_id(id)
            .filter(member_level_prices::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(price_to_domain))
    }

    async fn list_prices(&self, product_id: Id) -> Result<Vec<MemberLevelPrice>> {
        Ok(member_level_prices::Entity::find()
            .filter(member_level_prices::Column::DeletedAt.is_null())
            .filter(member_level_prices::Column::ProductId.eq(product_id))
            .order_by_asc(member_level_prices::Column::MemberLevelId)
            .order_by_asc(member_level_prices::Column::SkuId)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(price_to_domain)
            .collect())
    }

    async fn list_prices_for_level(
        &self,
        level_id: Id,
        product_ids: &[Id],
    ) -> Result<Vec<MemberLevelPrice>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(member_level_prices::Entity::find()
            .filter(member_level_prices::Column::DeletedAt.is_null())
            .filter(member_level_prices::Column::MemberLevelId.eq(level_id))
            .filter(member_level_prices::Column::ProductId.is_in(product_ids.to_vec()))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(price_to_domain)
            .collect())
    }

    async fn upsert_prices(&self, rows: &[LevelPriceInput], now: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        for row in rows {
            // The unique key includes soft-deleted rows: update/revive the row whatever its state.
            let existing = member_level_prices::Entity::find()
                .filter(member_level_prices::Column::MemberLevelId.eq(row.member_level_id))
                .filter(member_level_prices::Column::ProductId.eq(row.product_id))
                .filter(member_level_prices::Column::SkuId.eq(row.sku_id))
                .one(&txn)
                .await
                .dom()?;
            match existing {
                Some(m) => {
                    member_level_prices::ActiveModel {
                        id: Set(m.id),
                        price_amount: Set(row.price_amount.decimal()),
                        deleted_at: Set(None),
                        updated_at: Set(now),
                        ..Default::default()
                    }
                    .update(&txn)
                    .await
                    .dom()?;
                }
                None => {
                    member_level_prices::ActiveModel {
                        member_level_id: Set(row.member_level_id),
                        product_id: Set(row.product_id),
                        sku_id: Set(row.sku_id),
                        price_amount: Set(row.price_amount.decimal()),
                        created_at: Set(now),
                        updated_at: Set(now),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await
                    .dom()?;
                }
            }
        }
        txn.commit().await.dom()?;
        Ok(())
    }

    async fn delete_price(&self, id: Id, now: DateTime<Utc>) -> Result<()> {
        member_level_prices::Entity::update_many()
            .col_expr(member_level_prices::Column::DeletedAt, Expr::value(now))
            .filter(member_level_prices::Column::Id.eq(id))
            .filter(member_level_prices::Column::DeletedAt.is_null())
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}

fn user_alive(id: Id) -> Condition {
    Condition::all()
        .add(users::Column::Id.eq(id))
        .add(users::Column::DeletedAt.is_null())
}

#[async_trait]
impl MemberUserRepo for SeaMemberLevelRepo {
    async fn progress(&self, user_id: Id) -> Result<Option<MemberProgress>> {
        let row: Option<(Id, rust_decimal::Decimal, rust_decimal::Decimal)> = users::Entity::find()
            .select_only()
            .column(users::Column::MemberLevelId)
            .column(users::Column::TotalRecharged)
            .column(users::Column::TotalSpent)
            .filter(user_alive(user_id))
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(|(level, recharged, spent)| MemberProgress {
            member_level_id: level,
            total_recharged: Amount::new(recharged),
            total_spent: Amount::new(spent),
        }))
    }

    async fn set_level(&self, user_id: Id, level_id: Id, now: DateTime<Utc>) -> Result<()> {
        users::Entity::update_many()
            .col_expr(users::Column::MemberLevelId, Expr::value(level_id))
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .filter(user_alive(user_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn set_level_if_current(
        &self,
        user_id: Id,
        current: Id,
        next: Id,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        if current == next {
            return Ok(0);
        }
        Ok(users::Entity::update_many()
            .col_expr(users::Column::MemberLevelId, Expr::value(next))
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .filter(user_alive(user_id))
            .filter(users::Column::MemberLevelId.eq(current))
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected)
    }

    async fn add_recharged(&self, user_id: Id, amount: Amount, now: DateTime<Utc>) -> Result<()> {
        if !amount.is_positive() {
            return Ok(());
        }
        users::Entity::update_many()
            .col_expr(
                users::Column::TotalRecharged,
                Expr::col(users::Column::TotalRecharged).add(amount.decimal()),
            )
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .filter(user_alive(user_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn add_spent(&self, user_id: Id, amount: Amount, now: DateTime<Utc>) -> Result<()> {
        if !amount.is_positive() {
            return Ok(());
        }
        users::Entity::update_many()
            .col_expr(
                users::Column::TotalSpent,
                Expr::col(users::Column::TotalSpent).add(amount.decimal()),
            )
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .filter(user_alive(user_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn backfill_level(&self, level_id: Id, now: DateTime<Utc>) -> Result<u64> {
        if level_id <= 0 {
            return Ok(0);
        }
        Ok(users::Entity::update_many()
            .col_expr(users::Column::MemberLevelId, Expr::value(level_id))
            .col_expr(users::Column::UpdatedAt, Expr::value(now))
            .filter(users::Column::DeletedAt.is_null())
            .filter(
                Condition::any()
                    .add(users::Column::MemberLevelId.eq(0))
                    .add(users::Column::MemberLevelId.is_null()),
            )
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected)
    }

    async fn count_with_level(&self, level_id: Id) -> Result<u64> {
        users::Entity::find()
            .filter(users::Column::DeletedAt.is_null())
            .filter(users::Column::MemberLevelId.eq(level_id))
            .count(&self.db)
            .await
            .dom()
    }
}
