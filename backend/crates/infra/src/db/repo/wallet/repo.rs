//! [`WalletRepo`] and [`WalletAdminLookup`] backed by the wallet tables.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, ExprTrait, Func, LikeExpr, Query};
use sea_orm::{
    ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use zs_domain::identity::user::User;
use zs_domain::marketing::gift_card::{WALLET_TXN_TYPE, redeem_reference, redeem_remark};
use zs_domain::wallet::ports::{BalanceChangeRequest, GiftCardRedemption, LedgerError};
use zs_domain::wallet::{
    Account, RechargeFilter, RechargeOrder, Transaction, TransactionFilter, UserBrief,
    WalletAdminLookup, WalletRepo,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::ledger::{self, account_to_domain, txn_to_domain};
use crate::db::entity::{
    payment_channels, payments, users, wallet_accounts, wallet_recharge_orders, wallet_transactions,
};
use crate::db::repo::marketing::gift_card::{lock_for_redeem, mark_redeemed};
use crate::db::repo::support::DbResultExt;

/// Rows per `IN (…)` chunk.
const CHUNK: usize = 500;
/// Escape character of LIKE patterns (portable across the three backends).
const LIKE_ESCAPE: char = '!';

/// `%text%` with LIKE metacharacters escaped.
pub(crate) fn contains(text: &str) -> LikeExpr {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('%');
    for c in text.chars() {
        if matches!(c, '%' | '_' | LIKE_ESCAPE) {
            escaped.push(LIKE_ESCAPE);
        }
        escaped.push(c);
    }
    escaped.push('%');
    LikeExpr::new(escaped).escape(LIKE_ESCAPE)
}

/// Case-insensitive `LOWER(col) LIKE %text%` (DB-10).
pub(crate) fn ilike<C: sea_orm::sea_query::IntoColumnRef>(col: C, text: &str) -> Expr {
    Expr::expr(Func::lower(Expr::col(col))).like(contains(&text.to_lowercase()))
}

/// SeaORM implementation of the wallet ports.
#[derive(Debug, Clone)]
pub struct SeaWalletRepo {
    db: DatabaseConnection,
}

impl SeaWalletRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

pub(crate) fn recharge_to_domain(m: wallet_recharge_orders::Model) -> RechargeOrder {
    RechargeOrder {
        id: m.id,
        recharge_no: m.recharge_no,
        user_id: m.user_id,
        payment_id: m.payment_id,
        channel_id: m.channel_id,
        provider_type: m.provider_type,
        channel_type: m.channel_type,
        interaction_mode: m.interaction_mode,
        amount: Amount::new(m.amount),
        payable_amount: Amount::new(m.payable_amount),
        fee_rate: Amount::new(m.fee_rate),
        fee_amount: Amount::new(m.fee_amount),
        currency: m.currency,
        status: m.status,
        remark: m.remark,
        paid_at: m.paid_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn recharge_condition(f: &RechargeFilter) -> Condition {
    use wallet_recharge_orders::Column as C;
    let mut cond = Condition::all().add(C::DeletedAt.is_null());
    if !f.recharge_no.trim().is_empty() {
        cond = cond.add(C::RechargeNo.like(contains(f.recharge_no.trim())));
    }
    if f.user_id > 0 {
        cond = cond.add(C::UserId.eq(f.user_id));
    }
    if !f.user_keyword.trim().is_empty() {
        let keyword = f.user_keyword.trim();
        let users = Query::select()
            .column(users::Column::Id)
            .from(users::Entity)
            .cond_where(
                Condition::any()
                    .add(ilike((users::Entity, users::Column::Email), keyword))
                    .add(ilike((users::Entity, users::Column::DisplayName), keyword)),
            )
            .to_owned();
        cond = cond.add(C::UserId.in_subquery(users));
    }
    if f.payment_id > 0 {
        cond = cond.add(C::PaymentId.eq(f.payment_id));
    }
    if f.channel_id > 0 {
        cond = cond.add(C::ChannelId.eq(f.channel_id));
    }
    for (col, value) in [
        (C::ProviderType, &f.provider_type),
        (C::ChannelType, &f.channel_type),
        (C::Status, &f.status),
    ] {
        if !value.trim().is_empty() {
            cond = cond.add(col.eq(value.trim()));
        }
    }
    for (col, from, to) in [
        (C::CreatedAt, f.created_from, f.created_to),
        (C::PaidAt, f.paid_from, f.paid_to),
    ] {
        if let Some(from) = from {
            cond = cond.add(col.gte(from));
        }
        if let Some(to) = to {
            cond = cond.add(col.lte(to));
        }
    }
    cond
}

#[async_trait]
impl WalletRepo for SeaWalletRepo {
    async fn account(&self, user_id: Id) -> Result<Account> {
        if user_id <= 0 {
            return Err(zs_domain::Error::invalid());
        }
        let row = ledger::ensure_account(&self.db, user_id, false, Utc::now()).await?;
        Ok(account_to_domain(row))
    }

    async fn balances(&self, user_ids: &[Id]) -> Result<HashMap<Id, Amount>> {
        let mut out = HashMap::new();
        for chunk in user_ids.chunks(CHUNK) {
            let rows = wallet_accounts::Entity::find()
                .filter(wallet_accounts::Column::UserId.is_in(chunk.to_vec()))
                .filter(wallet_accounts::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?;
            out.extend(
                rows.into_iter()
                    .map(|r| (r.user_id, Amount::new(r.balance))),
            );
        }
        Ok(out)
    }

    async fn list_transactions(
        &self,
        f: &TransactionFilter,
        page: PageRequest,
    ) -> Result<Page<Transaction>> {
        use wallet_transactions::Column as C;
        let mut cond = Condition::all().add(C::DeletedAt.is_null());
        if f.user_id > 0 {
            cond = cond.add(C::UserId.eq(f.user_id));
        }
        if f.order_id > 0 {
            cond = cond.add(C::OrderId.eq(f.order_id));
        }
        if !f.kind.is_empty() {
            cond = cond.add(C::Type.eq(f.kind.clone()));
        }
        if !f.direction.is_empty() {
            cond = cond.add(C::Direction.eq(f.direction.clone()));
        }
        if let Some(from) = f.created_from {
            cond = cond.add(C::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(C::CreatedAt.lte(to));
        }
        let q = wallet_transactions::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(C::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(txn_to_domain).collect(),
            total,
        })
    }

    async fn change_balance(
        &self,
        change: &BalanceChangeRequest,
    ) -> std::result::Result<(Account, Transaction), LedgerError> {
        let txn = self.db.begin().await.dom()?;
        let out = ledger::change_balance(&txn, change, Utc::now()).await?;
        txn.commit().await.dom()?;
        Ok(out)
    }

    async fn list_recharges(
        &self,
        filter: &RechargeFilter,
        page: PageRequest,
    ) -> Result<Page<RechargeOrder>> {
        let q = wallet_recharge_orders::Entity::find().filter(recharge_condition(filter));
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(wallet_recharge_orders::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(recharge_to_domain).collect(),
            total,
        })
    }

    async fn recharge_stats(
        &self,
        user_id: Id,
        recharge_no: &str,
    ) -> Result<BTreeMap<String, i64>> {
        let filter = RechargeFilter {
            user_id,
            recharge_no: recharge_no.to_owned(),
            ..RechargeFilter::default()
        };
        let rows: Vec<(String, i64)> = wallet_recharge_orders::Entity::find()
            .select_only()
            .column(wallet_recharge_orders::Column::Status)
            .column_as(wallet_recharge_orders::Column::Id.count(), "total")
            .filter(recharge_condition(&filter))
            .group_by(wallet_recharge_orders::Column::Status)
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        Ok(rows.into_iter().collect())
    }

    async fn recharge_by_no(
        &self,
        user_id: Id,
        recharge_no: &str,
    ) -> Result<Option<RechargeOrder>> {
        if user_id <= 0 || recharge_no.trim().is_empty() {
            return Ok(None);
        }
        Ok(wallet_recharge_orders::Entity::find()
            .filter(wallet_recharge_orders::Column::UserId.eq(user_id))
            .filter(wallet_recharge_orders::Column::RechargeNo.eq(recharge_no.trim()))
            .filter(wallet_recharge_orders::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(recharge_to_domain))
    }

    async fn recharge_by_payment(
        &self,
        payment_id: Id,
        user_id: Id,
    ) -> Result<Option<RechargeOrder>> {
        if payment_id <= 0 || user_id <= 0 {
            return Ok(None);
        }
        Ok(wallet_recharge_orders::Entity::find()
            .filter(wallet_recharge_orders::Column::PaymentId.eq(payment_id))
            .filter(wallet_recharge_orders::Column::UserId.eq(user_id))
            .filter(wallet_recharge_orders::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(recharge_to_domain))
    }

    async fn redeem_gift_card(
        &self,
        user_id: Id,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<GiftCardRedemption> {
        // One transaction: lock card → credit wallet → flip card (conditional update).
        let txn = self.db.begin().await.dom()?;
        let mut card = lock_for_redeem(&txn, code, now).await?;
        let change = BalanceChangeRequest {
            user_id,
            delta: card.amount,
            kind: WALLET_TXN_TYPE.to_owned(),
            reference: redeem_reference(card.id),
            remark: redeem_remark(&card.code),
            currency: card.currency.clone(),
            operator_admin_id: None,
            order_id: None,
        };
        let (account, transaction) = ledger::credit(&txn, &change, now).await?;
        mark_redeemed(&txn, card.id, user_id, Some(transaction.id), now).await?;
        txn.commit().await.dom()?;
        card.status = zs_domain::marketing::gift_card::GiftCardStatus::Redeemed
            .as_str()
            .to_owned();
        card.redeemed_user_id = Some(user_id);
        card.redeemed_at = Some(now);
        card.wallet_txn_id = Some(transaction.id);
        card.updated_at = now;
        Ok(GiftCardRedemption {
            card,
            account,
            transaction,
        })
    }
}

#[async_trait]
impl WalletAdminLookup for SeaWalletRepo {
    async fn user(&self, id: Id) -> Result<Option<User>> {
        if id <= 0 {
            return Ok(None);
        }
        Ok(users::Entity::find_by_id(id)
            .filter(users::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(crate::db::repo::identity::user::to_domain))
    }

    async fn user_briefs(&self, ids: &[Id]) -> Result<Vec<UserBrief>> {
        let mut out = Vec::with_capacity(ids.len());
        for chunk in ids.chunks(CHUNK) {
            let rows: Vec<(Id, String, String)> = users::Entity::find()
                .select_only()
                .columns([
                    users::Column::Id,
                    users::Column::Email,
                    users::Column::DisplayName,
                ])
                .filter(users::Column::Id.is_in(chunk.to_vec()))
                .filter(users::Column::DeletedAt.is_null())
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            out.extend(rows.into_iter().map(|(id, email, display_name)| UserBrief {
                id,
                email,
                display_name,
            }));
        }
        Ok(out)
    }

    async fn channel_names(&self, ids: &[Id]) -> Result<HashMap<Id, String>> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(CHUNK) {
            let rows: Vec<(Id, String)> = payment_channels::Entity::find()
                .select_only()
                .columns([payment_channels::Column::Id, payment_channels::Column::Name])
                .filter(payment_channels::Column::Id.is_in(chunk.to_vec()))
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            out.extend(rows);
        }
        Ok(out)
    }

    async fn payment_statuses(&self, ids: &[Id]) -> Result<HashMap<Id, String>> {
        let mut out = HashMap::new();
        for chunk in ids.chunks(CHUNK) {
            let rows: Vec<(Id, String)> = payments::Entity::find()
                .select_only()
                .columns([payments::Column::Id, payments::Column::Status])
                .filter(payments::Column::Id.is_in(chunk.to_vec()))
                .filter(payments::Column::DeletedAt.is_null())
                .into_tuple()
                .all(&self.db)
                .await
                .dom()?;
            out.extend(rows);
        }
        Ok(out)
    }
}
