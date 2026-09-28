//! Regression tests for ADM-04 (`bugfix-lessons.md` §21): strict parsing of admin list
//! parameters — bool filters, page bounds and case-insensitive name search.
//! (The `sort_by` whitelist and NULL ordering are covered in `dashboard_users.rs`.)

mod integration_common;

use integration_common::{IntApp, data, err, seed_product};
use serde_json::json;

/// ADM-04 ①: a malformed `is_active` filter is a 400, not a silent `false`, on every list
/// that accepts it (original `ParseQueryBoolPtr`); valid spellings are accepted.
#[tokio::test]
async fn adm_04_bool_filters_are_strict() {
    let app = IntApp::new().await;
    for list in ["promotions", "coupons", "member-levels", "banners"] {
        let bad = app
            .admin("GET", &format!("/api/v1/admin/{list}?is_active=abc"), None)
            .await;
        err(&bad, 400, "请求参数错误");
        for ok in ["true", "0", "", "T"] {
            let res = app
                .admin("GET", &format!("/api/v1/admin/{list}?is_active={ok}"), None)
                .await;
            data(&res);
        }
    }
}

/// ADM-04 ②: `page` < 1 becomes 1 and `page_size` is clamped to 200 on admin lists
/// (original `ParsePagination`); the bounded upstream catalog falls back to its default.
#[tokio::test]
async fn adm_04_page_bounds() {
    let app = IntApp::new().await;
    let res = app
        .admin("GET", "/api/v1/admin/users?page=0&page_size=10000", None)
        .await;
    data(&res);
    assert_eq!(
        (&res["pagination"]["page"], &res["pagination"]["page_size"]),
        (&json!(1), &json!(200)),
        "{res}"
    );
    let res = app
        .admin("GET", "/api/v1/admin/users?page=abc&page_size=-5", None)
        .await;
    assert_eq!(
        (&res["pagination"]["page"], &res["pagination"]["page_size"]),
        (&json!(1), &json!(20)),
        "{res}"
    );
    // `ParsePaginationWithBounds(…, 50, 50)`: out of range → the default 50
    let (_, _, _, key, secret) = app.buyer("pager@example.com").await;
    let (status, body) = app
        .upstream(
            "GET",
            "/api/v1/upstream/products?page_size=10000",
            None,
            &key,
            &secret,
            0,
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["page_size"], 50, "{body}");
}

/// ADM-04 ③: the promotion name filter is case-insensitive on every database
/// (`LOWER(name) LIKE LOWER(?)`).
#[tokio::test]
async fn adm_04_name_search_ignores_case() {
    let app = IntApp::new().await;
    let (pid, _) = seed_product(&app.db, "adm04", "manual", &[("A", "10", true)]).await;
    let created = app
        .admin(
            "POST",
            "/api/v1/admin/promotions",
            Some(json!({"name": "summer sale", "type": "fixed", "scope_ref_id": pid, "value": 1})),
        )
        .await;
    data(&created);
    for q in ["SUMMER", "Sale", "mer sa"] {
        let res = app
            .admin(
                "GET",
                &format!("/api/v1/admin/promotions?name={}", q.replace(' ', "%20")),
                None,
            )
            .await;
        let items = data(&res).as_array().unwrap_or_else(|| panic!("{res}"));
        assert_eq!(items.len(), 1, "{q}: {res}");
        assert_eq!(items[0]["name"], "summer sale");
    }
    let none = app
        .admin("GET", "/api/v1/admin/promotions?name=winter", None)
        .await;
    assert_eq!(data(&none).as_array().map(Vec::len), Some(0), "{none}");
}
