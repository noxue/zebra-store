//! Admin `/api/v1/admin/resellers/*` (`transport/http/admin`). Finance routes and the
//! finance overview are compliance-gated (**[C]**).

use axum::extract::{Path, State};
use axum::http::request::Parts;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::identity::admin_auth::AdminPrincipal;
use zs_app::reseller::management::ProfileUpdate;
use zs_app::reseller::product_setting::Owner;
use zs_domain::identity::audit::NewAuthzAuditLog;
use zs_domain::reseller::operations::ReportQuery;
use zs_domain::reseller::ports::{
    DomainFilter, FinanceFilter, OrderFilter, ProfileFilter, SettingFilter, SiteConfigFilter,
    WithdrawAction,
};
use zs_domain::reseller::rules::DomainAction;
use zs_domain::reseller::site::SiteConfigInput;
use zs_domain::reseller::{
    BalanceAccount, LedgerEntry, Profile, ProfileStatus, ResellerDomain, WithdrawRequest, keys,
};
use zs_domain::{Error, ErrorKind, Id};
use zs_shared::page::{PageRequest, Pagination};

use super::dto::{self, BalanceResp, DomainResp, LedgerResp, OrderResp, WithdrawResp};
use super::{Params, SettingsRequest, admin_mgmt, admin_products, decimal};
use crate::extract::{Body, PathId, Query};
use crate::middleware::auth::CurrentAdmin;
use crate::middleware::compliance::ComplianceAcked;
use crate::middleware::request_id::RequestId;
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("/admin")
        .get("/resellers/operations/overview", overview)
        .get("/resellers/operations/finance", finance_overview)
        .get("/resellers/profiles", list_profiles)
        .get("/resellers/profiles/{id}", profile_detail)
        .put("/resellers/profiles/{id}", update_profile)
        .put("/resellers/profiles/{id}/system-domain", system_domain)
        .post("/resellers/profiles/{id}/approve", approve)
        .post("/resellers/profiles/{id}/reject", reject)
        .post("/resellers/profiles/{id}/disable", disable)
        .post("/resellers/profiles/{id}/restore", restore)
        .get("/resellers/domains", list_domains)
        .post("/resellers/domains/{id}/approve", approve_domain)
        .post("/resellers/domains/{id}/disable", disable_domain)
        .post("/resellers/domains/{id}/set-primary", set_primary_domain)
        .get("/resellers/site-configs", list_site_configs)
        .get("/resellers/site-configs/{reseller_id}", get_site_config)
        .put("/resellers/site-configs/{reseller_id}", put_site_config)
        .post(
            "/resellers/site-configs/{reseller_id}/reset",
            reset_site_config,
        )
        .get("/resellers/product-settings", list_settings)
        .get(
            "/resellers/product-settings/{reseller_id}/{product_id}",
            get_setting,
        )
        .post(
            "/resellers/product-settings/{reseller_id}/{product_id}/preview",
            preview_setting,
        )
        .put(
            "/resellers/product-settings/{reseller_id}/{product_id}",
            save_setting,
        )
        .delete(
            "/resellers/product-settings/{reseller_id}/{product_id}",
            reset_setting,
        )
        .get("/resellers/ledger-entries", list_ledger)
        .get("/resellers/balance-accounts", list_balances)
        .get("/resellers/withdraws", list_withdraws)
        .post("/resellers/withdraws/{id}/reject", reject_withdraw)
        .post("/resellers/withdraws/{id}/pay", pay_withdraw)
}

/// Operator context for the authz audit trail (`recordAudit`).
#[derive(Debug, Clone)]
struct Audit {
    admin: AdminPrincipal,
    request_id: String,
    method: String,
}

impl<S: Send + Sync> axum::extract::FromRequestParts<S> for Audit {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let CurrentAdmin(admin) = CurrentAdmin::from_request_parts(parts, state).await?;
        Ok(Self {
            admin,
            request_id: parts
                .extensions
                .get::<RequestId>()
                .map(|r| r.0.clone())
                .unwrap_or_default(),
            method: parts.method.as_str().to_owned(),
        })
    }
}

impl Audit {
    async fn record(&self, s: &AppState, action: &str, object: &str, detail: Value) {
        s.svc
            .identity
            .audit
            .record_authz(NewAuthzAuditLog {
                operator_admin_id: self.admin.id,
                operator_username: self.admin.username.clone(),
                target_admin_id: None,
                target_username: String::new(),
                action: action.to_owned(),
                role: String::new(),
                object: object.to_owned(),
                method: self.method.clone(),
                request_id: self.request_id.clone(),
                detail,
            })
            .await;
    }
}

fn paged<T, U>(page: PageRequest, items: Vec<T>, total: u64, f: impl Fn(&T) -> U) -> Paged<U> {
    Paged(items.iter().map(f).collect(), Pagination::new(page, total))
}

fn fetch_failed(e: Error) -> ApiError {
    e.or_internal("error.user_fetch_failed").into()
}

fn report_query(q: &Params) -> ReportQuery {
    let range = q.str("range");
    ReportQuery {
        range: if range.is_empty() { "7d".into() } else { range },
        from: q.time("from"),
        to: q.time("to"),
        timezone: q.str("tz"),
    }
}

/// Malformed `from` / `to` are rejected (`reporting.ParseQuery`).
fn check_report_times(q: &Params) -> Result<(), ApiError> {
    for key in ["from", "to"] {
        if !q.str(key).is_empty() && q.time(key).is_none() {
            return Err(Error::invalid().into());
        }
    }
    Ok(())
}

fn report_error(e: Error) -> ApiError {
    if e.kind() == ErrorKind::BadRequest {
        return e.into();
    }
    e.or_internal("error.dashboard_fetch_failed").into()
}

async fn overview(State(s): State<AppState>, Query(q): Query<Params>) -> ApiResult<Data<Value>> {
    check_report_times(&q)?;
    let data = s
        .svc
        .reseller
        .operations
        .overview(&report_query(&q))
        .await
        .map_err(report_error)?;
    ok(json!(data))
}

async fn finance_overview(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    check_report_times(&q)?;
    let data = s
        .svc
        .reseller
        .operations
        .finance(&report_query(&q))
        .await
        .map_err(report_error)?;
    ok(json!(data))
}

async fn list_profiles(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<Profile>> {
    let page = q.page();
    let filter = ProfileFilter {
        user_id: q.id("user_id"),
        status: q.str("status"),
        settlement_status: q.str("settlement_status"),
        keyword: q.str("keyword"),
        created_from: q.time("created_from"),
        created_to: q.time("created_to"),
    };
    let rows = s
        .svc
        .reseller
        .management
        .list_profiles(&filter, page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, Clone::clone))
}

/// `GET /resellers/profiles/:id` — profile with domains, site, rules and finance summary.
async fn profile_detail(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    let r = &s.svc.reseller;
    let profile = r
        .management
        .profile(id)
        .await
        .map_err(fetch_failed)?
        .ok_or_else(|| ApiError::from(Error::not_found(keys::BAD_REQUEST)))?;
    let domains = r.management.domains_of(id).await.map_err(fetch_failed)?;
    let site = r.site_config.site_config(id).await.map_err(fetch_failed)?;
    let summary = r
        .product_settings
        .summarize(id)
        .await
        .map_err(fetch_failed)?;
    let scope = FinanceFilter {
        reseller_id: Some(id),
        ..FinanceFilter::default()
    };
    let balances = r
        .finance
        .admin_balances(scope.clone(), PageRequest::new(Some(1), Some(20)))
        .await
        .map_err(fetch_failed)?
        .items;
    let recent = PageRequest::new(Some(1), Some(10));
    let ledger = r
        .finance
        .admin_ledger(scope.clone(), recent)
        .await
        .map_err(fetch_failed)?
        .items;
    let withdraws = r
        .finance
        .admin_withdraws(scope, recent)
        .await
        .map_err(fetch_failed)?
        .items;
    let orders = r
        .orders
        .list_admin(id, &OrderFilter::default(), recent)
        .await
        .map_err(|e| {
            if e.is_not_found() {
                ApiError::from(Error::not_found(keys::BAD_REQUEST))
            } else {
                fetch_failed(e)
            }
        })?
        .items;
    let mut out = json!({
        "profile": profile,
        "domains": dto::domains(&domains),
        "product_summary": dto::summary(&summary),
        "finance_summary": {
            "balances": balances.iter().map(BalanceResp::from).collect::<Vec<_>>(),
            "recent_ledger_count": ledger.len(),
            "recent_withdraw_count": withdraws.len(),
        },
        "recent_orders": orders.iter().map(OrderResp::from).collect::<Vec<_>>(),
        "recent_ledger_entries": ledger.iter().map(LedgerResp::from).collect::<Vec<_>>(),
        "recent_withdraws": withdraws.iter().map(WithdrawResp::from).collect::<Vec<_>>(),
    });
    if let Some(c) = &site {
        out["site_config"] = dto::admin_site_config(id, Some(c), None);
    }
    ok(out)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct MarkupRequest {
    default_markup_percent: String,
    max_markup_percent: String,
    settlement_status: String,
    reason: String,
}

async fn approve(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
    Body(req): Body<MarkupRequest>,
) -> ApiResult<Data<Value>> {
    let (default, max) = (
        decimal(&req.default_markup_percent)?,
        decimal(&req.max_markup_percent)?,
    );
    let profile = s
        .svc
        .reseller
        .management
        .approve(audit.admin.id, id, default.decimal(), max.decimal())
        .await
        .map_err(admin_mgmt)?;
    audit
        .record(
            &s,
            "reseller_profile_approve",
            "/admin/resellers/profiles/:id/approve",
            json!({"profile_id": id, "reseller_id": id, "next_status": ProfileStatus::Active}),
        )
        .await;
    ok(json!({"profile": profile, "system_domain": null}))
}

async fn update_profile(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
    Body(req): Body<MarkupRequest>,
) -> ApiResult<Data<Profile>> {
    let input = ProfileUpdate {
        default_markup_percent: decimal(&req.default_markup_percent)?.decimal(),
        max_markup_percent: decimal(&req.max_markup_percent)?.decimal(),
        settlement_status: req.settlement_status.clone(),
    };
    let p = s
        .svc
        .reseller
        .management
        .update_operational(audit.admin.id, id, input)
        .await
        .map_err(admin_mgmt)?;
    audit
        .record(
            &s,
            "reseller_profile_update",
            "/admin/resellers/profiles/:id",
            json!({
                "profile_id": id,
                "reseller_id": id,
                "default_markup_percent": p.default_markup_percent,
                "max_markup_percent": p.max_markup_percent,
                "settlement_status": p.settlement_status,
                "reason": req.reason.trim(),
            }),
        )
        .await;
    ok(p)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SystemDomainRequest {
    subdomain: String,
    domain: String,
}

async fn system_domain(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
    Body(req): Body<SystemDomainRequest>,
) -> ApiResult<Data<DomainResp>> {
    let raw = if req.subdomain.trim().is_empty() {
        &req.domain
    } else {
        &req.subdomain
    };
    let d = s
        .svc
        .reseller
        .management
        .assign_system_domain(id, raw)
        .await
        .map_err(admin_mgmt)?;
    audit
        .record(
            &s,
            "reseller_profile_system_domain_update",
            "/admin/resellers/profiles/:id/system-domain",
            json!({"profile_id": id, "reseller_id": id, "domain_id": d.id, "domain": d.domain}),
        )
        .await;
    ok(DomainResp::from(&d))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ReasonRequest {
    reason: String,
}

async fn review(
    s: &AppState,
    audit: &Audit,
    id: Id,
    to: ProfileStatus,
    reason: &str,
    action: &str,
) -> ApiResult<Data<Profile>> {
    let p = s
        .svc
        .reseller
        .management
        .review(audit.admin.id, id, to, reason)
        .await
        .map_err(admin_mgmt)?;
    let object = format!(
        "/admin/resellers/profiles/:id/{}",
        action.trim_start_matches("reseller_profile_")
    );
    audit
        .record(
            s,
            action,
            &object,
            json!({"profile_id": id, "reseller_id": id, "next_status": p.status}),
        )
        .await;
    ok(p)
}

async fn reject(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
    Body(req): Body<ReasonRequest>,
) -> ApiResult<Data<Profile>> {
    review(
        &s,
        &audit,
        id,
        ProfileStatus::Rejected,
        &req.reason,
        "reseller_profile_reject",
    )
    .await
}

async fn disable(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
    Body(req): Body<ReasonRequest>,
) -> ApiResult<Data<Profile>> {
    review(
        &s,
        &audit,
        id,
        ProfileStatus::Disabled,
        &req.reason,
        "reseller_profile_disable",
    )
    .await
}

async fn restore(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
) -> ApiResult<Data<Profile>> {
    review(
        &s,
        &audit,
        id,
        ProfileStatus::Active,
        "",
        "reseller_profile_restore",
    )
    .await
}

async fn list_domains(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<ResellerDomain>> {
    let page = q.page();
    let filter = DomainFilter {
        reseller_id: q.id("reseller_id"),
        user_id: q.id("user_id"),
        domain: q.str("domain"),
        kind: q.str("type"),
        status: q.str("status"),
        verification_status: q.str("verification_status"),
        keyword: q.str("keyword"),
        created_from: q.time("created_from"),
        created_to: q.time("created_to"),
    };
    let rows = s
        .svc
        .reseller
        .management
        .list_domains(&filter, page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, Clone::clone))
}

async fn domain_action(
    s: &AppState,
    audit: &Audit,
    id: Id,
    action: DomainAction,
    name: &str,
    object: &str,
) -> ApiResult<Data<DomainResp>> {
    let d = s
        .svc
        .reseller
        .management
        .domain_action(id, action)
        .await
        .map_err(admin_mgmt)?;
    audit
        .record(
            s,
            name,
            object,
            json!({"domain_id": id, "reseller_id": d.reseller_id, "domain": d.domain, "next_status": d.status}),
        )
        .await;
    ok(DomainResp::from(&d))
}

async fn approve_domain(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
) -> ApiResult<Data<DomainResp>> {
    domain_action(
        &s,
        &audit,
        id,
        DomainAction::Approve,
        "reseller_domain_approve",
        "/admin/resellers/domains/:id/approve",
    )
    .await
}

async fn disable_domain(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
) -> ApiResult<Data<DomainResp>> {
    domain_action(
        &s,
        &audit,
        id,
        DomainAction::Disable,
        "reseller_domain_disable",
        "/admin/resellers/domains/:id/disable",
    )
    .await
}

async fn set_primary_domain(
    State(s): State<AppState>,
    audit: Audit,
    PathId(id): PathId,
) -> ApiResult<Data<DomainResp>> {
    domain_action(
        &s,
        &audit,
        id,
        DomainAction::SetPrimary,
        "reseller_domain_set_primary",
        "/admin/resellers/domains/:id/set-primary",
    )
    .await
}

async fn list_site_configs(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<Value>> {
    let page = q.page();
    let filter = SiteConfigFilter {
        reseller_id: q.id("reseller_id"),
        keyword: q.str("keyword"),
        created_from: q.time("created_from"),
        created_to: q.time("created_to"),
    };
    let rows = s
        .svc
        .reseller
        .site_config
        .list_admin(&filter, page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, |c| {
        dto::admin_site_config(c.reseller_id, Some(c), None)
    }))
}

fn reseller_param(raw: &str) -> Result<Id, ApiError> {
    raw.trim()
        .parse::<Id>()
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::invalid().into())
}

async fn get_site_config(
    State(s): State<AppState>,
    Path(rid): Path<String>,
) -> ApiResult<Data<Value>> {
    let rid = reseller_param(&rid)?;
    let (config, profile) = s
        .svc
        .reseller
        .site_config
        .admin_get(rid)
        .await
        .map_err(|e| {
            if e.is_not_found() {
                ApiError::from(Error::not_found(keys::BAD_REQUEST))
            } else {
                fetch_failed(e)
            }
        })?;
    ok(dto::admin_site_config(rid, config.as_ref(), Some(&profile)))
}

async fn put_site_config(
    State(s): State<AppState>,
    audit: Audit,
    Path(rid): Path<String>,
    Body(req): Body<SiteConfigInput>,
) -> ApiResult<Data<Value>> {
    let rid = reseller_param(&rid)?;
    let saved = s
        .svc
        .reseller
        .site_config
        .update_admin(rid, &req)
        .await
        .map_err(admin_mgmt)?;
    audit
        .record(
            &s,
            "reseller_site_config_update",
            "/admin/resellers/site-configs/:reseller_id",
            json!({
                "reseller_id": rid,
                "config_id": saved.id,
                "site_name": saved.site_name,
                "changed_fields": ["site_name", "logo", "favicon", "announcement", "support", "seo", "footer_links", "nav_config"],
                "source": "admin",
            }),
        )
        .await;
    ok(dto::admin_site_config(rid, Some(&saved), None))
}

async fn reset_site_config(
    State(s): State<AppState>,
    audit: Audit,
    Path(rid): Path<String>,
) -> ApiResult<Data<Value>> {
    let rid = reseller_param(&rid)?;
    s.svc
        .reseller
        .site_config
        .reset_admin(rid)
        .await
        .map_err(admin_mgmt)?;
    audit
        .record(
            &s,
            "reseller_site_config_reset",
            "/admin/resellers/site-configs/:reseller_id/reset",
            json!({"reseller_id": rid, "source": "admin"}),
        )
        .await;
    ok(json!({"ok": true}))
}

async fn list_settings(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<Value>> {
    let page = q.page();
    let filter = SettingFilter {
        reseller_id: q.id("reseller_id"),
        user_id: q.id("user_id"),
        product_id: q.id("product_id"),
        keyword: q.str("keyword"),
        pricing_mode: q.str("pricing_mode"),
        listed: q.str("listed"),
    };
    let rows = s
        .svc
        .reseller
        .product_settings
        .list_admin(&filter, page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, dto::admin_setting))
}

fn setting_params(raw: &(String, String)) -> Result<(Id, Id), ApiError> {
    Ok((reseller_param(&raw.0)?, reseller_param(&raw.1)?))
}

fn product_error(e: Error) -> ApiError {
    admin_products(e, "error.save_failed")
}

async fn get_setting(
    State(s): State<AppState>,
    Path(raw): Path<(String, String)>,
) -> ApiResult<Data<Value>> {
    let (rid, pid) = setting_params(&raw)?;
    let view = s
        .svc
        .reseller
        .product_settings
        .get(Owner::Reseller(rid), pid)
        .await
        .map_err(product_error)?;
    ok(dto::setting_detail(&view))
}

async fn preview_setting(
    State(s): State<AppState>,
    Path(raw): Path<(String, String)>,
    Body(req): Body<SettingsRequest>,
) -> ApiResult<Data<Value>> {
    let (rid, pid) = setting_params(&raw)?;
    let items = s
        .svc
        .reseller
        .product_settings
        .preview(Owner::Reseller(rid), pid, &req.inputs()?)
        .await
        .map_err(product_error)?;
    ok(dto::preview(&items))
}

async fn save_setting(
    State(s): State<AppState>,
    audit: Audit,
    Path(raw): Path<(String, String)>,
    Body(req): Body<SettingsRequest>,
) -> ApiResult<Data<Value>> {
    let (rid, pid) = setting_params(&raw)?;
    let view = s
        .svc
        .reseller
        .product_settings
        .save(Owner::Reseller(rid), pid, &req.inputs()?)
        .await
        .map_err(product_error)?;
    let sku_ids: Vec<Id> = req.settings.iter().map(|s| s.sku_id).collect();
    audit
        .record(
            &s,
            "reseller_product_setting_save",
            "/admin/resellers/product-settings/:reseller_id/:product_id",
            json!({"reseller_id": rid, "product_id": pid, "sku_ids": sku_ids, "settings_count": req.settings.len()}),
        )
        .await;
    ok(dto::setting_detail(&view))
}

async fn reset_setting(
    State(s): State<AppState>,
    audit: Audit,
    Path(raw): Path<(String, String)>,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let (rid, pid) = setting_params(&raw)?;
    let sku_id = q.strict_id("sku_id")?.unwrap_or(0);
    s.svc
        .reseller
        .product_settings
        .reset(Owner::Reseller(rid), pid, sku_id)
        .await
        .map_err(product_error)?;
    audit
        .record(
            &s,
            "reseller_product_setting_reset",
            "/admin/resellers/product-settings/:reseller_id/:product_id",
            json!({"reseller_id": rid, "product_id": pid, "sku_ids": [sku_id]}),
        )
        .await;
    ok(json!({"deleted": true}))
}

fn finance_filter(q: &Params) -> FinanceFilter {
    FinanceFilter {
        admin: true,
        reseller_id: q.id("reseller_id"),
        user_id: q.id("user_id"),
        keyword: q.str("keyword"),
        currency: String::new(),
        kind: q.str("type"),
        status: q.str("status"),
        order_id: q.id("order_id"),
        order_no: q.str("order_no"),
        created_from: q.time("created_from"),
        created_to: q.time("created_to"),
    }
}

async fn list_ledger(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Query<Params>,
) -> ApiResult<Paged<LedgerEntry>> {
    let page = q.page();
    let rows = s
        .svc
        .reseller
        .finance
        .admin_ledger(finance_filter(&q), page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, Clone::clone))
}

async fn list_balances(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Query<Params>,
) -> ApiResult<Paged<BalanceAccount>> {
    let page = q.page();
    let filter = FinanceFilter {
        kind: String::new(),
        order_id: None,
        order_no: String::new(),
        created_from: None,
        created_to: None,
        ..finance_filter(&q)
    };
    let rows = s
        .svc
        .reseller
        .finance
        .admin_balances(filter, page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, Clone::clone))
}

async fn list_withdraws(
    State(s): State<AppState>,
    _: ComplianceAcked,
    Query(q): Query<Params>,
) -> ApiResult<Paged<WithdrawRequest>> {
    let page = q.page();
    let filter = FinanceFilter {
        kind: String::new(),
        order_id: None,
        order_no: String::new(),
        ..finance_filter(&q)
    };
    let rows = s
        .svc
        .reseller
        .finance
        .admin_withdraws(filter, page)
        .await
        .map_err(fetch_failed)?;
    Ok(paged(page, rows.items, rows.total, Clone::clone))
}

/// `respondAdminWithdrawReviewError`: not found is 404 with `error.bad_request`.
fn withdraw_error(e: Error) -> ApiError {
    if e.is_not_found() {
        return Error::not_found(keys::BAD_REQUEST).into();
    }
    e.or_internal("error.save_failed").into()
}

async fn reject_withdraw(
    State(s): State<AppState>,
    _: ComplianceAcked,
    CurrentAdmin(admin): CurrentAdmin,
    PathId(id): PathId,
    Body(req): Body<ReasonRequest>,
) -> ApiResult<Data<WithdrawRequest>> {
    ok(s.svc
        .reseller
        .finance
        .review_withdraw(admin.id, id, WithdrawAction::Reject, &req.reason)
        .await
        .map_err(withdraw_error)?)
}

async fn pay_withdraw(
    State(s): State<AppState>,
    _: ComplianceAcked,
    CurrentAdmin(admin): CurrentAdmin,
    PathId(id): PathId,
) -> ApiResult<Data<WithdrawRequest>> {
    ok(s.svc
        .reseller
        .finance
        .review_withdraw(admin.id, id, WithdrawAction::Pay, "")
        .await
        .map_err(withdraw_error)?)
}
