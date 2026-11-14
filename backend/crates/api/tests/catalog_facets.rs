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
