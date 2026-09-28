//! API credentials: user self-service (`/api/v1/api-credential*`) and admin review.

use axum::extract::State;
use axum::http::HeaderMap;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zs_domain::Error;
use zs_domain::integration::credential::{ApiCredential, CredentialFilter, CredentialStatus};
use zs_domain::integration::zs::SERVED_PROTOCOLS;

use super::{ActiveRequest, Params, id_param, page, param};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req};
use crate::middleware::auth::CurrentUser;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;
use zs_shared::page::Pagination;

/// `/api/v1/api-credential*` (user JWT).
pub(super) fn user() -> Routes {
    Routes::new("")
        .get("/api-credential", mine)
        .post("/api-credential/apply", apply)
        .post("/api-credential/regenerate", regenerate)
        .post("/api-credential/rotate", rotate)
        .post("/api-credential/connection-code", connection_code)
        .put("/api-credential/status", set_mine_active)
}

/// Starts a secret rotation (zebra-store spec §2): the new secret is shown once and
/// verifies next to the old one until first use or 7 days.
async fn rotate(State(s): State<AppState>, CurrentUser(u): CurrentUser) -> ApiResult<Data<Value>> {
    let (_, secret, expires_at) = s.svc.integration.credentials.rotate_mine(u.id).await?;
    ok(json!({"api_secret": secret, "rotation_expires_at": expires_at}))
}

/// Connection code (spec §3); generating one rotates the secret (shown once, inside
/// the code).
async fn connection_code(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    headers: HeaderMap,
) -> ApiResult<Data<Value>> {
    let (code, expires_at) = s
        .svc
        .integration
        .zs
        .connection_code(u.id, &super::zs::origin(&headers))
        .await?;
    ok(json!({"code": code, "rotation_expires_at": expires_at}))
}

/// `/api/v1/admin/api-credentials*`.
pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/api-credentials", list)
        .get("/api-credentials/{id}", get)
        .post("/api-credentials/{id}/approve", approve)
        .post("/api-credentials/{id}/reject", reject)
        .put("/api-credentials/{id}/status", set_active)
        .delete("/api-credentials/{id}", delete)
}

/// Owner view (original `GetMyApiCredential`): the key only once approved, the
/// secret never (only its last 4 characters).
async fn mine(State(s): State<AppState>, CurrentUser(u): CurrentUser) -> ApiResult<Data<Value>> {
    let protocols = json!(SERVED_PROTOCOLS);
    let Some(own) = s.svc.integration.credentials.mine(u.id).await? else {
        return ok(json!({
            "status": "none",
            "rotation_pending": false,
            "rotation_expires_at": null,
            "protocols": protocols,
        }));
    };
    let c = own.credential;
    let mut out = Map::new();
    out.insert(
        "rotation_pending".into(),
        own.rotation_expires_at.is_some().into(),
    );
    out.insert("rotation_expires_at".into(), json!(own.rotation_expires_at));
    out.insert("protocols".into(), protocols);
    out.insert("id".into(), c.id.into());
    out.insert("status".into(), c.status.as_str().into());
    out.insert("is_active".into(), c.is_active.into());
    out.insert("created_at".into(), json!(c.created_at));
    if c.status == CredentialStatus::Rejected {
        out.insert("reject_reason".into(), c.reject_reason.clone().into());
    }
    if c.status == CredentialStatus::Approved {
        out.insert("api_key".into(), c.api_key.clone().into());
        out.insert("approved_at".into(), json!(c.approved_at));
        out.insert("last_used_at".into(), json!(c.last_used_at));
        if !own.secret_tail.is_empty() {
            out.insert("api_secret_tail".into(), own.secret_tail.into());
        }
    }
    ok(Value::Object(out))
}

async fn apply(State(s): State<AppState>, CurrentUser(u): CurrentUser) -> ApiResult<Data<Value>> {
    let c = s.svc.integration.credentials.apply(u.id).await?;
    ok(json!({"id": c.id, "status": c.status.as_str()}))
}

/// New secret, shown once (UPS-10).
async fn regenerate(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
) -> ApiResult<Data<Value>> {
    let secret = s.svc.integration.credentials.regenerate_mine(u.id).await?;
    ok(json!({"api_secret": secret}))
}

async fn set_mine_active(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    Body(req): Body<ActiveRequest>,
) -> ApiResult<Data<Value>> {
    s.svc
        .integration
        .credentials
        .set_active_mine(u.id, req.is_active)
        .await?;
    ok(json!({"updated": true}))
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<ApiCredential>> {
    let filter = CredentialFilter {
        page: page(&q),
        status: param(&q, "status").to_owned(),
        user_id: id_param(&q, "user_id"),
        search: param(&q, "search").to_owned(),
    };
    let result = s.svc.integration.credentials.list(&filter).await?;
    Ok(Paged(
        result.items,
        Pagination::new(filter.page, result.total),
    ))
}

async fn get(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<ApiCredential>> {
    ok(s.svc.integration.credentials.get(id).await?)
}

/// Approval answers no key or secret (UPS-10).
async fn approve(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    let c = s.svc.integration.credentials.approve(id).await?;
    ok(json!({"credential": c, "approved": true}))
}

#[derive(Debug, Deserialize)]
struct RejectRequest {
    #[serde(default)]
    reason: String,
}

impl BindRules for RejectRequest {
    const FIELDS: &'static [BindField] = &[req("reason", "Reason")];
}

async fn reject(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<RejectRequest>,
) -> ApiResult<Data<Value>> {
    if req.reason.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    s.svc
        .integration
        .credentials
        .reject(id, &req.reason)
        .await?;
    ok(json!({"rejected": true}))
}

async fn set_active(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<ActiveRequest>,
) -> ApiResult<Data<Value>> {
    s.svc
        .integration
        .credentials
        .set_active(id, req.is_active)
        .await?;
    ok(json!({"updated": true}))
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.integration.credentials.delete(id).await?;
    ok(json!({"deleted": true}))
}
