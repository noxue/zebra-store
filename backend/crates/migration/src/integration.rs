//! Unique index of downstream order references (UPS-10): one order per
//! `(api_credential_id, downstream_order_no)` when the buyer sends an order number,
//! so concurrent retries of `POST /upstream/orders` cannot create two orders.
//!
//! SQLite / PostgreSQL use a partial index (`WHERE downstream_order_no <> ''`); MySQL
//! gets a virtual column that is `NULL` for empty numbers (never a duplicate) and a
//! plain unique index over it.

use sea_orm_migration::sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement,
};

/// Index name.
pub const INDEX: &str = "idx_downstream_refs_credential_no";
/// MySQL generated column.
pub const MYSQL_KEY_COLUMN: &str = "downstream_no_key";

/// SQLite / PostgreSQL statement.
pub fn partial_index_sql() -> String {
    format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS {INDEX} ON downstream_order_refs(api_credential_id, downstream_order_no) WHERE downstream_order_no <> ''"
    )
}

/// MySQL generated column.
pub fn mysql_column_sql() -> String {
    format!(
        "ALTER TABLE downstream_order_refs ADD COLUMN {MYSQL_KEY_COLUMN} VARCHAR(64) GENERATED ALWAYS AS (IF(downstream_order_no = '', NULL, downstream_order_no)) VIRTUAL"
    )
}

/// MySQL index.
pub fn mysql_index_sql() -> String {
    format!(
        "CREATE UNIQUE INDEX {INDEX} ON downstream_order_refs(api_credential_id, {MYSQL_KEY_COLUMN})"
    )
}

async fn mysql_exists(db: &DatabaseConnection, sql: &str, value: &str) -> Result<bool, DbErr> {
    let stmt = Statement::from_sql_and_values(
        DbBackend::MySql,
        sql,
        ["downstream_order_refs".into(), value.into()],
    );
    Ok(db.query_one_raw(stmt).await?.is_some())
}

/// Creates the index when missing (idempotent).
pub async fn downstream_ref_index(db: &DatabaseConnection) -> Result<(), DbErr> {
    match db.get_database_backend() {
        DbBackend::MySql => {
            if !mysql_exists(
                db,
                "SELECT 1 FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND COLUMN_NAME = ?",
                MYSQL_KEY_COLUMN,
            )
            .await?
            {
                db.execute_unprepared(&mysql_column_sql()).await?;
            }
            if !mysql_exists(
                db,
                "SELECT 1 FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND INDEX_NAME = ?",
                INDEX,
            )
            .await?
            {
                db.execute_unprepared(&mysql_index_sql()).await?;
            }
        }
        _ => {
            db.execute_unprepared(&partial_index_sql()).await?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements() {
        assert!(partial_index_sql().ends_with("WHERE downstream_order_no <> ''"));
        assert!(mysql_column_sql().contains("IF(downstream_order_no = '', NULL"));
        assert!(mysql_index_sql().contains("(api_credential_id, downstream_no_key)"));
    }
}
