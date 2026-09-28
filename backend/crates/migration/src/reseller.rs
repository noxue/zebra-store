//! Partial unique indexes of the reseller tables (`resellerstore.Migrate`).
//!
//! The original creates `UNIQUE ... WHERE deleted_at IS NULL` so a soft-deleted row
//! never blocks re-creating the same value:
//!
//! | index | columns |
//! |---|---|
//! | `idx_reseller_domains_active_domain` | `reseller_domains(domain)` |
//! | `idx_reseller_site_configs_active_reseller` | `reseller_site_configs(reseller_id)` |
//! | `idx_reseller_product_settings_active_scope` | `reseller_product_settings(reseller_id, product_id, sku_id)` |
//! | `idx_reseller_balance_accounts_active_currency` | `reseller_balance_accounts(reseller_id, currency)` |
//! | `idx_reseller_related_accounts_active_user` | `reseller_related_accounts(reseller_id, user_id)` |
//!
//! SQLite and PostgreSQL support partial indexes directly (`CREATE UNIQUE INDEX IF NOT
//! EXISTS … WHERE deleted_at IS NULL`). MySQL has no partial indexes, so each table
//! gets a virtual generated column `live_marker = IF(deleted_at IS NULL, 1, NULL)` and
//! a plain unique index on `(columns…, live_marker)`: live rows share the marker `1`
//! and must be unique, soft-deleted rows carry `NULL`, which MySQL never treats as a
//! duplicate. The marker is invisible to the entities (they select explicit columns).

use sea_orm_migration::sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement,
};

/// A unique index over live (not soft-deleted) rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveUnique {
    pub name: &'static str,
    pub table: &'static str,
    pub columns: &'static [&'static str],
}

/// Generated column emulating the partial index on MySQL.
pub const MYSQL_MARKER: &str = "live_marker";

/// The reseller partial unique indexes, in the original order.
pub const INDEXES: [LiveUnique; 5] = [
    LiveUnique {
        name: "idx_reseller_domains_active_domain",
        table: "reseller_domains",
        columns: &["domain"],
    },
    LiveUnique {
        name: "idx_reseller_site_configs_active_reseller",
        table: "reseller_site_configs",
        columns: &["reseller_id"],
    },
    LiveUnique {
        name: "idx_reseller_product_settings_active_scope",
        table: "reseller_product_settings",
        columns: &["reseller_id", "product_id", "sku_id"],
    },
    LiveUnique {
        name: "idx_reseller_balance_accounts_active_currency",
        table: "reseller_balance_accounts",
        columns: &["reseller_id", "currency"],
    },
    LiveUnique {
        name: "idx_reseller_related_accounts_active_user",
        table: "reseller_related_accounts",
        columns: &["reseller_id", "user_id"],
    },
];

/// `CREATE UNIQUE INDEX IF NOT EXISTS … WHERE deleted_at IS NULL` (SQLite, PostgreSQL).
pub fn partial_index_sql(idx: &LiveUnique) -> String {
    format!(
        "CREATE UNIQUE INDEX IF NOT EXISTS {} ON {}({}) WHERE deleted_at IS NULL",
        idx.name,
        idx.table,
        idx.columns.join(", ")
    )
}

/// MySQL: the generated marker column.
pub fn mysql_marker_sql(table: &str) -> String {
    format!(
        "ALTER TABLE {table} ADD COLUMN {MYSQL_MARKER} TINYINT GENERATED ALWAYS AS (IF(deleted_at IS NULL, 1, NULL)) VIRTUAL"
    )
}

/// MySQL: the unique index including the marker.
pub fn mysql_index_sql(idx: &LiveUnique) -> String {
    format!(
        "CREATE UNIQUE INDEX {} ON {}({}, {MYSQL_MARKER})",
        idx.name,
        idx.table,
        idx.columns.join(", ")
    )
}

async fn mysql_exists(
    db: &DatabaseConnection,
    sql: &str,
    values: [&str; 2],
) -> Result<bool, DbErr> {
    let stmt = Statement::from_sql_and_values(DbBackend::MySql, sql, values.map(Into::into));
    Ok(db.query_one_raw(stmt).await?.is_some())
}

/// Creates the live-row unique indexes when missing (idempotent).
pub async fn live_unique_indexes(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    for idx in &INDEXES {
        match backend {
            DbBackend::MySql => {
                let has_marker = mysql_exists(
                    db,
                    "SELECT 1 FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND COLUMN_NAME = ?",
                    [idx.table, MYSQL_MARKER],
                )
                .await?;
                if !has_marker {
                    db.execute_unprepared(&mysql_marker_sql(idx.table)).await?;
                }
                let has_index = mysql_exists(
                    db,
                    "SELECT 1 FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND INDEX_NAME = ?",
                    [idx.table, idx.name],
                )
                .await?;
                if !has_index {
                    db.execute_unprepared(&mysql_index_sql(idx)).await?;
                }
            }
            _ => {
                db.execute_unprepared(&partial_index_sql(idx)).await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statements_per_backend() {
        assert_eq!(
            partial_index_sql(&INDEXES[0]),
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_reseller_domains_active_domain ON reseller_domains(domain) WHERE deleted_at IS NULL"
        );
        assert_eq!(
            partial_index_sql(&INDEXES[3]),
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_reseller_balance_accounts_active_currency ON reseller_balance_accounts(reseller_id, currency) WHERE deleted_at IS NULL"
        );
        assert_eq!(
            mysql_index_sql(&INDEXES[4]),
            "CREATE UNIQUE INDEX idx_reseller_related_accounts_active_user ON reseller_related_accounts(reseller_id, user_id, live_marker)"
        );
        assert!(mysql_marker_sql("reseller_domains").contains("IF(deleted_at IS NULL, 1, NULL)"));
    }
}
