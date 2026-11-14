//! Admin-only versioned metadata. Public browse capabilities are wired separately.
use axum::extract::State;
use serde::{Deserialize, Serialize};
use zs_app::catalog::facets::FacetInput;
use zs_domain::catalog::facets::{FacetDocument, FacetSet};

use crate::extract::{Body, PathId};
use crate::response::{ApiResult, Data, ok};
use crate::routes::Routes;
use crate::state::AppState;

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
