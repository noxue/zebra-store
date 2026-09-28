//! Gift card admin endpoints.

use axum::extract::State;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_app::marketing::gift_card::GiftCardRow;
use zs_domain::marketing::gift_card::{GiftCard, GiftCardFilter, GiftCardUpdate, RedeemedUser};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::Pagination;

use super::{PageQuery, parse_query_id, parse_time};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req};
use crate::middleware::auth::CurrentAdmin;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .post("/gift-cards/generate", generate)
        .get("/gift-cards", list)
        .put("/gift-cards/{id}", update)
        .delete("/gift-cards/{id}", delete)
        .patch("/gift-cards/batch-status", batch_status)
        .post("/gift-cards/export", export)
}

#[derive(Debug, Deserialize)]
struct GenerateRequest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    quantity: i32,
    amount: Option<Amount>,
    #[serde(default)]
    expires_at: String,
}

/// Original `generateRequest`.
impl BindRules for GenerateRequest {
    const FIELDS: &'static [BindField] = &[
        req("name", "Name"),
        req("quantity", "Quantity"),
        req("amount", "Amount"),
    ];
}

async fn generate(
    State(s): State<AppState>,
    CurrentAdmin(admin): CurrentAdmin,
    Bind(req): Bind<GenerateRequest>,
) -> ApiResult<Data<Value>> {
    let Some(amount) = req.amount else {
        return Err(Error::invalid().into());
    };
    if req.name.is_empty() || req.quantity == 0 {
        return Err(Error::invalid().into());
    }
    let expires_at = parse_time(req.expires_at.trim())?;
    let (batch, created) = s
        .svc
        .marketing
        .gift_card
        .generate(&req.name, req.quantity, amount, expires_at, Some(admin.id))
        .await?;
    ok(json!({"batch": batch, "created": created}))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(flatten)]
    page: PageQuery,
    #[serde(default)]
    status: String,
    #[serde(default)]
    code: String,
    #[serde(default)]
    batch_no: String,
    redeemed_user_id: Option<String>,
    #[serde(default)]
    created_from: String,
    #[serde(default)]
    created_to: String,
    #[serde(default)]
    redeemed_from: String,
    #[serde(default)]
    redeemed_to: String,
    #[serde(default)]
    expires_from: String,
    #[serde(default)]
    expires_to: String,
}

/// List row: the card plus `is_expired` and the redeemer.
#[derive(Debug, Serialize)]
struct CardItem {
    #[serde(flatten)]
    card: GiftCard,
    is_expired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    redeemed_user: Option<RedeemedUser>,
}

impl From<GiftCardRow> for CardItem {
    fn from(r: GiftCardRow) -> Self {
        Self {
            card: r.card,
            is_expired: r.is_expired,
            redeemed_user: r.redeemed_user,
        }
    }
}

async fn list(State(s): State<AppState>, Query(q): Query<ListQuery>) -> ApiResult<Paged<CardItem>> {
    let filter = GiftCardFilter {
        code: q.code,
        status: q.status,
        batch_no: q.batch_no,
        redeemed_user_id: parse_query_id(q.redeemed_user_id.as_deref(), true)?,
        created_from: parse_time(q.created_from.trim())?,
        created_to: parse_time(q.created_to.trim())?,
        redeemed_from: parse_time(q.redeemed_from.trim())?,
        redeemed_to: parse_time(q.redeemed_to.trim())?,
        expires_from: parse_time(q.expires_from.trim())?,
        expires_to: parse_time(q.expires_to.trim())?,
    };
    let req = q.page.request();
    let page = s.svc.marketing.gift_card.list(filter, req).await?;
    Ok(Paged(
        page.items.into_iter().map(Into::into).collect(),
        Pagination::new(req, page.total),
    ))
}

#[derive(Debug, Deserialize)]
struct UpdateRequest {
    name: Option<String>,
    status: Option<String>,
    expires_at: Option<String>,
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<UpdateRequest>,
) -> ApiResult<Data<GiftCard>> {
    let mut update = GiftCardUpdate {
        name: req.name,
        status: req.status,
        ..GiftCardUpdate::default()
    };
    if let Some(raw) = req.expires_at {
        let raw = raw.trim();
        if raw.is_empty() {
            update.clear_expires_at = true;
        } else {
            update.expires_at = parse_time(raw)?;
        }
    }
    ok(s.svc.marketing.gift_card.update(id, update).await?)
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.marketing.gift_card.delete(id).await?;
    ok(json!({"deleted": true}))
}

#[derive(Debug, Deserialize)]
struct BatchStatusRequest {
    ids: Option<Vec<Id>>,
    #[serde(default)]
    status: String,
}

impl BindRules for BatchStatusRequest {
    const FIELDS: &'static [BindField] = &[req("ids", "IDs"), req("status", "Status")];
}

async fn batch_status(
    State(s): State<AppState>,
    Bind(req): Bind<BatchStatusRequest>,
) -> ApiResult<Data<Value>> {
    let Some(ids) = req.ids else {
        return Err(Error::invalid().into());
    };
    if req.status.is_empty() {
        return Err(Error::invalid().into());
    }
    let affected = s
        .svc
        .marketing
        .gift_card
        .batch_status(&ids, &req.status)
        .await?;
    ok(json!({"affected": affected}))
}

#[derive(Debug, Deserialize)]
struct ExportRequest {
    ids: Option<Vec<Id>>,
    #[serde(default)]
    format: String,
}

impl BindRules for ExportRequest {
    const FIELDS: &'static [BindField] = &[req("ids", "IDs"), req("format", "Format")];
}

async fn export(State(s): State<AppState>, Bind(req): Bind<ExportRequest>) -> ApiResult<Response> {
    let Some(ids) = req.ids else {
        return Err(Error::invalid().into());
    };
    if req.format.is_empty() {
        return Err(Error::invalid().into());
    }
    let (content, content_type, ext) = s.svc.marketing.gift_card.export(&ids, &req.format).await?;
    let filename = format!("gift_cards_{}.{ext}", Utc::now().format("%Y%m%d_%H%M%S"));
    let mut res = content.into_response();
    let h = res.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    Ok(res)
}
