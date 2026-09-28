//! SQL helpers shared by the catalog and marketing repositories.
//!
//! Persistence-layer dialect handling (DB-09, DB-10): JSON text extraction differs per
//! backend; case-insensitive matching uses `LOWER(x) LIKE LOWER(?)` everywhere.

use sea_orm::sea_query::{Expr, ExprTrait, Func, LikeExpr};
use sea_orm::{ConnectionTrait, DbBackend};

/// Locales searched in localized JSON columns.
pub const SEARCH_LOCALES: [&str; 3] = ["zh-CN", "zh-TW", "en-US"];

/// Rows per chunk for `IN (…)` lists and multi-row inserts (DLV-06).
pub const CHUNK: usize = 200;

/// Escape character of LIKE patterns (`!` is portable; `\\` needs doubling on MySQL).
const LIKE_ESCAPE: char = '!';

/// `%text%` with `%`, `_` and the escape character escaped, lower-cased.
pub fn contains_pattern(text: &str) -> LikeExpr {
    let mut escaped = String::with_capacity(text.len() + 2);
    escaped.push('%');
    for c in text.to_lowercase().chars() {
        if matches!(c, '%' | '_' | LIKE_ESCAPE) {
            escaped.push(LIKE_ESCAPE);
        }
        escaped.push(c);
    }
    escaped.push('%');
    LikeExpr::new(escaped).escape(LIKE_ESCAPE)
}

/// `LOWER(<sql>) LIKE '%text%'`.
pub fn ilike_sql(sql: impl Into<String>, text: &str) -> Expr {
    Expr::expr(Func::lower(Expr::cust(sql.into()))).like(contains_pattern(text))
}

/// Text value of `column->key` for the connection's backend (column must be table-qualified).
pub fn json_text(backend: DbBackend, column: &str, key: &str) -> String {
    match backend {
        DbBackend::Postgres => format!("(({column})::jsonb ->> '{key}')"),
        DbBackend::MySql => format!("JSON_UNQUOTE(JSON_EXTRACT({column}, '$.\"{key}\"'))"),
        _ => format!("json_extract({column}, '$.\"{key}\"')"),
    }
}

/// `true` when the JSON array column holds at least one element.
pub fn json_array_nonempty(backend: DbBackend, column: &str) -> String {
    match backend {
        DbBackend::Postgres => format!("COALESCE(json_array_length(({column})::json), 0) > 0"),
        DbBackend::MySql => format!("COALESCE(JSON_LENGTH({column}), 0) > 0"),
        _ => format!("COALESCE(json_array_length({column}), 0) > 0"),
    }
}

/// Characters of the original value kept in a tombstone (fits `varchar(255)` with the suffix).
const TOMBSTONE_KEEP: usize = 200;

/// Replacement for a unique column of a soft-deleted row, freeing the value for reuse
/// (ORD-11: soft delete + plain unique index would block re-creation).
pub fn tombstone(value: &str, id: i64) -> String {
    let kept: String = value.chars().take(TOMBSTONE_KEEP).collect();
    format!("{kept}__deleted_{id}")
}

/// Backend of any connection or transaction.
pub fn backend_of<C: ConnectionTrait>(conn: &C) -> DbBackend {
    conn.get_database_backend()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_text_per_backend() {
        assert_eq!(
            json_text(DbBackend::Sqlite, "products.title_json", "zh-CN"),
            "json_extract(products.title_json, '$.\"zh-CN\"')"
        );
        assert!(json_text(DbBackend::Postgres, "p.t", "en-US").contains("::jsonb ->> 'en-US'"));
        assert!(json_text(DbBackend::MySql, "p.t", "en-US").starts_with("JSON_UNQUOTE"));
    }
}
