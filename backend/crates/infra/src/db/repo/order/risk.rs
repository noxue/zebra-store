//! Risk lock keys and pending-order counts read inside the create transaction (RISK-01).

use std::collections::{BTreeSet, HashMap};

use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, PaginatorTrait, QueryFilter,
    QuerySelect, Set,
};
use zs_domain::order::model::OrderStatus;
use zs_domain::order::risk::{PendingCounts, PendingQuery};
use zs_domain::{Id, Result};

use crate::db::entity::{order_risk_lock_keys, orders};
use crate::db::repo::support::DbResultExt;

/// Locks the SHA-256 digests of `keys` in a fixed (sorted) order: insert-if-missing, then
/// `SELECT … FOR UPDATE` (`LockRiskKeys`). On SQLite the write of the insert already
/// serializes writers.
pub(crate) async fn lock_keys<C: ConnectionTrait>(
    conn: &C,
    keys: &[String],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<()> {
    let hashes: BTreeSet<String> = keys
        .iter()
        .map(|k| k.trim())
        .filter(|k| !k.is_empty())
        .map(|k| zs_shared::crypto::sha256_hex(k.as_bytes()))
        .collect();
    for hash in hashes {
        // Portable "insert if absent": `OnConflict::column(..).do_nothing()` renders
        // invalid SQL on MySQL (syntax error near IGNORE).
        crate::db::repo::support::insert_if_absent(
            conn,
            order_risk_lock_keys::ActiveModel {
                key_hash: Set(hash.clone()),
                created_at: Set(now),
            },
            order_risk_lock_keys::Column::KeyHash,
        )
        .await?;
        // Touch the row so SQLite takes the write lock even when the key existed.
        order_risk_lock_keys::Entity::update_many()
            .col_expr(
                order_risk_lock_keys::Column::CreatedAt,
                Expr::col(order_risk_lock_keys::Column::CreatedAt),
            )
            .filter(order_risk_lock_keys::Column::KeyHash.eq(hash.clone()))
            .exec(conn)
            .await
            .dom()?;
        order_risk_lock_keys::Entity::find_by_id(hash)
            .lock_exclusive()
            .one(conn)
            .await
            .dom()?;
    }
    Ok(())
}

fn pending_parents() -> sea_orm::Select<orders::Entity> {
    orders::Entity::find()
        .filter(orders::Column::DeletedAt.is_null())
        .filter(orders::Column::ParentId.is_null())
        .filter(orders::Column::Status.eq(OrderStatus::PendingPayment.as_str()))
}

#[derive(Debug, FromQueryResult)]
struct QtyRow {
    product_id: Id,
    quantity: Option<i64>,
}

/// Reads the counts [`zs_domain::order::risk::check_pending`] needs.
pub(crate) async fn pending_counts<C: ConnectionTrait>(
    conn: &C,
    q: &PendingQuery,
) -> Result<PendingCounts> {
    let mut counts = PendingCounts::default();
    if let Some(user_id) = q.user_id {
        counts.by_user = pending_parents()
            .filter(orders::Column::UserId.eq(user_id))
            .count(conn)
            .await
            .dom()? as i64;
    }
    if let Some(ip) = &q.guest_ip {
        counts.by_ip = pending_parents()
            .filter(orders::Column::RiskIp.eq(ip.clone()))
            .filter(orders::Column::UserId.eq(0))
            .count(conn)
            .await
            .dom()? as i64;
        if !q.guest_quantity_products.is_empty() {
            let placeholders = q
                .guest_quantity_products
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            let backend = conn.get_database_backend();
            let sql = format!(
                "SELECT oi.product_id AS product_id, CAST(COALESCE(SUM(oi.quantity), 0) AS {int}) AS quantity \
                 FROM order_items oi \
                 JOIN orders child ON child.id = oi.order_id AND child.deleted_at IS NULL \
                 JOIN orders parent ON parent.id = child.parent_id AND parent.deleted_at IS NULL \
                 WHERE oi.deleted_at IS NULL AND parent.risk_ip = {} AND parent.user_id = 0 \
                 AND parent.status = '{pending}' AND child.status = '{pending}' \
                 AND oi.product_id IN ({placeholders}) GROUP BY oi.product_id",
                match backend {
                    sea_orm::DbBackend::Postgres => "$1",
                    _ => "?",
                },
                pending = OrderStatus::PendingPayment.as_str(),
                int = match backend {
                    sea_orm::DbBackend::MySql => "SIGNED",
                    _ => "BIGINT",
                },
            );
            let rows = QtyRow::find_by_statement(sea_orm::Statement::from_sql_and_values(
                backend,
                sql,
                [ip.clone().into()],
            ))
            .all(conn)
            .await
            .dom()?;
            counts.guest_quantity = rows
                .into_iter()
                .map(|r| (r.product_id, r.quantity.unwrap_or(0)))
                .collect::<HashMap<_, _>>();
        }
    }
    if let Some(ip) = &q.member_ip {
        counts.by_ip = pending_parents()
            .filter(orders::Column::RiskIp.eq(ip.clone()))
            .filter(orders::Column::UserId.gt(0))
            .count(conn)
            .await
            .dom()? as i64;
    }
    Ok(counts)
}
