//! Online database backup (`zebra-store backup`, live QA I-16).
//!
//! SQLite is copied with `VACUUM INTO`, which produces a consistent, compacted copy
//! while the server keeps running (no `sqlite3` binary needed, so it works inside
//! the Docker image). MySQL and PostgreSQL are backed up with their own tools
//! (`mysqldump` / `pg_dump`), which this command points to.

use std::path::Path;

use sea_orm::{ConnectionTrait, DatabaseConnection, DbBackend, DbErr, Statement};

/// Writes a consistent copy of the connected SQLite database to `dest`.
///
/// `dest` must not exist yet (SQLite refuses to overwrite a non-empty file); missing
/// parent directories are created.
pub async fn backup_sqlite(db: &DatabaseConnection, dest: &Path) -> Result<(), DbErr> {
    match db.get_database_backend() {
        DbBackend::Sqlite => {}
        DbBackend::MySql => {
            return Err(DbErr::Custom(
                "built-in backup supports SQLite only; use mysqldump --single-transaction for MySQL"
                    .into(),
            ));
        }
        _ => {
            return Err(DbErr::Custom(
                "built-in backup supports SQLite only; use pg_dump -Fc for PostgreSQL".into(),
            ));
        }
    }
    if dest.exists() {
        return Err(DbErr::Custom(format!(
            "backup target {} already exists",
            dest.display()
        )));
    }
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).map_err(|e| DbErr::Custom(e.to_string()))?;
    }
    let target = dest
        .to_str()
        .ok_or_else(|| DbErr::Custom("backup path must be valid UTF-8".into()))?;
    db.execute_raw(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "VACUUM INTO ?",
        [target.into()],
    ))
    .await?;
    Ok(())
}
