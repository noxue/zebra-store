//! MySQL timestamp columns → `DATETIME(6)`.
//!
//! sea-orm maps `DateTimeUtc` to MySQL `TIMESTAMP`, which (a) stores whole seconds
//! and *rounds* fractional seconds, and (b) ends in 2038. Rounding broke token
//! revocation: `token_invalid_before = 12:00:57.6` was stored as `:58`, so a token
//! issued right after a password change (`iat = :57`) was rejected. `DATETIME(6)` keeps
//! microseconds and has no 2038 limit. Values stay UTC (the driver writes UTC).
//!
//! Idempotent: only columns that are still `timestamp` or `datetime` with precision
//! below 6 are altered. Schema sync afterwards only warns about the type difference.

use sea_orm_migration::sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement,
};

/// Columns needing conversion, straight from `information_schema`.
const FIND_SQL: &str = "SELECT TABLE_NAME AS table_name, COLUMN_NAME AS column_name, IS_NULLABLE AS is_nullable \
    FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() \
    AND (DATA_TYPE = 'timestamp' OR (DATA_TYPE = 'datetime' AND DATETIME_PRECISION < 6))";

/// One `ALTER TABLE` converting all listed `(column, nullable)` pairs, so InnoDB
/// rebuilds each table once instead of once per column (212 rebuilds made fresh
/// databases take minutes).
pub fn alter_sql(table: &str, columns: &[(String, bool)]) -> String {
    let parts: Vec<String> = columns
        .iter()
        .map(|(column, nullable)| {
            let null = if *nullable { "NULL" } else { "NOT NULL" };
            format!("MODIFY `{column}` DATETIME(6) {null}")
        })
        .collect();
    format!("ALTER TABLE `{table}` {}", parts.join(", "))
}

/// Converts every remaining MySQL timestamp column (no-op on other backends).
pub async fn datetime_precision(db: &DatabaseConnection) -> Result<(), DbErr> {
    if db.get_database_backend() != DbBackend::MySql {
        return Ok(());
    }
    let rows = db
        .query_all_raw(Statement::from_string(DbBackend::MySql, FIND_SQL))
        .await?;
    let mut by_table: std::collections::BTreeMap<String, Vec<(String, bool)>> =
        std::collections::BTreeMap::new();
    for row in rows {
        let table: String = row.try_get("", "table_name")?;
        let column: String = row.try_get("", "column_name")?;
        let nullable: String = row.try_get("", "is_nullable")?;
        by_table
            .entry(table)
            .or_default()
            .push((column, nullable == "YES"));
    }
    for (table, columns) in by_table {
        db.execute_unprepared(&alter_sql(&table, &columns)).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::alter_sql;

    #[test]
    fn one_statement_per_table_keeps_nullability() {
        let cols = vec![
            ("deleted_at".to_owned(), true),
            ("created_at".to_owned(), false),
        ];
        assert_eq!(
            alter_sql("users", &cols),
            "ALTER TABLE `users` MODIFY `deleted_at` DATETIME(6) NULL, MODIFY `created_at` DATETIME(6) NOT NULL"
        );
    }
}
