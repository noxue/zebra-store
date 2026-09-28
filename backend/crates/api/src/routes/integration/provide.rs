//! Provider-compat protocols (other shop systems buying from us through their own wire
//! format; docs/protocol/third-party/provider-compat.md):
//!
//! - acg-faka 共享店铺 `POST /shared/authentication/connect`, `POST /shared/commodity/*`
//!   (switch `integration.acg_faka_compat`);
//! - mcy OpenApi plugin `POST /plugin/open-api/*` (switch `integration.mcy_compat`);
//! - the owner's compat key: `GET|PUT /api/v1/api-credential/compat`,
//!   `POST /api/v1/api-credential/compat/issue` (user JWT).
//!
//! Handlers only read the raw request and hand it to the facade; a disabled protocol
//! answers 404 like a site without it.

use axum::Json;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::integration::provide::access::CompatView;
use zs_app::integration::provide::{acg_faka, mcy};
use zs_domain::integration::provide::MAX_COMPAT_BODY_BYTES;

use super::upstream::{client_ip, header};
use super::zs::origin;
use crate::extract::Body;
use crate::middleware::auth::CurrentUser;
use crate::response::{ApiResult, Data, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// acg-faka paths (relative to the site root).
const ACG_ROUTES: [(&str, acg_faka::Endpoint); 11] = [
    (
        "/shared/authentication/connect",
        acg_faka::Endpoint::Connect,
    ),
    ("/shared/commodity/items", acg_faka::Endpoint::Items),
    ("/shared/commodity/item", acg_faka::Endpoint::Item),
    ("/shared/commodity/inventory", acg_faka::Endpoint::Inventory),
    (
        "/shared/commodity/inventoryState",
        acg_faka::Endpoint::InventoryState,
    ),
    ("/shared/commodity/stock", acg_faka::Endpoint::Stock),
    ("/shared/commodity/valuation", acg_faka::Endpoint::Valuation),
    ("/shared/commodity/trade", acg_faka::Endpoint::Trade),
    ("/shared/commodity/query", acg_faka::Endpoint::Query),
    ("/shared/commodity/draftCard", acg_faka::Endpoint::DraftCard),
    ("/shared/commodity/draft", acg_faka::Endpoint::Draft),
];

/// mcy OpenApi paths (relative to the site root).
const MCY_ROUTES: [(&str, mcy::Endpoint); 7] = [
    ("/plugin/open-api/connect", mcy::Endpoint::Connect),
    ("/plugin/open-api/items", mcy::Endpoint::Items),
    ("/plugin/open-api/item", mcy::Endpoint::Item),
    ("/plugin/open-api/sku/stock", mcy::Endpoint::SkuStock),
    ("/plugin/open-api/sku/state", mcy::Endpoint::SkuState),
    ("/plugin/open-api/amount", mcy::Endpoint::Amount),
    ("/plugin/open-api/trade", mcy::Endpoint::Trade),
];

/// mcy request headers.
const HEADER_API_ID: &str = "Api-Id";
const HEADER_API_SIGNATURE: &str = "Api-Signature";

/// Routes mounted at the site root.
pub(super) fn root() -> Routes {
    let mut routes = Routes::new("");
    for (path, endpoint) in ACG_ROUTES {
        routes = routes.post(path, move |State(s): State<AppState>, req: Request| {
            acg(s, req, endpoint)
        });
    }
    for (path, endpoint) in MCY_ROUTES {
        routes = routes.post(path, move |State(s): State<AppState>, req: Request| {
            mcy_open_api(s, req, endpoint)
        });
    }
    routes
}

/// The owner's compat key routes under `/api/v1` (user JWT).
pub(super) fn user() -> Routes {
    Routes::new("")
        .get("/api-credential/compat", mine)
        .post("/api-credential/compat/issue", issue)
        .put("/api-credential/compat", update)
}

fn disabled() -> Response {
    StatusCode::NOT_FOUND.into_response()
}

async fn acg(s: AppState, req: Request, endpoint: acg_faka::Endpoint) -> Response {
    if !s.cfg.integration.acg_faka_compat {
        return disabled();
    }
    let ip = client_ip(&req, &s);
    let origin = origin(req.headers());
    let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_COMPAT_BODY_BYTES).await else {
        return Json(json!({"code": 0, "msg": "请求体过大"})).into_response();
    };
    let out = s
        .svc
        .provide
        .acg
        .serve(
            endpoint,
            acg_faka::Request {
                body: &body,
                client_ip: &ip,
                origin: &origin,
            },
        )
        .await;
    Json(out).into_response()
}

async fn mcy_open_api(s: AppState, req: Request, endpoint: mcy::Endpoint) -> Response {
    if !s.cfg.integration.mcy_compat {
        return disabled();
    }
    let ip = client_ip(&req, &s);
    let headers = req.headers().clone();
    let origin = origin(&headers);
    let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_COMPAT_BODY_BYTES).await else {
        return Json(json!({"code": 0, "msg": "请求体过大"})).into_response();
    };
    let api_id = header(&headers, HEADER_API_ID);
    let signature = header(&headers, HEADER_API_SIGNATURE);
    let out = s
        .svc
        .provide
        .mcy
        .serve(
            endpoint,
            mcy::Request {
                api_id: &api_id,
                signature: &signature,
                body: &body,
                client_ip: &ip,
                origin: &origin,
            },
        )
        .await;
    Json(out).into_response()
}

// ---------------------------------------------------------------------------
// Owner's compat key
// ---------------------------------------------------------------------------

async fn view_json(s: &AppState, v: CompatView, headers: &HeaderMap) -> Value {
    let base = s.svc.provide.desk.base_url(&origin(headers)).await;
    let cfg = &s.cfg.integration;
    json!({
        "app_id": v.app_id.to_string(),
        "app_key": v.app_key,
        "is_active": v.is_active,
        "ip_allowlist": v.ip_allowlist,
        "last_used_at": v.last_used_at,
        "site_url": base,
        "protocols": [
            {"id": "acg-faka", "enabled": cfg.acg_faka_compat, "path": "/shared/"},
            {"id": "mcy-open-api", "enabled": cfg.mcy_compat, "path": "/plugin/open-api/"},
        ],
    })
}

/// `GET /api-credential/compat`: `app_key` is empty until issued.
async fn mine(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    headers: HeaderMap,
) -> ApiResult<Data<Value>> {
    let v = s.svc.provide.access.mine(u.id).await?;
    ok(view_json(&s, v, &headers).await)
}

/// `POST /api-credential/compat/issue`: new `app_key` (the old one stops working).
async fn issue(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    headers: HeaderMap,
) -> ApiResult<Data<Value>> {
    let v = s.svc.provide.access.issue(u.id).await?;
    ok(view_json(&s, v, &headers).await)
}

#[derive(Debug, Deserialize)]
struct UpdateRequest {
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    ip_allowlist: Option<String>,
}

/// `PUT /api-credential/compat`: `{is_active?, ip_allowlist?}`.
async fn update(
    State(s): State<AppState>,
    CurrentUser(u): CurrentUser,
    headers: HeaderMap,
    Body(req): Body<UpdateRequest>,
) -> ApiResult<Data<Value>> {
    let v = s
        .svc
        .provide
        .access
        .update(u.id, req.is_active, req.ip_allowlist.as_deref())
        .await?;
    ok(view_json(&s, v, &headers).await)
}
