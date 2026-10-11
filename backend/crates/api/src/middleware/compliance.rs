//! Compatibility middleware for payment and finance routes.
//! Authentication and RBAC still apply; the retired acknowledgement no longer gates requests.

use axum::extract::{FromRef, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;

use crate::response::ApiError;
use crate::state::AppState;

/// Route-layer middleware (place it inside the admin JWT layer).
pub async fn require(State(_state): State<AppState>, req: Request, next: Next) -> Response {
    next.run(req).await
}

/// Extractor form of [`require`]: add `_: ComplianceAcked` to a handler.
#[derive(Debug, Clone, Copy)]
pub struct ComplianceAcked;

impl<S> FromRequestParts<S> for ComplianceAcked
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ApiError;

    async fn from_request_parts(_parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self)
    }
}
