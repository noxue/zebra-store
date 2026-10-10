//! Optional, proxy-neutral certificate domain reads.

use axum::Json;
use axum::extract::{Query, State, rejection::QueryRejection};
use axum::http::{HeaderMap, StatusCode, header};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_domain::identity::oauth::constant_time_eq;

use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/internal/reseller-domains/allow", allow)
        .get("/internal/reseller-domains", list)
}

#[derive(Debug, Deserialize)]
struct DomainQuery {
    domain: String,
}

/// Caddy's ask endpoint: 2xx only for a live reseller domain.
async fn allow(
    State(state): State<AppState>,
    query: Result<Query<DomainQuery>, QueryRejection>,
) -> StatusCode {
    if !state.cfg.reseller.enabled {
        return StatusCode::NOT_FOUND;
    }
    let Ok(Query(query)) = query else {
        return StatusCode::BAD_REQUEST;
    };
    match state
        .svc
        .reseller
        .tenant
        .allows_certificate(&query.domain)
        .await
    {
        Ok(true) => StatusCode::NO_CONTENT,
        Ok(false) => StatusCode::FORBIDDEN,
        Err(error) => {
            tracing::error!(%error, "reseller certificate eligibility failed");
            StatusCode::SERVICE_UNAVAILABLE
        }
    }
}

/// Authenticated inventory for an external certificate/deployment controller.
async fn list(State(state): State<AppState>, headers: HeaderMap) -> (StatusCode, Json<Value>) {
    let token = state.cfg.reseller.domain_sync_token.as_bytes();
    if !state.cfg.reseller.enabled || token.is_empty() {
        return rejected(StatusCode::NOT_FOUND, "not found");
    }
    let supplied = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if !constant_time_eq(token, supplied.as_bytes()) {
        return rejected(StatusCode::UNAUTHORIZED, "unauthorized");
    }
    match state.svc.reseller.tenant.certificate_domains().await {
        Ok(domains) => (
            StatusCode::OK,
            Json(json!({"status_code": 0, "msg": "success", "data": {"domains": domains}})),
        ),
        Err(error) => {
            tracing::error!(%error, "reseller certificate inventory failed");
            rejected(StatusCode::SERVICE_UNAVAILABLE, "service unavailable")
        }
    }
}

fn rejected(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (
        status,
        Json(json!({"status_code": status.as_u16(), "msg": msg, "data": null})),
    )
}
