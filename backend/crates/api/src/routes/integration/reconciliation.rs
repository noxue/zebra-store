//! Reconciliation (`/api/v1/admin/reconciliation/*`, compliance-gated **[C]**).

use axum::extract::State;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::integration::reconciliation::RunInput;
use zs_domain::Id;
use zs_domain::integration::reconciliation::{JobFilter, ReconciliationJob};
use zs_shared::page::Pagination;

use super::{Params, id_param, page, page_of, param};
use crate::extract::{Body, PathId, Query};
use crate::middleware::auth::CurrentAdmin;
use crate::middleware::compliance::ComplianceAcked;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .post("/reconciliation/run", run)
        .get("/reconciliation/jobs", list)
        .get("/reconciliation/jobs/{id}", get)
        .put("/reconciliation/items/{id}/resolve", resolve)
}

#[derive(Debug, Deserialize)]
struct RunBody {
    connection_id: Id,
    #[serde(rename = "type")]
    kind: String,
    time_range_start: DateTime<Utc>,
    time_range_end: DateTime<Utc>,
}

async fn run(
    _: ComplianceAcked,
    State(s): State<AppState>,
    Body(req): Body<RunBody>,
) -> ApiResult<Data<ReconciliationJob>> {
    let job = s
        .svc
        .integration
        .reconciliation
        .run(&RunInput {
            connection_id: req.connection_id,
            kind: req.kind,
            start: req.time_range_start,
            end: req.time_range_end,
        })
        .await?;
    ok(job)
}

async fn list(
    _: ComplianceAcked,
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<ReconciliationJob>> {
    let filter = JobFilter {
        page: page(&q),
        connection_id: id_param(&q, "connection_id"),
        status: param(&q, "status").to_owned(),
        kind: param(&q, "type").to_owned(),
    };
    let result = s.svc.integration.reconciliation.list(&filter).await?;
    Ok(Paged(
        result.items,
        Pagination::new(filter.page, result.total),
    ))
}

/// Job with a page of its items (`items_page`, `items_page_size`).
async fn get(
    _: ComplianceAcked,
    State(s): State<AppState>,
    PathId(id): PathId,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let items = page_of(
        &q,
        "items_page",
        "items_page_size",
        20,
        zs_shared::page::MAX_PAGE_SIZE,
    );
    let (job, page) = s.svc.integration.reconciliation.detail(id, items).await?;
    ok(json!({"job": job, "items": page.items, "items_total": page.total}))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ResolveBody {
    remark: String,
}

async fn resolve(
    _: ComplianceAcked,
    State(s): State<AppState>,
    CurrentAdmin(admin): CurrentAdmin,
    PathId(id): PathId,
    Body(req): Body<ResolveBody>,
) -> ApiResult<Data<Value>> {
    s.svc
        .integration
        .reconciliation
        .resolve(id, admin.id, &req.remark)
        .await?;
    ok(json!({"ok": true}))
}
