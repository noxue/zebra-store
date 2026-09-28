//! Order-side wallet allocation on top of the wallet group's ledger
//! (`db::repo::wallet::ledger`, run inside the caller's transaction): the order owns
//! `wallet_paid_amount` / `online_paid_amount`, the ledger owns balances and transactions
//! (`ApplyWalletBalance` / `ReleaseWalletBalance`).

use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use zs_domain::order::model::Order;
use zs_domain::wallet::Transaction;
use zs_domain::wallet::ports::BalanceChangeRequest;
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{orders, wallet_accounts};
use crate::db::repo::support::DbResultExt;
use crate::db::repo::wallet::ledger;

/// Current balance (`0` without an account); no lock.
pub(crate) async fn balance_of<C: ConnectionTrait>(conn: &C, user_id: Id) -> Result<Amount> {
    Ok(wallet_accounts::Entity::find()
        .filter(wallet_accounts::Column::UserId.eq(user_id))
        .filter(wallet_accounts::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?
        .map(|a| Amount::new(a.balance))
        .unwrap_or_default())
}

/// Debits `min(balance, total)` for a pending order paying with its balance and records
/// the allocation on the order. An order already holding wallet money keeps it.
pub(crate) async fn apply_order_balance<C: ConnectionTrait>(
    conn: &C,
    order: &mut Order,
    now: DateTime<Utc>,
) -> Result<Amount> {
    if order.wallet_paid_amount.is_positive() {
        return Ok(order.wallet_paid_amount);
    }
    let deducted = ledger::apply_order_balance(
        conn,
        order.id,
        order.user_id,
        order.total_amount,
        &order.currency,
        now,
    )
    .await?;
    if !deducted.is_positive() {
        return Ok(Amount::ZERO);
    }
    let online = (order.total_amount - deducted).non_negative();
    orders::Entity::update_many()
        .col_expr(
            orders::Column::WalletPaidAmount,
            Expr::value(deducted.decimal()),
        )
        .col_expr(
            orders::Column::OnlinePaidAmount,
            Expr::value(online.decimal()),
        )
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order.id))
        .exec(conn)
        .await
        .dom()?;
    order.wallet_paid_amount = deducted;
    order.online_paid_amount = online;
    order.updated_at = now;
    Ok(deducted)
}

/// Returns an order's wallet allocation: the allocation is claimed on the order row with
/// a conditional update first, so concurrent releases never credit twice.
pub(crate) async fn release_order_balance<C: ConnectionTrait>(
    conn: &C,
    order: &mut Order,
    kind: &str,
    remark: &str,
    now: DateTime<Utc>,
) -> Result<Amount> {
    let amount = order.wallet_paid_amount;
    if order.user_id == 0 || !amount.is_positive() {
        return Ok(Amount::ZERO);
    }
    let claimed = orders::Entity::update_many()
        .col_expr(
            orders::Column::WalletPaidAmount,
            Expr::value(Amount::ZERO.decimal()),
        )
        .col_expr(
            orders::Column::OnlinePaidAmount,
            Expr::value(order.total_amount.decimal()),
        )
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order.id))
        .filter(orders::Column::DeletedAt.is_null())
        .filter(orders::Column::WalletPaidAmount.gt(Amount::ZERO.decimal()))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if claimed == 0 {
        return Ok(Amount::ZERO);
    }
    let released = ledger::release_order_balance(
        conn,
        &ledger::Release {
            order_id: order.id,
            user_id: order.user_id,
            amount,
            kind,
            currency: &order.currency,
            remark,
        },
        now,
    )
    .await?;
    order.wallet_paid_amount = Amount::ZERO;
    order.online_paid_amount = order.total_amount;
    order.updated_at = now;
    Ok(released)
}

/// Idempotent credit per reference (`CreditInTransaction`).
pub(crate) async fn credit<C: ConnectionTrait>(
    conn: &C,
    req: &BalanceChangeRequest,
    now: DateTime<Utc>,
) -> Result<Transaction> {
    Ok(ledger::credit(conn, req, now).await?.1)
}
