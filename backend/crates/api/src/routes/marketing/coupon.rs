//! Coupon admin endpoints.

use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_domain::marketing::coupon::{Coupon, CouponFilter, CouponInput};
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
        .post("/coupons", create)
        .get("/coupons", list)
        .put("/coupons/{id}", update)
        .delete("/coupons/{id}", delete)
}

#[derive(Debug, Deserialize)]
struct CouponRequest {
    #[serde(default)]
    code: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    value: Amount,
    #[serde(default)]
    min_amount: Amount,
    #[serde(default)]
    max_discount: Amount,
    #[serde(default)]
    usage_limit: i32,
    #[serde(default)]
    per_user_limit: i32,
    disabled_wholesale_price: Option<bool>,
    per_item_discount: Option<bool>,
    #[serde(default)]
    payment_roles: Option<Vec<String>>,
    #[serde(default)]
    member_levels: Option<Vec<Id>>,
    scope_ref_ids: Option<Vec<Id>>,
    #[serde(default)]
    starts_at: String,
    #[serde(default)]
    ends_at: String,
    is_active: Option<bool>,
}

/// Original `CreateCouponRequest` (create and update).
impl BindRules for CouponRequest {
    const FIELDS: &'static [BindField] = &[
        req("code", "Code"),
        req("type", "Type"),
        req("value", "Value"),
        req("scope_ref_ids", "ScopeRefIDs"),
    ];
}

impl CouponRequest {
    /// `code`, `type`, non-zero `value` and `scope_ref_ids` are required (original binding rules).
    fn into_input(self) -> Result<CouponInput, Error> {
        let Some(scope_ref_ids) = self.scope_ref_ids else {
            return Err(Error::invalid());
        };
        if self.code.is_empty() || self.kind.is_empty() || self.value.is_zero() {
            return Err(Error::invalid());
        }
        Ok(CouponInput {
            code: self.code,
            kind: self.kind,
            value: self.value,
            min_amount: self.min_amount,
            max_discount: self.max_discount,
            usage_limit: self.usage_limit,
            per_user_limit: self.per_user_limit,
            disabled_wholesale_price: self.disabled_wholesale_price,
            per_item_discount: self.per_item_discount,
            payment_roles: self.payment_roles.unwrap_or_default(),
            member_levels: self.member_levels.unwrap_or_default(),
            scope_ref_ids,
            starts_at: parse_time(&self.starts_at)?,
            ends_at: parse_time(&self.ends_at)?,
            is_active: self.is_active,
        })
    }
}

async fn create(
    State(s): State<AppState>,
    Bind(req): Bind<CouponRequest>,
) -> ApiResult<Data<Coupon>> {
    ok(s.svc.marketing.coupon.create(req.into_input()?).await?)
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<CouponRequest>,
) -> ApiResult<Data<Coupon>> {
    ok(s.svc.marketing.coupon.update(id, req.into_input()?).await?)
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.marketing.coupon.delete(id).await?;
    ok(json!({"deleted": true}))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(flatten)]
    page: PageQuery,
    #[serde(default)]
    code: String,
    id: Option<String>,
    scope_ref_id: Option<String>,
    is_active: Option<String>,
}

async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Paged<Coupon>> {
    let filter = CouponFilter {
        id: parse_query_id(q.id.as_deref(), true)?,
        code: q.code,
        // PRC-15: a non-numeric scope_ref_id is rejected.
        scope_ref_id: parse_query_id(q.scope_ref_id.as_deref(), true)?,
        is_active: parse_bool(q.is_active.as_deref())?,
    };
    let req = q.page.request();
    let page = s.svc.marketing.coupon.list(filter, req).await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}
