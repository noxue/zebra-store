//! The `zebra-store` protocol v1 (`/api/v1/zs/*`, docs/protocol/zebra-store-v1.md):
//! signed supplier endpoints (`{ok, data}` / `{ok:false, error}` with real HTTP status
//! codes) and our event receiver `POST /api/v1/zs/events` (buyer side).

use axum::Json;
use axum::body::Bytes;
use axum::extract::{FromRequest, OriginalUri, Path, Request, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header as http_header};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zs_app::integration::credential::{Caller, ZsSignedRequest};
use zs_app::integration::inbound::InboundFailure;
use zs_app::integration::zs_supplier::OrderBody;
use zs_domain::Id;
use zs_domain::catalog::product::JsonMap;
use zs_domain::integration::adapter::{InboundError, InboundRequest};
use zs_domain::integration::zs::{ErrorCode, ItemRequest, MAX_BODY_BYTES, PROTOCOL_ID, ZsFailure};
use zs_shared::money::Amount;
use zs_shared::zs as proto;

use super::upstream::{client_ip, header};
use super::{Params, param};
use crate::extract::Query;
use crate::middleware::request_id::RequestId;
use crate::routes::Routes;
use crate::state::AppState;

/// Routes under `/api/v1` (no JWT: signature authentication per request).
pub(super) fn routes() -> Routes {
    Routes::new("")
        .get("/zs/handshake", handshake)
        .get("/zs/catalog/categories", categories)
        .get("/zs/catalog/products", products)
        .get("/zs/catalog/products/{id}", product)
        .get("/zs/catalog/changes", changes)
        .put("/zs/webhooks", put_webhook)
        .get("/zs/webhooks", get_webhook)
        .delete("/zs/webhooks", delete_webhook)
        .post("/zs/orders/quote", quote)
        .post("/zs/orders", create_order)
        .get("/zs/orders", list_orders)
        .get("/zs/orders/{order_no}", get_order)
        .post("/zs/orders/{order_no}/cancel", cancel_order)
        .post("/zs/events", events)
}

/// Error object of spec §1 / §8.
fn fail(f: &ZsFailure, request_id: &str) -> Response {
    let status =
        StatusCode::from_u16(f.code.http_status()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let body = json!({
        "ok": false,
        "error": {
            "code": f.code.as_str(),
            "message": f.message,
            "retryable": f.code.retryable(),
            "request_id": request_id,
        }
    });
    (status, Json(body)).into_response()
}

fn success(data: Value) -> Response {
    Json(json!({"ok": true, "data": data})).into_response()
}

fn request_id(req: &Request) -> String {
    req.extensions()
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_default()
}

/// The request's own origin (`scheme://host`), used when no brand URL is configured.
pub(super) fn origin(headers: &HeaderMap) -> String {
    let get = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
    };
    if let Some(o) = get("origin").filter(|o| o.starts_with("http")) {
        return o.trim_end_matches('/').to_owned();
    }
    let host = get("x-forwarded-host")
        .or_else(|| get("host"))
        .unwrap_or_else(|| "localhost".to_owned());
    let scheme = get("x-forwarded-proto").unwrap_or_else(|| "http".to_owned());
    format!("{scheme}://{host}")
}

/// An authenticated zebra-store request (rate limit per key, 1 MiB body, spec §2
/// signature with nonce).
#[derive(Debug)]
pub struct ZsSigned {
    pub caller: Caller,
    pub body: Bytes,
    pub headers: HeaderMap,
    pub client_ip: String,
    pub request_id: String,
}

impl FromRequest<AppState> for ZsSigned {
    type Rejection = Response;

    async fn from_request(req: Request, s: &AppState) -> Result<Self, Self::Rejection> {
        let rid = request_id(&req);
        let ip = client_ip(&req, s);
        let headers = req.headers().clone();
        let api_key = header(&headers, proto::HEADER_KEY);
        let limit_key = if api_key.is_empty() {
            format!("ip:{ip}")
        } else {
            format!("key:{api_key}")
        };
        if let Err(wait) = s.svc.integration.zs_limiter.hit(&limit_key) {
            let mut res = fail(
                &ZsFailure::new(ErrorCode::RateLimited, "too many requests"),
                &rid,
            );
            if let Ok(v) = HeaderValue::from_str(&wait.max(1).to_string()) {
                res.headers_mut().insert(http_header::RETRY_AFTER, v);
            }
            return Err(res);
        }
        let uri = req
            .extensions()
            .get::<OriginalUri>()
            .map_or_else(|| req.uri().clone(), |u| u.0.clone());
        let method = req.method().as_str().to_owned();
        let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await else {
            return Err(fail(
                &ZsFailure::invalid("request body larger than 1 MiB"),
                &rid,
            ));
        };
        let timestamp = header(&headers, proto::HEADER_TIMESTAMP);
        let nonce = header(&headers, proto::HEADER_NONCE);
        let signature = header(&headers, proto::HEADER_SIGNATURE);
        let caller = s
            .svc
            .integration
            .zs
            .authenticate(ZsSignedRequest {
                api_key: &api_key,
                timestamp: &timestamp,
                nonce: &nonce,
                signature: &signature,
                method: &method,
                path: uri.path(),
                query: uri.query().unwrap_or_default(),
                body: &body,
            })
            .await
            .map_err(|f| fail(&f, &rid))?;
        Ok(Self {
            caller,
            body,
            headers,
            client_ip: ip,
            request_id: rid,
        })
    }
}

impl ZsSigned {
    fn reply(&self, r: Result<Value, ZsFailure>) -> Response {
        match r {
            Ok(v) => success(v),
            Err(f) => fail(&f, &self.request_id),
        }
    }

    fn json<T: for<'de> Deserialize<'de>>(&self) -> Result<T, ZsFailure> {
        let raw: &[u8] = if self.body.is_empty() {
            b"{}"
        } else {
            &self.body
        };
        serde_json::from_slice(raw).map_err(|e| ZsFailure::invalid(format!("body: {e}")))
    }
}

fn limit(q: &Params) -> Option<u64> {
    param(q, "limit").parse().ok()
}

async fn handshake(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = s
        .svc
        .integration
        .zs
        .handshake(&req.caller, &origin(&req.headers))
        .await;
    req.reply(r)
}

async fn categories(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = s.svc.integration.zs.categories().await;
    req.reply(r)
}

async fn products(State(s): State<AppState>, Query(q): Query<Params>, req: ZsSigned) -> Response {
    let r = s
        .svc
        .integration
        .zs
        .products(&req.caller, param(&q, "cursor"), limit(&q))
        .await;
    req.reply(r)
}

async fn product(State(s): State<AppState>, Path(id): Path<String>, req: ZsSigned) -> Response {
    let Some(id) = id.trim().parse::<Id>().ok().filter(|v| *v > 0) else {
        return req.reply(Err(ZsFailure::not_found("product")));
    };
    let r = s.svc.integration.zs.product(&req.caller, id).await;
    req.reply(r)
}

async fn changes(State(s): State<AppState>, Query(q): Query<Params>, req: ZsSigned) -> Response {
    let since = q.get("since").map(String::as_str);
    let r = s.svc.integration.zs.changes(since, limit(&q)).await;
    req.reply(r)
}

/// Accepts a decimal as number or string.
fn amount_of(v: Option<&Value>) -> Result<Option<Amount>, ZsFailure> {
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.trim().is_empty() => Ok(None),
        Some(Value::String(s)) => s
            .trim()
            .parse()
            .map(Some)
            .map_err(|_| ZsFailure::invalid("balance_low_threshold: invalid amount")),
        Some(Value::Number(n)) => n
            .to_string()
            .parse()
            .map(Some)
            .map_err(|_| ZsFailure::invalid("balance_low_threshold: invalid amount")),
        Some(_) => Err(ZsFailure::invalid("balance_low_threshold: invalid amount")),
    }
}

async fn put_webhook(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = async {
        let body: Map<String, Value> = req.json()?;
        let url = body
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| ZsFailure::invalid("url: required"))?;
        let events: Vec<String> = match body.get("events") {
            None | Some(Value::Null) => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .map(|e| {
                    e.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| ZsFailure::invalid("events: strings expected"))
                })
                .collect::<Result<_, _>>()?,
            Some(_) => return Err(ZsFailure::invalid("events: array expected")),
        };
        let threshold = amount_of(body.get("balance_low_threshold"))?;
        s.svc
            .integration
            .zs
            .put_webhook(&req.caller, url, &events, threshold)
            .await
    }
    .await;
    req.reply(r)
}

async fn get_webhook(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = s.svc.integration.zs.get_webhook(&req.caller).await;
    req.reply(r)
}

async fn delete_webhook(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = s.svc.integration.zs.delete_webhook(&req.caller).await;
    req.reply(r)
}

#[derive(Debug, Deserialize)]
struct ItemBody {
    #[serde(default)]
    sku_id: Id,
    #[serde(default)]
    quantity: i32,
    #[serde(default)]
    manual_form_data: Option<JsonMap>,
}

impl From<ItemBody> for ItemRequest {
    fn from(b: ItemBody) -> Self {
        Self {
            sku_id: b.sku_id,
            quantity: b.quantity,
            manual_form_data: b.manual_form_data,
        }
    }
}

#[derive(Debug, Deserialize)]
struct QuoteBody {
    #[serde(default)]
    items: Vec<ItemBody>,
}

async fn quote(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = async {
        let body: QuoteBody = req.json()?;
        let items: Vec<ItemRequest> = body.items.into_iter().map(Into::into).collect();
        s.svc.integration.zs.quote(&req.caller, &items).await
    }
    .await;
    req.reply(r)
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
struct CreateBody {
    #[serde(default)]
    quote_id: Option<String>,
    #[serde(default)]
    items: Vec<ItemBody>,
    #[serde(default)]
    downstream_order_no: String,
    #[serde(default)]
    trace_id: String,
    #[serde(default = "default_true")]
    callback: bool,
}

async fn create_order(State(s): State<AppState>, req: ZsSigned) -> Response {
    let r = async {
        let key = header(&req.headers, proto::HEADER_IDEMPOTENCY_KEY);
        let body: CreateBody = req.json()?;
        let order = OrderBody {
            quote_id: body.quote_id,
            items: body.items.into_iter().map(Into::into).collect(),
            downstream_order_no: body.downstream_order_no,
            trace_id: body.trace_id,
            callback: body.callback,
        };
        let hash = proto::sha256_hex(&req.body);
        s.svc
            .integration
            .zs
            .create_order(&req.caller, &key, &hash, &order, &req.client_ip)
            .await
    }
    .await;
    req.reply(r)
}

async fn list_orders(
    State(s): State<AppState>,
    Query(q): Query<Params>,
    req: ZsSigned,
) -> Response {
    let r = s
        .svc
        .integration
        .zs
        .list_orders(
            &req.caller,
            param(&q, "downstream_order_no"),
            param(&q, "cursor"),
            limit(&q),
        )
        .await;
    req.reply(r)
}

async fn get_order(
    State(s): State<AppState>,
    Path(order_no): Path<String>,
    req: ZsSigned,
) -> Response {
    let r = s.svc.integration.zs.get_order(&req.caller, &order_no).await;
    req.reply(r)
}

async fn cancel_order(
    State(s): State<AppState>,
    Path(order_no): Path<String>,
    req: ZsSigned,
) -> Response {
    let r = s
        .svc
        .integration
        .zs
        .cancel_order(&req.caller, &order_no)
        .await;
    req.reply(r)
}

/// Events pushed by a zebra-store supplier (we are the buyer): verified by the
/// adapter with the connection of `ZS-Key`, then deduplicated / applied by the neutral
/// inbound core (UPS-02/03). Body capped at 1 MiB; rate limited per IP.
async fn events(State(s): State<AppState>, req: Request) -> Response {
    let rid = request_id(&req);
    let ip = client_ip(&req, &s);
    if s.svc.integration.callback_limiter.hit(&ip).is_err() {
        return fail(
            &ZsFailure::new(ErrorCode::RateLimited, "too many requests"),
            &rid,
        );
    }
    let headers = req.headers().clone();
    let uri = req
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| req.uri().clone(), |u| u.0.clone());
    let Ok(body) = axum::body::to_bytes(req.into_body(), MAX_BODY_BYTES).await else {
        return fail(&ZsFailure::invalid("request body larger than 1 MiB"), &rid);
    };
    let lookup = |name: &str| {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    let inbound = InboundRequest {
        method: "POST",
        path: uri.path(),
        query: uri.query().unwrap_or_default(),
        headers: &lookup,
        body: &body,
    };
    match s
        .svc
        .integration
        .inbound
        .handle(PROTOCOL_ID, &inbound)
        .await
    {
        Ok(n) => success(json!({"received": n})),
        Err(f) => {
            let failure = match f {
                InboundFailure::Adapter(
                    InboundError::MissingHeaders
                    | InboundError::InvalidTimestamp
                    | InboundError::TimestampExpired
                    | InboundError::InvalidSignature,
                )
                | InboundFailure::InvalidKey => ZsFailure::unauthorized(),
                InboundFailure::Adapter(
                    InboundError::InvalidBody | InboundError::MissingFields,
                ) => ZsFailure::invalid("invalid event"),
                InboundFailure::NotFound => ZsFailure::not_found("order"),
                InboundFailure::Processing => ZsFailure::new(
                    ErrorCode::Unavailable,
                    "event could not be applied yet, retry later",
                ),
                InboundFailure::UnknownProtocol | InboundFailure::Internal => ZsFailure::internal(),
            };
            fail(&failure, &rid)
        }
    }
}
