//! Case-insensitive search over plain and localized JSON columns
//! (port of the original `buildLocalizedLikeCondition`).
//!
//! JSON path extraction is dialect specific, so this helper is the single place
//! that branches on the backend.

use sea_orm::sea_query::{Expr, SimpleExpr};
use sea_orm::{Condition, DbBackend, Value};
use zs_shared::i18n::LOCALES;

/// `col LIKE %term% OR json(col)->locale LIKE %term% ...` for the given columns.
pub fn localized_like(backend: DbBackend, plain: &[&str], json: &[&str], term: &str) -> Condition {
    let pattern = format!("%{}%", term.trim());
    // sea-query passes custom SQL through verbatim, so the placeholder must match the
    // backend: `$1` for PostgreSQL, `?` for MySQL/SQLite.
    let (op, ph) = match backend {
        DbBackend::Postgres => ("ILIKE", "$1"),
        _ => ("LIKE", "?"),
    };
    let mut cond = Condition::any();
    for col in plain {
        cond = cond.add(expr(&format!("{col} {op} {ph}"), &pattern));
    }
    for col in json {
        for locale in LOCALES {
            let extract = match backend {
                DbBackend::Postgres => format!("({col}::jsonb ->> '{locale}')"),
                _ => format!("json_extract({col}, '$.\"{locale}\"')"),
            };
            cond = cond.add(expr(&format!("{extract} {op} {ph}"), &pattern));
        }
    }
    cond
}

fn expr(sql: &str, pattern: &str) -> SimpleExpr {
    Expr::cust_with_values(sql.to_owned(), [Value::from(pattern.to_owned())])
}

#[cfg(test)]
mod tests {
    use sea_orm::{DbBackend, EntityTrait, QueryFilter, QueryTrait};

    fn sql(backend: DbBackend) -> String {
        let cond = super::localized_like(backend, &["slug"], &["title_json"], "guide");
        crate::db::entity::posts::Entity::find()
            .filter(cond)
            .build(backend)
            .sql
    }

    // Regression: a literal `?` placeholder was sent verbatim to PostgreSQL.
    #[test]
    fn placeholders_match_the_backend() {
        let pg = sql(DbBackend::Postgres);
        assert!(pg.contains("ILIKE $1") && !pg.contains('?'), "{pg}");
        let sqlite = sql(DbBackend::Sqlite);
        assert!(
            sqlite.contains("LIKE ?") && !sqlite.contains("LIKE $1"),
            "{sqlite}"
        );
        let mysql = sql(DbBackend::MySql);
        assert!(
            mysql.contains("LIKE ?") && !mysql.contains("LIKE $1"),
            "{mysql}"
        );
    }
}
