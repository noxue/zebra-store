//! Browse metadata persistence: stable IDs, version conflicts and atomic normalized indexes.
#![expect(clippy::unwrap_used, reason = "isolated database fixture assertions")]
mod catalog_common;
mod common;

use catalog_common::{category, id_of, product};
use common::TestApp;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait,
    QueryFilter,
};
use serde_json::json;
use zs_domain::ErrorKind;
use zs_domain::catalog::facets::{FacetRepo, FacetSet};
use zs_infra::db::entity::extra::{
    product_catalog_documents as documents, product_catalog_facets as rows,
};
use zs_infra::db::repo::catalog::facets::SeaFacetRepo;

fn attributes(label: &str) -> FacetSet {
    serde_json::from_value(json!([
        {"key":"country","value_id":"provider:country:000A","label":{"zh-CN":label}},
        {"key":"family","value_id":"static_ip","label":{"zh-CN":"Static IP"}}
    ]))
    .unwrap()
}

async fn fixture() -> (TestApp, i64, SeaFacetRepo) {
    let mut config = common::test_config();
    config.reseller.enabled = true;
    config.reseller.subdomain_base = "shop.example.test".into();
    config.reseller.main_hosts = vec!["localhost".into(), "main.example.test".into()];
    let postgres = std::env::var("ZS_CATALOG_TEST_DATABASE_URL").ok();
    if let Some(url) = &postgres {
        assert!(url.starts_with("postgres://zebra_test:") && url.contains("@127.0.0.1:18942/"));
        assert!(!url.contains('?'));
        let connection = sea_orm::Database::connect(url).await.unwrap();
        let schema = format!("catalog_facets_{}", uuid::Uuid::new_v4().simple());
        connection
            .execute_unprepared(&format!("CREATE SCHEMA {schema}"))
            .await
            .unwrap();
        connection.close().await.unwrap();
        config.database.url = format!("{url}?options=-c%20search_path%3D{schema}");
        config.database.max_connections = 8;
    }
    let mut app = TestApp::with_config(config).await;
    assert_eq!(
        app.db.get_database_backend(),
        if postgres.is_some() {
            DbBackend::Postgres
        } else {
            DbBackend::Sqlite
        }
    );
    app.admin_token = Some(app.login("admin", "Admin12345").await);
    let category = category(&app, "facet-fixture", 0).await;
    let product = product(&app, category, "facet-product", json!({})).await;
    let id = id_of(&product);
    let repo = SeaFacetRepo::new(app.db.clone());
    (app, id, repo)
}

// MCY-04 / ORD-11: labels may change; IDs and product/SKU financial data do not.
#[tokio::test]
async fn renames_and_clear_rebuild_keep_versions_and_leave_the_product_untouched() {
    let (app, id, repo) = fixture().await;
    let before = app.get(&format!("/api/v1/admin/products/{id}")).await;
    assert_eq!(repo.get(id).await.unwrap().revision, 0);
    let original = repo.replace(id, 0, &attributes("Original")).await.unwrap();
    assert_eq!(original.revision, 1);
    // DB-01: repeated entity-first startup sync must retain metadata and indexes.
    zs_infra::db::sync_schema(&app.db).await.unwrap();
    zs_infra::db::sync_schema(&app.db).await.unwrap();
    assert_eq!(repo.get(id).await.unwrap(), original);
    assert_eq!(
        repo.replace(id, 1, &attributes("Original")).await.unwrap(),
        original
    );
    assert_eq!(rows::Entity::find().count(&app.db).await.unwrap(), 2);
    let renamed = repo.replace(id, 1, &attributes("Renamed")).await.unwrap();
    assert_eq!(renamed.revision, 2);
    assert_eq!(
        renamed.facets.entries()[1].value_id.as_str(),
        "provider:country:000A"
    );
    assert_eq!(
        renamed.facets.entries()[1].label.resolve("zh-CN"),
        "Renamed"
    );
    let cleared = repo.replace(id, 2, &FacetSet::default()).await.unwrap();
    assert_eq!(cleared.revision, 3);
    assert_eq!(rows::Entity::find().count(&app.db).await.unwrap(), 0);
    assert_eq!(documents::Entity::find().count(&app.db).await.unwrap(), 1);
    assert_eq!(
        repo.replace(id, 0, &attributes("Stale"))
            .await
            .unwrap_err()
            .key(),
        "error.catalog_facets_changed"
    );
    assert_eq!(
        repo.replace(id, 3, &attributes("Restored"))
            .await
            .unwrap()
            .revision,
        4
    );
    assert_eq!(rows::Entity::find().count(&app.db).await.unwrap(), 2);
    let after = app.get(&format!("/api/v1/admin/products/{id}")).await;
    assert_eq!(before, after);
}

// DB-01: one-connection SQLite exposes any accidental query outside the transaction.
#[tokio::test]
async fn concurrent_writers_have_one_winner_and_never_combine_their_indexes() {
    let (app, id, repo) = fixture().await;
    let a = attributes("First");
    let b = attributes("Second");
    let (left, right) = tokio::join!(repo.replace(id, 0, &a), repo.replace(id, 0, &b));
    assert_ne!(left.is_ok(), right.is_ok());
    let winner = match (left, right) {
        (Ok(winner), Err(error)) | (Err(error), Ok(winner)) => {
            assert_eq!(error.key(), "error.catalog_facets_changed");
            winner
        }
        other => panic!("expected one compare-and-swap winner, got {other:?}"),
    };
    assert_eq!(winner.revision, 1);
    assert_eq!(repo.get(id).await.unwrap(), winner);
    assert_eq!(documents::Entity::find().count(&app.db).await.unwrap(), 1);
    assert_eq!(rows::Entity::find().count(&app.db).await.unwrap(), 2);
}

#[tokio::test]
async fn a_projection_write_failure_rolls_back_the_entire_document_and_version() {
    let (app, id, repo) = fixture().await;
    let original = repo.replace(id, 0, &attributes("Original")).await.unwrap();
    let trigger = if app.db.get_database_backend() == DbBackend::Postgres {
        "CREATE FUNCTION reject_facet_index() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic index persistence failure'; END $$; CREATE TRIGGER reject_facet_index BEFORE INSERT ON product_catalog_facets FOR EACH ROW EXECUTE FUNCTION reject_facet_index()"
    } else {
        "CREATE TRIGGER reject_facet_index BEFORE INSERT ON product_catalog_facets BEGIN SELECT RAISE(ABORT,'synthetic index persistence failure'); END"
    };
    app.db.execute_unprepared(trigger).await.unwrap();
    assert!(repo.replace(id, 1, &attributes("Changed")).await.is_err());
    assert_eq!(repo.get(id).await.unwrap(), original);
    assert_eq!(rows::Entity::find().count(&app.db).await.unwrap(), 2);
    let drop_trigger = if app.db.get_database_backend() == DbBackend::Postgres {
        "DROP TRIGGER reject_facet_index ON product_catalog_facets; DROP FUNCTION reject_facet_index()"
    } else {
        "DROP TRIGGER reject_facet_index"
    };
    app.db.execute_unprepared(drop_trigger).await.unwrap();
    assert_eq!(
        repo.replace(id, 1, &attributes("Changed"))
            .await
            .unwrap()
            .revision,
        2
    );
}

#[tokio::test]
async fn missing_deleted_and_corrupt_metadata_fail_explicitly() {
    let (app, id, repo) = fixture().await;
    assert_eq!(repo.get(-1).await.unwrap_err().kind(), ErrorKind::NotFound);
    assert_eq!(
        repo.replace(-1, 0, &attributes("No product"))
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        repo.replace(id, u64::MAX, &attributes("No version"))
            .await
            .unwrap_err()
            .key(),
        "error.catalog_facets_revision_invalid"
    );
    let original = repo.replace(id, 0, &attributes("Original")).await.unwrap();
    rows::Entity::delete_many()
        .filter(rows::Column::ProductId.eq(id))
        .exec(&app.db)
        .await
        .unwrap();
    assert_eq!(repo.get(id).await.unwrap_err().kind(), ErrorKind::Internal);
    assert_eq!(
        repo.replace(id, 1, &original.facets)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Internal
    );
    // Read validation never silently converts a corrupt document into empty/legacy metadata.
    app.db
        .execute_unprepared(&format!(
            "UPDATE product_catalog_documents SET facets_json='null' WHERE product_id={id}"
        ))
        .await
        .unwrap();
    assert_eq!(repo.get(id).await.unwrap_err().kind(), ErrorKind::Internal);
    app.db
        .execute_unprepared(&format!(
            "UPDATE products SET deleted_at=CURRENT_TIMESTAMP WHERE id={id}"
        ))
        .await
        .unwrap();
    assert_eq!(repo.get(id).await.unwrap_err().kind(), ErrorKind::NotFound);
    assert_eq!(
        repo.replace(id, 1, &attributes("Deleted"))
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn future_schema_and_revision_exhaustion_never_downgrade_or_wrap_a_valid_document() {
    let (app, id, repo) = fixture().await;
    let original = repo.replace(id, 0, &attributes("Original")).await.unwrap();
    app.db
        .execute_unprepared(&format!(
            "UPDATE product_catalog_documents SET schema_version=2 WHERE product_id={id}"
        ))
        .await
        .unwrap();
    assert_eq!(repo.get(id).await.unwrap_err().kind(), ErrorKind::Internal);
    assert_eq!(
        repo.replace(id, 1, &original.facets)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Internal
    );
    app.db.execute_unprepared(&format!("UPDATE product_catalog_documents SET schema_version=1, revision=9223372036854775807 WHERE product_id={id}")).await.unwrap();
    let current = repo.get(id).await.unwrap();
    assert_eq!(current.revision, 9_223_372_036_854_775_807);
    assert_eq!(
        repo.replace(id, current.revision, &attributes("Changed"))
            .await
            .unwrap_err()
            .key(),
        "error.catalog_facets_revision_exhausted"
    );
    assert_eq!(repo.get(id).await.unwrap(), current);
}

#[tokio::test]
async fn malformed_index_values_are_not_accepted_when_the_document_is_valid() {
    let (app, id, repo) = fixture().await;
    repo.replace(id, 0, &attributes("Original")).await.unwrap();
    app.db.execute_unprepared(&format!("UPDATE product_catalog_facets SET value_id='padded ' WHERE product_id={id} AND dimension='country'")).await.unwrap();
    assert_eq!(repo.get(id).await.unwrap_err().kind(), ErrorKind::Internal);
    assert_eq!(
        documents::Entity::find()
            .one(&app.db)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
}

#[tokio::test]
async fn admin_http_uses_exact_revisions_and_returns_distinct_atomic_validation_errors() {
    let (app, id, repo) = fixture().await;
    let path = format!("/api/v1/admin/products/{id}/catalog-facets");
    let before = app.get(&path).await;
    assert_eq!(
        common::data(&before),
        &json!({"schema_version":1,"revision":"0","facets":[]})
    );
    let valid = json!({"schema_version":1,"expected_revision":"0","facets":attributes("Country")});
    let created = app.put(&path, valid.clone()).await;
    assert_eq!(common::data(&created)["revision"], "1");
    let original = repo.get(id).await.unwrap();
    let cases = [
        (
            json!({"schema_version":2,"expected_revision":"1","facets":[]}),
            "暂不支持此商品筛选版本，请刷新后再保存。",
        ),
        (
            json!({"schema_version":1,"expected_revision":"01","facets":[]}),
            "商品筛选版本无效，请刷新商品后重试。",
        ),
        (
            json!({"schema_version":1,"expected_revision":"1","facets":[{"key":"country","value_id":" padded","label":{"zh-CN":"Country"}}]}),
            "商品筛选值的标识格式不正确，请核对来源编号。",
        ),
        (
            json!({"schema_version":1,"expected_revision":"1","facets":[{"key":"country","value_id":"a","label":{"zh-CN":""}}]}),
            "商品筛选名称不能为空、过长或包含控制字符。",
        ),
        (
            json!({"schema_version":1,"expected_revision":"1","facets":[{"key":"country","value_id":"a","label":{"zh-CN":"A"}},{"key":"country","value_id":"b","label":{"zh-CN":"B"}}]}),
            "同一商品的每个筛选维度只能设置一个值。",
        ),
        (
            json!({"schema_version":1,"expected_revision":"1","facets":vec![json!({"key":"country","value_id":"a","label":{"zh-CN":"A"}});7]}),
            "商品筛选维度数量超过限制。",
        ),
    ];
    for (body, message) in cases {
        let result = app.put(&path, body).await;
        assert_eq!(result["status_code"], 400, "{result}");
        assert_eq!(result["msg"], message);
        assert_eq!(repo.get(id).await.unwrap(), original);
    }
    for body in [
        json!({"schema_version":1,"expected_revision":1,"facets":[]}),
        json!({"schema_version":1,"expected_revision":"1","facets":[],"price_amount":"0.01"}),
        json!({"schema_version":1,"expected_revision":"1","facets":[{"key":"country","value_id":"a","label":{"zh-CN":"A"},"stock":999}]}),
    ] {
        assert_eq!(app.put(&path, body).await["status_code"], 400);
        assert_eq!(repo.get(id).await.unwrap(), original);
    }
    let stale = app.put(&path, valid).await;
    assert_eq!(stale["status_code"], 400);
    assert_eq!(stale["msg"], "商品筛选配置已变更，请刷新商品后重新确认。");
    let english = app
        .put(
            &format!("{path}?lang=en-US"),
            json!({"schema_version":2,"expected_revision":"1","facets":[]}),
        )
        .await;
    assert_eq!(
        english["msg"],
        "This product filter schema is not supported. Refresh before saving."
    );
    assert_eq!(repo.get(id).await.unwrap(), original);
}

#[tokio::test]
async fn admin_metadata_routes_are_catalogued_and_require_the_operations_role() {
    let (app, id, repo) = fixture().await;
    let path = format!("/api/v1/admin/products/{id}/catalog-facets");
    assert_eq!(app.call("GET", &path, None, None).await["status_code"], 401);
    let created = app
        .post(
            "/api/v1/admin/authz/admins",
            json!({"username":"facetviewer","password":"FacetViewer12345"}),
        )
        .await;
    let aid = common::data(&created)["id"].as_i64().unwrap();
    let token = app.login("facetviewer", "FacetViewer12345").await;
    let body = json!({"schema_version":1,"expected_revision":"0","facets":attributes("Country")});
    assert_eq!(
        app.call("PUT", &path, Some(body.clone()), Some(&token))
            .await["status_code"],
        403
    );
    assert_eq!(repo.get(id).await.unwrap().revision, 0);
    common::data(
        &app.put(
            &format!("/api/v1/admin/authz/admins/{aid}/roles"),
            json!({"roles":["operations"]}),
        )
        .await,
    );
    let result = app.call("PUT", &path, Some(body), Some(&token)).await;
    assert_eq!(common::data(&result)["revision"], "1");
    let catalog = app.get("/api/v1/admin/authz/permissions/catalog").await;
    let items = common::data(&catalog).as_array().unwrap();
    for method in ["GET", "PUT"] {
        assert!(items.iter().any(
            |item| item["object"] == "/admin/products/:id/catalog-facets"
                && item["method"] == method
        ));
    }
}

#[tokio::test]
async fn revisions_beyond_javascript_safe_integers_round_trip_as_exact_decimal_strings() {
    let (app, id, repo) = fixture().await;
    repo.replace(id, 0, &attributes("Original")).await.unwrap();
    // Synthetic isolated fixture, never a production data edit. This value cannot
    // safely cross a JavaScript Number, but remains exact in the HTTP contract.
    app.db
        .execute_unprepared(&format!(
            "UPDATE product_catalog_documents SET revision=9007199254740993 WHERE product_id={id}"
        ))
        .await
        .unwrap();
    let path = format!("/api/v1/admin/products/{id}/catalog-facets");
    assert_eq!(
        common::data(&app.get(&path).await)["revision"],
        "9007199254740993"
    );
    let updated = app.put(&path, json!({"schema_version":1,"expected_revision":"9007199254740993","facets":attributes("Renamed")})).await;
    assert_eq!(common::data(&updated)["revision"], "9007199254740994");
    assert_eq!(repo.get(id).await.unwrap().revision, 9_007_199_254_740_994);
}

#[tokio::test]
async fn clearing_empty_metadata_invalidates_initial_and_later_inflight_writes() {
    let (app, id, repo) = fixture().await;
    let cleared = repo.replace(id, 0, &FacetSet::default()).await.unwrap();
    assert_eq!(cleared.revision, 1);
    assert_eq!(documents::Entity::find().count(&app.db).await.unwrap(), 1);
    assert_eq!(rows::Entity::find().count(&app.db).await.unwrap(), 0);
    assert_eq!(
        repo.replace(id, 0, &attributes("Delayed initial request"))
            .await
            .unwrap_err()
            .key(),
        "error.catalog_facets_changed"
    );
    let cleared_again = repo.replace(id, 1, &FacetSet::default()).await.unwrap();
    assert_eq!(cleared_again.revision, 2);
    assert_eq!(
        repo.replace(id, 1, &attributes("Delayed later request"))
            .await
            .unwrap_err()
            .key(),
        "error.catalog_facets_changed"
    );
    assert_eq!(repo.get(id).await.unwrap(), cleared_again);
}

async fn public_get(app: &TestApp, url: &str) -> serde_json::Value {
    app.call("GET", url, None, None).await
}
async fn public_at(
    app: &TestApp,
    host: &str,
    url: &str,
) -> (axum::http::StatusCode, serde_json::Value) {
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let request = axum::http::Request::builder()
        .uri(url)
        .header("host", host)
        .body(axum::body::Body::empty())
        .unwrap();
    let response = app.router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn add_browse_product(
    app: &TestApp,
    repo: &SeaFacetRepo,
    category: i64,
    slug: &str,
    values: &[(&str, &str, &str)],
) -> i64 {
    let id = id_of(&product(app, category, slug, json!({})).await);
    let values: Vec<_> = values.iter().map(|(key,id,label)| json!({"key":key,"value_id":id,"label":{"zh-CN":label,"en-US":label}})).collect();
    repo.replace(id, 0, &serde_json::from_value(json!(values)).unwrap())
        .await
        .unwrap();
    id
}
fn listed_ids(body: &serde_json::Value) -> Vec<i64> {
    assert_eq!(body["status_code"], 0, "{body}");
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_i64().unwrap())
        .collect()
}
fn option_ids(body: &serde_json::Value) -> Vec<&str> {
    assert_eq!(body["status_code"], 0, "{body}");
    body["data"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["value_id"].as_str().unwrap())
        .collect()
}

// MCY-04: selection uses fixed, case-sensitive identities, ANDed before pagination.
#[tokio::test]
async fn public_filters_intersect_before_pagination_and_preserve_legacy_products() {
    let (app, _, repo) = fixture().await;
    let cat = category(&app, "browse", 0).await;
    let a = add_browse_product(
        &app,
        &repo,
        cat,
        "browse-a",
        &[
            ("family", "ip", "IP"),
            ("country", "001Ab", "Country A"),
            ("city", "01", "City"),
            ("project", "project:01", "Project"),
            ("ip_type", "native", "Native"),
            ("line", "NONE", "Standard"),
        ],
    )
    .await;
    let b = add_browse_product(
        &app,
        &repo,
        cat,
        "browse-b",
        &[
            ("family", "ip", "IP"),
            ("country", "001Ab", "Country A"),
            ("city", "02", "Other city"),
        ],
    )
    .await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "browse-c",
        &[("family", "sms", "SMS"), ("country", "001ab", "Country B")],
    )
    .await;
    let legacy = id_of(&product(&app, cat, "browse-legacy", json!({})).await);
    let all = public_get(&app, &format!("/api/v1/public/products?category_id={cat}")).await;
    assert_eq!(all["pagination"]["total"], 4);
    assert!(listed_ids(&all).contains(&legacy));
    let exact = public_get(&app, &format!("/api/v1/public/products?category_id={cat}&facet_family=ip&facet_country=001Ab&facet_city=01&facet_project=project:01&facet_ip_type=native&facet_line=NONE")).await;
    assert_eq!(listed_ids(&exact), vec![a]);
    let wrong_line = public_get(
        &app,
        &format!("/api/v1/public/products?category_id={cat}&facet_line=none"),
    )
    .await;
    assert!(listed_ids(&wrong_line).is_empty());
    let first = public_get(&app, &format!("/api/v1/public/products?category_id={cat}&facet_family=ip&facet_country=001Ab&page_size=1")).await;
    assert_eq!(first["pagination"]["total"], 2);
    assert_eq!(listed_ids(&first).len(), 1);
    let second = public_get(&app, &format!("/api/v1/public/products?category_id={cat}&facet_family=ip&facet_country=001Ab&page_size=1&page=2")).await;
    let mut ids = listed_ids(&first);
    ids.extend(listed_ids(&second));
    ids.sort();
    assert_eq!(ids, vec![a, b]);
    let one = public_get(
        &app,
        &format!("/api/v1/public/products?category_id={cat}&facet_country=001Ab&facet_city=01"),
    )
    .await;
    assert_eq!(listed_ids(&one), vec![a]);
    for filters in [
        "facet_country=001AB",
        "facet_family=ip&facet_country=001ab",
        "facet_city=1",
    ] {
        let empty = public_get(
            &app,
            &format!("/api/v1/public/products?category_id={cat}&{filters}"),
        )
        .await;
        assert!(listed_ids(&empty).is_empty());
        assert_eq!(empty["pagination"]["total"], 0);
    }
}

#[tokio::test]
async fn options_ignore_their_own_selection_but_apply_all_other_filters_and_counts() {
    let (app, _, repo) = fixture().await;
    let cat = category(&app, "option-browse", 0).await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "option-a",
        &[
            ("family", "ip", "IP"),
            ("country", "A", "Country A"),
            ("city", "one", "One"),
        ],
    )
    .await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "option-b",
        &[
            ("family", "ip", "IP"),
            ("country", "A", "Country A"),
            ("city", "two", "Two"),
        ],
    )
    .await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "option-c",
        &[
            ("family", "ip", "IP"),
            ("country", "B", "Country B"),
            ("city", "three", "Three"),
        ],
    )
    .await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "option-d",
        &[
            ("family", "sms", "SMS"),
            ("country", "C", "Hidden by family"),
        ],
    )
    .await;
    let options = public_get(&app, &format!("/api/v1/public/catalog-facet-values?dimension=country&category_id={cat}&facet_family=ip&facet_country=A")).await;
    assert_eq!(option_ids(&options), vec!["A", "B"]);
    assert_eq!(options["data"][0]["product_count"], 2);
    assert_eq!(options["data"][1]["product_count"], 1);
    let city = public_get(&app, &format!("/api/v1/public/catalog-facet-values?dimension=city&category_id={cat}&facet_country=A&facet_city=missing")).await;
    assert_eq!(option_ids(&city), vec!["one", "two"]);
    let search = public_get(&app, &format!("/api/v1/public/catalog-facet-values?dimension=country&category_id={cat}&search=option-c")).await;
    assert_eq!(option_ids(&search), vec!["B"]);
}

// Renames update the representative label, without splitting identities or counts.
#[tokio::test]
async fn option_search_uses_current_labels_and_escapes_wildcards_after_grouping() {
    let (app, _, repo) = fixture().await;
    let cat = category(&app, "renamed-options", 0).await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "rename-a",
        &[("country", "000A", "Old name")],
    )
    .await;
    let b = add_browse_product(
        &app,
        &repo,
        cat,
        "rename-b",
        &[("country", "000A", "Old name")],
    )
    .await;
    add_browse_product(
        &app,
        &repo,
        cat,
        "rename-c",
        &[("country", "000a", "Other")],
    )
    .await;
    let renamed: FacetSet = serde_json::from_value(json!([{"key":"country","value_id":"000A","label":{"zh-CN":"New 100%_city","en-US":"Translated"}}])).unwrap();
    repo.replace(b, 1, &renamed).await.unwrap();
    let base = format!("/api/v1/public/catalog-facet-values?dimension=country&category_id={cat}");
    let all = public_get(&app, &base).await;
    assert_eq!(option_ids(&all), vec!["000A", "000a"]);
    assert_eq!(all["data"][0]["label"]["zh-CN"], "New 100%_city");
    assert_eq!(all["data"][0]["product_count"], 2);
    for term in ["New", "100%25_city", "TRANSLATED", "%25", "_"] {
        let found = public_get(&app, &format!("{base}&option_search={term}")).await;
        assert_eq!(option_ids(&found), vec!["000A"]);
        assert_eq!(found["data"][0]["product_count"], 2);
    }
    assert!(option_ids(&public_get(&app, &format!("{base}&option_search=Old")).await).is_empty());
    let page2 = public_get(&app, &format!("{base}&page_size=1&page=2")).await;
    assert_eq!(option_ids(&page2), vec!["000a"]);
    assert_eq!(page2["pagination"]["total"], 2);
    let selected = public_get(&app, &format!("{base}&value_id=000a&page_size=1")).await;
    assert_eq!(option_ids(&selected), vec!["000a"]);
    assert_eq!(selected["pagination"]["total"], 1);
    assert!(option_ids(&public_get(&app, &format!("{base}&value_id=000b")).await).is_empty());
}

// Active children of an inactive/deleted parent cannot leak through options or details.
#[tokio::test]
async fn public_options_list_and_detail_share_product_and_parent_category_visibility() {
    use sea_orm::Set;
    use zs_infra::db::entity::{categories, products};
    let (app, _, repo) = fixture().await;
    let parent = category(&app, "visibility-parent", 0).await;
    let child = category(&app, "visibility-child", parent).await;
    let a = add_browse_product(&app, &repo, child, "visibility-a", &[("country", "A", "A")]).await;
    let b = add_browse_product(&app, &repo, child, "visibility-b", &[("country", "B", "B")]).await;
    let c = add_browse_product(&app, &repo, child, "visibility-c", &[("country", "C", "C")]).await;
    products::Entity::update_many()
        .col_expr(
            products::Column::IsActive,
            sea_orm::sea_query::Expr::value(false),
        )
        .filter(products::Column::Id.eq(b))
        .exec(&app.db)
        .await
        .unwrap();
    products::ActiveModel {
        id: Set(c),
        deleted_at: Set(Some(chrono::Utc::now())),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let list = public_get(
        &app,
        &format!("/api/v1/public/products?category_id={parent}&facet_country=A"),
    )
    .await;
    assert_eq!(listed_ids(&list), vec![a]);
    let options = public_get(
        &app,
        &format!("/api/v1/public/catalog-facet-values?dimension=country&category_id={parent}"),
    )
    .await;
    assert_eq!(option_ids(&options), vec!["A"]);
    assert_eq!(
        public_get(&app, "/api/v1/public/products/visibility-a").await["status_code"],
        0
    );
    categories::ActiveModel {
        id: Set(parent),
        is_active: Set(false),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    for scope in [
        String::new(),
        format!("&category_id={parent}"),
        format!("&category_id={child}"),
    ] {
        let list = public_get(
            &app,
            &format!("/api/v1/public/products?facet_country=A{scope}"),
        )
        .await;
        assert!(listed_ids(&list).is_empty());
        assert_eq!(list["pagination"]["total"], 0);
        let options = public_get(
            &app,
            &format!("/api/v1/public/catalog-facet-values?dimension=country{scope}&value_id=A"),
        )
        .await;
        assert!(option_ids(&options).is_empty());
        assert_eq!(options["pagination"]["total"], 0);
    }
    assert_ne!(
        public_get(&app, "/api/v1/public/products/visibility-a").await["status_code"],
        0
    );
    categories::ActiveModel {
        id: Set(parent),
        is_active: Set(true),
        deleted_at: Set(Some(chrono::Utc::now())),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    assert!(
        option_ids(
            &public_get(
                &app,
                &format!(
                    "/api/v1/public/catalog-facet-values?dimension=country&category_id={child}"
                )
            )
            .await
        )
        .is_empty()
    );
}

#[tokio::test]
async fn public_capabilities_and_invalid_filters_pages_and_search_fail_explicitly() {
    let (app, _, _) = fixture().await;
    let caps = public_get(&app, "/api/v1/public/catalog-facets").await;
    assert_eq!(
        caps["data"],
        json!({"schema_version":1,"dimensions":["family","project","country","city","ip_type","line"],"revision_format":"decimal_string","supports_product_filters":true,"supports_value_search":true})
    );
    for url in [
        "/api/v1/public/products?facet_country=A&facet_country=B",
        "/api/v1/public/catalog-facet-values?dimension=country&facet_city=A&facet_city=B",
        "/api/v1/public/catalog-facet-values?dimension=unsupported",
        "/api/v1/public/products?facet_country=",
        "/api/v1/public/products?facet_country=bad%20id",
        "/api/v1/public/catalog-facet-values?dimension=country&value_id=",
        "/api/v1/public/catalog-facet-values?dimension=country&facet_line=bad%20id",
    ] {
        let res = public_get(&app, url).await;
        assert_eq!(res["status_code"], 400, "{res}");
        assert_ne!(res["msg"], "error.catalog_facets_value_invalid");
    }
    for url in [
        "/api/v1/public/products?page=18446744073709551615&page_size=200",
        "/api/v1/public/catalog-facet-values?dimension=country&page=18446744073709551615&page_size=200",
    ] {
        let res = public_get(&app, url).await;
        assert_eq!(res["status_code"], 400, "{res}");
        assert_ne!(res["msg"], "error.catalog_facets_page_invalid");
    }
    for term in ["x".repeat(161), "bad%00term".into()] {
        let res = public_get(
            &app,
            &format!("/api/v1/public/catalog-facet-values?dimension=country&option_search={term}"),
        )
        .await;
        assert_eq!(res["status_code"], 400, "{res}");
    }
}

// RSL-06: filtering, options and counts all obey the same reseller scope.
#[tokio::test]
async fn reseller_hidden_and_inactive_scopes_never_leak_option_values_or_totals() {
    use zs_domain::catalog::facets::{FacetFilters, FacetKey, FacetOptionsQuery};
    use zs_domain::reseller::ports::ProfileRepo;
    use zs_domain::reseller::pricing::SettingInput;
    use zs_domain::reseller::tenant::ResellerTenant;
    use zs_shared::page::PageRequest;
    let (app, _, repo) = fixture().await;
    let cat = category(&app, "tenant-options", 0).await;
    let a = add_browse_product(&app, &repo, cat, "tenant-a", &[("country", "A", "A")]).await;
    let b = add_browse_product(&app, &repo, cat, "tenant-b", &[("country", "B", "B")]).await;
    let owner = catalog_common::user(&app, "facets-reseller@example.test", 0).await;
    let profiles = zs_infra::db::repo::reseller::SeaResellerStore::new(app.db.clone());
    let profile = profiles
        .create_profile(owner, "Fixture", chrono::Utc::now())
        .await
        .unwrap();
    app.services
        .reseller
        .management
        .approve(1, profile.id, 0.into(), 0.into())
        .await
        .unwrap();
    app.services
        .reseller
        .product_settings
        .save(
            zs_app::reseller::product_setting::Owner::Reseller(profile.id),
            b,
            &[SettingInput {
                is_listed: false,
                ..Default::default()
            }],
        )
        .await
        .unwrap();
    let domain = app
        .services
        .reseller
        .management
        .assign_system_domain(profile.id, "facet-tenant")
        .await
        .unwrap();
    let (_, http_main) = public_at(
        &app,
        "main.example.test",
        &format!("/api/v1/public/catalog-facet-values?dimension=country&category_id={cat}"),
    )
    .await;
    assert_eq!(option_ids(&http_main), vec!["A", "B"]);
    let (status, http_reseller) = public_at(
        &app,
        &domain.domain,
        &format!("/api/v1/public/catalog-facet-values?dimension=country&category_id={cat}"),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);
    assert_eq!(option_ids(&http_reseller), vec!["A"]);
    assert_eq!(http_reseller["pagination"]["total"], 1);
    let (_, http_list) = public_at(
        &app,
        &domain.domain,
        &format!("/api/v1/public/products?category_id={cat}&facet_country=B"),
    )
    .await;
    assert!(listed_ids(&http_list).is_empty());
    assert_eq!(http_list["pagination"]["total"], 0);
    let (status, _) = public_at(
        &app,
        "unknown.shop.example.test",
        "/api/v1/public/catalog-facet-values?dimension=country",
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND);
    let query = FacetOptionsQuery {
        dimension: FacetKey::Country,
        search: String::new(),
        exact: None,
        page: PageRequest::default(),
    };
    let tenant = ResellerTenant::reseller("tenant.example.test", profile.id, owner, "");
    let filters = FacetFilters::default();
    let list = app
        .services
        .catalog
        .product
        .list_public_faceted_for(&tenant, &cat.to_string(), "", query.page, &filters)
        .await
        .unwrap();
    assert_eq!(list.items.iter().map(|p| p.id).collect::<Vec<_>>(), vec![a]);
    assert_eq!(list.total, 1);
    let options = app
        .services
        .catalog
        .browse
        .options(&tenant, &cat.to_string(), "", &filters, &query)
        .await
        .unwrap();
    assert_eq!(options.total, 1);
    assert_eq!(options.items[0].value_id.as_str(), "A");
    assert_eq!(options.items[0].product_count, 1);
    let main = app
        .services
        .catalog
        .browse
        .options(
            &ResellerTenant::main("main.example.test"),
            &cat.to_string(),
            "",
            &filters,
            &query,
        )
        .await
        .unwrap();
    assert_eq!(main.total, 2);
    use sea_orm::Set;
    use zs_infra::db::entity::reseller_profiles;
    reseller_profiles::ActiveModel {
        id: Set(profile.id),
        status: Set("disabled".into()),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    for tenant in [
        tenant,
        ResellerTenant::reseller("missing.example.test", 99999, owner, ""),
        ResellerTenant::unavailable("unknown.example.test", "not_found"),
    ] {
        let list = app
            .services
            .catalog
            .product
            .list_public_faceted_for(&tenant, &cat.to_string(), "", query.page, &filters)
            .await
            .unwrap();
        assert_eq!(list.total, 0);
        assert!(list.items.is_empty());
        let options = app
            .services
            .catalog
            .browse
            .options(&tenant, &cat.to_string(), "", &filters, &query)
            .await
            .unwrap();
        assert_eq!(options.total, 0);
        assert!(options.items.is_empty());
    }
    // Storage errors remain errors, rather than masquerading as an empty catalog.
    app.db
        .execute_unprepared("DROP TABLE reseller_profiles")
        .await
        .unwrap();
    let broken = ResellerTenant::reseller("broken.example.test", profile.id, owner, "");
    assert_eq!(
        app.services
            .catalog
            .browse
            .options(&broken, "", "", &filters, &query)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Internal
    );
    assert_eq!(
        app.services
            .catalog
            .product
            .list_public_faceted_for(&broken, "", "", query.page, &filters)
            .await
            .unwrap_err()
            .kind(),
        ErrorKind::Internal
    );
}

#[tokio::test]
async fn unsupported_documents_are_excluded_and_corrupt_option_identity_fails_explicitly() {
    use sea_orm::{Set, sea_query::Expr};
    let (app, id, repo) = fixture().await;
    repo.replace(id, 0, &attributes("Valid")).await.unwrap();
    documents::Entity::update_many()
        .col_expr(documents::Column::SchemaVersion, Expr::value(2))
        .exec(&app.db)
        .await
        .unwrap();
    assert!(
        option_ids(
            &public_get(
                &app,
                "/api/v1/public/catalog-facet-values?dimension=country"
            )
            .await
        )
        .is_empty()
    );
    assert!(
        listed_ids(
            &public_get(
                &app,
                "/api/v1/public/products?facet_country=provider:country:000A"
            )
            .await
        )
        .is_empty()
    );
    documents::Entity::update_many()
        .col_expr(documents::Column::SchemaVersion, Expr::value(1))
        .exec(&app.db)
        .await
        .unwrap();
    let row = rows::Entity::find()
        .filter(rows::Column::Dimension.eq("country"))
        .one(&app.db)
        .await
        .unwrap()
        .unwrap();
    rows::ActiveModel {
        id: Set(row.id),
        value_key: Set(b"other".to_vec()),
        ..Default::default()
    }
    .update(&app.db)
    .await
    .unwrap();
    let res = public_get(
        &app,
        "/api/v1/public/catalog-facet-values?dimension=country",
    )
    .await;
    assert_eq!(res["status_code"], 500);
    assert_eq!(repo.get(id).await.unwrap_err().kind(), ErrorKind::Internal);
}
