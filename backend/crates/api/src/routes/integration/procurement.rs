//! Purchase orders (`/api/v1/admin/procurement-orders*`).

use axum::extract::State;
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde_json::{Map, Value, json};
use zs_domain::Error;
use zs_domain::integration::hooks::FailureRefund;
use zs_domain::integration::keys;
use zs_domain::integration::procurement::{ProcurementFilter, ProcurementOrder, ProcurementStatus};
use zs_shared::money::Amount;
use zs_shared::page::Pagination;

use super::{Params, id_param, page, param, time_param};
use crate::extract::{PathId, Query};
use crate::response::{ApiError, ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/procurement-orders", list)
        .get("/procurement-orders/stats", stats)
        .get("/procurement-orders/{id}", get)
        .get(
            "/procurement-orders/{id}/upstream-payload/download",
            download,
        )
        .post("/procurement-orders/{id}/retry", retry)
        .post("/procurement-orders/{id}/retry-delivery", retry_delivery)
        .post("/procurement-orders/{id}/cancel", cancel)
}

/// Shared filter; times are RFC3339 and an invalid value is a 400 (UPS-22).
fn filter(q: &Params, with_status: bool) -> Result<ProcurementFilter, ApiError> {
    let status = if with_status {
        match param(q, "status") {
            "" => None,
            // Unknown statuses are rejected (400).
            raw => Some(ProcurementStatus::parse(raw).ok_or_else(Error::invalid)?),
        }
    } else {
        None
    };
    Ok(ProcurementFilter {
        page: page(q),
        connection_id: id_param(q, "connection_id"),
        status,
        local_order_no: param(q, "order_no").to_owned(),
        upstream_order_no: param(q, "upstream_order_no").to_owned(),
        created_from: time_param(q, "created_from")?,
        created_to: time_param(q, "created_to")?,
    })
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<ProcurementOrder>> {
    let f = filter(&q, true)?;
    let result = s.svc.integration.procurement.list(&f).await?;
    Ok(Paged(result.items, Pagination::new(f.page, result.total)))
}

/// Counts per status over the whole filtered set, ignoring `status` (UPS-22).
async fn stats(State(s): State<AppState>, Query(q): Query<Params>) -> ApiResult<Data<Value>> {
    let f = filter(&q, false)?;
    let (total, rows) = s.svc.integration.procurement.stats(&f).await?;
    let by_status: Map<String, Value> = rows.into_iter().map(|(k, v)| (k, v.into())).collect();
    ok(json!({"total": total, "by_status": by_status}))
}

async fn get(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<ProcurementOrder>> {
    ok(s.svc.integration.procurement.detail(id).await?)
}

/// Full supplier delivery as a text attachment.
async fn download(State(s): State<AppState>, PathId(id): PathId) -> Result<Response, ApiError> {
    let order = s
        .svc
        .integration
        .procurement
        .get(id)
        .await
        .map_err(|_| Error::not_found(keys::PROCUREMENT_NOT_FOUND))?;
    if order.upstream_payload.is_empty() {
        return Err(Error::not_found(keys::FULFILLMENT_NOT_FOUND).into());
    }
    let filename = format!("upstream-payload-{}.txt", order.id);
    Ok((
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8".to_owned()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        order.upstream_payload,
    )
        .into_response())
}

async fn retry(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.integration.procurement.retry(id).await?;
    ok(json!({"ok": true}))
}

async fn retry_delivery(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc
        .integration
        .procurement
        .retry_held_delivery(id)
        .await?;
    ok(json!({"retried": true, "purchase_resubmitted": false}))
}

/// LQA-I3: the response says how the buyer was refunded (`refund_type` `wallet` /
/// `manual` / `none`, `refund_amount`).
async fn cancel(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    let outcome = s.svc.integration.procurement.cancel(id).await?;
    let (kind, amount) = match outcome.refund {
        FailureRefund::Wallet(a) => ("wallet", a),
        FailureRefund::Manual(a) => ("manual", a),
        FailureRefund::None => ("none", Amount::ZERO),
    };
    ok(json!({"ok": true, "refund_type": kind, "refund_amount": amount}))
}
