//! Versioned admin metadata and visibility-scoped public browse endpoints.
use axum::extract::State;
use serde::{Deserialize, Serialize};
use zs_app::catalog::facets::FacetInput;
use zs_domain::catalog::facets::{
    FACET_SCHEMA_VERSION, FacetDocument, FacetFilters, FacetKey, FacetOption, FacetOptionsQuery,
    FacetSet, FacetValueId,
};

use super::PageQuery;
use crate::extract::{Body, PathId, Query};
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::routes::order::common::TenantCtx;
use crate::state::AppState;
use zs_shared::page::Pagination;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/products/{id}/catalog-facets", get)
        .put("/products/{id}/catalog-facets", replace)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplaceRequest {
    schema_version: u32,
    expected_revision: String,
    facets: Vec<FacetInput>,
}

#[derive(Debug, Serialize)]
struct DocumentResponse {
    schema_version: u32,
    revision: String,
    facets: FacetSet,
}

impl From<FacetDocument> for DocumentResponse {
    fn from(value: FacetDocument) -> Self {
        Self {
            schema_version: value.schema_version,
            revision: value.revision.to_string(),
            facets: value.facets,
        }
    }
}

async fn get(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<DocumentResponse>> {
    ok(s.svc.catalog.facets.get(id).await?.into())
}

async fn replace(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(input): Body<ReplaceRequest>,
) -> ApiResult<Data<DocumentResponse>> {
    ok(s.svc
        .catalog
        .facets
        .replace(
            id,
            input.schema_version,
            &input.expected_revision,
            input.facets,
        )
        .await?
        .into())
}

pub(super) fn public() -> Routes {
    Routes::new("/public")
        .get("/catalog-facets", capabilities)
        .get("/catalog-facet-values", options)
}

#[derive(Debug, Serialize)]
struct Capabilities {
    schema_version: u32,
    dimensions: [FacetKey; 6],
    revision_format: &'static str,
    supports_product_filters: bool,
    supports_value_search: bool,
}
async fn capabilities() -> ApiResult<Data<Capabilities>> {
    ok(Capabilities {
        schema_version: FACET_SCHEMA_VERSION,
        dimensions: FacetKey::ALL,
        revision_format: "decimal_string",
        supports_product_filters: true,
        supports_value_search: true,
    })
}

#[derive(Debug, Default, Deserialize)]
pub(super) struct FacetQuery {
    facet_family: Option<String>,
    facet_project: Option<String>,
    facet_country: Option<String>,
    facet_city: Option<String>,
    facet_ip_type: Option<String>,
    facet_line: Option<String>,
}
impl FacetQuery {
    pub(super) fn filters(self) -> zs_domain::Result<FacetFilters> {
        let mut values = Vec::new();
        for (key, value) in FacetKey::ALL.into_iter().zip([
            self.facet_family,
            self.facet_project,
            self.facet_country,
            self.facet_city,
            self.facet_ip_type,
            self.facet_line,
        ]) {
            if let Some(value) = value {
                values.push((key, parse_value(value)?));
            }
        }
        FacetFilters::try_from(values).map_err(|_| zs_domain::Error::invalid())
    }
}
fn parse_value(raw: String) -> zs_domain::Result<FacetValueId> {
    raw.try_into()
        .map_err(|_| zs_domain::Error::bad_request("error.catalog_facets_value_invalid"))
}

#[derive(Debug, Deserialize)]
struct OptionsRequest {
    dimension: FacetKey,
    #[serde(flatten)]
    page: PageQuery,
    #[serde(flatten)]
    filters: FacetQuery,
    #[serde(default)]
    category_id: String,
    #[serde(default)]
    search: String,
    #[serde(default)]
    option_search: String,
    value_id: Option<String>,
}
async fn options(
    State(s): State<AppState>,
    TenantCtx(tenant): TenantCtx,
    Query(q): Query<OptionsRequest>,
) -> ApiResult<Paged<FacetOption>> {
    let req = q.page.request();
    let query = FacetOptionsQuery {
        dimension: q.dimension,
        search: q.option_search,
        exact: q.value_id.map(parse_value).transpose()?,
        page: req,
    };
    let page = s
        .svc
        .catalog
        .browse
        .options(
            &tenant,
            &q.category_id,
            &q.search,
            &q.filters.filters()?,
            &query,
        )
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}
