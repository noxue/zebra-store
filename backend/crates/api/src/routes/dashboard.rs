//! `dashboard` endpoints: operational reports, admin user management, system
//! version / self-update and the (disabled) ad proxy.
//!
//! `/admin/user-login-logs` and `/admin/users/:id/2fa` are served by the
//! identity group; `/admin/users/:id/wallet*` by the wallet group.
//! `GET /admin/system/version` is not registered: the original backend has no
//! such route (the admin UI reads `app_version` from `/public/config`).

use std::collections::HashMap;

use axum::extract::{Path, State};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_app::dashboard::users::AdminUserUpdate;
use zs_domain::dashboard::inventory::InventoryAlert;
use zs_domain::dashboard::report::{DEFAULT_RANGE, ReportQuery};
use zs_domain::dashboard::stats::{OverviewResponse, RankingsResponse, TrendResponse};
use zs_domain::dashboard::system::{CapabilityResponse, CheckResult, UpdateState};
use zs_domain::dashboard::users::{
    AdminUserDetail, AdminUserItem, PROVIDER_GOOGLE, PROVIDER_TELEGRAM, UserCouponUsage,
    UserListFilter, UserSort, keys,
};
use zs_domain::identity::user::User;
use zs_domain::{Error, Id};
use zs_shared::page::{PageRequest, Pagination};

use super::{RouteSet, Routes};
use crate::extract::{Bind, BindField, BindRules, Body, Query, req};
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::state::AppState;

/// Routes of the `dashboard` group.
pub fn routes() -> RouteSet {
    RouteSet {
        admin: admin(),
        ..RouteSet::default()
    }
}

fn admin() -> Routes {
    Routes::new("/admin")
        .get("/dashboard/overview", overview)
        .get("/dashboard/trends", trends)
        .get("/dashboard/rankings", rankings)
        .get("/dashboard/inventory-alerts", inventory_alerts)
        .get("/users", list_users)
        .put("/users/batch-status", batch_status)
        .delete("/users/{id}/oauth/telegram", unbind_telegram)
        .delete("/users/{id}/oauth/google", unbind_google)
        .get("/users/{id}", get_user)
        .put("/users/{id}", update_user)
        .get("/users/{id}/coupon-usages", coupon_usages)
        .get("/system/version/check", system_version_check)
        .get("/system/update/capability", update_capability)
        .get("/system/update/status", update_status)
        .post("/system/update/start", update_start)
        .post("/system/update/rollback", update_rollback)
        .post("/system/restart", restart)
        .get("/ads/render/{slotCode}", ad_render)
        .post("/ads/impression", ad_impression)
}

type Params = Query<HashMap<String, String>>;

// ---------------------------------------------------------------------------
// Query helpers (original `ginutil` semantics)
// ---------------------------------------------------------------------------

fn text(q: &HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

/// Optional RFC 3339 time; malformed → `error.bad_request`.
fn opt_time(q: &HashMap<String, String>, key: &str) -> Result<Option<DateTime<Utc>>, ApiError> {
    let raw = text(q, key);
    if raw.is_empty() {
        return Ok(None);
    }
    DateTime::parse_from_rfc3339(&raw)
        .map(|t| Some(t.with_timezone(&Utc)))
        .map_err(|_| Error::invalid().into())
}

/// Go `ParseQueryBool`: absent/blank → false, invalid → `error.bad_request`.
fn opt_bool(q: &HashMap<String, String>, key: &str) -> Result<bool, ApiError> {
    match text(q, key).as_str() {
        "" | "0" | "f" | "F" | "false" | "FALSE" | "False" => Ok(false),
        "1" | "t" | "T" | "true" | "TRUE" | "True" => Ok(true),
        _ => Err(Error::invalid().into()),
    }
}

/// Lenient `page` / `page_size` (original `ParsePagination`).
fn page_of(q: &HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

/// Positive numeric `:id`, else `error.user_id_invalid`.
fn user_id(raw: &str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or_else(|| Error::bad_request(keys::USER_ID_INVALID).into())
}

// ---------------------------------------------------------------------------
// Reports
// ---------------------------------------------------------------------------

fn report_query(q: &HashMap<String, String>) -> Result<ReportQuery, ApiError> {
    Ok(ReportQuery {
        from: opt_time(q, "from")?,
        to: opt_time(q, "to")?,
        force_refresh: opt_bool(q, "force_refresh")?,
        range: q
            .get("range")
            .map_or_else(|| DEFAULT_RANGE.to_owned(), |r| r.trim().to_owned()),
        timezone: text(q, "tz"),
    })
}

async fn overview(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Data<OverviewResponse>> {
    ok(s.svc.dashboard.reports.overview(&report_query(&q)?).await?)
}

async fn trends(State(s): State<AppState>, Query(q): Params) -> ApiResult<Data<TrendResponse>> {
    ok(s.svc.dashboard.reports.trends(&report_query(&q)?).await?)
}

async fn rankings(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Data<RankingsResponse>> {
    ok(s.svc.dashboard.reports.rankings(&report_query(&q)?).await?)
}

async fn inventory_alerts(State(s): State<AppState>) -> ApiResult<Data<Vec<InventoryAlert>>> {
    ok(s.svc.dashboard.reports.inventory_alerts().await?)
}

// ---------------------------------------------------------------------------
// Users
// ---------------------------------------------------------------------------

async fn list_users(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Paged<AdminUserItem>> {
    let page = page_of(&q);
    let user_id = match text(&q, "user_id") {
        raw if raw.is_empty() => None,
        raw => Some(user_id(&raw)?),
    };
    let filter = UserListFilter {
        user_id,
        keyword: text(&q, "keyword"),
        status: text(&q, "status"),
        created_from: opt_time(&q, "created_from")?,
        created_to: opt_time(&q, "created_to")?,
        last_login_from: opt_time(&q, "last_login_from")?,
        last_login_to: opt_time(&q, "last_login_to")?,
        sort: UserSort::parse(&text(&q, "sort_by")),
        ascending: text(&q, "sort_order").eq_ignore_ascii_case("asc"),
    };
    let result = s.svc.dashboard.users.list(&filter, page).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}

async fn get_user(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Data<AdminUserDetail>> {
    ok(s.svc.dashboard.users.detail(user_id(&id)?).await?)
}

#[derive(Debug, Deserialize)]
struct UpdateUserRequest {
    nickname: Option<String>,
    locale: Option<String>,
    status: Option<String>,
    email: Option<String>,
    password: Option<String>,
    admin_note: Option<String>,
    email_verified: Option<bool>,
}

async fn update_user(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Body(req): Body<UpdateUserRequest>,
) -> ApiResult<Data<User>> {
    let id = user_id(&id)?;
    let update = AdminUserUpdate {
        nickname: req.nickname,
        locale: req.locale,
        status: req.status,
        email: req.email,
        password: req.password,
        admin_note: req.admin_note,
        email_verified: req.email_verified,
    };
    ok(s.svc.dashboard.users.update(id, update).await?)
}

#[derive(Debug, Deserialize)]
struct BatchStatusRequest {
    user_ids: Option<Vec<Id>>,
    status: Option<String>,
}

impl BindRules for BatchStatusRequest {
    const FIELDS: &'static [BindField] = &[req("user_ids", "UserIDs"), req("status", "Status")];
}

#[derive(Debug, Serialize)]
struct Updated {
    updated: usize,
}

async fn batch_status(
    State(s): State<AppState>,
    Bind(req): Bind<BatchStatusRequest>,
) -> ApiResult<Data<Updated>> {
    let (Some(ids), Some(status)) = (req.user_ids, req.status.filter(|s| !s.is_empty())) else {
        return Err(Error::invalid().into());
    };
    let updated = s.svc.dashboard.users.batch_status(&ids, &status).await?;
    ok(Updated { updated })
}

async fn unbind(s: &AppState, id: &str, provider: &str) -> ApiResult<Data<Value>> {
    s.svc.dashboard.users.unbind(user_id(id)?, provider).await?;
    ok(json!({ "unbound": true }))
}

async fn unbind_telegram(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Data<Value>> {
    unbind(&s, &id, PROVIDER_TELEGRAM).await
}

async fn unbind_google(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Data<Value>> {
    unbind(&s, &id, PROVIDER_GOOGLE).await
}

async fn coupon_usages(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Query(q): Params,
) -> ApiResult<Paged<UserCouponUsage>> {
    let id = user_id(&id)?;
    let page = page_of(&q);
    let result = s.svc.dashboard.users.coupon_usages(id, page).await?;
    Ok(Paged(result.items, Pagination::new(page, result.total)))
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

async fn system_version_check(State(s): State<AppState>) -> ApiResult<Data<CheckResult>> {
    ok(s.svc.dashboard.system.check())
}

async fn update_capability(State(s): State<AppState>) -> ApiResult<Data<CapabilityResponse>> {
    ok(s.svc.dashboard.system.capability())
}

async fn update_status(State(s): State<AppState>) -> ApiResult<Data<UpdateState>> {
    ok(s.svc.dashboard.system.status())
}

async fn update_start(State(s): State<AppState>) -> ApiResult<Data<UpdateState>> {
    ok(s.svc.dashboard.system.start_update()?)
}

async fn update_rollback(State(s): State<AppState>) -> ApiResult<Data<UpdateState>> {
    ok(s.svc.dashboard.system.rollback()?)
}

async fn restart(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    s.svc.dashboard.system.restart()?;
    ok(json!({ "restarting": true }))
}

// ---------------------------------------------------------------------------
// Ad proxy (disabled: behaves like the original when the ad system is unreachable)
// ---------------------------------------------------------------------------

async fn ad_render(Path(_slot): Path<String>) -> ApiResult<Data<Option<Value>>> {
    ok(None)
}

async fn ad_impression() -> ApiResult<Data<Option<Value>>> {
    ok(None)
}
