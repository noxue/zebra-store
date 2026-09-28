//! Helpers shared by repository implementations.

use chrono::Utc;
use sea_orm::DbErr;
use serde::Serialize;
use serde::de::DeserializeOwned;
use zs_domain::{Error, Result};

/// Converts sea-orm results into domain results (internal errors).
pub trait DbResultExt<T> {
    fn dom(self) -> Result<T>;
}

impl<T> DbResultExt<T> for std::result::Result<T, DbErr> {
    fn dom(self) -> Result<T> {
        self.map_err(Error::internal)
    }
}

/// Reads an optional JSON column into a typed value, defaulting on NULL or shape mismatch.
pub fn from_json<T: DeserializeOwned + Default>(value: Option<sea_orm::JsonValue>) -> T {
    value
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default()
}

/// Serializes a value into a JSON column.
pub fn to_json<T: Serialize>(value: &T) -> Result<Option<sea_orm::JsonValue>> {
    Ok(Some(serde_json::to_value(value)?))
}

/// Current UTC time for `created_at` / `updated_at` columns.
pub fn now() -> chrono::DateTime<Utc> {
    Utc::now()
}

/// Inserts `model` unless a unique key already holds the row; returns whether a row
/// was inserted. Works identically on every backend:
///
/// - PostgreSQL / SQLite: `INSERT … ON CONFLICT DO NOTHING` reports 0 rows on conflict.
/// - MySQL: sea-query renders `ON DUPLICATE KEY UPDATE id = id`, and with
///   `CLIENT_FOUND_ROWS` (always set by sqlx) a duplicate *also* reports 1 row, so
///   idempotency checks silently pass twice. There a plain insert is used and a
///   duplicate-key error (which only rolls back the statement, not the transaction)
///   means "already present".
pub async fn insert_if_absent<C, A>(
    conn: &C,
    model: A,
    pk: impl sea_orm::sea_query::IntoIden,
) -> Result<bool>
where
    C: sea_orm::ConnectionTrait,
    A: sea_orm::ActiveModelTrait + Send,
{
    use sea_orm::{DbBackend, EntityTrait, SqlErr, TryInsertResult};
    if conn.get_database_backend() == DbBackend::MySql {
        return match <A::Entity as EntityTrait>::insert(model)
            .exec_without_returning(conn)
            .await
        {
            Ok(n) => Ok(n > 0),
            Err(e) if matches!(e.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) => {
                Ok(false)
            }
            Err(e) => Err(Error::internal(e)),
        };
    }
    let mut on_conflict = sea_orm::sea_query::OnConflict::new();
    on_conflict.do_nothing_on([pk]);
    let res = <A::Entity as EntityTrait>::insert(model)
        .on_conflict(on_conflict)
        .try_insert()
        .exec_without_returning(conn)
        .await
        .dom()?;
    Ok(matches!(res, TryInsertResult::Inserted(n) if n > 0))
}

/// Attempts for a transaction that fails with a transient concurrency error.
pub const MAX_TXN_ATTEMPTS: u32 = 3;

/// True for errors that a retry of the whole transaction can resolve: MySQL deadlocks
/// (1213) and lock-wait timeouts (1205), PostgreSQL deadlocks (40P01) and
/// serialization failures (40001), SQLite `database is locked`.
///
/// Concurrent `INSERT … ON DUPLICATE KEY` on the same unique key (order risk locks)
/// makes InnoDB pick a deadlock victim; the original relies on a retry for this too.
pub fn is_transient(err: &Error) -> bool {
    let text = err.to_string().to_ascii_lowercase();
    [
        "deadlock",
        "could not serialize",
        "lock wait timeout",
        "database is locked",
    ]
    .iter()
    .any(|needle| text.contains(needle))
}

/// Backoff before retry `attempt` (1-based): 20 ms, 40 ms, …
pub fn txn_backoff(attempt: u32) -> std::time::Duration {
    std::time::Duration::from_millis(20 * u64::from(attempt))
}

#[cfg(test)]
mod transient_tests {
    use super::*;

    #[test]
    fn detects_backend_concurrency_errors() {
        let mysql = Error::internal_msg(
            "Execution Error: 1213 (40001): Deadlock found when trying to get lock",
        );
        let pg = Error::internal_msg(
            "error returned from database: could not serialize access due to concurrent update",
        );
        let sqlite =
            Error::internal_msg("error returned from database: (code: 5) database is locked");
        assert!(is_transient(&mysql) && is_transient(&pg) && is_transient(&sqlite));
        assert!(!is_transient(&Error::bad_request(
            "error.order_item_invalid"
        )));
    }
}
