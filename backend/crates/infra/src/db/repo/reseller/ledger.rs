//! [`LedgerRepo`]: profit / claw-back entries, balance caches and withdrawals
//! (`accounting_*.go`, `gormstore/{ledger,balance,withdraw}.go`).
//!
//! The `*_in` functions are the transactional building blocks; the order group
//! calls [`post_order_profit_in`] / [`deduct_refund_in`] / [`create_order_snapshot_in`]
//! with its own payment / refund / create-order transaction.

use std::collections::{BTreeSet, HashMap};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sea_orm::sea_query::{Expr, OnConflict};
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, ConnectionTrait, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use serde_json::{Value, json};
use zs_domain::reseller::accounting::{
    RefundDeductInput, WithdrawDraft, allocate_refund, available_at, balance_blocks_withdraw,
    balance_cache, deduct_status, fixed, plan_withdraw, profit_key, refund_key, split_key,
};
use zs_domain::reseller::ports::{
    FinanceFilter, LedgerRepo, OrderPaid, OrderRefunded, WithdrawAction,
};
use zs_domain::reseller::pricing::OrderPricingContext;
use zs_domain::reseller::{
    AdminRef, BalanceAccount, BalanceStatus, LedgerEntry, LedgerStatus, LedgerType, OrderBrief,
    OrderSnapshot, WithdrawRequest, WithdrawStatus, keys,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::{
    SeaResellerStore, balance_model, keyword_profile_ids, ledger_model, load_profiles,
    snapshot_model, user_profile_ids, withdraw_model,
};
use crate::db::entity::{
    admins, orders, reseller_balance_accounts as balances, reseller_ledger_entries as ledger,
    reseller_order_snapshots as snapshots, reseller_withdraw_requests as withdraws,
};
use crate::db::repo::catalog::sql::ilike_sql;
use crate::db::repo::support::DbResultExt;

/// `ON CONFLICT DO NOTHING` without a target, so it also matches the partial unique
/// indexes of the data migration (MySQL: `ON DUPLICATE KEY UPDATE id = id`).
fn ignore_duplicates<C: sea_orm::sea_query::IntoIden>(pk: C) -> OnConflict {
    let mut on_conflict = OnConflict::new();
    on_conflict.do_nothing_on([pk]);
    on_conflict
}

fn invalid_snapshot() -> Error {
    Error::internal_msg("reseller ledger invalid snapshot")
}

/// Live snapshot of an order.
pub async fn snapshot_by_order_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
) -> Result<Option<OrderSnapshot>> {
    Ok(snapshots::Entity::find()
        .filter(snapshots::Column::OrderId.eq(order_id))
        .filter(snapshots::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?
        .map(snapshot_model))
}

/// Persists the reseller snapshot of a newly created order (`CreateOrderSnapshot`).
pub async fn create_order_snapshot_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    ctx: &OrderPricingContext,
    now: DateTime<Utc>,
) -> Result<OrderSnapshot> {
    if order_id == 0 || ctx.reseller_id == 0 {
        return Err(invalid_snapshot());
    }
    let row = snapshots::ActiveModel {
        order_id: Set(order_id),
        reseller_id: Set(ctx.reseller_id),
        domain: Set(ctx.domain.clone()),
        currency: Set(ctx.currency.clone()),
        reseller_user_id: Set(ctx.reseller_user_id),
        buyer_user_id: Set(ctx.buyer_user_id),
        base_amount: Set(Amount::new(ctx.base_amount).decimal()),
        reseller_amount: Set(Amount::new(ctx.reseller_amount).decimal()),
        profit_amount: Set(Amount::new(ctx.profit_amount).decimal()),
        profit_eligible: Set(ctx.profit_eligible),
        profit_block_reason: Set(ctx.profit_block_reason.clone()),
        pricing_snapshot_json: Set(Some(ctx.pricing_snapshot())),
        risk_snapshot_json: Set(Some(ctx.risk_snapshot())),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    Ok(snapshot_model(row))
}

/// Locks (creating when missing) the balance account of a reseller + currency.
async fn balance_account_in<C: ConnectionTrait>(
    conn: &C,
    reseller_id: Id,
    currency: &str,
    now: DateTime<Utc>,
) -> Result<balances::Model> {
    let find = || {
        balances::Entity::find()
            .filter(balances::Column::ResellerId.eq(reseller_id))
            .filter(balances::Column::Currency.eq(currency))
            .filter(balances::Column::DeletedAt.is_null())
            .lock_exclusive()
    };
    if let Some(row) = find().one(conn).await.dom()? {
        return Ok(row);
    }
    let fresh = balances::ActiveModel {
        reseller_id: Set(reseller_id),
        currency: Set(currency.to_owned()),
        status: Set(BalanceStatus::Normal.as_str().to_owned()),
        available_amount_cache: Set(Decimal::ZERO),
        locked_amount_cache: Set(Decimal::ZERO),
        negative_amount_cache: Set(Decimal::ZERO),
        last_ledger_entry_id: Set(0),
        risk_note: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };
    // The partial unique index (reseller_id, currency) makes a concurrent create a no-op.
    balances::Entity::insert(fresh)
        .on_conflict(ignore_duplicates(balances::Column::Id))
        .try_insert()
        .exec_without_returning(conn)
        .await
        .dom()?;
    find().one(conn).await.dom()?.ok_or_else(invalid_snapshot)
}

/// Sums amounts of the reseller's entries in `currency` per status (exact decimals).
async fn sums_by_status<C: ConnectionTrait>(
    conn: &C,
    reseller_id: Id,
    currency: &str,
    statuses: &[LedgerStatus],
) -> Result<HashMap<LedgerStatus, Decimal>> {
    let rows: Vec<(Decimal, String)> = ledger::Entity::find()
        .select_only()
        .column(ledger::Column::Amount)
        .column(ledger::Column::Status)
        .filter(ledger::Column::ResellerId.eq(reseller_id))
        .filter(ledger::Column::Currency.eq(currency))
        .filter(ledger::Column::Status.is_in(statuses.iter().map(|s| s.as_str())))
        .filter(ledger::Column::DeletedAt.is_null())
        .into_tuple()
        .all(conn)
        .await
        .dom()?;
    let mut out = HashMap::new();
    for (amount, status) in rows {
        *out.entry(LedgerStatus::from_db(&status))
            .or_insert(Decimal::ZERO) += amount;
    }
    Ok(out)
}

/// Recomputes the balance caches from `available` / `locked` entries (RSL-02, RSL-04).
pub async fn refresh_balance_in<C: ConnectionTrait>(
    conn: &C,
    reseller_id: Id,
    currency: &str,
    now: DateTime<Utc>,
) -> Result<()> {
    let currency = currency.trim();
    if reseller_id == 0 || currency.is_empty() {
        return Ok(());
    }
    let account = balance_account_in(conn, reseller_id, currency, now).await?;
    let sums = sums_by_status(
        conn,
        reseller_id,
        currency,
        &[LedgerStatus::Available, LedgerStatus::Locked],
    )
    .await?;
    let get = |s| sums.get(&s).copied().unwrap_or_default();
    let cache = balance_cache(
        BalanceStatus::from_db(&account.status),
        get(LedgerStatus::Available),
        get(LedgerStatus::Locked),
    );
    balances::ActiveModel {
        id: Set(account.id),
        status: Set(cache.status.as_str().to_owned()),
        available_amount_cache: Set(cache.available.decimal()),
        locked_amount_cache: Set(cache.locked.decimal()),
        negative_amount_cache: Set(cache.negative.decimal()),
        updated_at: Set(now),
        ..Default::default()
    }
    .update(conn)
    .await
    .dom()?;
    Ok(())
}

/// Inserts an entry unless its idempotency key exists; `true` when inserted.
async fn insert_entry_in<C: ConnectionTrait>(conn: &C, entry: ledger::ActiveModel) -> Result<bool> {
    crate::db::repo::support::insert_if_absent(conn, entry, ledger::Column::Id).await
}

#[expect(clippy::too_many_arguments, reason = "mirrors the ledger columns")]
fn entry(
    reseller_id: Id,
    order_id: Option<Id>,
    kind: LedgerType,
    amount: Decimal,
    currency: &str,
    key: String,
    metadata: Value,
    status: LedgerStatus,
    available_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> ledger::ActiveModel {
    ledger::ActiveModel {
        reseller_id: Set(reseller_id),
        order_id: Set(order_id),
        type_: Set(kind.as_str().to_owned()),
        amount: Set(Amount::new(amount).decimal()),
        currency: Set(currency.to_owned()),
        idempotency_key: Set(key),
        metadata_json: Set(Some(metadata)),
        status: Set(status.as_str().to_owned()),
        available_at: Set(available_at),
        withdraw_request_id: Set(None),
        remark: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
}

/// Posts the reseller profit of a paid order (`PostOrderProfit`, RSL-05): skipped
/// without snapshot, for ineligible (self-dealing) or non-positive profit; idempotent
/// on `order_profit:{order_id}`.
pub async fn post_order_profit_in<C: ConnectionTrait>(
    conn: &C,
    event: &OrderPaid,
    now: DateTime<Utc>,
    confirm_days: i64,
) -> Result<bool> {
    if event.order_id == 0 || event.reseller_id.unwrap_or(0) == 0 {
        return Ok(false);
    }
    let Some(snapshot) = snapshot_by_order_in(conn, event.order_id).await? else {
        tracing::warn!(order_id = event.order_id, order_no = %event.order_no, "reseller accounting: missing snapshot, skip");
        return Ok(false);
    };
    let profit = snapshot.profit_amount.decimal();
    if !snapshot.profit_eligible || profit <= Decimal::ZERO {
        return Ok(false);
    }
    let mut currency = snapshot.currency.trim().to_owned();
    if currency.is_empty() {
        event.currency.trim().clone_into(&mut currency);
    }
    if currency.is_empty() {
        return Err(invalid_snapshot());
    }
    let mut metadata = json!({
        "order_no": event.order_no,
        "reseller_domain": snapshot.domain,
        "wallet_paid_amount": event.wallet_paid_amount,
        "online_paid_amount": event.online_paid_amount,
        "snapshot_id": snapshot.id,
        "profit_block_reason": snapshot.profit_block_reason,
    });
    if let Some(p) = &event.payment {
        metadata["payment_id"] = json!(p.id);
        metadata["payment_channel_id"] = json!(p.channel_id);
        metadata["payment_amount"] = json!(p.amount);
        metadata["payment_status"] = json!(p.status);
    }
    let created = insert_entry_in(
        conn,
        entry(
            snapshot.reseller_id,
            Some(event.order_id),
            LedgerType::OrderProfit,
            profit,
            &currency,
            profit_key(event.order_id),
            metadata,
            LedgerStatus::PendingConfirm,
            Some(available_at(now, confirm_days)),
            now,
        ),
    )
    .await?;
    if created {
        refresh_balance_in(conn, snapshot.reseller_id, &currency, now).await?;
    }
    Ok(created)
}

/// Claws back profit for a refund (`HandleRefundDeduct`, RSL-01 / RSL-04): ratio on
/// the full order amount, cumulative deduction capped at the profit, status aligned
/// with the order's profit entry; idempotent on `refund_deduct:{refund_record_id}`.
pub async fn deduct_refund_in<C: ConnectionTrait>(
    conn: &C,
    event: &OrderRefunded,
    now: DateTime<Utc>,
    confirm_days: i64,
) -> Result<bool> {
    if event.refund_record_id == 0
        || event.reseller_id.unwrap_or(0) == 0
        || !event.refund_amount.is_positive()
    {
        return Ok(false);
    }
    // Serialize claw-backs of one order (no-op lock on SQLite, whose writers are serialized).
    let Some(row) = snapshots::Entity::find()
        .filter(snapshots::Column::OrderId.eq(event.order_id))
        .filter(snapshots::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
    else {
        tracing::warn!(
            order_id = event.order_id,
            refund_record_id = event.refund_record_id,
            "reseller refund: missing snapshot, skip"
        );
        return Ok(false);
    };
    let snapshot = snapshot_model(row);
    if !snapshot.profit_eligible || !snapshot.profit_amount.is_positive() {
        return Ok(false);
    }
    let order_amount = if snapshot.reseller_amount.is_positive() {
        snapshot.reseller_amount
    } else {
        event.order_total_amount
    };
    let earlier: Vec<Decimal> = ledger::Entity::find()
        .select_only()
        .column(ledger::Column::Amount)
        .filter(ledger::Column::OrderId.eq(event.order_id))
        .filter(ledger::Column::Type.eq(LedgerType::RefundDeduct.as_str()))
        .filter(ledger::Column::DeletedAt.is_null())
        .into_tuple()
        .all(conn)
        .await
        .dom()?;
    let Some(deduction) = zs_domain::reseller::accounting::refund_deduction(RefundDeductInput {
        profit: snapshot.profit_amount.decimal(),
        order_amount: order_amount.decimal(),
        refund_amount: event.refund_amount.decimal(),
        refunded_before: event.refunded_before.decimal(),
        deducted_so_far: earlier.iter().copied().sum(),
    })?
    else {
        return Ok(false);
    };
    let profit_entry = ledger::Entity::find()
        .filter(ledger::Column::IdempotencyKey.eq(profit_key(event.order_id)))
        .filter(ledger::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?;
    let (status, deduct_available_at) = deduct_status(
        profit_entry
            .as_ref()
            .map(|e| LedgerStatus::from_db(&e.status)),
        profit_entry.as_ref().and_then(|e| e.available_at),
        now,
        confirm_days,
    );
    let currency = [
        &snapshot.currency,
        &event.refund_currency,
        &event.order_currency,
    ]
    .into_iter()
    .map(|c| c.trim())
    .find(|c| !c.is_empty())
    .ok_or_else(invalid_snapshot)?
    .to_owned();
    let metadata = json!({
        "refund_record_id": event.refund_record_id,
        "refund_type": event.refund_type,
        "refund_amount": fixed(deduction.refund_amount, 2),
        "refunded_before": event.refunded_before,
        "refund_allocation_json": {
            "refund_record_id": event.refund_record_id,
            "order_id": event.order_id,
            "refund_amount": fixed(deduction.refund_amount, 2),
            "order_amount": order_amount,
            "items": allocate_refund(&snapshot.pricing_snapshot_json, &deduction),
        },
        "snapshot_id": snapshot.id,
        "deduct_status": status.as_str(),
    });
    let created = insert_entry_in(
        conn,
        entry(
            snapshot.reseller_id,
            Some(event.order_id),
            LedgerType::RefundDeduct,
            -deduction.deduct,
            &currency,
            refund_key(event.refund_record_id),
            metadata,
            status,
            deduct_available_at,
            now,
        ),
    )
    .await?;
    refresh_balance_in(conn, snapshot.reseller_id, &currency, now).await?;
    Ok(created)
}

/// Confirms due `pending_confirm` entries and refreshes every affected balance in the
/// same transaction (RSL-04). The conditional UPDATE makes concurrent runs idempotent.
pub async fn confirm_due_in<C: ConnectionTrait>(conn: &C, now: DateTime<Utc>) -> Result<u64> {
    let due = || {
        Condition::all()
            .add(ledger::Column::Status.eq(LedgerStatus::PendingConfirm.as_str()))
            .add(ledger::Column::AvailableAt.is_not_null())
            .add(ledger::Column::AvailableAt.lte(now))
            .add(ledger::Column::DeletedAt.is_null())
    };
    let scopes: BTreeSet<(Id, String)> = ledger::Entity::find()
        .select_only()
        .column(ledger::Column::ResellerId)
        .column(ledger::Column::Currency)
        .filter(due())
        .into_tuple::<(Id, String)>()
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .collect();
    let affected = ledger::Entity::update_many()
        .col_expr(
            ledger::Column::Status,
            Expr::value(LedgerStatus::Available.as_str()),
        )
        .col_expr(ledger::Column::UpdatedAt, Expr::value(now))
        .filter(due())
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    for (reseller_id, currency) in scopes {
        refresh_balance_in(conn, reseller_id, &currency, now).await?;
    }
    Ok(affected)
}

/// Locks available entries for a withdrawal (`ApplyWithdraw`, RSL-02 / RSL-05).
pub async fn apply_withdraw_in<C: ConnectionTrait>(
    conn: &C,
    reseller_id: Id,
    draft: &WithdrawDraft,
    now: DateTime<Utc>,
) -> Result<WithdrawRequest> {
    let account = balance_account_in(conn, reseller_id, &draft.currency, now).await?;
    if balance_blocks_withdraw(BalanceStatus::from_db(&account.status)) {
        return Err(Error::bad_request(keys::BALANCE_FROZEN));
    }
    // Write the account row first: takes the SQLite write lock before anything is
    // read, so concurrent withdrawals of one account are serialized on every backend.
    balances::Entity::update_many()
        .col_expr(balances::Column::UpdatedAt, Expr::value(now))
        .filter(balances::Column::Id.eq(account.id))
        .exec(conn)
        .await
        .dom()?;
    let net = sums_by_status(
        conn,
        reseller_id,
        &draft.currency,
        &[LedgerStatus::Available],
    )
    .await?
    .remove(&LedgerStatus::Available)
    .unwrap_or_default();
    let rows = ledger::Entity::find()
        .filter(ledger::Column::ResellerId.eq(reseller_id))
        .filter(ledger::Column::Currency.eq(draft.currency.as_str()))
        .filter(ledger::Column::Status.eq(LedgerStatus::Available.as_str()))
        .filter(ledger::Column::WithdrawRequestId.is_null())
        .filter(ledger::Column::Amount.gt(Decimal::ZERO))
        .filter(ledger::Column::DeletedAt.is_null())
        .order_by_asc(ledger::Column::AvailableAt)
        .order_by_asc(ledger::Column::Id)
        .lock_exclusive()
        .all(conn)
        .await
        .dom()?;
    let amounts: Vec<(Id, Amount)> = rows.iter().map(|r| (r.id, Amount::new(r.amount))).collect();
    let plan = plan_withdraw(draft.amount, net, &amounts)?;
    if let Some((id, locked, remainder)) = plan.split
        && let Some(row) = rows.iter().find(|r| r.id == id)
    {
        ledger::ActiveModel {
            id: Set(id),
            amount: Set(locked.decimal()),
            updated_at: Set(now),
            ..Default::default()
        }
        .update(conn)
        .await
        .dom()?;
        let nanos = now.timestamp_nanos_opt().unwrap_or_default();
        let mut rest = entry(
            row.reseller_id,
            row.order_id,
            LedgerType::from_db(&row.type_),
            remainder.decimal(),
            &row.currency,
            split_key(id, nanos),
            row.metadata_json.clone().unwrap_or(Value::Null),
            LedgerStatus::Available,
            row.available_at,
            now,
        );
        rest.remark = Set(row.remark.clone());
        insert_entry_in(conn, rest).await?;
    }
    let req = withdraws::ActiveModel {
        reseller_id: Set(reseller_id),
        amount: Set(draft.amount.decimal()),
        currency: Set(draft.currency.clone()),
        channel: Set(draft.channel.clone()),
        account: Set(draft.account.clone()),
        status: Set(WithdrawStatus::Pending.as_str().to_owned()),
        reject_reason: Set(String::new()),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    let locked = ledger::Entity::update_many()
        .col_expr(
            ledger::Column::Status,
            Expr::value(LedgerStatus::Locked.as_str()),
        )
        .col_expr(ledger::Column::WithdrawRequestId, Expr::value(req.id))
        .col_expr(ledger::Column::UpdatedAt, Expr::value(now))
        .filter(ledger::Column::Id.is_in(plan.lock_ids.clone()))
        .filter(ledger::Column::Status.eq(LedgerStatus::Available.as_str()))
        .filter(ledger::Column::WithdrawRequestId.is_null())
        .filter(ledger::Column::DeletedAt.is_null())
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if locked != plan.lock_ids.len() as u64 {
        // Another withdrawal took an entry first; the caller's transaction rolls back.
        return Err(Error::bad_request(keys::WITHDRAW_INSUFFICIENT));
    }
    refresh_balance_in(conn, reseller_id, &draft.currency, now).await?;
    Ok(withdraw_model(req))
}

/// Pays or rejects a pending withdrawal (`ReviewWithdraw`).
pub async fn review_withdraw_in<C: ConnectionTrait>(
    conn: &C,
    id: Id,
    admin_id: Id,
    action: WithdrawAction,
    reason: &str,
    now: DateTime<Utc>,
) -> Result<Option<WithdrawRequest>> {
    let Some(req) = withdraws::Entity::find_by_id(id)
        .filter(withdraws::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
    else {
        return Ok(None);
    };
    // Conditional transition: only one reviewer can move a pending request.
    let (status, reject_reason) = match action {
        WithdrawAction::Reject => (WithdrawStatus::Rejected, reason.trim().to_owned()),
        WithdrawAction::Pay => (WithdrawStatus::Paid, String::new()),
    };
    let moved = withdraws::Entity::update_many()
        .col_expr(withdraws::Column::Status, Expr::value(status.as_str()))
        .col_expr(withdraws::Column::RejectReason, Expr::value(reject_reason))
        .col_expr(withdraws::Column::ProcessedBy, Expr::value(admin_id))
        .col_expr(withdraws::Column::ProcessedAt, Expr::value(now))
        .col_expr(withdraws::Column::UpdatedAt, Expr::value(now))
        .filter(withdraws::Column::Id.eq(id))
        .filter(withdraws::Column::Status.eq(WithdrawStatus::Pending.as_str()))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if moved == 0 {
        return Err(Error::invalid());
    }
    let entries = ledger::Entity::update_many()
        .col_expr(ledger::Column::UpdatedAt, Expr::value(now))
        .filter(ledger::Column::WithdrawRequestId.eq(id))
        .filter(ledger::Column::DeletedAt.is_null());
    let entries = match action {
        WithdrawAction::Reject => entries
            .col_expr(
                ledger::Column::Status,
                Expr::value(LedgerStatus::Available.as_str()),
            )
            .col_expr(
                ledger::Column::WithdrawRequestId,
                Expr::value(Option::<Id>::None),
            ),
        WithdrawAction::Pay => entries.col_expr(
            ledger::Column::Status,
            Expr::value(LedgerStatus::Withdrawn.as_str()),
        ),
    };
    entries.exec(conn).await.dom()?;
    refresh_balance_in(conn, req.reseller_id, &req.currency, now).await?;
    Ok(withdraws::Entity::find_by_id(id)
        .one(conn)
        .await
        .dom()?
        .map(withdraw_model))
}

/// Reseller ids an admin filter (`reseller_id`, `user_id`, `keyword`) narrows to;
/// `None` = no restriction.
async fn admin_scope<C: ConnectionTrait>(conn: &C, f: &FinanceFilter) -> Result<Option<Vec<Id>>> {
    let mut scope: Option<Vec<Id>> = f.reseller_id.map(|id| vec![id]);
    let mut narrow = |ids: Vec<Id>| {
        scope = Some(match scope.take() {
            Some(prev) => prev.into_iter().filter(|i| ids.contains(i)).collect(),
            None => ids,
        });
    };
    if f.admin
        && let Some(uid) = f.user_id
    {
        narrow(user_profile_ids(conn, uid).await?);
    }
    Ok(scope)
}

#[async_trait]
impl LedgerRepo for SeaResellerStore {
    async fn post_order_profit(
        &self,
        event: &OrderPaid,
        now: DateTime<Utc>,
        confirm_days: i64,
    ) -> Result<bool> {
        let txn = self.db.begin().await.dom()?;
        let created = post_order_profit_in(&txn, event, now, confirm_days).await?;
        txn.commit().await.dom()?;
        Ok(created)
    }

    async fn deduct_refund(
        &self,
        event: &OrderRefunded,
        now: DateTime<Utc>,
        confirm_days: i64,
    ) -> Result<bool> {
        let txn = self.db.begin().await.dom()?;
        let created = deduct_refund_in(&txn, event, now, confirm_days).await?;
        txn.commit().await.dom()?;
        Ok(created)
    }

    async fn confirm_due(&self, now: DateTime<Utc>) -> Result<u64> {
        let txn = self.db.begin().await.dom()?;
        let affected = confirm_due_in(&txn, now).await?;
        txn.commit().await.dom()?;
        Ok(affected)
    }

    async fn apply_withdraw(
        &self,
        reseller_id: Id,
        draft: &WithdrawDraft,
        now: DateTime<Utc>,
    ) -> Result<WithdrawRequest> {
        let txn = self.db.begin().await.dom()?;
        let req = apply_withdraw_in(&txn, reseller_id, draft, now).await?;
        txn.commit().await.dom()?;
        Ok(req)
    }

    async fn review_withdraw(
        &self,
        id: Id,
        admin_id: Id,
        action: WithdrawAction,
        reason: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<WithdrawRequest>> {
        let txn = self.db.begin().await.dom()?;
        let req = review_withdraw_in(&txn, id, admin_id, action, reason, now).await?;
        txn.commit().await.dom()?;
        Ok(req)
    }

    async fn list_balances(
        &self,
        f: &FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<BalanceAccount>> {
        let mut cond = Condition::all().add(balances::Column::DeletedAt.is_null());
        if let Some(ids) = admin_scope(&self.db, f).await? {
            cond = cond.add(balances::Column::ResellerId.is_in(ids));
        }
        if f.admin && !f.keyword.trim().is_empty() {
            cond = cond.add(
                balances::Column::ResellerId
                    .is_in(keyword_profile_ids(&self.db, &f.keyword).await?),
            );
        }
        if !f.currency.trim().is_empty() {
            cond = cond.add(balances::Column::Currency.eq(f.currency.trim()));
        }
        if !f.status.trim().is_empty() {
            cond = cond.add(balances::Column::Status.eq(f.status.trim()));
        }
        let q = balances::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let q = if f.admin {
            q.order_by_desc(balances::Column::Id)
        } else {
            q.order_by_asc(balances::Column::Currency)
                .order_by_desc(balances::Column::Id)
        };
        let mut items: Vec<BalanceAccount> = q
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(balance_model)
            .collect();
        if f.admin {
            let profiles = load_profiles(
                &self.db,
                &items.iter().map(|b| b.reseller_id).collect::<Vec<_>>(),
            )
            .await?;
            for b in &mut items {
                b.profile = profiles.get(&b.reseller_id).cloned();
            }
        }
        Ok(Page { items, total })
    }

    async fn list_ledger(&self, f: &FinanceFilter, page: PageRequest) -> Result<Page<LedgerEntry>> {
        let mut cond = Condition::all().add(ledger::Column::DeletedAt.is_null());
        if let Some(ids) = admin_scope(&self.db, f).await? {
            cond = cond.add(ledger::Column::ResellerId.is_in(ids));
        }
        if f.admin && !f.keyword.trim().is_empty() {
            cond = cond.add(
                ledger::Column::ResellerId.is_in(keyword_profile_ids(&self.db, &f.keyword).await?),
            );
        }
        for (col, v) in [
            (ledger::Column::Currency, &f.currency),
            (ledger::Column::Type, &f.kind),
            (ledger::Column::Status, &f.status),
        ] {
            if !v.trim().is_empty() {
                cond = cond.add(col.eq(v.trim()));
            }
        }
        if let Some(oid) = f.order_id.filter(|i| *i > 0) {
            cond = cond.add(ledger::Column::OrderId.eq(oid));
        }
        if f.admin && !f.order_no.trim().is_empty() {
            let order_ids = sea_orm::sea_query::Query::select()
                .column(orders::Column::Id)
                .from(orders::Entity)
                .and_where(orders::Column::OrderNo.eq(f.order_no.trim()))
                .and_where(orders::Column::DeletedAt.is_null())
                .to_owned();
            cond = cond.add(ledger::Column::OrderId.in_subquery(order_ids));
        }
        if let Some(from) = f.created_from {
            cond = cond.add(ledger::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(ledger::Column::CreatedAt.lte(to));
        }
        let q = ledger::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let mut items: Vec<LedgerEntry> = q
            .order_by_desc(ledger::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(ledger_model)
            .collect();
        if f.admin {
            let profiles = load_profiles(
                &self.db,
                &items.iter().map(|e| e.reseller_id).collect::<Vec<_>>(),
            )
            .await?;
            let order_ids: Vec<Id> = items.iter().filter_map(|e| e.order_id).collect();
            let orders: HashMap<Id, OrderBrief> = if order_ids.is_empty() {
                HashMap::new()
            } else {
                orders::Entity::find()
                    .filter(orders::Column::Id.is_in(order_ids))
                    .filter(orders::Column::DeletedAt.is_null())
                    .all(&self.db)
                    .await
                    .dom()?
                    .into_iter()
                    .map(|o| {
                        (
                            o.id,
                            OrderBrief {
                                id: o.id,
                                order_no: o.order_no,
                                status: o.status,
                                currency: o.currency,
                                total_amount: Amount::new(o.total_amount),
                                created_at: o.created_at,
                            },
                        )
                    })
                    .collect()
            };
            for e in &mut items {
                e.profile = profiles.get(&e.reseller_id).cloned();
                e.order = e.order_id.and_then(|id| orders.get(&id).cloned());
            }
        }
        Ok(Page { items, total })
    }

    async fn list_withdraws(
        &self,
        f: &FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        let mut cond = Condition::all().add(withdraws::Column::DeletedAt.is_null());
        if let Some(ids) = admin_scope(&self.db, f).await? {
            cond = cond.add(withdraws::Column::ResellerId.is_in(ids));
        }
        let keyword = f.keyword.trim();
        if f.admin && !keyword.is_empty() {
            cond = cond.add(
                Condition::any()
                    .add(
                        withdraws::Column::ResellerId
                            .is_in(keyword_profile_ids(&self.db, keyword).await?),
                    )
                    .add(ilike_sql("account", keyword)),
            );
        }
        if !f.currency.trim().is_empty() {
            cond = cond.add(withdraws::Column::Currency.eq(f.currency.trim()));
        }
        if !f.status.trim().is_empty() {
            cond = cond.add(withdraws::Column::Status.eq(f.status.trim()));
        }
        if let Some(from) = f.created_from {
            cond = cond.add(withdraws::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(withdraws::Column::CreatedAt.lte(to));
        }
        let q = withdraws::Entity::find().filter(cond);
        let total = q.clone().count(&self.db).await.dom()?;
        let mut items: Vec<WithdrawRequest> = q
            .order_by_desc(withdraws::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(withdraw_model)
            .collect();
        if f.admin {
            let profiles = load_profiles(
                &self.db,
                &items.iter().map(|w| w.reseller_id).collect::<Vec<_>>(),
            )
            .await?;
            let admin_ids: Vec<Id> = items.iter().filter_map(|w| w.processed_by).collect();
            let admins: HashMap<Id, AdminRef> = if admin_ids.is_empty() {
                HashMap::new()
            } else {
                admins::Entity::find()
                    .filter(admins::Column::Id.is_in(admin_ids))
                    .all(&self.db)
                    .await
                    .dom()?
                    .into_iter()
                    .map(|a| {
                        (
                            a.id,
                            AdminRef {
                                id: a.id,
                                username: a.username,
                            },
                        )
                    })
                    .collect()
            };
            for w in &mut items {
                w.profile = profiles.get(&w.reseller_id).cloned();
                w.processor = w.processed_by.and_then(|id| admins.get(&id).cloned());
            }
        }
        Ok(Page { items, total })
    }
}
