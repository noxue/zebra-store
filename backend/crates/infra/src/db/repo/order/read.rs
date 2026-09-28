//! [`OrderRepo`]: order, refund, payment and lookup reads.

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, FromQueryResult, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Select,
};
use zs_domain::order::model::{Order, OrderStatus, RefundRecord};
use zs_domain::order::ports::{AdminOrderFilter, OrderRepo, Owner, RefundFilter, Scope, UserBrief};
use zs_domain::payment::channel::PaymentChannel;
use zs_domain::payment::model::Payment;
use zs_domain::payment::types::{FeePolicy, PaymentStatus};
use zs_domain::{Id, Result};
use zs_shared::page::{Page, PageRequest};

use super::{map, ops, refund};
use crate::db::entity::{
    coupons, order_items, order_refund_records, orders, payment_channels, payments, products,
    promotions, user_oauth_identities, users,
};
use crate::db::repo::catalog::sql::{
    SEARCH_LOCALES, backend_of, contains_pattern, ilike_sql, json_text,
};
use crate::db::repo::payment::channel::to_domain as channel_to_domain;
use crate::db::repo::payment::records::to_domain as payment_to_domain;
use crate::db::repo::support::DbResultExt;

/// LQA-I3: the order (or one of its children) is still paid / fulfilling while its
/// purchase order failed and waits for an admin. Portable SQL (no dialect branch).
const PROCUREMENT_ISSUE_SQL: &str = "EXISTS (SELECT 1 FROM procurement_orders p \
     JOIN orders c ON c.id = p.local_order_id \
     WHERE (c.id = orders.id OR c.parent_id = orders.id) \
     AND c.status IN ('paid', 'fulfilling') \
     AND p.status IN ('rejected', 'manual_review', 'canceled') \
     AND p.deleted_at IS NULL)";

/// OAuth provider name of Telegram identities.
const TELEGRAM_PROVIDER: &str = "telegram";

/// SeaORM implementation of [`OrderRepo`].
#[derive(Debug, Clone)]
pub struct SeaOrderRepo {
    db: DatabaseConnection,
}

impl SeaOrderRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn page(&self, q: Select<orders::Entity>, page: PageRequest) -> Result<Page<Order>> {
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: map::attach(&self.db, rows).await?,
            total,
        })
    }

    async fn one(&self, q: Select<orders::Entity>) -> Result<Option<Order>> {
        let Some(row) = q.one(&self.db).await.dom()? else {
            return Ok(None);
        };
        Ok(map::attach(&self.db, vec![row]).await?.pop())
    }

    /// Order ids (parents) whose item titles match `keyword` (parents via their children).
    async fn product_keyword_parent_ids(&self, keyword: &str) -> Result<Vec<Id>> {
        let backend = backend_of(&self.db);
        let mut any = Condition::any();
        for locale in SEARCH_LOCALES {
            any = any.add(ilike_sql(
                json_text(backend, "order_items.title_json", locale),
                keyword,
            ));
        }
        let order_ids: Vec<Id> = order_items::Entity::find()
            .select_only()
            .column(order_items::Column::OrderId)
            .filter(order_items::Column::DeletedAt.is_null())
            .filter(any)
            .distinct()
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows: Vec<(Id, Option<Id>)> = orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .column(orders::Column::ParentId)
            .filter(orders::Column::Id.is_in(order_ids))
            .filter(orders::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows
            .into_iter()
            .map(|(id, parent)| parent.unwrap_or(id))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect())
    }

    /// Users matching `keyword` by email, display name or OAuth identity.
    async fn user_keyword_ids(&self, keyword: &str) -> Result<Vec<Id>> {
        let pattern = contains_pattern(keyword);
        let mut ids: HashSet<Id> = users::Entity::find()
            .select_only()
            .column(users::Column::Id)
            .filter(users::Column::DeletedAt.is_null())
            .filter(
                Condition::any()
                    .add(
                        Expr::expr(sea_orm::sea_query::Func::lower(Expr::col(
                            users::Column::Email,
                        )))
                        .like(pattern.clone()),
                    )
                    .add(
                        Expr::expr(sea_orm::sea_query::Func::lower(Expr::col(
                            users::Column::DisplayName,
                        )))
                        .like(pattern.clone()),
                    ),
            )
            .into_tuple::<Id>()
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .collect();
        ids.extend(
            user_oauth_identities::Entity::find()
                .select_only()
                .column(user_oauth_identities::Column::UserId)
                .filter(
                    Condition::any()
                        .add(
                            Expr::expr(sea_orm::sea_query::Func::lower(Expr::col(
                                user_oauth_identities::Column::Provider,
                            )))
                            .like(pattern.clone()),
                        )
                        .add(
                            Expr::expr(sea_orm::sea_query::Func::lower(Expr::col(
                                user_oauth_identities::Column::ProviderUserId,
                            )))
                            .like(pattern.clone()),
                        )
                        .add(
                            Expr::expr(sea_orm::sea_query::Func::lower(Expr::col(
                                user_oauth_identities::Column::Username,
                            )))
                            .like(pattern),
                        ),
                )
                .into_tuple::<Id>()
                .all(&self.db)
                .await
                .dom()?,
        );
        Ok(ids.into_iter().collect())
    }
}

fn alive() -> Select<orders::Entity> {
    orders::Entity::find().filter(orders::Column::DeletedAt.is_null())
}

fn owner_cond(owner: &Owner) -> Condition {
    match owner {
        Owner::User(id) => Condition::all().add(orders::Column::UserId.eq(*id)),
        Owner::Guest { email, credential } => Condition::all()
            .add(orders::Column::UserId.eq(0))
            .add(orders::Column::GuestEmail.eq(email.clone()))
            .add(orders::Column::GuestPassword.eq(credential.clone())),
    }
}

fn scope_cond(scope: Scope) -> Condition {
    match scope {
        Scope::Main => Condition::all().add(orders::Column::ResellerId.is_null()),
        Scope::Reseller(id) => Condition::all().add(orders::Column::ResellerId.eq(id)),
    }
}

#[derive(Debug, FromQueryResult)]
struct StatusCount {
    status: String,
    count: i64,
}

#[async_trait]
impl OrderRepo for SeaOrderRepo {
    async fn get(&self, id: Id) -> Result<Option<Order>> {
        map::load(&self.db, id).await
    }

    async fn find_parent(
        &self,
        order_no: &str,
        owner: &Owner,
        scope: Scope,
    ) -> Result<Option<Order>> {
        self.one(
            alive()
                .filter(orders::Column::OrderNo.eq(order_no.trim()))
                .filter(orders::Column::ParentId.is_null())
                .filter(owner_cond(owner))
                .filter(scope_cond(scope)),
        )
        .await
    }

    async fn find_parent_by_id(
        &self,
        id: Id,
        owner: &Owner,
        scope: Scope,
    ) -> Result<Option<Order>> {
        self.one(
            alive()
                .filter(orders::Column::Id.eq(id))
                .filter(orders::Column::ParentId.is_null())
                .filter(owner_cond(owner))
                .filter(scope_cond(scope)),
        )
        .await
    }

    async fn find_any(&self, order_no: &str, owner: &Owner, scope: Scope) -> Result<Option<Order>> {
        self.one(
            alive()
                .filter(orders::Column::OrderNo.eq(order_no.trim()))
                .filter(owner_cond(owner))
                .filter(scope_cond(scope)),
        )
        .await
    }

    async fn list_for_owner(
        &self,
        owner: &Owner,
        scope: Scope,
        status: &str,
        order_no: &str,
        page: PageRequest,
    ) -> Result<Page<Order>> {
        let mut q = alive()
            .filter(orders::Column::ParentId.is_null())
            .filter(owner_cond(owner))
            .filter(scope_cond(scope));
        if !status.trim().is_empty() {
            q = q.filter(orders::Column::Status.eq(status.trim()));
        }
        if !order_no.trim().is_empty() {
            q = q.filter(orders::Column::OrderNo.contains(order_no.trim()));
        }
        self.page(q.order_by_desc(orders::Column::Id), page).await
    }

    async fn stats_for_user(
        &self,
        user_id: Id,
        scope: Scope,
        order_no: &str,
    ) -> Result<BTreeMap<String, i64>> {
        let mut q = orders::Entity::find()
            .select_only()
            .column(orders::Column::Status)
            .column_as(Expr::col(orders::Column::Id).count(), "count")
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::UserId.eq(user_id))
            .filter(orders::Column::ParentId.is_null())
            .filter(scope_cond(scope));
        if !order_no.trim().is_empty() {
            q = q.filter(orders::Column::OrderNo.contains(order_no.trim()));
        }
        Ok(q.group_by(orders::Column::Status)
            .into_model::<StatusCount>()
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|r| (r.status, r.count))
            .collect())
    }

    async fn list_admin(&self, f: &AdminOrderFilter) -> Result<Page<Order>> {
        let mut q = alive().filter(orders::Column::ParentId.is_null());
        if f.user_id > 0 {
            q = q.filter(orders::Column::UserId.eq(f.user_id));
        }
        if !f.user_keyword.trim().is_empty() {
            let ids = self.user_keyword_ids(f.user_keyword.trim()).await?;
            q = q.filter(orders::Column::UserId.is_in(ids));
        }
        if !f.status.trim().is_empty() {
            q = q.filter(orders::Column::Status.eq(f.status.trim()));
        }
        if !f.order_no.trim().is_empty() {
            q = q.filter(orders::Column::OrderNo.eq(f.order_no.trim()));
        }
        if !f.guest_email.trim().is_empty() {
            q = q.filter(orders::Column::GuestEmail.eq(f.guest_email.trim()));
        }
        if !f.product_keyword.trim().is_empty() {
            let ids = self
                .product_keyword_parent_ids(f.product_keyword.trim())
                .await?;
            q = q.filter(orders::Column::Id.is_in(ids));
        }
        if let Some(from) = f.created_from {
            q = q.filter(orders::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            q = q.filter(orders::Column::CreatedAt.lte(to));
        }
        match f.reseller.trim() {
            "" => {}
            "main" => q = q.filter(orders::Column::ResellerId.is_null()),
            "reseller" => q = q.filter(orders::Column::ResellerId.is_not_null()),
            raw => q = q.filter(orders::Column::ResellerId.eq(raw.parse::<Id>().unwrap_or(-1))),
        }
        if f.procurement_issue {
            q = q.filter(Expr::cust(PROCUREMENT_ISSUE_SQL));
        }
        let column = match f.sort_by.trim().to_ascii_lowercase().as_str() {
            "created_at" => orders::Column::CreatedAt,
            "updated_at" => orders::Column::UpdatedAt,
            "total_amount" => orders::Column::TotalAmount,
            _ => orders::Column::Id,
        };
        q = if f.sort_asc {
            q.order_by_asc(column)
        } else {
            q.order_by_desc(column)
        };
        self.page(q, f.page).await
    }

    async fn set_status(&self, id: Id, status: OrderStatus, now: DateTime<Utc>) -> Result<()> {
        ops::write_status(&self.db, id, status, now).await
    }

    async fn refunds_of(&self, order_ids: &[Id]) -> Result<Vec<RefundRecord>> {
        refund::records_of(&self.db, order_ids).await
    }

    async fn refund(&self, id: Id) -> Result<Option<RefundRecord>> {
        Ok(order_refund_records::Entity::find_by_id(id)
            .filter(order_refund_records::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(map::refund_to_domain))
    }

    async fn list_refunds(&self, f: &RefundFilter) -> Result<Page<RefundRecord>> {
        let mut q = order_refund_records::Entity::find()
            .filter(order_refund_records::Column::DeletedAt.is_null());
        if f.user_id > 0 {
            q = q.filter(order_refund_records::Column::UserId.eq(f.user_id));
        }
        if !f.user_keyword.trim().is_empty() {
            let ids = self.user_keyword_ids(f.user_keyword.trim()).await?;
            q = q.filter(order_refund_records::Column::UserId.is_in(ids));
        }
        if !f.guest_email.trim().is_empty() {
            q = q.filter(order_refund_records::Column::GuestEmail.eq(f.guest_email.trim()));
        }
        if !f.order_no.trim().is_empty() {
            let ids: Vec<Id> = orders::Entity::find()
                .select_only()
                .column(orders::Column::Id)
                .filter(orders::Column::OrderNo.eq(f.order_no.trim()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            q = q.filter(order_refund_records::Column::OrderId.is_in(ids));
        }
        if !f.product_keyword.trim().is_empty() {
            let parents = self
                .product_keyword_parent_ids(f.product_keyword.trim())
                .await?;
            let mut ids = parents.clone();
            ids.extend(
                orders::Entity::find()
                    .select_only()
                    .column(orders::Column::Id)
                    .filter(orders::Column::ParentId.is_in(parents))
                    .into_tuple::<Id>()
                    .all(&self.db)
                    .await
                    .dom()?,
            );
            q = q.filter(order_refund_records::Column::OrderId.is_in(ids));
        }
        if let Some(from) = f.created_from {
            q = q.filter(order_refund_records::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            q = q.filter(order_refund_records::Column::CreatedAt.lte(to));
        }
        let q = q.order_by_desc(order_refund_records::Column::Id);
        let total = q.clone().count(&self.db).await.dom()?;
        let items = q
            .offset(f.page.offset())
            .limit(f.page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(map::refund_to_domain)
            .collect();
        Ok(Page { items, total })
    }

    async fn payments_of(&self, order_id: Id) -> Result<Vec<Payment>> {
        Ok(payments::Entity::find()
            .filter(payments::Column::OrderId.eq(order_id))
            .filter(payments::Column::DeletedAt.is_null())
            .order_by_desc(payments::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(payment_to_domain)
            .collect())
    }

    async fn latest_pending_payment(
        &self,
        order_id: Id,
        now: DateTime<Utc>,
    ) -> Result<Option<Payment>> {
        let policies = [
            FeePolicy::None,
            FeePolicy::MerchantAbsorbed,
            FeePolicy::CustomerSurcharge,
        ];
        Ok(payments::Entity::find()
            .filter(payments::Column::DeletedAt.is_null())
            .filter(payments::Column::OrderId.eq(order_id))
            .filter(payments::Column::Status.is_in([
                PaymentStatus::Initiated.as_str(),
                PaymentStatus::Pending.as_str(),
            ]))
            .filter(payments::Column::SupersededAt.is_null())
            .filter(payments::Column::FeePolicy.is_in(policies.iter().map(|p| p.as_str())))
            .order_by_desc(payments::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .find(|p| {
                p.expired_at.is_none_or(|e| e > now)
                    && (!p.pay_url.trim().is_empty() || !p.qr_code.trim().is_empty())
            })
            .map(payment_to_domain))
    }

    async fn payment(&self, id: Id) -> Result<Option<Payment>> {
        Ok(payments::Entity::find_by_id(id)
            .filter(payments::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(payment_to_domain))
    }

    async fn channel(&self, id: Id) -> Result<Option<PaymentChannel>> {
        Ok(payment_channels::Entity::find_by_id(id)
            .filter(payment_channels::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(channel_to_domain))
    }

    async fn channels(&self, ids: &[Id]) -> Result<Vec<PaymentChannel>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        Ok(payment_channels::Entity::find()
            .filter(payment_channels::Column::Id.is_in(ids.to_vec()))
            .filter(payment_channels::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(channel_to_domain)
            .collect())
    }

    async fn active_channels(&self) -> Result<Vec<PaymentChannel>> {
        Ok(payment_channels::Entity::find()
            .filter(payment_channels::Column::DeletedAt.is_null())
            .filter(payment_channels::Column::IsActive.eq(true))
            .order_by_desc(payment_channels::Column::SortOrder)
            .order_by_asc(payment_channels::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(channel_to_domain)
            .collect())
    }

    async fn product_channel_ids(&self, product_ids: &[Id]) -> Result<Vec<String>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        products::Entity::find()
            .select_only()
            .column(products::Column::PaymentChannelIds)
            .filter(products::Column::Id.is_in(product_ids.to_vec()))
            .into_tuple::<String>()
            .all(&self.db)
            .await
            .dom()
    }

    async fn users(&self, ids: &[Id]) -> Result<HashMap<Id, UserBrief>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(users::Entity::find()
            .filter(users::Column::Id.is_in(ids.to_vec()))
            .filter(users::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|u| {
                (
                    u.id,
                    UserBrief {
                        email: u.email,
                        display_name: u.display_name,
                        locale: u.locale,
                    },
                )
            })
            .collect())
    }

    async fn telegram_user_id(&self, user_id: Id) -> Result<Option<String>> {
        if user_id <= 0 {
            return Ok(None);
        }
        Ok(user_oauth_identities::Entity::find()
            .filter(user_oauth_identities::Column::UserId.eq(user_id))
            .filter(user_oauth_identities::Column::Provider.eq(TELEGRAM_PROVIDER))
            .one(&self.db)
            .await
            .dom()?
            .map(|i| i.provider_user_id.trim().to_owned())
            .filter(|s| !s.is_empty()))
    }

    async fn coupon_code(&self, id: Id) -> Result<Option<String>> {
        Ok(coupons::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .dom()?
            .map(|c| c.code))
    }

    async fn promotion_names(&self, ids: &[Id]) -> Result<HashMap<Id, String>> {
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        Ok(promotions::Entity::find()
            .filter(promotions::Column::Id.is_in(ids.to_vec()))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(|p| (p.id, p.name))
            .collect())
    }

    async fn member_level_of(&self, user_id: Id) -> Result<Id> {
        Ok(users::Entity::find_by_id(user_id)
            .filter(users::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(|u| u.member_level_id)
            .unwrap_or(0))
    }
}
