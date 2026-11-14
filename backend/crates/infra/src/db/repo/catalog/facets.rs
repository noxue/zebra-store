//! Atomic browse metadata updates; every transaction query uses its own handle (DB-01).
use async_trait::async_trait;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait, QueryFilter, QuerySelect, Set,
    TransactionTrait,
};
use zs_domain::catalog::facets::{FACET_SCHEMA_VERSION, Facet, FacetDocument, FacetRepo, FacetSet};
use zs_domain::catalog::product::keys;
use zs_domain::{Error, Id, Result};

use crate::db::entity::extra::{
    product_catalog_documents as documents, product_catalog_facets as rows,
};
use crate::db::entity::products;
use crate::db::repo::support::{
    DbResultExt, MAX_TXN_ATTEMPTS, insert_if_absent, is_transient, now,
};

const CHANGED: &str = "error.catalog_facets_changed";
const INVALID_REVISION: &str = "error.catalog_facets_revision_invalid";
const EXHAUSTED: &str = "error.catalog_facets_revision_exhausted";

#[derive(Debug, Clone)]
pub struct SeaFacetRepo {
    db: DatabaseConnection,
}

impl SeaFacetRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }

    async fn replace_once(
        &self,
        product_id: Id,
        expected: i64,
        facets: &FacetSet,
    ) -> Result<FacetDocument> {
        let txn = self.db.begin().await.dom()?;
        let product = products::Entity::find_by_id(product_id)
            .filter(products::Column::DeletedAt.is_null())
            .lock_exclusive()
            .one(&txn)
            .await
            .dom()?;
        if product.is_none() {
            return Err(Error::not_found(keys::NOT_FOUND));
        }
        let stored = documents::Entity::find()
            .filter(documents::Column::ProductId.eq(product_id))
            .one(&txn)
            .await
            .dom()?;
        let original = decode(stored.as_ref())?;
        verify_projection(&txn, product_id, &original.facets).await?;
        if original.revision != u64::try_from(expected).map_err(Error::internal)? {
            return Err(Error::bad_request(CHANGED));
        }
        // An explicit clear is a tombstone even when nothing is stored yet:
        // it must invalidate an older in-flight initial publication.
        if original.facets == *facets && !facets.entries().is_empty() {
            txn.commit().await.dom()?;
            return Ok(original);
        }
        let revision = expected
            .checked_add(1)
            .ok_or_else(|| Error::bad_request(EXHAUSTED))?;
        let payload = serde_json::to_value(facets)?;
        let at = now();
        if stored.is_none() {
            let inserted = insert_if_absent(
                &txn,
                documents::ActiveModel {
                    product_id: Set(product_id),
                    schema_version: Set(
                        i32::try_from(FACET_SCHEMA_VERSION).map_err(Error::internal)?
                    ),
                    revision: Set(revision),
                    facets_json: Set(payload),
                    created_at: Set(at),
                    updated_at: Set(at),
                    ..Default::default()
                },
                documents::Column::Id,
            )
            .await?;
            if !inserted {
                return Err(Error::bad_request(CHANGED));
            }
        } else {
            let changed = documents::Entity::update_many()
                .col_expr(documents::Column::Revision, Expr::value(revision))
                .col_expr(documents::Column::FacetsJson, Expr::value(payload))
                .col_expr(documents::Column::UpdatedAt, Expr::value(at))
                .filter(documents::Column::ProductId.eq(product_id))
                .filter(documents::Column::Revision.eq(expected))
                .exec(&txn)
                .await
                .dom()?;
            if changed.rows_affected != 1 {
                return Err(Error::bad_request(CHANGED));
            }
        }
        // ORD-11: value rows have no historical references, so clear by hard deletion.
        rows::Entity::delete_many()
            .filter(rows::Column::ProductId.eq(product_id))
            .exec(&txn)
            .await
            .dom()?;
        for facet in facets.entries() {
            rows::Entity::insert(rows::ActiveModel {
                product_id: Set(product_id),
                dimension: Set(facet.key.as_str().to_owned()),
                value_id: Set(facet.value_id.as_str().to_owned()),
                label_json: Set(serde_json::to_value(&facet.label)?),
                created_at: Set(at),
                updated_at: Set(at),
                ..Default::default()
            })
            .exec(&txn)
            .await
            .dom()?;
        }
        txn.commit().await.dom()?;
        Ok(FacetDocument {
            schema_version: FACET_SCHEMA_VERSION,
            revision: u64::try_from(revision).map_err(Error::internal)?,
            facets: facets.clone(),
        })
    }
}

fn decode(row: Option<&documents::Model>) -> Result<FacetDocument> {
    match row {
        None => Ok(FacetDocument {
            schema_version: FACET_SCHEMA_VERSION,
            revision: 0,
            facets: FacetSet::default(),
        }),
        Some(row)
            if row.revision > 0
                && u32::try_from(row.schema_version).ok() == Some(FACET_SCHEMA_VERSION) =>
        {
            Ok(FacetDocument {
                schema_version: FACET_SCHEMA_VERSION,
                revision: u64::try_from(row.revision).map_err(Error::internal)?,
                // Corrupt or unsupported metadata is an error, never a fabricated empty set.
                facets: serde_json::from_value(row.facets_json.clone())?,
            })
        }
        Some(_) => Err(Error::internal(
            "invalid persisted catalog version or revision",
        )),
    }
}

async fn verify_projection<C: ConnectionTrait>(
    connection: &C,
    product_id: Id,
    expected: &FacetSet,
) -> Result<()> {
    let stored = rows::Entity::find()
        .filter(rows::Column::ProductId.eq(product_id))
        .all(connection)
        .await
        .dom()?;
    let values = stored
        .into_iter()
        .map(|row| {
            Ok(Facet {
                key: serde_json::from_value(serde_json::Value::String(row.dimension))?,
                value_id: row.value_id.try_into().map_err(Error::internal)?,
                label: serde_json::from_value(row.label_json)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if FacetSet::try_from(values).map_err(Error::internal)? != *expected {
        return Err(Error::internal(
            "catalog document and browse index disagree",
        ));
    }
    Ok(())
}

#[async_trait]
impl FacetRepo for SeaFacetRepo {
    async fn get(&self, product_id: Id) -> Result<FacetDocument> {
        let txn = self.db.begin().await.dom()?;
        let product = products::Entity::find_by_id(product_id)
            .filter(products::Column::DeletedAt.is_null())
            .lock_shared()
            .one(&txn)
            .await
            .dom()?;
        if product.is_none() {
            return Err(Error::not_found(keys::NOT_FOUND));
        }
        let row = documents::Entity::find()
            .filter(documents::Column::ProductId.eq(product_id))
            .one(&txn)
            .await
            .dom()?;
        let document = decode(row.as_ref())?;
        verify_projection(&txn, product_id, &document.facets).await?;
        txn.commit().await.dom()?;
        Ok(document)
    }

    async fn replace(
        &self,
        product_id: Id,
        expected_revision: u64,
        facets: &FacetSet,
    ) -> Result<FacetDocument> {
        let expected =
            i64::try_from(expected_revision).map_err(|_| Error::bad_request(INVALID_REVISION))?;
        for attempt in 0..MAX_TXN_ATTEMPTS {
            match self.replace_once(product_id, expected, facets).await {
                Err(error) if is_transient(&error) && attempt + 1 < MAX_TXN_ATTEMPTS => continue,
                result => return result,
            }
        }
        Err(Error::internal("catalog transaction retries exhausted"))
    }
}
