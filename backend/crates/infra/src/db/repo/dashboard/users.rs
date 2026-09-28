//! [`AdminUserRepo`] over users, wallet accounts, OAuth identities, coupon
//! usages, coupons and products.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, Query};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, Order,
    PaginatorTrait, QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::dashboard::users::{
    AdminUserRepo, CouponBrief, CouponUsage, OAuthIdentity, ScopeProduct, UnbindOutcome,
    UsableProviders, UserListFilter, UserSort, keeps_usable_login,
};
use zs_domain::identity::user::{STATUS_DISABLED, User};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::db::entity::{
    coupon_usages, coupons, products, user_oauth_identities as oauth, users, wallet_accounts,
};
use crate::db::repo::identity::user::to_domain;
use crate::db::repo::support::{DbResultExt, now};

/// Rows per `IN (…)` list.
const CHUNK: usize = 200;
/// Wallet balance of the listed user, 0 without an account (portable SQL:
/// only fixed identifiers, no user input).
const WALLET_BALANCE_SQL: &str = "COALESCE((SELECT MAX(wallet_accounts.balance) FROM wallet_accounts \
     WHERE wallet_accounts.user_id = users.id AND wallet_accounts.deleted_at IS NULL), 0)";

/// SeaORM implementation of [`AdminUserRepo`].
#[derive(Debug, Clone)]
pub struct SeaAdminUserRepo {
    db: DatabaseConnection,
}

impl SeaAdminUserRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn identity(m: oauth::Model) -> OAuthIdentity {
    OAuthIdentity {
        id: m.id,
        provider: m.provider,
        provider_user_id: m.provider_user_id,
        username: m.username,
        avatar_url: m.avatar_url,
        auth_at: m.auth_at,
        created_at: m.created_at,
    }
}

fn condition(f: &UserListFilter) -> Condition {
    let mut c = Condition::all().add(users::Column::DeletedAt.is_null());
    if let Some(id) = f.user_id {
        c = c.add(users::Column::Id.eq(id));
    }
    if !f.keyword.is_empty() {
        let kw = f.keyword.as_str();
        let linked = Query::select()
            .column(oauth::Column::UserId)
            .from(oauth::Entity)
            .cond_where(
                Condition::any()
                    .add(oauth::Column::Provider.contains(kw))
                    .add(oauth::Column::ProviderUserId.contains(kw))
                    .add(oauth::Column::Username.contains(kw)),
            )
            .to_owned();
        c = c.add(
            Condition::any()
                .add(users::Column::Email.contains(kw))
                .add(users::Column::DisplayName.contains(kw))
                .add(users::Column::Id.in_subquery(linked)),
        );
    }
    if !f.status.is_empty() {
        c = c.add(users::Column::Status.eq(f.status.clone()));
    }
    if let Some(t) = f.created_from {
        c = c.add(users::Column::CreatedAt.gte(t));
    }
    if let Some(t) = f.created_to {
        c = c.add(users::Column::CreatedAt.lte(t));
    }
    if let Some(t) = f.last_login_from {
        c = c.add(users::Column::LastLoginAt.gte(t));
    }
    if let Some(t) = f.last_login_to {
        c = c.add(users::Column::LastLoginAt.lte(t));
    }
    c
}

#[async_trait]
impl AdminUserRepo for SeaAdminUserRepo {
    async fn list(&self, filter: &UserListFilter, page: PageRequest) -> Result<Page<User>> {
        let query = users::Entity::find().filter(condition(filter));
        let total = query.clone().count(&self.db).await.dom()?;
        let dir = if filter.ascending {
            Order::Asc
        } else {
            Order::Desc
        };
        let query = match filter.sort {
            UserSort::Id => query,
            UserSort::CreatedAt => query.order_by(users::Column::CreatedAt, dir),
            // Users who never logged in stay last in both directions.
            UserSort::LastLoginAt => query
                .order_by(Expr::col(users::Column::LastLoginAt).is_null(), Order::Asc)
                .order_by(users::Column::LastLoginAt, dir),
            UserSort::WalletBalance => query.order_by(Expr::cust(WALLET_BALANCE_SQL), dir),
        };
        let rows = query
            .order_by_desc(users::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_domain).collect(),
            total,
        })
    }

    async fn get(&self, id: Id) -> Result<Option<User>> {
        Ok(users::Entity::find_by_id(id)
            .filter(users::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_by_email(&self, email: &str) -> Result<Option<User>> {
        Ok(users::Entity::find()
            .filter(users::Column::Email.eq(email))
            .filter(users::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn save_admin_edit(&self, u: &User) -> Result<()> {
        users::ActiveModel {
            id: Set(u.id),
            email: Set(u.email.clone()),
            display_name: Set(u.display_name.clone()),
            password_hash: Set(u.password_hash.clone()),
            locale: Set(u.locale.clone()),
            status: Set(u.status.clone()),
            admin_note: Set(u.admin_note.clone()),
            email_verified_at: Set(u.email_verified_at),
            token_version: Set(u.token_version),
            token_invalid_before: Set(u.token_invalid_before),
            updated_at: Set(now()),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn batch_status(&self, ids: &[Id], status: &str, at: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        for chunk in ids.chunks(CHUNK) {
            let mut update = users::Entity::update_many()
                .col_expr(users::Column::Status, Expr::value(status))
                .col_expr(users::Column::UpdatedAt, Expr::value(at));
            if status == STATUS_DISABLED {
                update = update
                    .col_expr(users::Column::TokenInvalidBefore, Expr::value(at))
                    .col_expr(
                        users::Column::TokenVersion,
                        Expr::col(users::Column::TokenVersion).add(1),
                    );
            }
            update
                .filter(users::Column::Id.is_in(chunk.to_vec()))
                .filter(users::Column::DeletedAt.is_null())
                .exec(&txn)
                .await
                .dom()?;
        }
        txn.commit().await.dom()
    }

    async fn balances(&self, ids: &[Id]) -> Result<HashMap<Id, Amount>> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(CHUNK) {
            let rows: Vec<(Id, rust_decimal::Decimal)> = wallet_accounts::Entity::find()
                .select_only()
                .column(wallet_accounts::Column::UserId)
                .column(wallet_accounts::Column::Balance)
                .filter(wallet_accounts::Column::DeletedAt.is_null())
                .filter(wallet_accounts::Column::UserId.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            out.extend(rows.into_iter().map(|(id, b)| (id, Amount::new(b))));
        }
        Ok(out)
    }

    async fn identities(&self, user_id: Id) -> Result<Vec<OAuthIdentity>> {
        Ok(oauth::Entity::find()
            .filter(oauth::Column::UserId.eq(user_id))
            .order_by_asc(oauth::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(identity)
            .collect())
    }

    async fn unbind(
        &self,
        user_id: Id,
        provider: &str,
        usable: UsableProviders,
    ) -> Result<UnbindOutcome> {
        let txn = self.db.begin().await.dom()?;
        let Some(user) = users::Entity::find_by_id(user_id)
            .filter(users::Column::DeletedAt.is_null())
            .one(&txn)
            .await
            .dom()?
            .map(to_domain)
        else {
            return Ok(UnbindOutcome::UserNotFound);
        };
        if !user.is_active() {
            return Ok(UnbindOutcome::UserDisabled);
        }
        let identities: Vec<OAuthIdentity> = oauth::Entity::find()
            .filter(oauth::Column::UserId.eq(user_id))
            .order_by_asc(oauth::Column::Id)
            .all(&txn)
            .await
            .dom()?
            .into_iter()
            .map(identity)
            .collect();
        let Some(target) = identities.iter().find(|i| i.provider == provider) else {
            return Ok(UnbindOutcome::NotBound);
        };
        if !keeps_usable_login(&user, target.id, &identities, usable) {
            return Ok(UnbindOutcome::Locked);
        }
        oauth::Entity::delete_by_id(target.id)
            .exec(&txn)
            .await
            .dom()?;
        txn.commit().await.dom()?;
        Ok(UnbindOutcome::Unbound)
    }

    async fn coupon_usages(&self, user_id: Id, page: PageRequest) -> Result<Page<CouponUsage>> {
        let query = coupon_usages::Entity::find()
            .filter(coupon_usages::Column::UserId.eq(user_id))
            .filter(coupon_usages::Column::DeletedAt.is_null());
        let total = query.clone().count(&self.db).await.dom()?;
        let rows = query
            .order_by_desc(coupon_usages::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows
                .into_iter()
                .map(|m| CouponUsage {
                    id: m.id,
                    coupon_id: m.coupon_id,
                    order_id: m.order_id,
                    discount_amount: Amount::new(m.discount_amount),
                    created_at: m.created_at,
                })
                .collect(),
            total,
        })
    }

    async fn coupons(&self, ids: &[Id]) -> Result<Vec<CouponBrief>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(CHUNK) {
            let rows = coupons::Entity::find()
                .filter(coupons::Column::DeletedAt.is_null())
                .filter(coupons::Column::Id.is_in(chunk.to_vec()))
                .all(&self.db)
                .await
                .dom()?;
            out.extend(rows.into_iter().map(|c| CouponBrief {
                id: c.id,
                code: c.code,
                kind: c.type_,
                scope_ref_ids: c.scope_ref_ids,
            }));
        }
        Ok(out)
    }

    async fn products(&self, ids: &[Id]) -> Result<Vec<ScopeProduct>> {
        let mut out = Vec::new();
        for chunk in ids.chunks(CHUNK) {
            let rows: Vec<(Id, Option<serde_json::Value>)> = products::Entity::find()
                .select_only()
                .column(products::Column::Id)
                .column(products::Column::TitleJson)
                .filter(products::Column::DeletedAt.is_null())
                .filter(products::Column::Id.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            out.extend(
                rows.into_iter()
                    .map(|(id, title)| ScopeProduct { id, title }),
            );
        }
        Ok(out)
    }
}
