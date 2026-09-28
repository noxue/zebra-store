//! Reseller tenant middleware (`ResellerTenantMiddleware` + `RequireMainTenantForResellerConsole`).
//!
//! Storefront routes (`/public`, `/auth`, `/guest` and user routes) resolve the
//! request host to a [`Tenant`] stored in the request extensions:
//! - resolution error → HTTP 500 `{"code":"internal_error","message":"failed to resolve tenant"}`;
//! - unknown / inactive reseller host → HTTP 404 `{"code":"not_found","message":"site unavailable"}`.
//!
//! Handlers read it with `Extension<Tenant>` (absent on admin / callback routes and
//! when the feature is off → treat as the main shop). The content group's
//! `/public/config` handler should pass [`Tenant::to_content`] to the public config
//! service so the reseller overlay applies.

use axum::Json;
use axum::Router;
use axum::extract::{FromRequestParts, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::{Next, from_fn_with_state};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use zs_domain::{Error, Id};

use crate::middleware::auth::CurrentUser;
use crate::response::ApiError;
use crate::routes::Routes;
use crate::state::AppState;

pub use zs_domain::reseller::tenant::ResellerTenant as Tenant;

/// `X-Forwarded-Host` (honoured only with `reseller.trusted_forwarded_host`).
const FORWARDED_HOST: &str = "x-forwarded-host";

/// Resolves the tenant of a storefront request.
pub async fn resolve(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let headers = req.headers();
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| req.uri().authority().map(|a| a.as_str().to_owned()));
    let forwarded = headers.get(FORWARDED_HOST).and_then(|v| v.to_str().ok());
    let resolver = &state.svc.reseller.tenant;
    let raw = resolver.request_host(host.as_deref(), forwarded);
    match resolver.resolve(&raw).await {
        Ok(tenant) if tenant.unavailable => (
            StatusCode::NOT_FOUND,
            Json(json!({"code": "not_found", "message": "site unavailable"})),
        )
            .into_response(),
        Ok(tenant) => {
            req.extensions_mut().insert(tenant);
            next.run(req).await
        }
        Err(error) => {
            tracing::error!(%error, host = %raw, "tenant resolution failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"code": "internal_error", "message": "failed to resolve tenant"})),
            )
                .into_response()
        }
    }
}

/// Adds the tenant layer to a storefront router (no-op for an empty route table,
/// on which axum refuses route layers).
pub fn storefront(routes: Routes, state: &AppState) -> Router<AppState> {
    let empty = routes.permissions.is_empty();
    let router = routes.into_parts().0;
    if empty { router } else { layer(router, state) }
}

/// Adds the tenant layer to a non-empty router.
pub fn layer(router: Router<AppState>, state: &AppState) -> Router<AppState> {
    router.route_layer(from_fn_with_state(state.clone(), resolve))
}

/// Tenant of the request (main shop when the middleware did not run).
pub fn tenant_of(parts: &Parts) -> Tenant {
    parts
        .extensions
        .get::<Tenant>()
        .cloned()
        .unwrap_or_default()
}

/// The authenticated user of a reseller-console route. Rejects (status 403) when
/// the request comes from a reseller site: the console exists on the main shop only
/// (RSL-03), and the handler never runs.
#[derive(Debug, Clone, Copy)]
pub struct ConsoleUser(pub Id);

impl<S: Send + Sync> FromRequestParts<S> for ConsoleUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let CurrentUser(user) = CurrentUser::from_request_parts(parts, state).await?;
        if parts
            .extensions
            .get::<Tenant>()
            .is_some_and(Tenant::is_reseller)
        {
            return Err(Error::forbidden(zs_domain::error::keys::FORBIDDEN).into());
        }
        Ok(Self(user.id))
    }
}
