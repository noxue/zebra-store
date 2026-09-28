//! QA-A16 (live QA I-16): `zebra-store backup` copies a live SQLite database
//! without a `sqlite3` binary; QA-A01: the operator can list/delete stray settings keys.

#![expect(clippy::unwrap_used, reason = "integration test")]

use serde_json::json;
use zs_app::config::DatabaseConfig;
use zs_domain::settings::SettingsStore;
use zs_infra::db::repo::settings::SeaSettingsStore;

fn file_db(name: &str) -> (std::path::PathBuf, DatabaseConfig) {
    let dir = std::env::temp_dir().join(format!("zs-backup-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = DatabaseConfig {
        url: format!("sqlite://{}?mode=rwc", dir.join("live.db").display()),
        ..Default::default()
    };
    (dir, cfg)
}

#[tokio::test]
async fn qa_a16_sqlite_online_backup() {
    let (dir, cfg) = file_db("copy");
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let store = SeaSettingsStore::new(db.clone());
    store
        .set("site_config", &json!({"brand": {"site_name": "Zebra"}}))
        .await
        .unwrap();

    let dest = dir.join("nested").join("bk.db");
    zs_infra::db::backup::backup_sqlite(&db, &dest)
        .await
        .unwrap();
    // the live database keeps working after the copy
    store.set("order_config", &json!({})).await.unwrap();

    let copy = zs_infra::db::connect(&DatabaseConfig {
        url: format!("sqlite://{}?mode=rw", dest.display()),
        ..Default::default()
    })
    .await
    .unwrap();
    let copied = SeaSettingsStore::new(copy);
    assert_eq!(
        copied.get("site_config").await.unwrap(),
        Some(json!({"brand": {"site_name": "Zebra"}}))
    );
    assert_eq!(copied.get("order_config").await.unwrap(), None);

    // an existing target is never overwritten
    let err = zs_infra::db::backup::backup_sqlite(&db, &dest)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("already exists"), "{err}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn qa_a01_prune_unknown_settings_keys() {
    let (dir, cfg) = file_db("prune");
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    let store = SeaSettingsStore::new(db.clone());
    store.set("site_config", &json!({})).await.unwrap();
    store.set("foo_bar_unknown", &json!({})).await.unwrap();
    let unknown =
        zs_app::content::settings::SettingsService::unknown_keys(store.keys().await.unwrap());
    assert_eq!(unknown, vec!["foo_bar_unknown".to_owned()]);
    assert!(store.delete("foo_bar_unknown").await.unwrap());
    assert!(!store.delete("foo_bar_unknown").await.unwrap());
    assert_eq!(store.get("site_config").await.unwrap(), Some(json!({})));
    let _ = std::fs::remove_dir_all(&dir);
}
