//! Shared converter program connections and per-product type/parameter bindings.

use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};
use zs_domain::Id;
use zs_domain::integration::card_converter::{Binding, ConverterInput};

use super::Params;
use crate::extract::{Body, PathId, Query};
use crate::response::{ApiResult, Data};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn admin() -> Routes {
    Routes::new("/admin")
        .get("/card-converters", list)
        .post("/card-converters", save)
        .put("/card-converters/{id}", update)
        .delete("/card-converters/{id}", delete)
        .post("/card-converters/{id}/test", test)
        .post("/card-converters/{id}/refresh-types", refresh_types)
        .get("/card-converter-bindings", bindings)
        .get("/card-converters/pending", pending)
        .put("/card-converter-bindings", save_binding)
        .delete("/card-converter-bindings", delete_binding)
        .get("/card-converters/{id}/events", events)
}

async fn list(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    Ok(Data(json!(s.svc.integration.card_converters.list().await?)))
}

async fn pending(State(s): State<AppState>) -> ApiResult<Data<Value>> {
    let local = s.svc.order.service.pending_conversion_count().await?;
    let upstream = s
        .svc
        .integration
        .procurement
        .held_converter_count(&s.svc.integration.card_converters)
        .await?;
    Ok(Data(
        json!({"local":local,"upstream":upstream,"total":local.saturating_add(upstream)}),
    ))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConverterBody {
    name: String,
    base_url: String,
    token: String,
    enabled: bool,
}

async fn save(
    State(s): State<AppState>,
    Body(body): Body<ConverterBody>,
) -> ApiResult<Data<Value>> {
    save_inner(s, 0, body).await
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(body): Body<ConverterBody>,
) -> ApiResult<Data<Value>> {
    save_inner(s, id, body).await
}

async fn save_inner(s: AppState, id: Id, body: ConverterBody) -> ApiResult<Data<Value>> {
    let saved = s
        .svc
        .integration
        .card_converters
        .save(
            id,
            ConverterInput {
                name: body.name,
                base_url: body.base_url,
                token: body.token,
                enabled: body.enabled,
            },
        )
        .await?;
    Ok(Data(json!(saved)))
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.integration.card_converters.delete(id).await?;
    Ok(Data(json!({"deleted": true})))
}

async fn test(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    Ok(Data(s.svc.integration.card_converters.test(id).await?))
}

async fn refresh_types(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    Ok(Data(json!(
        s.svc.integration.card_converters.refresh_types(id).await?
    )))
}

async fn bindings(State(s): State<AppState>, Query(q): Query<Params>) -> ApiResult<Data<Value>> {
    let product_id = q.get("product_id").and_then(|v| v.parse::<Id>().ok());
    Ok(Data(json!(
        s.svc
            .integration
            .card_converters
            .bindings(product_id)
            .await?
    )))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct BindingBody {
    id: Id,
    product_id: Id,
    sku_id: Id,
    converter_id: Id,
    type_id: String,
    fields: Vec<String>,
    extra_template: Value,
}

async fn save_binding(
    State(s): State<AppState>,
    Body(body): Body<BindingBody>,
) -> ApiResult<Data<Value>> {
    let saved = s
        .svc
        .integration
        .card_converters
        .save_binding(Binding {
            id: body.id,
            product_id: body.product_id,
            sku_id: body.sku_id,
            converter_id: body.converter_id,
            type_id: body.type_id,
            fields: body.fields,
            extra_template: body.extra_template,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
        .await?;
    Ok(Data(json!(saved)))
}

async fn delete_binding(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let product_id = q
        .get("product_id")
        .and_then(|v| v.parse::<Id>().ok())
        .unwrap_or(0);
    let sku_id = q
        .get("sku_id")
        .and_then(|v| v.parse::<Id>().ok())
        .unwrap_or(0);
    s.svc
        .integration
        .card_converters
        .delete_binding(product_id, sku_id)
        .await?;
    Ok(Data(json!({"deleted": true})))
}

async fn events(
    State(s): State<AppState>,
    PathId(id): PathId,
    Query(q): Query<Params>,
) -> ApiResult<Data<Value>> {
    let limit = q
        .get("limit")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(50);
    Ok(Data(json!(
        s.svc.integration.card_converters.events(id, limit).await?
    )))
}
