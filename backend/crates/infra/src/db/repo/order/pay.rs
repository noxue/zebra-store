//! Order payments: creation (`CreatePayment` transaction), gateway bookkeeping and callback
//! settlement (`applyPaymentUpdate`) — PAY-02/03/04/05/11/12/23/24.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseTransaction, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, Set,
};
use zs_domain::order::model::{Order, OrderStatus, keys};
use zs_domain::order::ports::{BeginOutcome, BeginPayment, Settled};
use zs_domain::payment::callback::{
    CallbackInput, CallbackOutcome, OrderPaymentState, apply_callback, success_exception_code,
    validate_callback_facts,
};
use zs_domain::payment::channel::PaymentChannel;
use zs_domain::payment::eligibility::{
    Payer, check_amount, check_currency, check_order_channel, decode_channel_ids, product_allows,
    product_channel_intersection,
};
use zs_domain::payment::errors::keys as pay_keys;
use zs_domain::payment::fee::{
    calculate_payment_amounts, covered_order_amount, required_online_amount,
};
use zs_domain::payment::model::Payment;
use zs_domain::payment::returns::new_gateway_order_no;
use zs_domain::payment::types::{
    FeePolicy, InteractionMode, PaymentStatus, SITE_CURRENCY_DEFAULT, channel_type, provider,
};
use zs_domain::reseller::ports::{OrderPaid, PaymentMeta};
use zs_domain::wallet::model::txn_type;
use zs_domain::wallet::ports::BalanceChangeRequest;
use zs_domain::wallet::rules::underpaid_reference;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use super::{map, ops, wallet};
use crate::db::entity::{orders, payment_channels, payments, products};
use crate::db::repo::payment::channel::to_domain as channel_to_domain;
use crate::db::repo::payment::records::to_domain as payment_to_domain;
use crate::db::repo::payment::settlement::save_callback_columns;
use crate::db::repo::reseller::ledger::post_order_profit_in;
use crate::db::repo::support::{DbResultExt, to_json};

/// Remark of a wallet allocation returned because the buyer switched to online payment.
const SWITCH_TO_ONLINE_REMARK: &str = "用户改为在线支付，退回余额";
/// Remark of a wallet allocation returned after the gateway creation failed.
const CREATE_FAILED_REMARK: &str = "在线支付创建失败，退回余额";
/// Remark of a wallet allocation returned after a failed/expired payment.
const PAYMENT_FAILED_REMARK: &str = "在线支付失败，退回余额";
/// Remark of an underpaid payment credited to the wallet (PAY-02).
const UNDERPAID_REMARK: &str = "支付金额不足以完成订单，款项已转入余额";

fn payer_of(order: &Order) -> Option<Payer> {
    (order.user_id > 0).then(|| Payer {
        user_id: order.user_id,
        member_level_id: order.member_level_id.unwrap_or(0),
    })
}

/// Official WeChat/Alipay settle in CNY (`shouldUseCNYPaymentCurrency`).
fn forces_cny(channel: &PaymentChannel) -> bool {
    channel.is_official(channel_type::WECHAT) || channel.is_official(channel_type::ALIPAY)
}

fn has_pay_link(p: &payments::Model) -> bool {
    !p.pay_url.trim().is_empty() || !p.qr_code.trim().is_empty()
}

/// Product channel whitelist of every item of the order (PAY-11), inside the transaction.
async fn check_product_channels<C: ConnectionTrait>(
    conn: &C,
    order: &Order,
    channel_id: Id,
) -> Result<()> {
    let ids: Vec<Id> = order
        .all_items()
        .iter()
        .map(|i| i.product_id)
        .filter(|id| *id > 0)
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let lists: Vec<Vec<Id>> = products::Entity::find()
        .filter(products::Column::Id.is_in(ids))
        .all(conn)
        .await
        .dom()?
        .into_iter()
        .map(|p| decode_channel_ids(&p.payment_channel_ids))
        .collect();
    if product_allows(&product_channel_intersection(&lists), channel_id) {
        Ok(())
    } else {
        Err(Error::bad_request(
            pay_keys::CHANNEL_NOT_ALLOWED_FOR_PRODUCT,
        ))
    }
}

struct NewPayment<'a> {
    order: &'a Order,
    channel: Option<&'a PaymentChannel>,
    amount: Amount,
    fee_rate: Amount,
    fixed_fee: Amount,
    fee_amount: Amount,
    fee_policy: FeePolicy,
    currency: String,
    status: PaymentStatus,
    now: DateTime<Utc>,
}

async fn insert_payment<C: ConnectionTrait>(conn: &C, p: NewPayment<'_>) -> Result<Payment> {
    let (channel_id, provider_type, ch_type, mode, gateway_no) = match p.channel {
        Some(c) => (
            c.id,
            c.provider_type.clone(),
            c.channel_type.clone(),
            c.interaction_mode.clone(),
            new_gateway_order_no(p.now),
        ),
        None => (
            0,
            provider::WALLET.to_owned(),
            channel_type::BALANCE.to_owned(),
            InteractionMode::Balance.as_str().to_owned(),
            String::new(),
        ),
    };
    let paid_at = (p.status == PaymentStatus::Success).then_some(p.now);
    let row = payments::ActiveModel {
        order_id: Set(p.order.id),
        channel_id: Set(channel_id),
        provider_type: Set(provider_type),
        channel_type: Set(ch_type),
        interaction_mode: Set(mode),
        amount: Set(p.amount.decimal()),
        fee_rate: Set(p.fee_rate.decimal()),
        fixed_fee: Set(p.fixed_fee.decimal()),
        fee_amount: Set(p.fee_amount.decimal()),
        fee_policy: Set(p.fee_policy.as_str().to_owned()),
        currency: Set(p.currency),
        status: Set(p.status.as_str().to_owned()),
        exception_code: Set(String::new()),
        provider_ref: Set(String::new()),
        gateway_order_no: Set(gateway_no),
        provider_payload: Set(to_json(&serde_json::Map::new())?),
        pay_url: Set(String::new()),
        qr_code: Set(String::new()),
        created_at: Set(p.now),
        updated_at: Set(p.now),
        paid_at: Set(paid_at),
        expired_at: Set(None),
        superseded_at: Set(None),
        superseded_by_payment_id: Set(None),
        callback_at: Set(None),
        deleted_at: Set(None),
        ..Default::default()
    }
    .insert(conn)
    .await
    .map_err(|e| Error::internal(e).or_internal(keys::PAYMENT_CREATE_FAILED))?;
    Ok(payment_to_domain(row))
}

/// Open payments of the order other than `keep` become expired + superseded (PAY-03).
pub(crate) async fn supersede_in<C: ConnectionTrait>(
    conn: &C,
    order_id: Id,
    keep: Id,
    now: DateTime<Utc>,
) -> Result<u64> {
    if order_id <= 0 || keep <= 0 {
        return Ok(0);
    }
    Ok(payments::Entity::update_many()
        .col_expr(
            payments::Column::Status,
            Expr::value(PaymentStatus::Expired.as_str()),
        )
        .col_expr(payments::Column::ExpiredAt, Expr::value(Some(now)))
        .col_expr(payments::Column::SupersededAt, Expr::value(Some(now)))
        .col_expr(
            payments::Column::SupersededByPaymentId,
            Expr::value(Some(keep)),
        )
        .col_expr(payments::Column::UpdatedAt, Expr::value(now))
        .filter(payments::Column::DeletedAt.is_null())
        .filter(payments::Column::OrderId.eq(order_id))
        .filter(payments::Column::Id.ne(keep))
        .filter(payments::Column::Status.is_in([
            PaymentStatus::Initiated.as_str(),
            PaymentStatus::Pending.as_str(),
        ]))
        .exec(conn)
        .await
        .dom()?
        .rows_affected)
}

/// Posts the reseller profit of a freshly paid order (RSL-05, idempotent per order).
async fn post_profit<C: ConnectionTrait>(
    conn: &C,
    order: &Order,
    payment: &Payment,
    confirm_days: i64,
    now: DateTime<Utc>,
) -> Result<()> {
    if order.reseller_id.unwrap_or(0) <= 0 {
        return Ok(());
    }
    post_order_profit_in(
        conn,
        &OrderPaid {
            order_id: order.id,
            order_no: order.order_no.clone(),
            reseller_id: order.reseller_id,
            currency: order.currency.clone(),
            wallet_paid_amount: order.wallet_paid_amount,
            online_paid_amount: order.online_paid_amount,
            payment: Some(PaymentMeta {
                id: payment.id,
                channel_id: payment.channel_id,
                amount: payment.amount,
                status: payment.status.as_str().to_owned(),
            }),
        },
        now,
        confirm_days,
    )
    .await?;
    Ok(())
}

/// The `CreatePayment` transaction.
pub(crate) async fn begin_in<C: ConnectionTrait>(
    conn: &C,
    req: &BeginPayment,
) -> Result<BeginOutcome> {
    let now = req.now;
    let mut order = map::load_locked(conn, req.order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    if order.parent_id.is_some() {
        return Err(Error::bad_request(pay_keys::PAYMENT_INVALID));
    }
    if order.status != OrderStatus::PendingPayment || order.expires_at.is_some_and(|e| e <= now) {
        return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
    }

    let mut channel = None;
    let mut fee_rate = Decimal::ZERO;
    if req.channel_id != 0 {
        let ch = payment_channels::Entity::find_by_id(req.channel_id)
            .filter(payment_channels::Column::DeletedAt.is_null())
            .one(conn)
            .await
            .dom()?
            .map(channel_to_domain)
            .ok_or_else(|| Error::not_found(pay_keys::CHANNEL_NOT_FOUND))?;
        if !ch.is_active {
            return Err(Error::bad_request(pay_keys::CHANNEL_INACTIVE));
        }
        fee_rate = ch.fee_rate.decimal();
        if fee_rate < Decimal::ZERO || fee_rate > Decimal::ONE_HUNDRED {
            return Err(Error::bad_request(pay_keys::CHANNEL_CONFIG_INVALID));
        }
        check_order_channel(&ch, payer_of(&order))?;
        check_product_channels(conn, &order, ch.id).await?;

        let existing = payments::Entity::find()
            .filter(payments::Column::DeletedAt.is_null())
            .filter(payments::Column::OrderId.eq(order.id))
            .filter(payments::Column::ChannelId.eq(ch.id))
            .filter(payments::Column::Status.is_in([
                PaymentStatus::Initiated.as_str(),
                PaymentStatus::Pending.as_str(),
            ]))
            .filter(payments::Column::SupersededAt.is_null())
            .order_by_desc(payments::Column::Id)
            .all(conn)
            .await
            .dom()?
            .into_iter()
            .find(|p| p.expired_at.is_none_or(|e| e > now) && has_pay_link(p));
        if let Some(existing) = existing {
            let fee_positive = existing.fee_amount > Decimal::ZERO;
            let legacy = fee_positive
                && matches!(
                    FeePolicy::parse(&existing.fee_policy),
                    None | Some(FeePolicy::LegacyCustomerSurcharge)
                );
            if !legacy || req.reuse_legacy_fee_payment {
                return Ok(BeginOutcome::Reused {
                    payment: payment_to_domain(existing),
                    channel: ch,
                    order,
                });
            }
        }
        channel = Some(ch);
    }

    if req.use_balance {
        if order.user_id == 0 {
            return Err(Error::bad_request(pay_keys::PAYMENT_INVALID));
        }
        wallet::apply_order_balance(conn, &mut order, now).await?;
    } else if order.wallet_paid_amount.is_positive() {
        wallet::release_order_balance(
            conn,
            &mut order,
            txn_type::ORDER_REFUND,
            SWITCH_TO_ONLINE_REMARK,
            now,
        )
        .await?;
    }

    let online = required_online_amount(order.total_amount, order.wallet_paid_amount);
    if !online.is_positive() {
        let payment = insert_payment(
            conn,
            NewPayment {
                order: &order,
                channel: None,
                amount: order.wallet_paid_amount,
                fee_rate: Amount::ZERO,
                fixed_fee: Amount::ZERO,
                fee_amount: Amount::ZERO,
                fee_policy: FeePolicy::None,
                currency: order.currency.clone(),
                status: PaymentStatus::Success,
                now,
            },
        )
        .await?;
        supersede_in(conn, order.id, payment.id, now).await?;
        ops::mark_paid(conn, &mut order, now).await?;
        post_profit(conn, &order, &payment, req.reseller_confirm_days, now).await?;
        return Ok(BeginOutcome::PaidByWallet { payment, order });
    }
    let Some(ch) = channel else {
        return Err(if req.wallet_only {
            Error::bad_request(keys::WALLET_ONLY_PAYMENT_REQUIRED)
        } else {
            Error::bad_request(pay_keys::PAYMENT_INVALID)
        });
    };
    check_currency(&ch, &order.currency)?;
    check_amount(&ch, online)?;
    let fixed_fee = if ch.fixed_fee.is_positive() {
        ch.fixed_fee
    } else {
        Amount::ZERO
    };
    let amounts = calculate_payment_amounts(
        online,
        fee_rate,
        fixed_fee.decimal(),
        req.customer_fee_enabled,
    );
    let currency = if forces_cny(&ch) {
        SITE_CURRENCY_DEFAULT.to_owned()
    } else {
        order.currency.clone()
    };
    let payment = insert_payment(
        conn,
        NewPayment {
            order: &order,
            channel: Some(&ch),
            amount: amounts.payable,
            fee_rate: Amount::new(fee_rate),
            fixed_fee,
            fee_amount: amounts.fee,
            fee_policy: amounts.policy,
            currency,
            status: PaymentStatus::Initiated,
            now,
        },
    )
    .await?;
    orders::Entity::update_many()
        .col_expr(
            orders::Column::OnlinePaidAmount,
            Expr::value(online.decimal()),
        )
        .col_expr(orders::Column::UpdatedAt, Expr::value(now))
        .filter(orders::Column::Id.eq(order.id))
        .exec(conn)
        .await
        .dom()?;
    order.online_paid_amount = online;
    Ok(BeginOutcome::Created {
        payment,
        channel: ch,
        order,
    })
}

/// Persists the gateway creation result (the payment becomes `pending`).
pub(crate) async fn save_started_in<C: ConnectionTrait>(conn: &C, p: &Payment) -> Result<()> {
    payments::Entity::update_many()
        .col_expr(payments::Column::Status, Expr::value(p.status.as_str()))
        .col_expr(payments::Column::PayUrl, Expr::value(p.pay_url.clone()))
        .col_expr(payments::Column::QrCode, Expr::value(p.qr_code.clone()))
        .col_expr(
            payments::Column::ProviderRef,
            Expr::value(p.provider_ref.clone()),
        )
        .col_expr(
            payments::Column::ProviderPayload,
            Expr::value(to_json(&p.provider_payload)?),
        )
        .col_expr(
            payments::Column::GatewayOrderNo,
            Expr::value(p.gateway_order_no.clone()),
        )
        .col_expr(payments::Column::Amount, Expr::value(p.amount.decimal()))
        .col_expr(payments::Column::Currency, Expr::value(p.currency.clone()))
        .col_expr(payments::Column::UpdatedAt, Expr::value(p.updated_at))
        .filter(payments::Column::Id.eq(p.id))
        .filter(payments::Column::Status.eq(PaymentStatus::Initiated.as_str()))
        .exec(conn)
        .await
        .dom()?;
    Ok(())
}

/// Gateway creation failed: payment `failed`, the order's wallet part returned.
pub(crate) async fn fail_started_in<C: ConnectionTrait>(
    conn: &C,
    payment_id: Id,
    order_id: Id,
    now: DateTime<Utc>,
) -> Result<()> {
    payments::Entity::update_many()
        .col_expr(
            payments::Column::Status,
            Expr::value(PaymentStatus::Failed.as_str()),
        )
        .col_expr(payments::Column::UpdatedAt, Expr::value(now))
        .filter(payments::Column::Id.eq(payment_id))
        .filter(payments::Column::Status.is_in([
            PaymentStatus::Initiated.as_str(),
            PaymentStatus::Pending.as_str(),
        ]))
        .exec(conn)
        .await
        .dom()?;
    if let Some(mut order) = map::load_locked(conn, order_id).await?
        && order.status == OrderStatus::PendingPayment
    {
        wallet::release_order_balance(
            conn,
            &mut order,
            txn_type::ORDER_REFUND,
            CREATE_FAILED_REMARK,
            now,
        )
        .await?;
    }
    Ok(())
}

/// Applies a verified gateway result to an order payment under row locks
/// (`applyPaymentUpdate`, PAY-02/03/04): facts re-validated on the locked rows, first
/// success only pays the order, underpaid money goes to the wallet.
pub(crate) async fn settle_in(
    conn: &DatabaseTransaction,
    input: &CallbackInput,
    confirm_days: i64,
    now: DateTime<Utc>,
) -> Result<Settled> {
    let row = payments::Entity::find_by_id(input.payment_id)
        .filter(payments::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .ok_or_else(|| Error::not_found(pay_keys::PAYMENT_NOT_FOUND))?;
    let mut payment = payment_to_domain(row);
    if payment.order_id <= 0 {
        return Err(Error::bad_request(pay_keys::PAYMENT_INVALID));
    }
    let mut order = map::load_locked(conn, payment.order_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
    validate_callback_facts(&payment, &order.order_no, input)?;
    let previous = payment.status;
    let idle = Settled {
        payment: payment.clone(),
        order: None,
        order_paid: false,
        underpaid: false,
        recharge_credited: None,
    };
    if previous == PaymentStatus::Success || previous == input.status {
        apply_callback(&mut payment, input, now);
        save_callback_columns(conn, &payment, None).await?;
        return Ok(Settled {
            payment,
            order: Some(order),
            ..idle
        });
    }

    let open = order.status == OrderStatus::PendingPayment && order.paid_at.is_none();
    let required = required_online_amount(order.total_amount, order.wallet_paid_amount);
    let covered = covered_order_amount(&payment, input.amount);
    let success = input.status == PaymentStatus::Success;
    let underpaid = success && open && covered < required;
    let can_fulfill = success && open && !underpaid;

    let outcome = apply_callback(&mut payment, input, now);
    if success {
        payment.exception_code = success_exception_code(
            &payment,
            OrderPaymentState {
                open,
                paid: order.paid_at.is_some(),
                underpaid,
            },
        )
        .unwrap_or_default()
        .to_owned();
    }
    if matches!(outcome, CallbackOutcome::Transitioned { .. })
        && save_callback_columns(conn, &payment, Some(previous)).await? != 1
    {
        return Err(Error::internal_msg("payment row changed concurrently"));
    }
    if success {
        ops::expire_open_payments(conn, &[order.id], now).await?;
    }
    if can_fulfill {
        ops::mark_paid(conn, &mut order, now).await?;
        post_profit(conn, &order, &payment, confirm_days, now).await?;
    }
    if underpaid && order.user_id > 0 && covered.is_positive() {
        wallet::credit(
            conn,
            &BalanceChangeRequest {
                user_id: order.user_id,
                delta: covered,
                kind: txn_type::ORDER_UNDERPAID_CREDIT.to_owned(),
                reference: underpaid_reference(payment.id),
                remark: UNDERPAID_REMARK.to_owned(),
                currency: order.currency.clone(),
                operator_admin_id: None,
                order_id: Some(order.id),
            },
            now,
        )
        .await?;
    } else if underpaid {
        tracing::warn!(
            payment_id = payment.id,
            order_id = order.id,
            "payment_callback_underpaid_credit_skipped"
        );
    }
    if matches!(input.status, PaymentStatus::Failed | PaymentStatus::Expired)
        && order.status == OrderStatus::PendingPayment
    {
        wallet::release_order_balance(
            conn,
            &mut order,
            txn_type::ORDER_REFUND,
            PAYMENT_FAILED_REMARK,
            now,
        )
        .await?;
    }
    Ok(Settled {
        payment,
        order: Some(order),
        order_paid: can_fulfill,
        underpaid,
        recharge_credited: None,
    })
}
