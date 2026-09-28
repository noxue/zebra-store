//! Wallet ledger: balance movements that run inside a caller-provided transaction.
//!
//! This is the `WalletLedger` port for other groups (order payment/refund, underpaid
//! credits, gift cards, recharges). Every function takes `&impl ConnectionTrait` — pass the
//! caller's `&DatabaseTransaction` and never the pool while a transaction is open (DB-01).
//!
//! Guarantees:
//! - the account row is locked (`FOR UPDATE` on MySQL/PostgreSQL) and created when missing;
//! - the balance is written with a compare-and-set on the value read under the lock
//!   (`WHERE id = ? AND balance = <before>`, rows affected must be 1), so two concurrent
//!   debits can never both spend the same money;
//! - `wallet_transactions.reference` is the idempotency key: a movement whose reference
//!   already exists is not applied again (the existing transaction is returned).

use chrono::{DateTime, Utc};
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, PaginatorTrait, QueryFilter,
    QuerySelect, Set,
};
use zs_domain::wallet::ports::{BalanceChangeRequest, LedgerError};
use zs_domain::wallet::rules::{
    ORDER_PAY_REMARK, ORDER_RELEASE_REMARK, allocation_reference, clean_remark, normalize_currency,
    order_debit_amount, plan_change,
};
use zs_domain::wallet::{Account, Transaction, txn_type};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;

use crate::db::entity::{wallet_accounts, wallet_transactions};
use crate::db::repo::support::DbResultExt;

type LedgerResult<T> = Result<T, LedgerError>;

pub(crate) fn account_to_domain(m: wallet_accounts::Model) -> Account {
    Account {
        id: m.id,
        user_id: m.user_id,
        balance: Amount::new(m.balance),
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

pub(crate) fn txn_to_domain(m: wallet_transactions::Model) -> Transaction {
    Transaction {
        id: m.id,
        user_id: m.user_id,
        operator_admin_id: m.operator_admin_id,
        order_id: m.order_id,
        kind: m.type_,
        direction: m.direction,
        amount: Amount::new(m.amount),
        balance_before: Amount::new(m.balance_before),
        balance_after: Amount::new(m.balance_after),
        currency: m.currency,
        reference: m.reference,
        remark: m.remark,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

async fn find_account<C: ConnectionTrait>(
    conn: &C,
    user_id: Id,
    lock: bool,
) -> Result<Option<wallet_accounts::Model>, sea_orm::DbErr> {
    let mut q = wallet_accounts::Entity::find()
        .filter(wallet_accounts::Column::UserId.eq(user_id))
        .filter(wallet_accounts::Column::DeletedAt.is_null());
    if lock {
        q = q.lock_exclusive();
    }
    q.one(conn).await
}

/// Returns the user's account, creating a zero-balance one when missing. With `lock`,
/// the row is locked for the rest of the caller's transaction (`ensureAccountForUpdate`).
pub async fn ensure_account<C: ConnectionTrait>(
    conn: &C,
    user_id: Id,
    lock: bool,
    now: DateTime<Utc>,
) -> zs_domain::Result<wallet_accounts::Model> {
    if let Some(row) = find_account(conn, user_id, lock).await.dom()? {
        return Ok(row);
    }
    let fresh = wallet_accounts::ActiveModel {
        user_id: Set(user_id),
        balance: Set(rust_decimal::Decimal::ZERO),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    // The unique index on user_id turns a concurrent create into a no-op.
    let mut on_conflict = OnConflict::new();
    on_conflict.do_nothing_on([wallet_accounts::Column::Id]);
    wallet_accounts::Entity::insert(fresh)
        .on_conflict(on_conflict)
        .try_insert()
        .exec_without_returning(conn)
        .await
        .dom()?;
    find_account(conn, user_id, lock)
        .await
        .dom()?
        .ok_or_else(|| Error::internal_msg("wallet account create failed"))
}

/// Transaction with `reference`, if any (soft-deleted rows included: the unique index
/// covers them too).
pub async fn transaction_by_reference<C: ConnectionTrait>(
    conn: &C,
    reference: &str,
) -> zs_domain::Result<Option<Transaction>> {
    Ok(wallet_transactions::Entity::find()
        .filter(wallet_transactions::Column::Reference.eq(reference))
        .one(conn)
        .await
        .dom()?
        .map(txn_to_domain))
}

/// Number of transactions of `kind` recorded for an order (allocation rounds, PAY-02).
pub async fn count_order_transactions<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    kind: &str,
) -> zs_domain::Result<u64> {
    wallet_transactions::Entity::find()
        .filter(wallet_transactions::Column::OrderId.eq(order_id))
        .filter(wallet_transactions::Column::Type.eq(kind))
        .filter(wallet_transactions::Column::DeletedAt.is_null())
        .count(conn)
        .await
        .dom()
}

/// Applies `req.delta` to the user's balance and records the transaction.
///
/// Positive deltas credit, negative deltas debit (never below zero). When a transaction
/// with `req.reference` already exists nothing changes and it is returned instead.
pub async fn change_balance<C: ConnectionTrait>(
    conn: &C,
    req: &BalanceChangeRequest,
    now: DateTime<Utc>,
) -> LedgerResult<(Account, Transaction)> {
    if req.user_id <= 0 {
        return Err(LedgerError::NotSupportedForGuest);
    }
    let reference = req.reference.trim();
    if reference.is_empty() {
        return Err(LedgerError::ReferenceRequired);
    }
    let account = ensure_account(conn, req.user_id, true, now).await?;
    if let Some(existing) = transaction_by_reference(conn, reference).await? {
        return Ok((account_to_domain(account), existing));
    }
    let plan = plan_change(Amount::new(account.balance), req.delta)?;
    let updated = wallet_accounts::Entity::update_many()
        .col_expr(
            wallet_accounts::Column::Balance,
            Expr::value(plan.after.decimal()),
        )
        .col_expr(wallet_accounts::Column::UpdatedAt, Expr::value(now))
        .filter(wallet_accounts::Column::Id.eq(account.id))
        .filter(wallet_accounts::Column::Balance.eq(plan.before.decimal()))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if updated != 1 {
        return Err(LedgerError::Store(Error::internal_msg(
            "wallet balance changed concurrently",
        )));
    }
    let row = wallet_transactions::ActiveModel {
        user_id: Set(req.user_id),
        operator_admin_id: Set(req.operator_admin_id),
        order_id: Set(req.order_id),
        type_: Set(req.kind.clone()),
        direction: Set(plan.direction.to_owned()),
        amount: Set(plan.amount.decimal()),
        balance_before: Set(plan.before.decimal()),
        balance_after: Set(plan.after.decimal()),
        currency: Set(normalize_currency(&req.currency)),
        reference: Set(reference.to_owned()),
        remark: Set(req.remark.clone()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    let account = Account {
        balance: plan.after,
        updated_at: now,
        ..account_to_domain(account)
    };
    Ok((account, txn_to_domain(row)))
}

/// Credits a positive amount (`CreditInTransaction`); idempotent per reference.
pub async fn credit<C: ConnectionTrait>(
    conn: &C,
    req: &BalanceChangeRequest,
    now: DateTime<Utc>,
) -> LedgerResult<(Account, Transaction)> {
    if !req.delta.is_positive() {
        return Err(LedgerError::InvalidAmount);
    }
    change_balance(conn, req, now).await
}

/// Debits exactly `amount` (positive); fails with `InsufficientBalance` otherwise.
pub async fn debit<C: ConnectionTrait>(
    conn: &C,
    req: &BalanceChangeRequest,
    amount: Amount,
    now: DateTime<Utc>,
) -> LedgerResult<(Account, Transaction)> {
    if !amount.is_positive() {
        return Err(LedgerError::InvalidAmount);
    }
    let req = BalanceChangeRequest {
        delta: -amount,
        ..req.clone()
    };
    change_balance(conn, &req, now).await
}

/// Order balance use (`ApplyOrderBalance`): debits `min(balance, total)` with a
/// round-aware reference (`order:<id>:order_pay[:n]`, PAY-02). Returns the debited
/// amount (zero when the balance is empty). The caller persists the order allocation.
pub async fn apply_order_balance<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    user_id: Id,
    total: Amount,
    currency: &str,
    now: DateTime<Utc>,
) -> LedgerResult<Amount> {
    if user_id <= 0 {
        return Err(LedgerError::NotSupportedForGuest);
    }
    if !total.is_positive() {
        return Ok(Amount::ZERO);
    }
    let account = ensure_account(conn, user_id, true, now).await?;
    let deduct = order_debit_amount(Amount::new(account.balance), total);
    if !deduct.is_positive() {
        return Ok(Amount::ZERO);
    }
    let rounds = count_order_transactions(conn, order_id, txn_type::ORDER_PAY).await?;
    let req = BalanceChangeRequest {
        user_id,
        delta: -deduct,
        kind: txn_type::ORDER_PAY.to_owned(),
        reference: allocation_reference(order_id, txn_type::ORDER_PAY, rounds),
        remark: ORDER_PAY_REMARK.to_owned(),
        currency: currency.to_owned(),
        operator_admin_id: None,
        order_id: Some(order_id),
    };
    let (_, txn) = change_balance(conn, &req, now).await?;
    Ok(txn.amount)
}

/// An order wallet allocation to return (`OrderReleaseInput`).
#[derive(Debug, Clone, Copy)]
pub struct Release<'a> {
    pub order_id: Id,
    pub user_id: Id,
    pub amount: Amount,
    /// `order_refund`, `admin_refund`, …
    pub kind: &'a str,
    pub currency: &'a str,
    pub remark: &'a str,
}

/// Returns an order's wallet allocation (`ReleaseOrderBalance`) with a round-aware
/// reference (`order:<id>:<kind>[:n]`).
///
/// The caller must first clear the order's `wallet_paid_amount` with a conditional update
/// in the same transaction (the "claim"), and only call this when the claim won.
pub async fn release_order_balance<C: ConnectionTrait>(
    conn: &C,
    r: &Release<'_>,
    now: DateTime<Utc>,
) -> LedgerResult<Amount> {
    if r.user_id <= 0 || !r.amount.is_positive() {
        return Ok(Amount::ZERO);
    }
    let rounds = count_order_transactions(conn, r.order_id, r.kind).await?;
    let req = BalanceChangeRequest {
        user_id: r.user_id,
        delta: r.amount,
        kind: r.kind.to_owned(),
        reference: allocation_reference(r.order_id, r.kind, rounds),
        remark: clean_remark(r.remark, ORDER_RELEASE_REMARK),
        currency: r.currency.to_owned(),
        operator_admin_id: None,
        order_id: Some(r.order_id),
    };
    let (_, txn) = change_balance(conn, &req, now).await?;
    Ok(txn.amount)
}
