//! Site connections (`/api/v1/admin/site-connections*`).

use axum::extract::State;
use axum::http::HeaderMap;
use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};
use zs_domain::Error;
use zs_domain::integration::adapter::ConnectionCode;
use zs_domain::integration::connection::{
    ConnectionInput, ConnectionStatus, Decimal, Endpoint, PROTOCOL_DUJIAO_NEXT, SiteConnection,
    normalize_base_url,
};
use zs_domain::integration::keys;
use zs_domain::integration::protocol::PingInfo;
use zs_shared::page::Pagination;

use super::{Params, de_opt_decimal, page};
use crate::extract::{Bind, BindField, BindRules, Body, PathId, Query, req};
use crate::response::{ApiResult, Data, Paged, ok};
use crate::routes::Routes;
use crate::state::AppState;

pub(super) fn connections() -> Routes {
    Routes::new("/admin")
        .get("/site-connections", list)
        .get("/site-connections/protocols", protocols)
        .post("/site-connections/parse-code", parse_code)
        .post("/site-connections/handshake", handshake)
        .get("/site-connections/{id}", get)
        .post("/site-connections", create)
        .put("/site-connections/{id}", update)
        .delete("/site-connections/{id}", delete)
        .post("/site-connections/{id}/ping", ping)
        .put("/site-connections/{id}/status", set_status)
        .post("/site-connections/{id}/reapply-markup", reapply_markup)
}

/// `retry_intervals` arrives as a JSON string (`"[30,60,300]"`) or an array.
fn de_intervals<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => String::new(),
    })
}

fn de_i32<'de, D: Deserializer<'de>>(d: D) -> Result<i32, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_i64().and_then(|v| i32::try_from(v).ok()).unwrap_or(0),
        Value::String(s) => s.trim().parse().unwrap_or(0),
        _ => 0,
    })
}

/// Create / update body (original `CreateInput` / `UpdateInput`).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ConnectionRequest {
    name: String,
    base_url: String,
    api_key: String,
    api_secret: String,
    protocol: String,
    callback_url: String,
    #[serde(deserialize_with = "de_i32")]
    retry_max: i32,
    #[serde(deserialize_with = "de_intervals")]
    retry_intervals: String,
    #[serde(deserialize_with = "de_opt_decimal")]
    exchange_rate: Option<Decimal>,
    #[serde(deserialize_with = "de_opt_decimal")]
    price_markup_percent: Option<Decimal>,
    price_rounding_mode: Option<String>,
    auto_sync_price: Option<bool>,
    /// Adapter-specific configuration fields (object; other shapes are ignored).
    #[serde(deserialize_with = "de_opt_object")]
    extra: Option<zs_domain::catalog::product::JsonMap>,
}

fn de_opt_object<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<Option<zs_domain::catalog::product::JsonMap>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => Some(m),
        _ => None,
    })
}

impl From<ConnectionRequest> for ConnectionInput {
    fn from(r: ConnectionRequest) -> Self {
        Self {
            name: r.name,
            base_url: r.base_url,
            api_key: r.api_key,
            api_secret: r.api_secret,
            protocol: r.protocol,
            callback_url: r.callback_url,
            retry_max: r.retry_max,
            retry_intervals: r.retry_intervals,
            exchange_rate: r.exchange_rate,
            price_markup_percent: r.price_markup_percent,
            price_rounding_mode: r.price_rounding_mode,
            auto_sync_price: r.auto_sync_price,
            extra: r.extra,
        }
    }
}

async fn list(
    State(s): State<AppState>,
    Query(q): Query<Params>,
) -> ApiResult<Paged<SiteConnection>> {
    let req = page(&q);
    let result = s.svc.integration.connections.list(req).await?;
    Ok(Paged(result.items, Pagination::new(req, result.total)))
}

async fn get(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<SiteConnection>> {
    ok(s.svc.integration.connections.get(id).await?)
}

async fn create(
    State(s): State<AppState>,
    Body(req): Body<ConnectionRequest>,
) -> ApiResult<Data<SiteConnection>> {
    ok(s.svc.integration.connections.create(&req.into()).await?)
}

async fn update(
    State(s): State<AppState>,
    PathId(id): PathId,
    Body(req): Body<ConnectionRequest>,
) -> ApiResult<Data<SiteConnection>> {
    ok(s.svc
        .integration
        .connections
        .update(id, &req.into())
        .await?)
}

async fn delete(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    s.svc.integration.connections.delete(id).await?;
    ok(json!({"deleted": true}))
}

async fn ping(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<PingInfo>> {
    ok(s.svc.integration.connections.ping(id).await?)
}

/// `{status}` (original) or `{is_active}` (admin toggle).
#[derive(Debug, Deserialize)]
struct StatusRequest {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
}

impl BindRules for StatusRequest {
    const FIELDS: &'static [BindField] = &[req("status", "Status")];
}

async fn set_status(
    State(s): State<AppState>,
    PathId(id): PathId,
    Bind(req): Bind<StatusRequest>,
) -> ApiResult<Data<Value>> {
    let status = match (req.status.as_deref(), req.is_active) {
        (Some(raw), _) if !raw.trim().is_empty() => ConnectionStatus::parse(raw),
        (_, Some(true)) => Some(ConnectionStatus::Active),
        (_, Some(false)) => Some(ConnectionStatus::Disabled),
        _ => None,
    }
    .ok_or_else(Error::invalid)?;
    s.svc.integration.connections.set_status(id, status).await?;
    ok(json!({"updated": true}))
}

/// Registered supplier systems (drives the connection form).
async fn protocols(State(s): State<AppState>) -> ApiResult<Data<Vec<Value>>> {
    ok(s.svc
        .integration
        .connections
        .protocols()
        .into_iter()
        .map(|m| {
            json!({
                "id": m.id,
                "name": m.name,
                "description": m.description,
                "fields": m.fields,
                "capabilities": m.capabilities.names(),
                "supports_connection_code": m.supports_connection_code,
                "callback_path": m.inbound_path,
            })
        })
        .collect())
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct CodeRequest {
    code: String,
}

/// Parses a connection code (`400 error.connection_code_invalid` when unusable).
async fn parse_code(
    State(s): State<AppState>,
    Body(req): Body<CodeRequest>,
) -> ApiResult<Data<ConnectionCode>> {
    ok(s.svc.integration.connections.parse_code(&req.code)?)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct HandshakeRequest {
    base_url: String,
    api_key: String,
    api_secret: String,
    protocol: String,
}

/// Handshake with a supplier before saving (any protocol): site, currency, features,
/// balance plus the suggested exchange rate and callback URL. Supplier failures come
/// back as `ok: false` with `error`.
async fn handshake(
    State(s): State<AppState>,
    headers: HeaderMap,
    Body(req): Body<HandshakeRequest>,
) -> ApiResult<Data<Value>> {
    let protocol = if req.protocol.trim().is_empty() {
        PROTOCOL_DUJIAO_NEXT.to_owned()
    } else {
        req.protocol.trim().to_owned()
    };
    let conns = &s.svc.integration.connections;
    let meta = conns
        .adapter(&protocol)
        .map(|a| a.meta())
        .ok_or_else(|| Error::bad_request(keys::CONNECTION_INVALID))?;
    let base_url = normalize_base_url(&req.base_url)?;
    if req.api_key.trim().is_empty() || req.api_secret.trim().is_empty() {
        return Err(Error::bad_request(keys::CONNECTION_INVALID).into());
    }
    let site = s.svc.integration.supplier.site().await?;
    let ours = if site.site_url.trim().is_empty() {
        super::zs::origin(&headers)
    } else {
        site.site_url.trim().trim_end_matches('/').to_owned()
    };
    let suggested_callback_url = format!("{ours}{}", meta.inbound_path);
    let endpoint = Endpoint {
        base_url,
        api_key: req.api_key.trim().to_owned(),
        api_secret: req.api_secret.clone(),
        protocol: protocol.clone(),
        ..Endpoint::default()
    };
    match conns.probe(&endpoint).await {
        Ok(p) => {
            let h = p.handshake;
            let suggested_rate = (!h.currency.is_empty()
                && h.currency.eq_ignore_ascii_case(&site.currency))
            .then_some("1");
            ok(json!({
                "ok": true,
                "protocol": protocol,
                "version": h.version,
                "site": {"name": h.site_name, "url": h.site_url, "currency": h.currency},
                "features": h.features,
                "capabilities": p.capabilities.names(),
                "limits": h.limits,
                "account": {"balance": h.balance, "currency": h.account_currency},
                "suggested_exchange_rate": suggested_rate,
                "suggested_callback_url": suggested_callback_url,
            }))
        }
        Err(error) => ok(json!({
            "ok": false,
            "protocol": protocol,
            "version": "",
            "site": {"name": "", "url": "", "currency": ""},
            "features": [],
            "capabilities": [],
            "limits": {},
            "account": {"balance": "", "currency": ""},
            "suggested_exchange_rate": null,
            "suggested_callback_url": suggested_callback_url,
            "error": error.to_string(),
        })),
    }
}

async fn reapply_markup(State(s): State<AppState>, PathId(id): PathId) -> ApiResult<Data<Value>> {
    let n = s.svc.integration.connections.reapply_markup(id).await?;
    ok(json!({"updated_products": n}))
}
