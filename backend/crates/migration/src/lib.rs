//! Data migrations of Zebra Store: idempotent, dialect-aware statements the
//! entity-first schema sync cannot express. They run after every `schema sync`
//! (`zs_infra::db::sync_schema`) and must be safe to repeat; no bookkeeping table
//! is used.

pub mod integration;
pub mod mysql_time;
pub mod reseller;

use sea_orm_migration::sea_orm::{
    ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement,
};

/// Unique indexes created by data migrations rather than by the entities.
///
/// sea-orm's schema sync drops every unique index whose column set is not declared on
/// an entity (despite documenting itself as non-destructive), and on MySQL/PostgreSQL
/// it even generates an invalid drop statement, which makes a *second* start fail.
/// [`before_sync`] therefore removes these indexes before the sync and [`run`]
/// recreates them afterwards, before the server accepts traffic.
pub fn custom_unique_indexes() -> Vec<(&'static str, &'static str)> {
    let mut all: Vec<(&'static str, &'static str)> = reseller::INDEXES
        .iter()
        .map(|i| (i.table, i.name))
        .collect();
    all.push(("downstream_order_refs", integration::INDEX));
    all
}

/// Drops the [`custom_unique_indexes`] (if present) so schema sync leaves the tables alone.
pub async fn before_sync(db: &DatabaseConnection) -> Result<(), DbErr> {
    let backend = db.get_database_backend();
    for (table, name) in custom_unique_indexes() {
        match backend {
            DbBackend::MySql => {
                let exists = db
                    .query_one_raw(Statement::from_sql_and_values(
                        DbBackend::MySql,
                        "SELECT 1 FROM information_schema.STATISTICS WHERE TABLE_SCHEMA = DATABASE() AND TABLE_NAME = ? AND INDEX_NAME = ?",
                        [table.into(), name.into()],
                    ))
                    .await?
                    .is_some();
                if exists {
                    db.execute_unprepared(&format!("DROP INDEX {name} ON {table}"))
                        .await?;
                }
            }
            _ => {
                db.execute_unprepared(&format!("DROP INDEX IF EXISTS {name}"))
                    .await?;
            }
        }
    }
    Ok(())
}

/// Applies every data migration.
pub async fn run(db: &DatabaseConnection) -> Result<(), DbErr> {
    mysql_time::datetime_precision(db).await?;
    reseller::live_unique_indexes(db).await?;
    integration::downstream_ref_index(db).await
}
