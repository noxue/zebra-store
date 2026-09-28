//! Payment-only [`PaymentSettlement`]: applies verified callbacks to the `payments` row inside
//! one transaction with a conditional update. Order/wallet settlement is added by the order
//! group (it replaces this adapter in `wire::payment`).

use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction, EntityTrait,
    QueryFilter, QuerySelect, TransactionTrait,
};
use std::sync::Arc;
use zs_domain::payment::callback::{
    CallbackInput, CallbackOutcome, apply_callback, validate_callback_facts,
};
use zs_domain::payment::errors::keys;
use zs_domain::payment::model::Payment;
use zs_domain::payment::settlement::PaymentSettlement;
use zs_domain::payment::types::PaymentStatus;
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;

use super::records::{business_no_on, to_domain};
use crate::db::entity::payments;
use crate::db::repo::support::{DbResultExt, to_json};

/// Settles the payment row only (logs that order settlement is pending).
#[derive(Clone)]
pub struct PaymentRowSettlement {
    db: DatabaseConnection,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for PaymentRowSettlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaymentRowSettlement")
    }
}

impl PaymentRowSettlement {
    pub fn new(db: DatabaseConnection, clock: Arc<dyn Clock>) -> Self {
        Self { db, clock }
    }
}

/// Writes the mutable callback columns; when `expected` is set the update is conditional on
/// the stored status (concurrent callbacks cannot both transition the row).
pub(crate) async fn save_callback_columns(
    txn: &DatabaseTransaction,
    p: &Payment,
    expected: Option<PaymentStatus>,
) -> Result<u64> {
    let mut q = payments::Entity::update_many()
        .col_expr(payments::Column::Status, Expr::value(p.status.as_str()))
        .col_expr(payments::Column::Currency, Expr::value(p.currency.clone()))
        .col_expr(
            payments::Column::ProviderRef,
            Expr::value(p.provider_ref.clone()),
        )
        .col_expr(
            payments::Column::ProviderPayload,
            Expr::value(to_json(&p.provider_payload)?),
        )
        .col_expr(payments::Column::PaidAt, Expr::value(p.paid_at))
        .col_expr(payments::Column::ExpiredAt, Expr::value(p.expired_at))
        .col_expr(payments::Column::CallbackAt, Expr::value(p.callback_at))
        .col_expr(
            payments::Column::ExceptionCode,
            Expr::value(p.exception_code.clone()),
        )
        .col_expr(payments::Column::UpdatedAt, Expr::value(p.updated_at))
        .filter(payments::Column::Id.eq(p.id));
    if let Some(status) = expected {
        q = q.filter(payments::Column::Status.eq(status.as_str()));
    }
    Ok(q.exec(txn).await.dom()?.rows_affected)
}

/// Expires the other open payments of an order after a success (`ExpirePendingByOrderIDs`).
pub(crate) async fn expire_other_pending<C: ConnectionTrait>(
    conn: &C,
    order_id: i64,
    at: chrono::DateTime<chrono::Utc>,
) -> Result<u64> {
    if order_id <= 0 {
        return Ok(0);
    }
    let res = payments::Entity::update_many()
        .col_expr(
            payments::Column::Status,
            Expr::value(PaymentStatus::Expired.as_str()),
        )
        .col_expr(payments::Column::ExpiredAt, Expr::value(Some(at)))
        .col_expr(payments::Column::UpdatedAt, Expr::value(at))
        .filter(payments::Column::DeletedAt.is_null())
        .filter(payments::Column::OrderId.eq(order_id))
        .filter(payments::Column::Status.is_in([
            PaymentStatus::Initiated.as_str(),
            PaymentStatus::Pending.as_str(),
        ]))
        .exec(conn)
        .await
        .dom()?;
    Ok(res.rows_affected)
}

#[async_trait]
impl PaymentSettlement for PaymentRowSettlement {
    async fn settle(&self, input: CallbackInput) -> Result<Payment> {
        let now = self.clock.now();
        let txn = self.db.begin().await.dom()?;
        let row = payments::Entity::find_by_id(input.payment_id)
            .filter(payments::Column::DeletedAt.is_null())
            .lock_exclusive()
            .one(&txn)
            .await
            .dom()?
            .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
        let mut payment = to_domain(row);
        let business_no = business_no_on(&txn, &payment)
            .await?
            .ok_or_else(|| Error::not_found("error.order_not_found"))?;
        validate_callback_facts(&payment, &business_no, &input)?;
        let previous = payment.status;
        let outcome = apply_callback(&mut payment, &input, now);
        match outcome {
            CallbackOutcome::Idempotent => {
                save_callback_columns(&txn, &payment, None).await?;
            }
            CallbackOutcome::Transitioned { first_success, .. } => {
                if save_callback_columns(&txn, &payment, Some(previous)).await? != 1 {
                    // A concurrent callback changed the row first; let the gateway retry.
                    return Err(Error::internal_msg("payment row changed concurrently"));
                }
                if first_success {
                    expire_other_pending(&txn, payment.order_id, now).await?;
                }
            }
        }
        txn.commit().await.dom()?;
        tracing::info!(
            payment_id = payment.id,
            order_id = payment.order_id,
            status = %payment.status,
            outcome = ?outcome,
            "payment_settled_row_only (order settlement pending implementation)"
        );
        Ok(payment)
    }
}
