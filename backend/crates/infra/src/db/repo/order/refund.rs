//! Refund transactions (`AdminRefundToWallet`, `AdminManualRefund`,
//! `UpdatePaymentFeeRefunded`; RFD-01 … RFD-04, RSL-01).

use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, IntoActiveModel, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use zs_domain::order::model::{Order, RefundRecord, keys, refund_type};
use zs_domain::order::ports::{RefundDone, RefundRequest};
use zs_domain::order::refund::{payment_fee_refund_amount, plan_refund, refundable_fee_snapshot};
use zs_domain::payment::refund::{
    GatewayRefundAttempt, GatewayRefundAttemptUpdate, NewGatewayRefundAttempt,
    ReservedGatewayRefund, status as gateway_refund_status,
};
use zs_domain::reseller::ports::OrderRefunded;
use zs_domain::wallet::model::txn_type;
use zs_domain::wallet::ports::BalanceChangeRequest;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use super::{map, ops, wallet};
use crate::db::entity::{gateway_refund_attempts, order_refund_records, orders, payments};
use crate::db::repo::affiliate::clawback_on_refund;
use crate::db::repo::payment::records::to_domain as payment_to_domain;
use crate::db::repo::reseller::ledger::deduct_refund_in;
use crate::db::repo::support::DbResultExt;

/// Remark of a wallet refund without one (`管理员退款到余额`).
const WALLET_REFUND_REMARK: &str = "管理员退款到余额";

fn gateway_attempt_to_domain(model: gateway_refund_attempts::Model) -> GatewayRefundAttempt {
    let payload = model
        .payload
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    GatewayRefundAttempt {
        id: model.id,
        order_id: model.order_id,
        payment_id: model.payment_id,
        channel_id: model.channel_id,
        request_date: model.request_date,
        refund_no: model.refund_no,
        provider_ref: model.provider_ref,
        amount: Amount::new(model.amount),
        status: model.status,
        remark: model.remark,
        payment_fee_refunded: model.payment_fee_refunded,
        payload,
        error: model.error,
        refund_record_id: model.refund_record_id,
        created_at: model.created_at,
        updated_at: model.updated_at,
    }
}

pub(crate) async fn reserve_gateway_refund_in<C: ConnectionTrait>(
    conn: &C,
    input: &NewGatewayRefundAttempt,
) -> Result<ReservedGatewayRefund> {
    // Locking the successful payment serializes retries across processes, so a pending
    // provider request is always queried instead of being submitted a second time.
    payments::Entity::find_by_id(input.payment_id)
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    if let Some(existing) = gateway_refund_attempts::Entity::find()
        .filter(gateway_refund_attempts::Column::PaymentId.eq(input.payment_id))
        .filter(gateway_refund_attempts::Column::Status.eq(gateway_refund_status::PENDING))
        .order_by_desc(gateway_refund_attempts::Column::Id)
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
    {
        return Ok(ReservedGatewayRefund {
            attempt: gateway_attempt_to_domain(existing),
            created: false,
        });
    }
    // A concurrent retry may have finished while this request was waiting for the payment
    // lock. Treat a matching successful request as the same idempotent operation.
    if let Some(existing) = gateway_refund_attempts::Entity::find()
        .filter(gateway_refund_attempts::Column::PaymentId.eq(input.payment_id))
        .filter(gateway_refund_attempts::Column::Status.eq(gateway_refund_status::SUCCEEDED))
        .filter(gateway_refund_attempts::Column::Amount.eq(input.amount.decimal()))
        .filter(gateway_refund_attempts::Column::Remark.eq(input.remark.clone()))
        .filter(gateway_refund_attempts::Column::PaymentFeeRefunded.eq(input.payment_fee_refunded))
        .order_by_desc(gateway_refund_attempts::Column::Id)
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
    {
        return Ok(ReservedGatewayRefund {
            attempt: gateway_attempt_to_domain(existing),
            created: false,
        });
    }
    let model = gateway_refund_attempts::ActiveModel {
        order_id: Set(input.order_id),
        payment_id: Set(input.payment_id),
        channel_id: Set(input.channel_id),
        request_date: Set(input.request_date.clone()),
        refund_no: Set(input.refund_no.clone()),
        provider_ref: Set(input.provider_ref.clone()),
        amount: Set(input.amount.decimal()),
        status: Set(gateway_refund_status::PENDING.to_owned()),
        remark: Set(input.remark.clone()),
        payment_fee_refunded: Set(input.payment_fee_refunded),
        payload: Set(Some(serde_json::Value::Object(Default::default()))),
        error: Set(String::new()),
        refund_record_id: Set(None),
        created_at: Set(input.now),
        updated_at: Set(input.now),
        ..Default::default()
    }
    .insert(conn)
    .await
    .dom()?;
    Ok(ReservedGatewayRefund {
        attempt: gateway_attempt_to_domain(model),
        created: true,
    })
}

pub(crate) async fn get_gateway_refund_attempt_in<C: ConnectionTrait>(
    conn: &C,
    id: Id,
) -> Result<Option<GatewayRefundAttempt>> {
    Ok(gateway_refund_attempts::Entity::find_by_id(id)
        .one(conn)
        .await
        .dom()?
        .map(gateway_attempt_to_domain))
}

pub(crate) async fn list_gateway_refund_attempts_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
) -> Result<Vec<GatewayRefundAttempt>> {
    Ok(gateway_refund_attempts::Entity::find()
        .filter(gateway_refund_attempts::Column::OrderId.eq(order_id))
        .order_by_desc(gateway_refund_attempts::Column::Id)
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(gateway_attempt_to_domain)
        .collect())
}

pub(crate) async fn update_gateway_refund_attempt_in<C: ConnectionTrait>(
    conn: &C,
    id: Id,
    update: &GatewayRefundAttemptUpdate,
) -> Result<GatewayRefundAttempt> {
    let mut model = gateway_refund_attempts::Entity::find_by_id(id)
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?
        .into_active_model();
    model.status = Set(update.status.clone());
    model.payload = Set(Some(serde_json::Value::Object(update.payload.clone())));
    model.error = Set(update.error.clone());
    model.updated_at = Set(update.now);
    Ok(gateway_attempt_to_domain(model.update(conn).await.dom()?))
}

/// Root (parent) order id of an order.
fn root_id(order: &Order) -> Id {
    order.parent_id.filter(|p| *p > 0).unwrap_or(order.id)
}

/// Fee that a refund of `amount` gives back, excluding `exclude_record` from the history.
async fn fee_refund<C: ConnectionTrait>(
    conn: &C,
    order: &Order,
    amount: Amount,
    exclude_record: Id,
) -> Result<Amount> {
    let root = root_id(order);
    let payments: Vec<_> = payments::Entity::find()
        .filter(payments::Column::OrderId.eq(root))
        .filter(payments::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(payment_to_domain)
        .collect();
    let (paid, fee) = refundable_fee_snapshot(&payments, &order.currency);
    if !paid.is_positive() || !fee.is_positive() {
        return Ok(Amount::ZERO);
    }
    let mut family: Vec<Id> = vec![root];
    family.extend(
        orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .filter(orders::Column::ParentId.eq(root))
            .filter(orders::Column::DeletedAt.is_null())
            .into_tuple::<Id>()
            .all(conn)
            .await
            .dom()?,
    );
    let mut principal_before = Amount::ZERO;
    let mut fee_before = Amount::ZERO;
    for r in order_refund_records::Entity::find()
        .filter(order_refund_records::Column::OrderId.is_in(family))
        .filter(order_refund_records::Column::DeletedAt.is_null())
        .all(conn)
        .await
        .dom()?
    {
        if r.id == exclude_record || !r.payment_fee_refunded {
            continue;
        }
        principal_before += Amount::new(r.amount);
        fee_before += Amount::new(r.payment_fee_refunded_amount);
    }
    Ok(payment_fee_refund_amount(
        paid,
        fee,
        principal_before,
        fee_before,
        amount,
    ))
}

/// Applies a refund inside `conn` (a transaction).
pub(crate) async fn refund_in<C: ConnectionTrait>(
    conn: &C,
    req: &RefundRequest,
) -> Result<RefundDone> {
    let now = req.now;
    let not_found = || Error::not_found(keys::ORDER_NOT_FOUND);
    // Lock order: parent first, then the child (the parent-refund path locks the parent
    // and then updates its children), so the two paths never deadlock.
    let head = map::load(conn, req.order_id).await?.ok_or_else(not_found)?;
    let parent = match head.parent_id.filter(|p| *p > 0) {
        Some(pid) => map::load_locked(conn, pid).await?,
        None => None,
    };
    let order = map::load_locked(conn, req.order_id)
        .await?
        .ok_or_else(not_found)?;
    let gateway_attempt = if let Some(attempt_id) = req.gateway_refund_attempt_id {
        let attempt = gateway_refund_attempts::Entity::find_by_id(attempt_id)
            .lock_exclusive()
            .one(conn)
            .await
            .dom()?
            .ok_or_else(not_found)?;
        if attempt.order_id != order.id || Amount::new(attempt.amount) != req.amount {
            return Err(Error::invalid());
        }
        if let Some(record_id) = attempt.refund_record_id {
            let record = order_refund_records::Entity::find_by_id(record_id)
                .filter(order_refund_records::Column::DeletedAt.is_null())
                .one(conn)
                .await
                .dom()?
                .ok_or_else(not_found)?;
            return Ok(RefundDone {
                order,
                record: map::refund_to_domain(record),
                transaction: None,
            });
        }
        Some(attempt)
    } else {
        None
    };
    if req.to_wallet && order.user_id == 0 {
        return Err(Error::invalid());
    }
    let plan = plan_refund(&order, req.amount, req.max_refund_days, now)?;
    // LQA-R4: a child refund also counts against its parent, so parent + child refunds
    // can never exceed what the buyer paid, and the parent shows the refunded total.
    if let Some(parent) = &parent
        && req.amount > parent.total_amount - parent.refunded_amount
    {
        return Err(zs_domain::order::refund::refund_exceeded());
    }
    let remark = req.remark.trim().to_owned();

    let transaction = if req.to_wallet {
        let nanos = now
            .timestamp_nanos_opt()
            .unwrap_or_else(|| now.timestamp_micros());
        Some(
            wallet::credit(
                conn,
                &BalanceChangeRequest {
                    user_id: order.user_id,
                    delta: plan.amount,
                    kind: txn_type::ADMIN_REFUND.to_owned(),
                    reference: format!("order:{}:admin_refund:{nanos}", order.id),
                    remark: if remark.is_empty() {
                        WALLET_REFUND_REMARK.to_owned()
                    } else {
                        remark.clone()
                    },
                    currency: order.currency.clone(),
                    operator_admin_id: None,
                    order_id: Some(order.id),
                },
                now,
            )
            .await?,
        )
    } else {
        None
    };

    orders::Entity::update_many()
        .col_expr(
            orders::Column::RefundedAmount,
            Expr::value(plan.new_refunded.decimal()),
        )
        .col_expr(orders::Column::Status, Expr::value(plan.target.as_str()))
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order.id))
        .exec(conn)
        .await
        .dom()?;
    match order.parent_id {
        None => {
            orders::Entity::update_many()
                .col_expr(orders::Column::Status, Expr::value(plan.target.as_str()))
                .col_expr(orders::Column::UpdatedAt, Expr::value(now))
                .filter(orders::Column::ParentId.eq(order.id))
                .filter(orders::Column::DeletedAt.is_null())
                .filter(orders::Column::Status.ne(plan.target.as_str()))
                .exec(conn)
                .await
                .dom()?;
        }
        Some(parent_id) => {
            if let Some(parent) = &parent {
                let total = (parent.refunded_amount + plan.amount).min(parent.total_amount);
                orders::Entity::update_many()
                    .col_expr(orders::Column::RefundedAmount, Expr::value(total.decimal()))
                    .col_expr(orders::Column::UpdatedAt, Expr::value(now))
                    .filter(orders::Column::Id.eq(parent.id))
                    .exec(conn)
                    .await
                    .dom()?;
            }
            ops::sync_parent(conn, parent_id, now).await?;
        }
    }

    let fee_amount = if !req.to_wallet && req.payment_fee_refunded {
        fee_refund(conn, &order, plan.amount, 0).await?
    } else {
        Amount::ZERO
    };
    let currency = match order.currency.trim().to_ascii_uppercase() {
        c if c.is_empty() => "CNY".to_owned(),
        c => c,
    };
    let record = order_refund_records::ActiveModel {
        user_id: Set(order.user_id),
        guest_email: Set(order.guest_email.clone()),
        order_id: Set(order.id),
        type_: Set(if req.to_wallet {
            refund_type::WALLET
        } else {
            refund_type::MANUAL
        }
        .to_owned()),
        amount: Set(plan.amount.decimal()),
        payment_fee_refunded: Set(!req.to_wallet && req.payment_fee_refunded),
        payment_fee_refunded_amount: Set(fee_amount.decimal()),
        currency: Set(currency),
        remark: Set(remark),
        created_at: Set(now),
        updated_at: Set(now),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .map_err(|e| Error::internal(e).or_internal(keys::ORDER_UPDATE_FAILED))?;

    if let Some(attempt) = gateway_attempt {
        gateway_refund_attempts::Entity::update_many()
            .col_expr(
                gateway_refund_attempts::Column::Status,
                Expr::value(gateway_refund_status::SUCCEEDED),
            )
            .col_expr(
                gateway_refund_attempts::Column::ProviderRef,
                Expr::value(req.gateway_refund_provider_ref.trim()),
            )
            .col_expr(
                gateway_refund_attempts::Column::Payload,
                Expr::value(serde_json::Value::Object(
                    req.gateway_refund_payload.clone(),
                )),
            )
            .col_expr(
                gateway_refund_attempts::Column::RefundRecordId,
                Expr::value(record.id),
            )
            .col_expr(
                gateway_refund_attempts::Column::Error,
                Expr::value(String::new()),
            )
            .col_expr(gateway_refund_attempts::Column::UpdatedAt, Expr::value(now))
            .filter(gateway_refund_attempts::Column::Id.eq(attempt.id))
            .exec(conn)
            .await
            .dom()?;
    }

    let reason = if req.to_wallet {
        "order_refunded_to_wallet"
    } else {
        "order_refunded_manual"
    };
    if req.to_wallet || order.user_id > 0 {
        clawback_on_refund(
            conn,
            order.id,
            order.total_amount,
            plan.amount,
            plan.refunded_before,
            reason,
            now,
        )
        .await?;
    }
    // LQA-R4: the reseller snapshot and profit entry live on the parent (the order
    // the buyer paid); a child refund claws back proportionally against the parent.
    let (profit_order, refunded_before) = match &parent {
        Some(p) => (p, p.refunded_amount),
        None => (&order, plan.refunded_before),
    };
    deduct_refund_in(
        conn,
        &OrderRefunded {
            order_id: profit_order.id,
            order_no: profit_order.order_no.clone(),
            reseller_id: profit_order.reseller_id.or(order.reseller_id),
            order_currency: profit_order.currency.clone(),
            order_total_amount: profit_order.total_amount,
            refund_record_id: record.id,
            refund_type: record.type_.clone(),
            refund_amount: plan.amount,
            refund_currency: record.currency.clone(),
            refunded_before,
        },
        now,
        req.reseller_confirm_days,
    )
    .await?;

    let updated = map::load(conn, order.id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    Ok(RefundDone {
        order: updated,
        record: map::refund_to_domain(record),
        transaction,
    })
}

/// Switches the fee-refunded flag of a manual refund (lock order, then record; RFD-01).
pub(crate) async fn set_fee_flag_in<C: ConnectionTrait>(
    conn: &C,
    record_id: Id,
    refunded: bool,
    now: DateTime<Utc>,
) -> Result<RefundRecord> {
    let not_found = || Error::not_found(keys::ORDER_NOT_FOUND);
    let initial = order_refund_records::Entity::find_by_id(record_id)
        .filter(order_refund_records::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?
        .ok_or_else(not_found)?;
    if initial.type_ != refund_type::MANUAL {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }
    let order = map::load_locked(conn, initial.order_id)
        .await?
        .ok_or_else(not_found)?;
    let record = order_refund_records::Entity::find_by_id(record_id)
        .filter(order_refund_records::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(not_found)?;
    if record.order_id != order.id {
        return Err(not_found());
    }
    let amount = if refunded {
        fee_refund(conn, &order, Amount::new(record.amount), record.id).await?
    } else {
        Amount::ZERO
    };
    order_refund_records::Entity::update_many()
        .col_expr(
            order_refund_records::Column::PaymentFeeRefunded,
            Expr::value(refunded),
        )
        .col_expr(
            order_refund_records::Column::PaymentFeeRefundedAmount,
            Expr::value(amount.decimal()),
        )
        .col_expr(order_refund_records::Column::UpdatedAt, Expr::value(now))
        .filter(order_refund_records::Column::Id.eq(record.id))
        .exec(conn)
        .await
        .dom()?;
    let mut out = map::refund_to_domain(record);
    out.payment_fee_refunded = refunded;
    out.payment_fee_refunded_amount = amount;
    out.updated_at = now;
    Ok(out)
}

/// Refund records of the given orders, oldest first.
pub(crate) async fn records_of<C: ConnectionTrait>(
    conn: &C,
    order_ids: &[Id],
) -> Result<Vec<RefundRecord>> {
    if order_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(order_refund_records::Entity::find()
        .filter(order_refund_records::Column::OrderId.is_in(order_ids.to_vec()))
        .filter(order_refund_records::Column::DeletedAt.is_null())
        .order_by_asc(order_refund_records::Column::CreatedAt)
        .order_by_asc(order_refund_records::Column::Id)
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(map::refund_to_domain)
        .collect())
}
