//! [`PaymentRepo`] backed by `payments` (plus read-only lookups of `orders` and
//! `wallet_recharge_orders` for business numbers).

use std::collections::HashMap;

use async_trait::async_trait;
use sea_orm::sea_query::Query;
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect,
};
use zs_domain::payment::model::{
    AdminPaymentFilter, Payment, PaymentRefs, PaymentRepo, RechargeRef,
};
use zs_domain::payment::types::PaymentStatus;
use zs_domain::{Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{orders, payments, wallet_recharge_orders};
use crate::db::repo::support::{DbResultExt, from_json};

/// SeaORM implementation of [`PaymentRepo`].
#[derive(Debug, Clone)]
pub struct SeaPaymentRepo {
    db: DatabaseConnection,
}

impl SeaPaymentRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

pub(crate) fn to_domain(m: payments::Model) -> Payment {
    let status = PaymentStatus::parse(&m.status).unwrap_or_else(|| {
        tracing::warn!(payment_id = m.id, status = %m.status, "payment_status_unknown");
        PaymentStatus::Pending
    });
    Payment {
        id: m.id,
        order_id: m.order_id,
        channel_id: m.channel_id,
        provider_type: m.provider_type,
        channel_type: m.channel_type,
        interaction_mode: m.interaction_mode,
        amount: Amount::new(m.amount),
        fee_rate: Amount::new(m.fee_rate),
        fixed_fee: Amount::new(m.fixed_fee),
        fee_amount: Amount::new(m.fee_amount),
        fee_policy: m.fee_policy,
        currency: m.currency,
        status,
        exception_code: m.exception_code,
        provider_ref: m.provider_ref,
        gateway_order_no: m.gateway_order_no,
        provider_payload: from_json(m.provider_payload),
        pay_url: m.pay_url,
        qr_code: m.qr_code,
        created_at: m.created_at,
        updated_at: m.updated_at,
        paid_at: m.paid_at,
        expired_at: m.expired_at,
        superseded_at: m.superseded_at,
        superseded_by_payment_id: m.superseded_by_payment_id,
        callback_at: m.callback_at,
    }
}

fn alive() -> Condition {
    Condition::all().add(payments::Column::DeletedAt.is_null())
}

/// Business number of a payment using any connection (transaction-safe, DB-01).
pub(crate) async fn business_no_on<C: ConnectionTrait>(
    conn: &C,
    payment: &Payment,
) -> Result<Option<String>> {
    if payment.order_id > 0 {
        let row = orders::Entity::find_by_id(payment.order_id)
            .filter(orders::Column::DeletedAt.is_null())
            .one(conn)
            .await
            .dom()?;
        return Ok(row.map(|o| o.order_no));
    }
    let row = wallet_recharge_orders::Entity::find()
        .filter(wallet_recharge_orders::Column::PaymentId.eq(payment.id))
        .filter(wallet_recharge_orders::Column::DeletedAt.is_null())
        .one(conn)
        .await
        .dom()?;
    Ok(row.map(|r| r.recharge_no))
}

async fn latest_by<C: ConnectionTrait>(
    conn: &C,
    col: payments::Column,
    value: &str,
) -> Result<Option<Payment>> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let row = payments::Entity::find()
        .filter(alive())
        .filter(col.eq(value))
        .order_by_desc(payments::Column::Id)
        .one(conn)
        .await
        .dom()?;
    Ok(row.map(to_domain))
}

#[async_trait]
impl PaymentRepo for SeaPaymentRepo {
    async fn get(&self, id: Id) -> Result<Option<Payment>> {
        if id <= 0 {
            return Ok(None);
        }
        let row = payments::Entity::find_by_id(id)
            .filter(alive())
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(to_domain))
    }

    async fn find_by_gateway_order_no(&self, gateway_order_no: &str) -> Result<Option<Payment>> {
        latest_by(&self.db, payments::Column::GatewayOrderNo, gateway_order_no).await
    }

    async fn find_latest_by_provider_ref(&self, provider_ref: &str) -> Result<Option<Payment>> {
        latest_by(&self.db, payments::Column::ProviderRef, provider_ref).await
    }

    async fn list_admin(&self, f: &AdminPaymentFilter) -> Result<(Vec<Payment>, u64)> {
        let mut cond = alive();
        if f.user_id != 0 {
            let order_ids = Query::select()
                .column(orders::Column::Id)
                .from(orders::Entity)
                .and_where(orders::Column::UserId.eq(f.user_id))
                .and_where(orders::Column::DeletedAt.is_null())
                .to_owned();
            let recharge_payments = Query::select()
                .column(wallet_recharge_orders::Column::PaymentId)
                .from(wallet_recharge_orders::Entity)
                .and_where(wallet_recharge_orders::Column::UserId.eq(f.user_id))
                .to_owned();
            cond = cond.add(
                Condition::any()
                    .add(payments::Column::OrderId.in_subquery(order_ids))
                    .add(payments::Column::Id.in_subquery(recharge_payments)),
            );
        }
        if f.order_id != 0 {
            cond = cond.add(payments::Column::OrderId.eq(f.order_id));
        }
        if f.channel_id != 0 {
            cond = cond.add(payments::Column::ChannelId.eq(f.channel_id));
        }
        for (col, value) in [
            (payments::Column::ProviderType, &f.provider_type),
            (payments::Column::ChannelType, &f.channel_type),
            (payments::Column::Status, &f.status),
        ] {
            if !value.is_empty() {
                cond = cond.add(col.eq(value.clone()));
            }
        }
        if let Some(from) = f.created_from {
            cond = cond.add(payments::Column::CreatedAt.gte(from));
        }
        if let Some(to) = f.created_to {
            cond = cond.add(payments::Column::CreatedAt.lte(to));
        }
        let total = if f.skip_count {
            0
        } else {
            payments::Entity::find()
                .filter(cond.clone())
                .count(&self.db)
                .await
                .dom()?
        };
        let rows = payments::Entity::find()
            .filter(cond)
            .order_by_desc(payments::Column::Id)
            .offset(f.page.offset())
            .limit(f.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok((rows.into_iter().map(to_domain).collect(), total))
    }

    async fn refs(&self, list: &[Payment]) -> Result<PaymentRefs> {
        let mut order_ids: Vec<Id> = list
            .iter()
            .map(|p| p.order_id)
            .filter(|id| *id > 0)
            .collect();
        order_ids.sort_unstable();
        order_ids.dedup();
        let payment_ids: Vec<Id> = list.iter().map(|p| p.id).filter(|id| *id > 0).collect();
        let mut refs = PaymentRefs::default();
        if !order_ids.is_empty() {
            let rows = orders::Entity::find()
                .filter(orders::Column::Id.is_in(order_ids))
                .filter(orders::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?;
            refs.order_nos = rows
                .into_iter()
                .map(|o| (o.id, o.order_no.trim().to_owned()))
                .collect();
        }
        if !payment_ids.is_empty() {
            let rows = wallet_recharge_orders::Entity::find()
                .filter(wallet_recharge_orders::Column::PaymentId.is_in(payment_ids))
                .filter(wallet_recharge_orders::Column::DeletedAt.is_null())
                .all(&self.db)
                .await
                .dom()?;
            refs.recharges = rows
                .into_iter()
                .map(|r| {
                    (
                        r.payment_id,
                        RechargeRef {
                            recharge_no: r.recharge_no.trim().to_owned(),
                            status: r.status.trim().to_owned(),
                            user_id: r.user_id,
                        },
                    )
                })
                .collect::<HashMap<_, _>>();
        }
        Ok(refs)
    }

    async fn business_no(&self, payment: &Payment) -> Result<Option<String>> {
        business_no_on(&self.db, payment).await
    }
}
