//! Schema sync must be repeatable on every backend (regression: sea-orm dropped the
//! migration-created unique indexes on the second start, failing on MySQL/PostgreSQL).
//!
//! Runs only when `ZS_TEST_PG_URL` / `ZS_TEST_MYSQL_URL` point at a disposable database,
//! e.g. the docker containers used by `scripts/db_smoke.sh`.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

use sea_orm::{ConnectionTrait, DbBackend, Statement};
use zs_app::config::DatabaseConfig;

async fn sync_three_times_keeps_custom_indexes(env: &str) {
    // Skipped silently when the database is not configured.
    let Ok(url) = std::env::var(env) else {
        return;
    };
    let cfg = DatabaseConfig {
        url,
        max_connections: 2,
        ..Default::default()
    };
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    for _ in 0..3 {
        zs_infra::db::sync_schema(&db).await.unwrap();
    }
    for (table, name) in zs_migration::custom_unique_indexes() {
        let (sql, values) = match db.get_database_backend() {
            DbBackend::MySql => (
                "SELECT 1 FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND INDEX_NAME = ?",
                vec![table.into(), name.into()],
            ),
            _ => (
                "SELECT 1 FROM pg_indexes WHERE tablename = $1 AND indexname = $2",
                vec![table.into(), name.into()],
            ),
        };
        let found = db
            .query_one_raw(Statement::from_sql_and_values(
                db.get_database_backend(),
                sql,
                values,
            ))
            .await
            .unwrap();
        assert!(
            found.is_some(),
            "{name} on {table} missing after repeated sync"
        );
    }
}

#[tokio::test]
async fn postgres_sync_is_repeatable() {
    sync_three_times_keeps_custom_indexes("ZS_TEST_PG_URL").await;
}

#[tokio::test]
async fn mysql_sync_is_repeatable() {
    sync_three_times_keeps_custom_indexes("ZS_TEST_MYSQL_URL").await;
}
