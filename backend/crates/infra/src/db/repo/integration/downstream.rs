//! [`OrderRefRepo`] on `downstream_order_refs`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter,
    QueryOrder, Set,
};
use zs_domain::integration::downstream::{CallbackStatus, NewOrderRef, OrderRef, OrderRefRepo};
use zs_domain::{Error, Id, Result};

use crate::db::entity::downstream_order_refs;
use crate::db::repo::support::DbResultExt;

fn to_domain(m: downstream_order_refs::Model) -> OrderRef {
    OrderRef {
        id: m.id,
        order_id: m.order_id,
        api_credential_id: m.api_credential_id,
        downstream_order_no: m.downstream_order_no,
        callback_url: m.callback_url,
        trace_id: m.trace_id,
        callback_status: CallbackStatus::from_stored(&m.callback_status),
        callback_retry_count: m.callback_retry_count,
        last_callback_at: m.last_callback_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

/// Inserts a downstream reference on the caller's connection / transaction.
///
/// **For the order group**: call it inside the transaction that creates the order of
/// `POST /upstream/orders`. A non-empty `downstream_order_no` is unique per credential
/// (index `idx_downstream_refs_credential_no`, UPS-10); a concurrent duplicate fails
/// with the returned `error.duplicate_downstream_order` so the caller can roll back and
/// answer with the existing order.
pub async fn insert_ref_in<C: ConnectionTrait>(
    conn: &C,
    new: &NewOrderRef,
    now: DateTime<Utc>,
) -> Result<OrderRef> {
    let row = downstream_order_refs::ActiveModel {
        order_id: Set(new.order_id),
        api_credential_id: Set(new.api_credential_id),
        downstream_order_no: Set(new.downstream_order_no.clone()),
        callback_url: Set(new.callback_url.clone()),
        trace_id: Set(new.trace_id.clone()),
        callback_status: Set(CallbackStatus::Pending.as_str().to_owned()),
        callback_retry_count: Set(0),
        last_callback_at: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    }
    .insert(conn)
    .await;
    match row {
        Ok(r) => Ok(to_domain(r)),
        Err(e) if is_unique_violation(&e) => Err(Error::bad_request(DUPLICATE_KEY)),
        Err(e) => Err(Error::internal(e)),
    }
}

/// Key of a duplicate `(credential, downstream_order_no)`.
pub const DUPLICATE_KEY: &str = "error.duplicate_downstream_order";

fn is_unique_violation(e: &sea_orm::DbErr) -> bool {
    matches!(
        e.sql_err(),
        Some(sea_orm::SqlErr::UniqueConstraintViolation(_))
    )
}

/// SeaORM implementation of [`OrderRefRepo`].
#[derive(Debug, Clone)]
pub struct SeaOrderRefRepo {
    db: DatabaseConnection,
}

impl SeaOrderRefRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl OrderRefRepo for SeaOrderRefRepo {
    async fn get(&self, id: Id) -> Result<Option<OrderRef>> {
        Ok(downstream_order_refs::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn get_by_order(&self, order_id: Id) -> Result<Option<OrderRef>> {
        Ok(downstream_order_refs::Entity::find()
            .filter(downstream_order_refs::Column::OrderId.eq(order_id))
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn find_by_downstream_no(
        &self,
        credential_id: Id,
        downstream_order_no: &str,
    ) -> Result<Option<OrderRef>> {
        if credential_id <= 0 || downstream_order_no.is_empty() {
            return Ok(None);
        }
        Ok(downstream_order_refs::Entity::find()
            .filter(downstream_order_refs::Column::ApiCredentialId.eq(credential_id))
            .filter(downstream_order_refs::Column::DownstreamOrderNo.eq(downstream_order_no))
            .order_by_asc(downstream_order_refs::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(to_domain))
    }

    async fn update_delivery(
        &self,
        id: Id,
        status: CallbackStatus,
        retry_count: i32,
        last_callback_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<()> {
        downstream_order_refs::Entity::update_many()
            .col_expr(
                downstream_order_refs::Column::CallbackStatus,
                Expr::value(status.as_str()),
            )
            .col_expr(
                downstream_order_refs::Column::CallbackRetryCount,
                Expr::value(retry_count),
            )
            .col_expr(
                downstream_order_refs::Column::LastCallbackAt,
                Expr::value(last_callback_at),
            )
            .col_expr(downstream_order_refs::Column::UpdatedAt, Expr::value(now))
            .filter(downstream_order_refs::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}
