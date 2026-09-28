//! Product mappings and supplier catalog browsing (`/api/v1/admin/product-mappings*`,
//! `/upstream-products`, `/upstream-categories`).

use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_app::integration::mapping::{CategoryImportResult, ImportRequest};
use zs_domain::integration::keys;
use zs_domain::integration::mapping::{MappingFilter, ProductMapping, UpstreamStatus};
use zs_domain::{Error, Id};
use zs_shared::page::Pagination;

use super::{ActiveRequest, IdsRequest, Params, id_param, page, page_of, param};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req, req_min1};
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

/// Default / max page size of the supplier product browser (original 50).
const UPSTREAM_PAGE_SIZE: u64 = 50;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/product-mappings", list)
        .get("/product-mappings/{id}", get)
        .post("/product-mappings/import", import)
        .post("/product-mappings/batch-import", batch_import)
        .post("/product-mappings/{id}/sync", sync)
        .put("/product-mappings/{id}/status", set_status)
        .delete("/product-mappings/{id}", delete)
        .post("/product-mappings/batch-sync", batch_sync)
        .post("/product-mappings/batch-status", batch_status)
        .post("/product-mappings/batch-delete", batch_delete)
        .get("/upstream-products", upstream_products)
        .get("/upstream-categories", upstream_categories)
        .post(
            "/product-mappings/batch-import-by-category",
            import_by_category,
        )
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<ProductMapping>> {
    let upstream_status = match param(&q, "upstream_status") {
        "" => None,
        raw => Some(
            UpstreamStatus::parse(raw)
                .ok_or_else(|| Error::bad_request(keys::INVALID_UPSTREAM_STATUS))?,
        ),
    };
    let active = match param(&q, "product_status") {
        "" => None,
        "active" => Some(true),
        "inactive" => Some(false),
        _ => return Err(Error::bad_request(keys::INVALID_PRODUCT_STATUS).into()),
    };
    let filter = MappingFilter {
        page: page(&q),
        connection_id: id_param(&q, "connection_id"),
        upstream_status,
        active,
        search: param(&q, "search").to_owned(),
    };
    let result = s.svc.integration.mappings.list(&filter).await?;
    Ok(Paged(
        result.items,
        Pagination::new(filter.page, result.total),
    ))
}

async fn get(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    let (mapping, skus) = s.svc.integration.mappings.detail(id).await?;
    ok(json!({"mapping": mapping, "sku_mappings": skus}))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ImportBody {
    connection_id: Id,
    upstream_product_id: Id,
    category_id: Id,
    slug: String,
    auto_create_category: bool,
}

impl BindRules for ImportBody {
    const FIELDS: &'static [BindField] = &[
        req("connection_id", "ConnectionID"),
        req("upstream_product_id", "UpstreamProductID"),
    ];
}

async fn import(
    State(s): State<AppState>,
    Bind(req): Bind<ImportBody>,
) -> ApiResult<Data<ProductMapping>> {
    if req.connection_id <= 0 || req.upstream_product_id <= 0 {
        return Err(Error::invalid().into());
    }
    let m = s
        .svc
        .integration
        .mappings
        .import(&ImportRequest {
            connection_id: req.connection_id,
            upstream_product_id: req.upstream_product_id,
            category_id: req.category_id,
            slug: req.slug,
            auto_create_category: req.auto_create_category,
        })
        .await?;
    ok(m)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BatchImportBody {
    connection_id: Id,
    upstream_product_ids: Vec<Id>,
    category_id: Id,
    auto_create_category: bool,
}

impl BindRules for BatchImportBody {
    const FIELDS: &'static [BindField] = &[
        req("connection_id", "ConnectionID"),
        req_min1("upstream_product_ids", "UpstreamProductIDs"),
    ];
}

async fn batch_import(
    State(s): State<AppState>,
    Bind(req): Bind<BatchImportBody>,
) -> ApiResult<Data<Value>> {
    if req.connection_id <= 0 || req.upstream_product_ids.is_empty() {
        return Err(Error::invalid().into());
    }
    let outcomes = s
        .svc
        .integration
        .mappings
        .batch_import(
            req.connection_id,
            &req.upstream_product_ids,
            req.category_id,
            req.auto_create_category,
        )
        .await
        .map_err(|e| e.or_internal(keys::MAPPING_IMPORT_FAILED))?;
    let mut success = 0;
    let results: Vec<Value> = outcomes
        .into_iter()
        .map(|(id, r)| match r {
            Ok(_) => {
                success += 1;
                json!({"upstream_product_id": id, "success": true})
            }
            Err(e) => json!({"upstream_product_id": id, "success": false, "error": e.to_string()}),
        })
        .collect();
    ok(json!({
        "results": results,
        "total": req.upstream_product_ids.len(),
        "success_count": success,
    }))
}

async fn sync(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.integration.mappings.sync(id).await?;
    ok(json!({"synced": true}))
}

async fn set_status(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<ActiveRequest>,
) -> ApiResult<Data<Value>> {
    s.svc
        .integration
        .mappings
        .set_active(id, req.is_active)
        .await?;
    ok(json!({"updated": true}))
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.integration.mappings.delete(id).await?;
    ok(json!({"deleted": true}))
}

fn batch_result(total: usize, success: usize) -> Value {
    json!({"total": total, "success_count": success})
}

async fn batch_sync(
    State(s): State<AppState>,
    Bind(req): Bind<IdsRequest>,
) -> ApiResult<Data<Value>> {
    let ids = req.ids()?;
    let mut success = 0;
    for id in ids {
        if s.svc.integration.mappings.sync(*id).await.is_ok() {
            success += 1;
        }
    }
    ok(batch_result(ids.len(), success))
}

async fn batch_status(
    State(s): State<AppState>,
    Bind(req): Bind<IdsRequest>,
) -> ApiResult<Data<Value>> {
    let ids = req.ids()?;
    let mut success = 0;
    for id in ids {
        if s.svc
            .integration
            .mappings
            .set_active(*id, req.is_active)
            .await
            .is_ok()
        {
            success += 1;
        }
    }
    ok(batch_result(ids.len(), success))
}

async fn batch_delete(
    State(s): State<AppState>,
    Bind(req): Bind<IdsRequest>,
) -> ApiResult<Data<Value>> {
    let ids = req.ids()?;
    let mut success = 0;
    for id in ids {
        if s.svc.integration.mappings.delete(*id).await.is_ok() {
            success += 1;
        }
    }
    ok(batch_result(ids.len(), success))
}

async fn upstream_products(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let connection_id = id_param(&q, "connection_id");
    if connection_id == 0 {
        return Err(Error::invalid().into());
    }
    let p = page_of(
        &q,
        "page",
        "page_size",
        UPSTREAM_PAGE_SIZE,
        UPSTREAM_PAGE_SIZE,
    );
    let to_i64 = |v: u64| i64::try_from(v).unwrap_or(1);
    let (result, mapped) = s
        .svc
        .integration
        .mappings
        .upstream_products(connection_id, to_i64(p.page), to_i64(p.page_size))
        .await?;
    ok(json!({
        "items": result.items,
        "total": result.total,
        "mapped_ids": if p.page == 1 { json!(mapped) } else { Value::Null },
    }))
}

async fn upstream_categories(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let connection_id = id_param(&q, "connection_id");
    if connection_id == 0 {
        return Err(Error::invalid().into());
    }
    let list = s
        .svc
        .integration
        .mappings
        .upstream_categories(connection_id)
        .await?;
    ok(json!({"supported": list.supported, "categories": list.categories}))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ByCategoryBody {
    connection_id: Id,
    upstream_category_id: Id,
    auto_create_category: bool,
    local_category_id: Id,
}

impl BindRules for ByCategoryBody {
    const FIELDS: &'static [BindField] = &[
        req("connection_id", "ConnectionID"),
        req("upstream_category_id", "UpstreamCategoryID"),
    ];
}

async fn import_by_category(
    State(s): State<AppState>,
    Bind(req): Bind<ByCategoryBody>,
) -> ApiResult<Data<CategoryImportResult>> {
    if req.connection_id <= 0 || req.upstream_category_id <= 0 {
        return Err(Error::invalid().into());
    }
    let r = s
        .svc
        .integration
        .mappings
        .import_category(
            req.connection_id,
            req.upstream_category_id,
            req.auto_create_category,
            req.local_category_id,
        )
        .await
        .map_err(|e| e.or_internal(keys::CATEGORY_IMPORT_FAILED))?;
    ok(r)
}
