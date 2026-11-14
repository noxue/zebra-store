//! Atomic browse metadata updates; every transaction query uses its own handle (DB-01).
use async_trait::async_trait;
use sea_orm::sea_query::{Expr, ExprTrait, Query, SelectStatement};
use sea_orm::{
    ColumnTrait, Condition, ConnectionTrait, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, Set, TransactionTrait,
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
                value_key: Set(facet.value_id.as_str().as_bytes().to_vec()),
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
            if row.value_key != row.value_id.as_bytes() {
                return Err(Error::internal_msg("catalog index identity bytes disagree"));
            }
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

/// Only understood, committed documents can contribute public browse values.
pub(super) fn valid_documents() -> SelectStatement {
    documents::Entity::find()
        .select_only()
        .column(documents::Column::ProductId)
        .filter(documents::Column::SchemaVersion.eq(FACET_SCHEMA_VERSION as i32))
        .filter(documents::Column::Revision.gt(0))
        .into_query()
}

#[derive(Debug, sea_orm::FromQueryResult)]
struct OptionCount {
    value_key: Vec<u8>,
    product_count: i64,
}

#[async_trait]
impl zs_domain::catalog::facets::FacetBrowseRepo for SeaFacetRepo {
    async fn options(
        &self,
        filter: &zs_domain::catalog::product::ProductFilter,
        query: &zs_domain::catalog::facets::FacetOptionsQuery,
    ) -> Result<zs_shared::page::Page<zs_domain::catalog::facets::FacetOption>> {
        use zs_domain::catalog::facets::{Facet, FacetOption};
        let offset = query
            .page
            .page
            .checked_sub(1)
            .and_then(|p| p.checked_mul(query.page.page_size))
            .filter(|n| *n <= i64::MAX as u64)
            .ok_or_else(|| Error::bad_request("error.catalog_facets_page_invalid"))?;
        if !(1..=zs_shared::page::MAX_PAGE_SIZE).contains(&query.page.page_size) {
            return Err(Error::bad_request("error.catalog_facets_page_invalid"));
        }
        // Keep representatives, counts and the page in one read snapshot. The
        // ORM applies supported transaction settings without business dialect branches.
        let txn = self
            .db
            .begin_with_config(
                Some(sea_orm::IsolationLevel::RepeatableRead),
                Some(sea_orm::AccessMode::ReadOnly),
            )
            .await
            .dom()?;
        let mut scope = filter.clone();
        scope.facets = scope.facets.without(query.dimension);
        let eligible = products::Entity::find()
            .select_only()
            .column(products::Column::Id)
            .filter(super::product::filter_condition(&txn, &scope))
            .into_query();
        let base = Condition::all()
            .add(rows::Column::Dimension.eq(query.dimension.as_str()))
            .add(Expr::col(rows::Column::ProductId).in_subquery(eligible))
            .add(Expr::col(rows::Column::ProductId).in_subquery(valid_documents()));
        // One deterministic, latest committed label per exact byte identity. Search
        // the representative after grouping, so a rename never reduces its count.
        let representatives = Query::select()
            .expr(Expr::col(rows::Column::Id).max())
            .from(rows::Entity)
            .cond_where(base.clone())
            .group_by_col(rows::Column::ValueKey)
            .to_owned();
        let mut options =
            rows::Entity::find().filter(Expr::col(rows::Column::Id).in_subquery(representatives));
        if let Some(value) = &query.exact {
            options = options
                .filter(rows::Column::ValueId.eq(value.as_str()))
                .filter(rows::Column::ValueKey.eq(value.as_str().as_bytes().to_vec()));
        }
        if !query.search.is_empty() {
            let mut labels = Condition::any();
            for locale in super::sql::SEARCH_LOCALES {
                labels = labels.add(super::sql::ilike_sql(
                    super::sql::json_text(
                        super::sql::backend_of(&txn),
                        "product_catalog_facets.label_json",
                        locale,
                    ),
                    &query.search,
                ));
            }
            options = options.filter(labels);
        }
        let total = options.clone().count(&txn).await.dom()?;
        let selected = options
            .order_by_asc(rows::Column::ValueKey)
            .offset(offset)
            .limit(query.page.page_size)
            .all(&txn)
            .await
            .dom()?;
        let keys: Vec<_> = selected.iter().map(|r| r.value_key.clone()).collect();
        let mut counts = std::collections::BTreeMap::new();
        if !keys.is_empty() {
            let results = rows::Entity::find()
                .select_only()
                .column(rows::Column::ValueKey)
                .column_as(Expr::col(rows::Column::Id).count(), "product_count")
                .filter(base)
                .filter(rows::Column::ValueKey.is_in(keys))
                .group_by(rows::Column::ValueKey)
                .into_model::<OptionCount>()
                .all(&txn)
                .await
                .dom()?;
            for count in results {
                counts.insert(
                    count.value_key,
                    u64::try_from(count.product_count).map_err(Error::internal)?,
                );
            }
        }
        let mut items = Vec::with_capacity(selected.len());
        for row in selected {
            if row.value_key != row.value_id.as_bytes() {
                return Err(Error::internal_msg("catalog index identity bytes disagree"));
            }
            let value_id: zs_domain::catalog::facets::FacetValueId =
                row.value_id.try_into().map_err(Error::internal)?;
            let label: zs_shared::i18n::LocalizedText =
                serde_json::from_value(row.label_json).map_err(Error::internal)?;
            let _validated = FacetSet::try_from(vec![Facet {
                key: query.dimension,
                value_id: value_id.clone(),
                label: label.clone(),
            }])
            .map_err(Error::internal)?;
            let product_count = counts
                .remove(&row.value_key)
                .ok_or_else(|| Error::internal_msg("catalog option count missing"))?;
            items.push(FacetOption {
                value_id,
                label,
                product_count,
            });
        }
        txn.commit().await.dom()?;
        Ok(zs_shared::page::Page { items, total })
    }
}
