//! Member level endpoints (admin + public).

use axum::extract::State;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zs_domain::marketing::member_level::{
    LevelPriceInput, MemberLevel, MemberLevelFields, MemberLevelPrice, PublicMemberLevel,
};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::Pagination;

use super::{PageQuery, parse_bool, parse_query_id};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req};
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Default admin page size of member levels (original `ParsePaginationWithKeys(…, 50)`).
const LEVELS_PAGE_SIZE: u64 = 50;

pub(super) fn public() -> Routes {
    Routes::new("/public").get("/member-levels", public_levels)
}

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/member-levels", list)
        .post("/member-levels", create)
        .put("/member-levels/{id}", update)
        .delete("/member-levels/{id}", delete)
        .get("/member-level-prices", prices)
        .post("/member-level-prices/batch", upsert_prices)
        .delete("/member-level-prices/{id}", delete_price)
        .post("/member-levels/backfill", backfill)
        .put("/users/{id}/member-level", set_user_level)
}

async fn public_levels(State(s): State<AppState>) -> ApiResult<Data<Vec<PublicMemberLevel>>> {
    ok(s.svc.marketing.member_level.list_public().await?)
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(flatten)]
    page: PageQuery,
    is_active: Option<String>,
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Paged<MemberLevel>> {
    let is_active = parse_bool(q.is_active.as_deref())?;
    let req = q.page.request_with_default(Some(LEVELS_PAGE_SIZE));
    let page = s.svc.marketing.member_level.list(is_active, req).await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

#[derive(Debug, Deserialize)]
struct LevelRequest {
    name: Option<Map<String, Value>>,
    #[serde(default)]
    slug: String,
    #[serde(default)]
    icon: String,
    #[serde(default)]
    discount_rate: Amount,
    #[serde(default)]
    recharge_threshold: Amount,
    #[serde(default)]
    spend_threshold: Amount,
    #[serde(default)]
    is_default: bool,
    #[serde(default)]
    sort_order: i32,
    is_active: Option<bool>,
}

/// Original `CreateMemberLevelRequest` (create and update).
impl BindRules for LevelRequest {
    const FIELDS: &'static [BindField] = &[req("name", "NameJSON"), req("slug", "Slug")];
}

impl LevelRequest {
    fn into_fields(self, current_active: bool) -> Result<MemberLevelFields, Error> {
        let Some(name) = self.name else {
            return Err(Error::invalid());
        };
        if self.slug.is_empty() {
            return Err(Error::invalid());
        }
        Ok(MemberLevelFields {
            name,
            slug: self.slug,
            icon: self.icon,
            discount_rate: self.discount_rate,
            recharge_threshold: self.recharge_threshold,
            spend_threshold: self.spend_threshold,
            is_default: self.is_default,
            sort_order: self.sort_order,
            is_active: self.is_active.unwrap_or(current_active),
        })
    }
}

async fn create(
    State(s): State<AppState>,
    Bind(req): Bind<LevelRequest>,
) -> ApiResult<Data<MemberLevel>> {
    ok(s.svc
        .marketing
        .member_level
        .create(req.into_fields(true)?)
        .await?)
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<LevelRequest>,
) -> ApiResult<Data<MemberLevel>> {
    let levels = &s.svc.marketing.member_level;
    let current = levels.get(id).await?;
    ok(levels
        .update(id, req.into_fields(current.is_active)?)
        .await?)
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.marketing.member_level.delete(id).await?;
    ok(json!({"deleted": true}))
}

#[derive(Debug, Default, Deserialize)]
struct PricesQuery {
    product_id: Option<String>,
}

async fn prices(
    State(s): State<AppState>,
    Query(q): Query<PricesQuery>,
) -> ApiResult<Data<Vec<MemberLevelPrice>>> {
    let product_id = parse_query_id(q.product_id.as_deref(), false)?;
    if product_id == 0 {
        return Err(Error::invalid().into());
    }
    ok(s.svc.marketing.member_level.prices(product_id).await?)
}

#[derive(Debug, Deserialize)]
struct PriceRow {
    #[serde(default)]
    member_level_id: Id,
    #[serde(default)]
    product_id: Id,
    #[serde(default)]
    sku_id: Id,
    #[serde(default)]
    price_amount: Amount,
}

#[derive(Debug, Deserialize)]
struct PricesRequest {
    prices: Option<Vec<PriceRow>>,
}

impl BindRules for PricesRequest {
    const FIELDS: &'static [BindField] = &[req("prices", "Prices")];
}

async fn upsert_prices(
    State(s): State<AppState>,
    Bind(req): Bind<PricesRequest>,
) -> ApiResult<Data<Value>> {
    let rows = req.prices.ok_or_else(Error::invalid)?;
    if rows.iter().any(|r| {
        r.member_level_id <= 0 || r.product_id <= 0 || r.sku_id < 0 || r.price_amount.is_zero()
    }) {
        return Err(Error::invalid().into());
    }
    let rows: Vec<LevelPriceInput> = rows
        .into_iter()
        .map(|r| LevelPriceInput {
            member_level_id: r.member_level_id,
            product_id: r.product_id,
            sku_id: r.sku_id,
            price_amount: r.price_amount,
        })
        .collect();
    s.svc.marketing.member_level.upsert_prices(&rows).await?;
    ok(json!({"saved": true}))
}

async fn delete_price(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.marketing.member_level.delete_price(id).await?;
    ok(json!({"deleted": true}))
}

async fn backfill(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    let affected = s.svc.marketing.member_level.backfill_default().await?;
    ok(json!({"affected": affected}))
}

#[derive(Debug, Deserialize)]
struct SetLevelRequest {
    #[serde(default)]
    member_level_id: Id,
}

async fn set_user_level(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<SetLevelRequest>,
) -> ApiResult<Data<Value>> {
    if req.member_level_id < 0 {
        return Err(Error::invalid().into());
    }
    s.svc
        .marketing
        .member_level
        .set_user_level(id, req.member_level_id)
        .await?;
    ok(json!({"updated": true}))
}
