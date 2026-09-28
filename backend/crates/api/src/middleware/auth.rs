//! Admin authentication + RBAC middleware.

use axum::extract::{FromRequestParts, MatchedPath, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use zs_app::identity::admin_auth::AdminPrincipal;
use zs_app::identity::user_auth::UserPrincipal;
use zs_domain::Error;

use crate::response::ApiError;
use crate::routes::to_colon;
use crate::state::AppState;

/// Extracts the bearer token, mapping header problems to the original error keys.
pub fn bearer(parts_headers: &axum::http::HeaderMap) -> Result<&str, Error> {
    let header = parts_headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty())
        .ok_or_else(|| Error::unauthorized("error.auth_header_missing"))?;
    match header.split_once(' ') {
        Some(("Bearer", token)) if !token.is_empty() => Ok(token),
        _ => Err(Error::unauthorized("error.auth_header_invalid")),
    }
}

/// Requires a valid admin access token; stores [`AdminPrincipal`] in extensions.
pub async fn require_admin(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Response {
    let principal = match bearer(req.headers()) {
        Ok(token) => state.svc.identity.admin_auth.authenticate(token).await,
        Err(e) => Err(e),
    };
    match principal {
        Ok(p) => {
            req.extensions_mut().insert(p);
            next.run(req).await
        }
        Err(e) => ApiError::from(e).into_response(),
    }
}

/// Casbin check of the matched route pattern (super admins bypass).
pub async fn rbac(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let Some(principal) = req.extensions().get::<AdminPrincipal>().cloned() else {
        return ApiError::from(Error::unauthorized("error.unauthorized")).into_response();
    };
    if principal.is_super {
        return next.run(req).await;
    }
    let object = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| to_colon(m.as_str()))
        .unwrap_or_else(|| req.uri().path().to_owned());
    if zs_domain::authz::is_self_service(req.method().as_str(), &object) {
        return next.run(req).await;
    }
    match state
        .svc
        .identity
        .authz
        .enforce_admin(principal.id, &object, req.method().as_str())
    {
        Ok(true) => next.run(req).await,
        Ok(false) => {
            tracing::warn!(admin_id = principal.id, %object, method = %req.method(), "rbac permission denied");
            ApiError::from(Error::forbidden("error.forbidden")).into_response()
        }
        Err(e) => ApiError::from(e.or_internal("error.unauthorized")).into_response(),
    }
}

/// Extractor for the authenticated administrator.
#[derive(Debug, Clone)]
pub struct CurrentAdmin(pub AdminPrincipal);

impl<S: Send + Sync> FromRequestParts<S> for CurrentAdmin {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AdminPrincipal>()
            .cloned()
            .map(Self)
            .ok_or_else(|| Error::unauthorized("error.unauthorized").into())
    }
}

/// Requires a valid user access token; stores [`UserPrincipal`] in extensions.
pub async fn require_user(State(state): State<AppState>, mut req: Request, next: Next) -> Response {
    let principal = match bearer(req.headers()) {
        Ok(token) => state.svc.identity.user_auth.authenticate(token).await,
        Err(e) => Err(e),
    };
    match principal {
        Ok(p) => {
            req.extensions_mut().insert(p);
            next.run(req).await
        }
        Err(e) => ApiError::from(e).into_response(),
    }
}

/// Extractor for the authenticated user (routes behind [`require_user`]).
#[derive(Debug, Clone)]
pub struct CurrentUser(pub UserPrincipal);

impl<S: Send + Sync> FromRequestParts<S> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<UserPrincipal>()
            .cloned()
            .map(Self)
            .ok_or_else(|| Error::unauthorized("error.unauthorized").into())
    }
}

/// Optional user for routes open to guests: a valid bearer token yields `Some`,
/// no/invalid token yields `None`.
#[derive(Debug, Clone)]
pub struct MaybeUser(pub Option<UserPrincipal>);

impl<S> FromRequestParts<S> for MaybeUser
where
    S: Send + Sync,
    AppState: axum::extract::FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = <AppState as axum::extract::FromRef<S>>::from_ref(state);
        let Ok(token) = bearer(&parts.headers) else {
            return Ok(Self(None));
        };
        Ok(Self(
            app.svc.identity.user_auth.authenticate(token).await.ok(),
        ))
    }
}
