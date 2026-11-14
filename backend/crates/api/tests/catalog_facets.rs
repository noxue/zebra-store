//! Browse metadata persistence: stable IDs, version conflicts and atomic normalized indexes.
#![expect(clippy::unwrap_used, reason = "isolated database fixture assertions")]
mod catalog_common;
mod common;

use catalog_common::{category, id_of, product};
use common::TestApp;
use sea_orm::{ColumnTrait, ConnectionTrait, DbBackend, EntityTrait, PaginatorTrait, QueryFilter};
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
