//! Versioned browse metadata commands, separate from price and stock writes.
use std::sync::Arc;

use serde::Deserialize;
use zs_domain::catalog::facets::{
    FACET_SCHEMA_VERSION, Facet, FacetDocument, FacetError, FacetKey, FacetRepo, FacetSet,
};
use zs_domain::{Error, Id, Result};
use zs_shared::i18n::LocalizedText;

/// Raw boundary input. Opaque identities are validated before persistence.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FacetInput {
    pub key: FacetKey,
    pub value_id: String,
    pub label: LocalizedText,
}

#[derive(Clone)]
pub struct FacetService {
    repo: Arc<dyn FacetRepo>,
}

impl std::fmt::Debug for FacetService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FacetService")
    }
}

fn invalid(error: FacetError) -> Error {
    Error::bad_request(match error {
        FacetError::InvalidValueId => "error.catalog_facets_value_invalid",
        FacetError::InvalidLabel => "error.catalog_facets_label_invalid",
        FacetError::DuplicateDimension => "error.catalog_facets_duplicate_dimension",
        FacetError::TooManyFacets => "error.catalog_facets_too_many",
    })
}

impl FacetService {
    pub fn new(repo: Arc<dyn FacetRepo>) -> Self {
        Self { repo }
    }

    pub async fn get(&self, product_id: Id) -> Result<FacetDocument> {
        self.repo.get(product_id).await
    }

    pub async fn replace(
        &self,
        product_id: Id,
        schema_version: u32,
        expected_revision: &str,
        values: Vec<FacetInput>,
    ) -> Result<FacetDocument> {
        if schema_version != FACET_SCHEMA_VERSION {
            return Err(Error::bad_request(
                "error.catalog_facets_schema_unsupported",
            ));
        }
        // Revisions cross the JavaScript boundary as canonical decimal strings,
        // never imprecise JSON numbers. Storage uses a signed bigint.
        let revision = expected_revision
            .parse::<i64>()
            .ok()
            .filter(|r| *r >= 0 && r.to_string() == expected_revision)
            .and_then(|r| u64::try_from(r).ok())
            .ok_or_else(|| Error::bad_request("error.catalog_facets_revision_invalid"))?;
        let facets = values
            .into_iter()
            .map(|input| {
                Ok(Facet {
                    key: input.key,
                    value_id: input.value_id.try_into().map_err(invalid)?,
                    label: input.label,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let facets = FacetSet::try_from(facets).map_err(invalid)?;
        self.repo.replace(product_id, revision, &facets).await
    }
}

const MAX_OPTION_SEARCH_CHARACTERS: usize = 160;

/// Reject unrepresentable offsets before a database driver or multiplication can fail.
pub fn validate_page(page: zs_shared::page::PageRequest) -> Result<()> {
    let offset = page
        .page
        .checked_sub(1)
        .and_then(|p| p.checked_mul(page.page_size));
    if !(1..=zs_shared::page::MAX_PAGE_SIZE).contains(&page.page_size)
        || offset.is_none_or(|n| n > i64::MAX as u64)
    {
        return Err(Error::bad_request("error.catalog_facets_page_invalid"));
    }
    Ok(())
}

#[derive(Clone)]
pub struct FacetBrowseService {
    repo: Arc<dyn zs_domain::catalog::facets::FacetBrowseRepo>,
    products: super::product::ProductService,
}
impl std::fmt::Debug for FacetBrowseService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FacetBrowseService")
    }
}
impl FacetBrowseService {
    pub fn new(
        repo: Arc<dyn zs_domain::catalog::facets::FacetBrowseRepo>,
        products: super::product::ProductService,
    ) -> Self {
        Self { repo, products }
    }
    pub async fn options(
        &self,
        tenant: &zs_domain::reseller::tenant::ResellerTenant,
        category: &str,
        search: &str,
        filters: &zs_domain::catalog::facets::FacetFilters,
        query: &zs_domain::catalog::facets::FacetOptionsQuery,
    ) -> Result<zs_shared::page::Page<zs_domain::catalog::facets::FacetOption>> {
        validate_page(query.page)?;
        if query.search.chars().count() > MAX_OPTION_SEARCH_CHARACTERS
            || query.search.chars().any(char::is_control)
        {
            return Err(Error::bad_request("error.catalog_facets_search_invalid"));
        }
        let scope = self
            .products
            .public_filter_for(tenant, category, search, query.page, filters)
            .await?;
        self.repo.options(&scope, query).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct NeverWrite(AtomicUsize);

    #[async_trait::async_trait]
    impl FacetRepo for NeverWrite {
        async fn get(&self, _: Id) -> Result<FacetDocument> {
            Err(Error::not_found("error.product_not_found"))
        }
        async fn replace(&self, _: Id, _: u64, _: &FacetSet) -> Result<FacetDocument> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Err(Error::internal_msg("invalid command reached persistence"))
        }
    }

    #[tokio::test]
    async fn unsupported_schema_and_lossy_or_noncanonical_revisions_never_write() {
        let repo = Arc::new(NeverWrite::default());
        let service = FacetService::new(repo.clone());
        let error = service.replace(1, 2, "0", Vec::new()).await;
        assert_eq!(
            error.err().map(|e| e.key().to_owned()),
            Some("error.catalog_facets_schema_unsupported".into())
        );
        for revision in [
            "",
            "-1",
            "+1",
            "01",
            " 1",
            "1 ",
            "1.0",
            "9223372036854775808",
        ] {
            let error = service.replace(1, 1, revision, Vec::new()).await;
            assert_eq!(
                error.err().map(|e| e.key().to_owned()),
                Some("error.catalog_facets_revision_invalid".into())
            );
        }
        assert_eq!(repo.0.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn invalid_values_and_duplicate_dimensions_are_distinct_and_never_write() {
        let repo = Arc::new(NeverWrite::default());
        let service = FacetService::new(repo.clone());
        let value = |id: &str| FacetInput {
            key: FacetKey::Country,
            value_id: id.into(),
            label: LocalizedText::single("Country"),
        };
        let error = service.replace(1, 1, "0", vec![value(" padded")]).await;
        assert_eq!(
            error.err().map(|e| e.key().to_owned()),
            Some("error.catalog_facets_value_invalid".into())
        );
        let error = service
            .replace(1, 1, "0", vec![value("A"), value("B")])
            .await;
        assert_eq!(
            error.err().map(|e| e.key().to_owned()),
            Some("error.catalog_facets_duplicate_dimension".into())
        );
        assert_eq!(repo.0.load(Ordering::SeqCst), 0);
    }
}
