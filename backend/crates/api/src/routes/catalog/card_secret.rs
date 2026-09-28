//! Card secret admin endpoints.

use axum::extract::{DefaultBodyLimit, Multipart, State};
use axum::handler::Handler;
use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::catalog::card_secret::{CreateSecretsInput, ExportFile};
use zs_domain::catalog::card_secret::{
    BatchSource, BatchSummary, CardSecret, IMPORT_TEMPLATE, SecretFilter, SecretStats, keys,
};
use zs_domain::{Error, Id};
use zs_shared::page::Pagination;

use super::{PageQuery, parse_bool_value, parse_query_id};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req};
use crate::middleware::auth::CurrentAdmin;
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Largest accepted import upload (CSV/TXT).
const MAX_IMPORT_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .post("/card-secrets/batch", create_batch)
        .post(
            "/card-secrets/import",
            import.layer(DefaultBodyLimit::max(MAX_IMPORT_BYTES)),
        )
        .get("/card-secrets", list)
        .put("/card-secrets/{id}", update)
        .patch("/card-secrets/batch-status", batch_status)
        .post("/card-secrets/batch-delete", batch_delete)
        .post("/card-secrets/export", export)
        .post("/card-secrets/export-available", export_available)
        .get("/card-secrets/stats", stats)
        .get("/card-secrets/batches", batches)
        .get("/card-secrets/template", template)
}

fn invalid() -> Error {
    Error::bad_request(keys::INVALID)
}

#[derive(Debug, Deserialize)]
struct CreateBatchRequest {
    #[serde(default)]
    product_id: Id,
    #[serde(default)]
    sku_id: Id,
    secrets: Option<Vec<String>>,
    #[serde(default)]
    batch_no: String,
    #[serde(default)]
    note: String,
    deduplicate: Option<bool>,
}

/// Original `CreateCardSecretBatchRequest`.
impl BindRules for CreateBatchRequest {
    const FIELDS: &'static [BindField] =
        &[req("product_id", "ProductID"), req("secrets", "Secrets")];
}

fn created_json(c: &zs_app::catalog::card_secret::CreatedSecrets) -> Value {
    json!({"created": c.created, "batch_id": c.batch_id, "batch_no": c.batch_no})
}

async fn create_batch(
    State(s): State<AppState>,
    CurrentAdmin(admin): CurrentAdmin,
    Bind(req): Bind<CreateBatchRequest>,
) -> ApiResult<Data<Value>> {
    let Some(secrets) = req.secrets else {
        return Err(Error::invalid().into());
    };
    if req.product_id <= 0 {
        return Err(Error::invalid().into());
    }
    let created = s
        .svc
        .catalog
        .card_secret
        .create_batch(
            CreateSecretsInput {
                product_id: req.product_id,
                sku_id: req.sku_id,
                secrets,
                batch_no: req.batch_no,
                note: req.note,
                admin_id: Some(admin.id),
                deduplicate: req.deduplicate,
            },
            BatchSource::Manual,
        )
        .await?;
    ok(created_json(&created))
}

/// `POST /admin/card-secrets/import` (multipart: `product_id`, `sku_id`, `file`, `batch_no`, `note`, `deduplicate`).
async fn import(
    State(s): State<AppState>,
    CurrentAdmin(admin): CurrentAdmin,
    mut form: Multipart,
) -> ApiResult<Data<Value>> {
    let mut fields = std::collections::HashMap::new();
    let mut file: Option<Vec<u8>> = None;
    while let Some(field) = form.next_field().await.map_err(|_| invalid())? {
        let name = field.name().unwrap_or_default().to_owned();
        if name == "file" {
            if file.is_none() {
                let bytes = field.bytes().await.map_err(|_| invalid())?;
                file = Some(bytes.to_vec());
            }
        } else {
            let text = field.text().await.map_err(|_| invalid())?;
            fields.entry(name).or_insert(text);
        }
    }
    let get = |k: &str| fields.get(k).map(String::as_str);
    let product_id = parse_query_id(get("product_id"), true).ok_or_else(invalid)?;
    let sku_id = parse_query_id(get("sku_id").or(Some("0")), false).ok_or_else(invalid)?;
    let file = file.ok_or_else(invalid)?;
    let deduplicate = parse_bool_value(get("deduplicate")).map_err(|()| invalid())?;
    let content = String::from_utf8_lossy(&file).into_owned();
    let created = s
        .svc
        .catalog
        .card_secret
        .import(
            CreateSecretsInput {
                product_id,
                sku_id,
                secrets: Vec::new(),
                batch_no: get("batch_no").unwrap_or_default().trim().to_owned(),
                note: get("note").unwrap_or_default().trim().to_owned(),
                admin_id: Some(admin.id),
                deduplicate,
            },
            &content,
        )
        .await?;
    ok(created_json(&created))
}

#[derive(Debug, Default, Deserialize)]
struct ListQuery {
    #[serde(flatten)]
    page: PageQuery,
    product_id: Option<String>,
    sku_id: Option<String>,
    batch_id: Option<String>,
    #[serde(default)]
    status: String,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    batch_no: String,
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> ApiResult<Paged<CardSecret>> {
    let filter = SecretFilter {
        product_id: parse_query_id(q.product_id.as_deref(), false).ok_or_else(invalid)?,
        sku_id: parse_query_id(q.sku_id.as_deref(), false).ok_or_else(invalid)?,
        batch_id: parse_query_id(q.batch_id.as_deref(), false).ok_or_else(invalid)?,
        status: q.status,
        secret: q.secret,
        batch_no: q.batch_no,
    };
    let req = q.page.request();
    let page = s.svc.catalog.card_secret.list(filter, req).await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

#[derive(Debug, Deserialize)]
struct UpdateRequest {
    secret: Option<String>,
    status: Option<String>,
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<UpdateRequest>,
) -> ApiResult<Data<CardSecret>> {
    let secret = req.secret.unwrap_or_default();
    let status = req.status.unwrap_or_default();
    if secret.trim().is_empty() && status.trim().is_empty() {
        return Err(Error::invalid().into());
    }
    ok(s.svc
        .catalog
        .card_secret
        .update(id, &secret, &status)
        .await?)
}

#[derive(Debug, Default, Deserialize)]
struct FilterRequest {
    #[serde(default)]
    product_id: Id,
    #[serde(default)]
    sku_id: Id,
    #[serde(default)]
    batch_id: Id,
    #[serde(default)]
    status: String,
    #[serde(default)]
    secret: String,
    #[serde(default)]
    batch_no: String,
}

fn to_filter(f: Option<FilterRequest>) -> SecretFilter {
    let f = f.unwrap_or_default();
    SecretFilter {
        product_id: f.product_id,
        sku_id: f.sku_id,
        batch_id: f.batch_id,
        status: f.status,
        secret: f.secret,
        batch_no: f.batch_no,
    }
    .normalized()
}

#[derive(Debug, Deserialize)]
struct BatchStatusRequest {
    #[serde(default)]
    ids: Vec<Id>,
    #[serde(default)]
    batch_id: Id,
    filter: Option<FilterRequest>,
    #[serde(default)]
    status: String,
}

impl BindRules for BatchStatusRequest {
    const FIELDS: &'static [BindField] = &[req("status", "Status")];
}

async fn batch_status(
    State(s): State<AppState>,
    Bind(req): Bind<BatchStatusRequest>,
) -> ApiResult<Data<Value>> {
    if req.status.is_empty() {
        return Err(Error::invalid().into());
    }
    let affected = s
        .svc
        .catalog
        .card_secret
        .batch_status(&req.ids, req.batch_id, to_filter(req.filter), &req.status)
        .await?;
    ok(json!({"affected": affected}))
}

#[derive(Debug, Deserialize)]
struct BatchDeleteRequest {
    #[serde(default)]
    ids: Vec<Id>,
    #[serde(default)]
    batch_id: Id,
    filter: Option<FilterRequest>,
}

async fn batch_delete(
    State(s): State<AppState>,
    Body(req): Body<BatchDeleteRequest>,
) -> ApiResult<Data<Value>> {
    let affected = s
        .svc
        .catalog
        .card_secret
        .batch_delete(&req.ids, req.batch_id, to_filter(req.filter))
        .await?;
    ok(json!({"affected": affected}))
}

/// Serves an export as an attachment.
fn download(file: ExportFile, prefix: &str, count_header: bool) -> Response {
    let filename = format!(
        "{prefix}-{}.{}",
        Utc::now().format("%Y%m%d-%H%M%S"),
        file.extension
    );
    let mut res = file.content.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(file.content_type),
    );
    if let Ok(v) = HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    if count_header && let Ok(v) = HeaderValue::from_str(&file.count.to_string()) {
        h.insert("X-Exported-Count", v);
    }
    res
}

#[derive(Debug, Deserialize)]
struct ExportRequest {
    #[serde(default)]
    ids: Vec<Id>,
    #[serde(default)]
    batch_id: Id,
    filter: Option<FilterRequest>,
    #[serde(default)]
    format: String,
}

impl BindRules for ExportRequest {
    const FIELDS: &'static [BindField] = &[req("format", "Format")];
}

async fn export(State(s): State<AppState>, Bind(req): Bind<ExportRequest>) -> ApiResult<Response> {
    if req.format.is_empty() {
        return Err(Error::invalid().into());
    }
    let file = s
        .svc
        .catalog
        .card_secret
        .export(&req.ids, req.batch_id, to_filter(req.filter), &req.format)
        .await?;
    Ok(download(file, "card-secrets", false))
}

#[derive(Debug, Deserialize)]
struct ExportAvailableRequest {
    #[serde(default)]
    product_id: Id,
    #[serde(default)]
    sku_id: Id,
    #[serde(default)]
    batch_id: Id,
    #[serde(default)]
    limit: i64,
    #[serde(default)]
    format: String,
    #[serde(default)]
    delete_after_export: bool,
}

impl BindRules for ExportAvailableRequest {
    const FIELDS: &'static [BindField] = &[
        req("product_id", "ProductID"),
        req("limit", "Limit"),
        req("format", "Format"),
    ];
}

async fn export_available(
    State(s): State<AppState>,
    Bind(req): Bind<ExportAvailableRequest>,
) -> ApiResult<Response> {
    if req.product_id <= 0 || req.limit == 0 || req.format.is_empty() {
        return Err(Error::invalid().into());
    }
    let file = s
        .svc
        .catalog
        .card_secret
        .export_available(
            req.product_id,
            req.sku_id,
            req.batch_id,
            req.limit,
            &req.format,
            req.delete_after_export,
        )
        .await?;
    Ok(download(file, "card-secrets-available", true))
}

#[derive(Debug, Default, Deserialize)]
struct StatsQuery {
    #[serde(flatten)]
    page: PageQuery,
    product_id: Option<String>,
    sku_id: Option<String>,
}

impl StatsQuery {
    fn ids(&self) -> Result<(Id, Id), Error> {
        let product_id = parse_query_id(self.product_id.as_deref(), true).ok_or_else(invalid)?;
        let sku_id = parse_query_id(self.sku_id.as_deref(), false).ok_or_else(invalid)?;
        Ok((product_id, sku_id))
    }
}

async fn stats(
    State(s): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Data<SecretStats>> {
    let (product_id, sku_id) = q.ids()?;
    ok(s.svc.catalog.card_secret.stats(product_id, sku_id).await?)
}

async fn batches(
    State(s): State<AppState>,
    Query(q): Query<StatsQuery>,
) -> ApiResult<Paged<BatchSummary>> {
    let (product_id, sku_id) = q.ids()?;
    let req = q.page.request();
    let page = s
        .svc
        .catalog
        .card_secret
        .batches(product_id, sku_id, req)
        .await?;
    Ok(Paged(page.items, Pagination::new(req, page.total)))
}

async fn template() -> Response {
    let mut res = IMPORT_TEMPLATE.into_response();
    let h = res.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    h.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"card-secrets-template.csv\""),
    );
    res
}
