//! Promotion admin endpoints.

use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_domain::marketing::promotion::{Promotion, PromotionFilter, PromotionInput};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::Pagination;

use super::{PageQuery, parse_bool, parse_query_id, parse_time};
use crate::extract::{Bind, BindField, BindRules, PathId, Query, req};
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .post("/promotions", create)
        .get("/promotions", list)
        .put("/promotions/{id}", update)
        .delete("/promotions/{id}", delete)
}

#[derive(Debug, Deserialize)]
struct PromotionRequest {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    scope_ref_id: Id,
    #[serde(default)]
    value: Amount,
    #[serde(default)]
    min_amount: Amount,
    #[serde(default)]
    starts_at: String,
    #[serde(default)]
    ends_at: String,
    is_active: Option<bool>,
}

/// Original `CreatePromotionRequest` (create and update).
impl BindRules for PromotionRequest {
    const FIELDS: &'static [BindField] = &[
        req("name", "Name"),
        req("type", "Type"),
        req("scope_ref_id", "ScopeRefID"),
        req("value", "Value"),
    ];
}

impl PromotionRequest {
    /// `name`, `type`, `scope_ref_id` and non-zero `value` are required (original binding rules).
    fn into_input(self) -> Result<PromotionInput, Error> {
        if self.name.is_empty()
            || self.kind.is_empty()
            || self.scope_ref_id == 0
            || self.value.is_zero()
        {
            return Err(Error::invalid());
        }
        Ok(PromotionInput {
            name: self.name,
            kind: self.kind,
            scope_ref_id: self.scope_ref_id,
            value: self.value,
            min_amount: self.min_amount,
            starts_at: parse_time(&self.starts_at)?,
            ends_at: parse_time(&self.ends_at)?,
            is_active: self.is_active,
        })
    }
}

async fn create(
    State(s): State<AppState>,
    Bind(req): Bind<PromotionRequest>,
) -> ApiResult<Data<Promotion>> {
    ok(s.svc.marketing.promotion.create(req.into_input()?).await?)
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<PromotionRequest>,
) -> ApiResult<Data<Promotion>> {
    ok(s.svc
        .marketing
        .promotion
        .update(id, req.into_input()?)
        .await?)
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.marketing.promotion.delete(id).await?;
    ok(json!({"deleted": true}))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(flatten)]
    page: PageQuery,
    #[serde(default)]
    name: String,
    id: Option<String>,
    scope_ref_id: Option<String>,
    is_active: Option<String>,
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Paged<Promotion>> {
    let filter = PromotionFilter {
        id: parse_query_id(q.id.as_deref(), true)?,
        name: q.name,
        // The original ignores a malformed scope_ref_id.
        scope_ref_id: parse_query_id(q.scope_ref_id.as_deref(), false).unwrap_or(0),
        is_active: parse_bool(q.is_active.as_deref())?,
    };
    let req = q.page.request();
    let page = s.svc.marketing.promotion.list(filter, req).await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}
