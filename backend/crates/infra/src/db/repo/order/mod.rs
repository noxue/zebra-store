//! Order repositories and transactional stores: creation, cancellation, status changes,
//! payments and settlement, delivery and refunds. Every store method is one database
//! transaction whose reads and writes all use the transaction handle (DB-01).

pub mod cart;
pub(crate) mod create;
pub(crate) mod fulfill;
pub(crate) mod map;
pub(crate) mod ops;
pub(crate) mod pay;
pub mod read;
pub(crate) mod refund;
pub(crate) mod risk;
pub(crate) mod wallet;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::{DatabaseConnection, TransactionTrait};
use zs_domain::integration::hooks::UpstreamDelivery;
use zs_domain::order::model::{Fulfillment, JsonMap, Order, OrderStatus, RefundRecord, keys};
use zs_domain::order::ports::{
    BeginOutcome, BeginPayment, NewOrder, OrderPaymentStore, OrderStore, OrderWallet, RefundDone,
    RefundRequest, Settled, UpstreamDelivered,
};
use zs_domain::order::status::is_transition_allowed;
use zs_domain::payment::callback::CallbackInput;
use zs_domain::payment::model::Payment;
use zs_domain::wallet::model::txn_type;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use crate::db::repo::catalog::ordering::release_secrets_in;
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`OrderStore`], [`OrderPaymentStore`] and [`OrderWallet`].
#[derive(Clone)]
pub struct SeaOrderStore {
    db: DatabaseConnection,
    /// Key of the guest credential digests (`app.secret_key`, ORD-02).
    guest_secret: String,
}

impl std::fmt::Debug for SeaOrderStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeaOrderStore")
    }
}

impl SeaOrderStore {
    pub fn new(db: DatabaseConnection, guest_secret: &str) -> Self {
        Self {
            db,
            guest_secret: guest_secret.trim().to_owned(),
        }
    }
}

/// Runs `$body` in a transaction, retrying the whole transaction on transient
/// concurrency errors (deadlock victim, serialization failure, SQLite busy).
macro_rules! in_txn {
    ($self:ident, |$txn:ident| $body:expr) => {{
        let mut attempt = 0_u32;
        loop {
            attempt += 1;
            let $txn = $self.db.begin().await.dom()?;
            let result = match $body {
                Ok(v) => $txn.commit().await.dom().map(|()| v),
                Err(e) => {
                    let _ = $txn.rollback().await;
                    Err(e)
                }
            };
            match result {
                Err(e)
                    if attempt < crate::db::repo::support::MAX_TXN_ATTEMPTS
                        && crate::db::repo::support::is_transient(&e) =>
                {
                    tracing::warn!(attempt, error = %e, "retrying transaction after transient error");
                    tokio::time::sleep(crate::db::repo::support::txn_backoff(attempt)).await;
                }
                other => break other,
            }
        }
    }};
}

#[async_trait]
impl OrderStore for SeaOrderStore {
    async fn create(&self, new: &NewOrder, now: DateTime<Utc>) -> Result<Order> {
        if self.guest_secret.is_empty() && new.order.user_id == 0 {
            return Err(Error::internal_msg("guest credential secret is required"));
        }
        in_txn!(self, |txn| create::create_in(
            &txn,
            new,
            &self.guest_secret,
            now
        )
        .await)
    }

    async fn cancel(&self, order_id: Id, now: DateTime<Utc>) -> Result<Order> {
        in_txn!(self, |txn| {
            async {
                let mut order = map::load_locked(&txn, order_id)
                    .await?
                    .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
                match order.parent_id {
                    None => ops::cancel_locked(&txn, &mut order, now).await?,
                    Some(parent) => {
                        // `cancelSingleOrderInTx`: one child of a parent.
                        if order.status != OrderStatus::PendingPayment {
                            return Err(Error::bad_request(keys::ORDER_CANCEL_NOT_ALLOWED));
                        }
                        ops::write_status(&txn, order.id, OrderStatus::Canceled, now).await?;
                        use sea_orm::sea_query::Expr;
                        use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
                        crate::db::entity::orders::Entity::update_many()
                            .col_expr(
                                crate::db::entity::orders::Column::CanceledAt,
                                Expr::value(Some(now)),
                            )
                            .filter(crate::db::entity::orders::Column::Id.eq(order.id))
                            .exec(&txn)
                            .await
                            .dom()?;
                        release_secrets_in(&txn, order.id, now).await?;
                        ops::release_manual_stock(&txn, &order.items).await?;
                        wallet::release_order_balance(
                            &txn,
                            &mut order,
                            txn_type::ORDER_REFUND,
                            "订单取消退回余额",
                            now,
                        )
                        .await?;
                        ops::expire_open_payments(&txn, &[order.id], now).await?;
                        ops::sync_parent(&txn, parent, now).await?;
                        order.status = OrderStatus::Canceled;
                        order.canceled_at = Some(now);
                    }
                }
                Ok(order)
            }
            .await
        })
    }

    async fn set_status(
        &self,
        order_id: Id,
        target: OrderStatus,
        now: DateTime<Utc>,
    ) -> Result<()> {
        in_txn!(self, |txn| {
            async {
                let order = map::load_locked(&txn, order_id)
                    .await?
                    .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
                if !is_transition_allowed(order.status, target) {
                    return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
                }
                ops::write_status(&txn, order.id, target, now).await?;
                if let Some(parent) = order.parent_id {
                    ops::sync_parent(&txn, parent, now).await?;
                }
                Ok(())
            }
            .await
        })
    }

    async fn set_parent_status(
        &self,
        order_id: Id,
        target: OrderStatus,
        now: DateTime<Utc>,
    ) -> Result<()> {
        in_txn!(self, |txn| {
            async {
                let order = map::load_locked(&txn, order_id)
                    .await?
                    .ok_or_else(|| Error::not_found(keys::ORDER_NOT_FOUND))?;
                let invalid = || Error::bad_request(keys::ORDER_STATUS_INVALID);
                match target {
                    OrderStatus::Completed => {
                        if !zs_domain::order::status::can_complete_parent(&order) {
                            return Err(invalid());
                        }
                        ops::write_status(&txn, order.id, target, now).await?;
                        for child in &order.children {
                            match child.status {
                                OrderStatus::Completed => {}
                                OrderStatus::Delivered => {
                                    ops::write_status(&txn, child.id, target, now).await?
                                }
                                _ => return Err(invalid()),
                            }
                        }
                    }
                    OrderStatus::PartiallyRefunded | OrderStatus::Refunded => {
                        ops::write_status(&txn, order.id, target, now).await?;
                        for child in &order.children {
                            if child.status == target {
                                continue;
                            }
                            if !is_transition_allowed(child.status, target) {
                                return Err(invalid());
                            }
                            ops::write_status(&txn, child.id, target, now).await?;
                        }
                    }
                    _ => return Err(invalid()),
                }
                Ok(())
            }
            .await
        })
    }

    async fn sync_parent(&self, parent_id: Id, now: DateTime<Utc>) -> Result<Option<OrderStatus>> {
        in_txn!(self, |txn| ops::sync_parent(&txn, parent_id, now).await)
    }

    async fn auto_fulfill(&self, order_id: Id, now: DateTime<Utc>) -> Result<Fulfillment> {
        in_txn!(self, |txn| fulfill::auto_in(&txn, order_id, now).await)
    }

    async fn prepare_auto_fulfill(&self, order_id: Id, now: DateTime<Utc>) -> Result<String> {
        in_txn!(self, |txn| fulfill::prepare_auto_in(&txn, order_id, now)
            .await)
    }

    async fn finalize_auto_fulfill(
        &self,
        order_id: Id,
        payload: &str,
        now: DateTime<Utc>,
    ) -> Result<Fulfillment> {
        in_txn!(self, |txn| fulfill::finalize_auto_in(
            &txn, order_id, payload, now
        )
        .await)
    }

    async fn manual_fulfill(
        &self,
        order_id: Id,
        admin_id: Id,
        payload: &str,
        delivery_data: &JsonMap,
        delivered_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Fulfillment> {
        in_txn!(self, |txn| fulfill::manual_in(
            &txn,
            order_id,
            admin_id,
            payload,
            delivery_data,
            delivered_at,
            now
        )
        .await)
    }

    async fn upstream_fulfilling(&self, order_id: Id, now: DateTime<Utc>) -> Result<bool> {
        in_txn!(self, |txn| fulfill::upstream_move_in(
            &txn,
            order_id,
            OrderStatus::Paid,
            OrderStatus::Fulfilling,
            now
        )
        .await)
    }

    async fn upstream_deliver(
        &self,
        order_id: Id,
        delivery: &UpstreamDelivery,
        now: DateTime<Utc>,
    ) -> Result<Option<UpstreamDelivered>> {
        in_txn!(self, |txn| fulfill::upstream_in(
            &txn, order_id, delivery, now
        )
        .await)
    }

    async fn upstream_rollback(&self, order_id: Id, now: DateTime<Utc>) -> Result<bool> {
        in_txn!(self, |txn| fulfill::upstream_move_in(
            &txn,
            order_id,
            OrderStatus::Fulfilling,
            OrderStatus::Paid,
            now
        )
        .await)
    }

    async fn refund(&self, request: &RefundRequest) -> Result<RefundDone> {
        in_txn!(self, |txn| refund::refund_in(&txn, request).await)
    }

    async fn set_refund_fee_flag(
        &self,
        record_id: Id,
        refunded: bool,
        now: DateTime<Utc>,
    ) -> Result<RefundRecord> {
        in_txn!(self, |txn| refund::set_fee_flag_in(
            &txn, record_id, refunded, now
        )
        .await)
    }
}

#[async_trait]
impl OrderPaymentStore for SeaOrderStore {
    async fn begin(&self, request: &BeginPayment) -> Result<BeginOutcome> {
        in_txn!(self, |txn| pay::begin_in(&txn, request).await)
    }

    async fn save_started(&self, payment: &Payment) -> Result<()> {
        pay::save_started_in(&self.db, payment).await
    }

    async fn fail_started(&self, payment_id: Id, order_id: Id, now: DateTime<Utc>) -> Result<()> {
        in_txn!(self, |txn| pay::fail_started_in(
            &txn, payment_id, order_id, now
        )
        .await)
    }

    async fn supersede_others(
        &self,
        order_id: Id,
        keep_payment_id: Id,
        now: DateTime<Utc>,
    ) -> Result<u64> {
        pay::supersede_in(&self.db, order_id, keep_payment_id, now).await
    }

    async fn settle(
        &self,
        input: &CallbackInput,
        reseller_confirm_days: i64,
        now: DateTime<Utc>,
    ) -> Result<Settled> {
        in_txn!(self, |txn| pay::settle_in(
            &txn,
            input,
            reseller_confirm_days,
            now
        )
        .await)
    }
}

#[async_trait]
impl OrderWallet for SeaOrderStore {
    async fn balance(&self, user_id: Id) -> Result<Amount> {
        wallet::balance_of(&self.db, user_id).await
    }
}
