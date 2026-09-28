//! Database connection, schema synchronisation and repositories.

pub mod backup;
pub mod entity;
pub mod repo;

use std::time::Duration;

use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbBackend, DbErr};
use zs_app::config::DatabaseConfig;

/// SQLite busy timeout, matching the original (`busy_timeout=5000`).
const SQLITE_BUSY_TIMEOUT: Duration = Duration::from_millis(5000);

/// Pool size used for SQLite whatever `database.max_connections` says.
///
/// SQLite has a single writer. With several pooled connections, a (deferred) transaction that
/// read before another connection committed fails its first write with `SQLITE_BUSY_SNAPSHOT`
/// (extended code 517, "database is locked") — `busy_timeout` cannot retry that case, so jobs
/// such as `order:auto_fulfill` failed after every payment. All repositories keep a transaction's
/// statements on its own handle (DB-01; integration tests run with one connection), so a single
/// connection serialises access without deadlocks.
pub const SQLITE_MAX_CONNECTIONS: u32 = 1;

/// Effective pool bounds `(max, min)` for a database URL.
pub fn pool_size(url: &str, max_connections: u32, min_connections: u32) -> (u32, u32) {
    if url.starts_with("sqlite:") {
        (
            SQLITE_MAX_CONNECTIONS,
            min_connections.min(SQLITE_MAX_CONNECTIONS),
        )
    } else {
        (max_connections, min_connections)
    }
}

/// Opens a connection pool; the URL scheme selects SQLite, MySQL or PostgreSQL.
pub async fn connect(cfg: &DatabaseConfig) -> Result<DatabaseConnection, DbErr> {
    #[cfg(feature = "testkit")]
    if let Some(db) = testkit::fresh_server_database(cfg).await {
        return db;
    }
    if let Some(path) = sqlite_file_path(&cfg.url)
        && let Some(parent) = std::path::Path::new(path).parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| DbErr::Custom(e.to_string()))?;
    }
    let (max_connections, min_connections) =
        pool_size(&cfg.url, cfg.max_connections, cfg.min_connections);
    let mut opts = ConnectOptions::new(cfg.url.clone());
    opts.max_connections(max_connections)
        .min_connections(min_connections)
        .connect_timeout(Duration::from_secs(cfg.connect_timeout_seconds))
        .idle_timeout(Duration::from_secs(cfg.idle_timeout_seconds))
        .sqlx_logging(cfg.sql_log)
        .map_sqlx_sqlite_opts(|o| {
            o.busy_timeout(SQLITE_BUSY_TIMEOUT)
                .journal_mode(sea_orm::sqlx::sqlite::SqliteJournalMode::Wal)
                .synchronous(sea_orm::sqlx::sqlite::SqliteSynchronous::Normal)
                .foreign_keys(false)
        });
    let db = Database::connect(opts).await?;
    tracing::info!(backend = ?db.get_database_backend(), "database connected");
    Ok(db)
}

/// Creates missing tables, columns and unique keys for every registered entity.
pub async fn sync_schema(db: &DatabaseConnection) -> Result<(), DbErr> {
    // Custom unique indexes are dropped first and recreated by `zs_migration::run`,
    // otherwise sea-orm's sync tries (and fails on MySQL/PostgreSQL) to drop them on
    // every restart.
    zs_migration::before_sync(db).await?;
    db.get_schema_registry("zs_infra::db::entity::*")
        .sync(db)
        .await?;
    // Idempotent data migrations the entities cannot express (e.g. partial unique indexes).
    zs_migration::run(db).await
}

/// Returns the backend of a connection (used by dialect-specific data migrations only).
pub fn backend(db: &DatabaseConnection) -> DbBackend {
    db.get_database_backend()
}

fn sqlite_file_path(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("sqlite://")?;
    let path = rest.split('?').next()?;
    (!path.is_empty() && path != ":memory:").then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sqlite_path() {
        assert_eq!(
            sqlite_file_path("sqlite://data/zebra.db?mode=rwc"),
            Some("data/zebra.db")
        );
        assert_eq!(sqlite_file_path("sqlite::memory:"), None);
        assert_eq!(sqlite_file_path("mysql://x"), None);
    }

    #[test]
    fn sqlite_pool_is_single_connection() {
        assert_eq!(pool_size("sqlite://data/zebra.db?mode=rwc", 10, 2), (1, 1));
        assert_eq!(pool_size("sqlite::memory:", 10, 0), (1, 0));
        assert_eq!(pool_size("postgres://u@h/db", 10, 2), (10, 2));
        assert_eq!(pool_size("mysql://u@h/db", 20, 5), (20, 5));
    }
}

/// Test support (feature `testkit`): run the in-memory-SQLite integration tests
/// against PostgreSQL or MySQL instead.
///
/// When a test asks for `sqlite::memory:` and `ZS_TEST_DATABASE_URL` points at a
/// server database (e.g. `postgres://zebra:zebra@127.0.0.1:15432/zebra`), a fresh
/// database `zs_t_<random>` is created on that server for the test and used instead.
/// Drop leftovers with `scripts/drop_test_dbs.sh`.
#[cfg(feature = "testkit")]
mod testkit {
    use rand::Rng;
    use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbErr};
    use zs_app::config::DatabaseConfig;

    /// Environment variable selecting the server used for test databases.
    const ENV: &str = "ZS_TEST_DATABASE_URL";
    /// Prefix of per-test databases (see `scripts/drop_test_dbs.sh`).
    const PREFIX: &str = "zs_t_";

    pub async fn fresh_server_database(
        cfg: &DatabaseConfig,
    ) -> Option<Result<DatabaseConnection, DbErr>> {
        if cfg.url != "sqlite::memory:" {
            return None;
        }
        let base = std::env::var(ENV).ok().filter(|v| !v.trim().is_empty())?;
        Some(create(cfg, &base).await)
    }

    async fn create(cfg: &DatabaseConfig, base: &str) -> Result<DatabaseConnection, DbErr> {
        let suffix: u64 = rand::rng().random();
        let name = format!("{PREFIX}{suffix:016x}");
        let admin = Database::connect(base.to_owned()).await?;
        admin
            .execute_unprepared(&format!("CREATE DATABASE {name}"))
            .await?;
        admin.close().await?;
        let (prefix, _) = base
            .rsplit_once('/')
            .ok_or_else(|| DbErr::Custom(format!("{ENV} must end with /<database>")))?;
        let mut test_cfg = cfg.clone();
        test_cfg.url = format!("{prefix}/{name}");
        // Recursion is safe: the new URL is no longer `sqlite::memory:`.
        Box::pin(super::connect(&test_cfg)).await
    }
}
