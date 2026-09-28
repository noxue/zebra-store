//! Compliance gate for payment/finance admin routes marked **[C]**.
//!
//! Until a super administrator acknowledges the compliance statement, gated
//! routes answer 403 with `compliance_required` (super admins) or
//! `compliance_required_by_super_admin` (everyone else).
//!
//! Use either the route layer
//! `axum::middleware::from_fn_with_state(state, compliance::require)` (after
//! admin authentication) or the [`ComplianceAcked`] extractor as a handler argument.

use axum::extract::{FromRef, FromRequestParts, Request, State};
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use zs_app::identity::admin_auth::AdminPrincipal;
use zs_app::identity::compliance::ComplianceService;

use crate::response::ApiError;
use crate::state::AppState;

fn gate(state: &AppState, principal: Option<&AdminPrincipal>) -> Result<(), ApiError> {
    if state.svc.identity.compliance.is_acknowledged() {
        return Ok(());
    }
    let is_super = principal.is_some_and(|p| p.is_super);
    Err(ComplianceService::gate_error(is_super).into())
}

/// Route-layer middleware (place it inside the admin JWT layer).
pub async fn require(State(state): State<AppState>, req: Request, next: Next) -> Response {
    match gate(&state, req.extensions().get::<AdminPrincipal>()) {
        Ok(()) => next.run(req).await,
        Err(e) => e.into_response(),
    }
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

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let app = AppState::from_ref(state);
        gate(&app, parts.extensions.get::<AdminPrincipal>()).map(|()| Self)
    }
}
