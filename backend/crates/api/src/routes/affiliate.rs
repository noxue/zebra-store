//! `affiliate` endpoints: click tracking, the user's affiliate center and the admin
//! affiliate pages (finance pages compliance gated **[C]**).

use std::collections::HashMap;

use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_app::affiliate::ClickInput;
use zs_domain::affiliate::{
    AdminUserItem, Commission, CommissionFilter, Dashboard, Profile, ProfileFilter, WithdrawFilter,
    WithdrawRequest, keys,
};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::{PageRequest, Pagination};

use super::{RouteSet, Routes};
use crate::client::Client;
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req};
use crate::middleware::auth::{CurrentAdmin, CurrentUser};
use crate::middleware::compliance::ComplianceAcked;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::state::AppState;

/// Routes of the `affiliate` group.
pub fn routes() -> RouteSet {
    RouteSet {
        public: Routes::new("/public").post("/affiliate/click", click),
        user: Routes::new("")
            .post("/affiliate/open", open)
            .get("/affiliate/dashboard", dashboard)
            .get("/affiliate/commissions", commissions)
            .get("/affiliate/withdraws", withdraws)
            .post("/affiliate/withdraws", apply_withdraw),
        admin: Routes::new("/admin")
            .get("/affiliates/users", admin_users)
            .patch("/affiliates/users/{id}/status", admin_status)
            .patch("/affiliates/users/batch-status", admin_batch_status)
            .get("/affiliates/commissions", admin_commissions)
            .get("/affiliates/withdraws", admin_withdraws)
            .post("/affiliates/withdraws/{id}/reject", admin_reject)
            .post("/affiliates/withdraws/{id}/pay", admin_pay),
        ..RouteSet::default()
    }
}

type Params = Query<HashMap<String, String>>;

fn page_of(q: &HashMap<String, String>) -> PageRequest {
    let num = |k: &str| q.get(k).and_then(|v| v.trim().parse::<u64>().ok());
    PageRequest::new(num("page"), num("page_size"))
}

fn text(q: &HashMap<String, String>, key: &str) -> String {
    q.get(key).map(|s| s.trim().to_owned()).unwrap_or_default()
}

/// Admin id filters ignore parse errors like the original (`id, _ := ParseQueryUint`).
fn lenient_id(q: &HashMap<String, String>, key: &str) -> Id {
    q.get(key)
        .and_then(|v| v.trim().parse::<Id>().ok())
        .filter(|id| *id > 0)
        .unwrap_or(0)
}

fn fetch_failed(e: Error) -> Error {
    e.or_internal(keys::USER_FETCH_FAILED)
}

// ---------------------------------------------------------------------------
// Storefront DTOs (original `affiliatepresenter`)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ProfileResp {
    id: Id,
    code: String,
    status: String,
    created_at: DateTime<Utc>,
}

impl From<Profile> for ProfileResp {
    fn from(p: Profile) -> Self {
        Self {
            id: p.id,
            code: p.affiliate_code,
            status: p.status,
            created_at: p.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct CommissionResp {
    id: Id,
    commission_type: String,
    commission_amount: Amount,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    confirm_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    available_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

impl From<Commission> for CommissionResp {
    fn from(c: Commission) -> Self {
        Self {
            id: c.id,
            commission_type: c.commission_type,
            commission_amount: c.commission_amount,
            status: c.status,
            confirm_at: c.confirm_at,
            available_at: c.available_at,
            created_at: c.created_at,
        }
    }
}

#[derive(Debug, Serialize)]
struct WithdrawResp {
    id: Id,
    amount: Amount,
    channel: String,
    account: String,
    status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    reject_reason: String,
    created_at: DateTime<Utc>,
}

impl From<WithdrawRequest> for WithdrawResp {
    fn from(w: WithdrawRequest) -> Self {
        Self {
            id: w.id,
            amount: w.amount,
            channel: w.channel,
            account: w.account,
            status: w.status,
            reject_reason: w.reject_reason,
            created_at: w.created_at,
        }
    }
}

// ---------------------------------------------------------------------------
// Public / user endpoints
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ClickRequest {
    #[serde(default)]
    affiliate_code: String,
    #[serde(default)]
    visitor_key: String,
    #[serde(default)]
    landing_path: String,
    #[serde(default)]
    referrer: String,
}

impl BindRules for ClickRequest {
    const FIELDS: &'static [BindField] = &[req("affiliate_code", "AffiliateCode")];
}

async fn click(
    State(s): State<AppState>,
    Client(client): Client,
    Bind(req): Bind<ClickRequest>,
) -> ApiResult<Data<Value>> {
    if req.affiliate_code.is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .affiliate
        .service
        .track_click(&ClickInput {
            affiliate_code: req.affiliate_code,
            visitor_key: req.visitor_key,
            landing_path: req.landing_path,
            referrer: req.referrer,
            client_ip: client.ip,
            user_agent: client.user_agent,
        })
        .await
        .map_err(|e| e.or_internal(keys::SAVE_FAILED))?;
    ok(json!({ "ok": true }))
}

async fn open(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
) -> ApiResult<Data<ProfileResp>> {
    let profile = s.svc.affiliate.service.open(u.id).await?;
    ok(profile.into())
}

async fn dashboard(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
) -> ApiResult<Data<Dashboard>> {
    ok(s.svc
        .affiliate
        .service
        .dashboard(u.id)
        .await
        .map_err(fetch_failed)?)
}

async fn commissions(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Params,
) -> ApiResult<Paged<CommissionResp>> {
    let req = page_of(&q);
    let page = s
        .svc
        .affiliate
        .service
        .user_commissions(u.id, &text(&q, "status"), req)
        .await
        .map_err(fetch_failed)?;
    Ok(Paged(
        page.items.into_iter().map(Into::into).collect(),
        Pagination::new(req, page.total),
    ))
}

async fn withdraws(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Query(q): Params,
) -> ApiResult<Paged<WithdrawResp>> {
    let req = page_of(&q);
    let page = s
        .svc
        .affiliate
        .service
        .user_withdraws(u.id, &text(&q, "status"), req)
        .await
        .map_err(fetch_failed)?;
    Ok(Paged(
        page.items.into_iter().map(Into::into).collect(),
        Pagination::new(req, page.total),
    ))
}

#[derive(Debug, Deserialize)]
struct WithdrawBody {
    amount: Option<Value>,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    account: String,
}

impl BindRules for WithdrawBody {
    const FIELDS: &'static [BindField] = &[
        req("amount", "Amount"),
        req("channel", "Channel"),
        req("account", "Account"),
    ];
}

async fn apply_withdraw(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Bind(req): Bind<WithdrawBody>,
) -> ApiResult<Data<WithdrawResp>> {
    let amount = match req.amount {
        Some(Value::String(s)) => s,
        Some(Value::Number(n)) => n.to_string(),
        _ => return Err(Error::invalid().into()),
    };
    if req.channel.is_empty() || req.account.is_empty() {
        return Err(Error::invalid().into());
    }
    let row = s
        .svc
        .affiliate
        .service
        .apply_withdraw(u.id, &amount, &req.channel, &req.account)
        .await?;
    ok(row.into())
}

// ---------------------------------------------------------------------------
// Admin endpoints
// ---------------------------------------------------------------------------

async fn admin_users(
    State(s): State<AppState>,
    Query(q): Params,
) -> ApiResult<Paged<AdminUserItem>> {
    let filter = ProfileFilter {
        user_id: lenient_id(&q, "user_id"),
        status: text(&q, "status"),
        code: text(&q, "code"),
        keyword: text(&q, "keyword"),
    };
    let req = page_of(&q);
    let page = s
        .svc
        .affiliate
        .service
        .admin_users(&filter, req)
        .await
        .map_err(fetch_failed)?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

#[derive(Debug, Deserialize)]
struct StatusRequest {
    #[serde(default)]
    status: String,
}

impl BindRules for StatusRequest {
    const FIELDS: &'static [BindField] = &[req("status", "Status")];
}

async fn admin_status(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<StatusRequest>,
) -> ApiResult<Data<Profile>> {
    if req.status.is_empty() {
        return Err(Error::invalid().into());
    }
    ok(s.svc.affiliate.service.set_status(id, &req.status).await?)
}

#[derive(Debug, Deserialize)]
struct BatchStatusRequest {
    #[serde(default)]
    profile_ids: Vec<Id>,
    #[serde(default)]
    status: String,
}

impl BindRules for BatchStatusRequest {
    const FIELDS: &'static [BindField] =
        &[req("profile_ids", "ProfileIDs"), req("status", "Status")];
}

async fn admin_batch_status(
    State(s): State<AppState>,
    Bind(req): Bind<BatchStatusRequest>,
) -> ApiResult<Data<Value>> {
    if req.profile_ids.is_empty() || req.status.is_empty() {
        return Err(Error::invalid().into());
    }
    let updated = s
        .svc
        .affiliate
        .service
        .batch_status(&req.profile_ids, &req.status)
        .await?;
    ok(json!({ "updated": updated }))
}

async fn admin_commissions(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Params,
) -> ApiResult<Paged<Commission>> {
    let filter = CommissionFilter {
        profile_id: lenient_id(&q, "affiliate_profile_id"),
        order_no: text(&q, "order_no"),
        status: text(&q, "status"),
        keyword: text(&q, "keyword"),
        ..CommissionFilter::default()
    };
    let req = page_of(&q);
    let page = s
        .svc
        .affiliate
        .service
        .admin_commissions(&filter, req)
        .await
        .map_err(fetch_failed)?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

async fn admin_withdraws(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Params,
) -> ApiResult<Paged<WithdrawRequest>> {
    let filter = WithdrawFilter {
        profile_id: lenient_id(&q, "affiliate_profile_id"),
        status: text(&q, "status"),
        keyword: text(&q, "keyword"),
    };
    let req = page_of(&q);
    let page = s
        .svc
        .affiliate
        .service
        .admin_withdraws(&filter, req)
        .await
        .map_err(fetch_failed)?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

#[derive(Debug, Deserialize)]
struct RejectRequest {
    #[serde(default)]
    reason: String,
}

async fn admin_reject(
    State(s): State<AppState>,
    _: ComplianceAcked,
    CurrentAdmin(admin): CurrentAdmin,
    PathId(id): PathId,
    Body(req): Body<RejectRequest>,
) -> ApiResult<Data<WithdrawRequest>> {
    ok(s.svc
        .affiliate
        .service
        .review_withdraw(admin.id, id, false, &req.reason)
        .await?)
}

async fn admin_pay(
    State(s): State<AppState>,
    _: ComplianceAcked,
    CurrentAdmin(admin): CurrentAdmin,
    PathId(id): PathId,
) -> ApiResult<Data<WithdrawRequest>> {
    ok(s.svc
        .affiliate
        .service
        .review_withdraw(admin.id, id, true, "")
        .await?)
}
