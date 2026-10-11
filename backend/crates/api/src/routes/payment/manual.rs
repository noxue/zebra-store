//! Key-authenticated manual collection-code review and confirmation. The web
//! review page and external monitors call these same endpoints.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use serde::Deserialize;
use zs_app::identity::admin_auth::ClientInfo;
use zs_app::order::manual_payment::ManualPaymentReview;
use zs_domain::payment::errors::keys;
use zs_domain::{Error, Id};
use zs_shared::money::Amount;

use crate::client::Client;
use crate::extract::Body;
use crate::middleware::auth::bearer;
use crate::middleware::rate_limit::{MSG_RATE_LIMITED, check};
use crate::response::{ApiError, ApiResult, Data, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/manual-payments/{id}", review)
        .post("/manual-payments/{id}/confirm", confirm)
}

fn id(raw: &str) -> Result<Id, ApiError> {
    raw.parse::<Id>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| Error::bad_request(keys::PAYMENT_INVALID).into())
}

fn auth<'a>(
    s: &AppState,
    headers: &'a HeaderMap,
    client: &ClientInfo,
) -> Result<&'a str, ApiError> {
    check(
        &s.svc.payment.callback_limiter,
        &client.ip,
        MSG_RATE_LIMITED,
    )?;
    bearer(headers).map_err(Into::into)
}

async fn review(
    State(s): State<AppState>,
    Client(client): Client,
    Path(raw): Path<String>,
    headers: HeaderMap,
) -> ApiResult<Data<ManualPaymentReview>> {
    let key = auth(&s, &headers, &client)?;
    ok(s.svc
        .order
        .service
        .manual_payment_review(id(&raw)?, key)
        .await?)
}

#[derive(Debug, Deserialize)]
struct ConfirmBody {
    received_amount: Amount,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    note: String,
}

async fn confirm(
    State(s): State<AppState>,
    Client(client): Client,
    Path(raw): Path<String>,
    headers: HeaderMap,
    Body(body): Body<ConfirmBody>,
) -> ApiResult<Data<ManualPaymentReview>> {
    let key = auth(&s, &headers, &client)?;
    ok(s.svc
        .order
        .service
        .manual_payment_confirm(id(&raw)?, key, body.received_amount, body.force, &body.note)
        .await?)
}
