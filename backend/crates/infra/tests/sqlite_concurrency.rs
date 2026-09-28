//! Concurrent read-then-write transactions on a file SQLite database must not fail with
//! `SQLITE_BUSY_SNAPSHOT` ("database is locked", code 517) — the E2E suite saw
//! `order:auto_fulfill` jobs fail that way right after a payment callback.

use sea_orm::{ConnectionTrait, Statement, TransactionTrait};
use zs_app::config::DatabaseConfig;

#[tokio::test]
async fn concurrent_read_then_write_transactions_succeed() {
    let dir = std::env::temp_dir().join(format!("zs-sqlite-conc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("conc.db");
    let _ = std::fs::remove_file(&path);
    let cfg = DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", path.display()),
        max_connections: 8,
        ..Default::default()
    };
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    let backend = db.get_database_backend();
    db.execute_raw(Statement::from_string(
        backend,
        "CREATE TABLE counter (id INTEGER PRIMARY KEY, n INTEGER NOT NULL)",
    ))
    .await
    .unwrap();
    db.execute_raw(Statement::from_string(
        backend,
        "INSERT INTO counter VALUES (1, 0)",
    ))
    .await
    .unwrap();

    let tasks: Vec<_> = (0..16)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move {
                let txn = db.begin().await?;
                // read first (deferred transaction takes a read snapshot) ...
                txn.query_one_raw(Statement::from_string(
                    backend,
                    "SELECT n FROM counter WHERE id = 1",
                ))
                .await?;
                tokio::task::yield_now().await;
                // ... then write
                txn.execute_raw(Statement::from_string(
                    backend,
                    "UPDATE counter SET n = n + 1 WHERE id = 1",
                ))
                .await?;
                txn.commit().await
            })
        })
        .collect();
    for t in tasks {
        t.await.unwrap().unwrap();
    }
    let row = db
        .query_one_raw(Statement::from_string(
            backend,
            "SELECT n FROM counter WHERE id = 1",
        ))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.try_get::<i64>("", "n").unwrap(), 16);
    let _ = std::fs::remove_dir_all(&dir);
}
