//! Schema synchronisation creates every table and is idempotent.

use sea_orm::{ConnectionTrait, Statement};
use zs_app::config::DatabaseConfig;

#[tokio::test]
async fn sync_creates_all_tables_idempotently() {
    let cfg = DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        ..Default::default()
    };
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let rows = db
        .query_all_raw(Statement::from_string(
            db.get_database_backend(),
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        ))
        .await
        .unwrap();
    // 60 original tables + casbin_rule + jobs + 10 integration / zebra-store tables
    // + api_compat_keys (provider-compat facades) + procurement_deliveries
    assert_eq!(rows.len(), 74, "tables: {rows:?}");
}
