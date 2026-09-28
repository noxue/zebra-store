//! Channel API (`/api/v1/channel/*`) used by the Telegram bot: HMAC
//! authentication middleware, the channel response envelope and the
//! endpoints that do not depend on orders (port of `channelapi`,
//! `channelbot` and `middleware/channel_auth.go`).

use std::collections::{HashMap, HashSet};

use axum::body::{Body, Bytes};
use axum::extract::{FromRequestParts, OriginalUri, Path, Request, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use zs_app::notify::channel::{Heartbeat, Resolved};
use zs_app::notify::clients::SignedRequest;
use zs_domain::catalog::product::FulfillmentType;
use zs_domain::catalog::storefront::{PublicProduct, PublicSku};
use zs_domain::catalog::wholesale::WholesaleTier;
use zs_domain::notify::channel::{AuthFailure, ChannelAuthError, TelegramIdentityInput};
use zs_domain::{Error, Id};
use zs_shared::money::Amount;
use zs_shared::page::PageRequest;
use zs_shared::sign;

use super::{Params, Routes};
use crate::extract::Query;
use crate::i18n;
use crate::middleware::rate_limit::{self, MSG_RATE_LIMITED};
use crate::middleware::request_id::RequestId;
use crate::state::AppState;

/// Largest accepted request body (original `MaxBytesReader` 10 MiB).
const MAX_BODY_BYTES: usize = 10 << 20;
/// Default / maximum page size of `/channel/catalog/products` (original bounds 5 / 20).
const PRODUCTS_DEFAULT_PAGE_SIZE: u64 = 5;
const PRODUCTS_MAX_PAGE_SIZE: u64 = 20;
/// Product summary length in characters.
const SUMMARY_CHARS: usize = 100;
const DEFAULT_LOCALE: &str = "zh-CN";
const DEFAULT_CURRENCY: &str = "CNY";

/// `/api/v1/channel/*` routes (authenticated by [`require_channel`]).
pub fn routes() -> Routes {
    Routes::new("/channel")
        .get("/telegram/config", telegram_config)
        .post("/telegram/heartbeat", telegram_heartbeat)
        .post("/identities/telegram/resolve", resolve_identity)
        .post("/identities/telegram/provision", provision_identity)
        .post("/identities/telegram/bind", bind_identity)
        .get("/me", current_identity)
        .get("/catalog/categories", catalog_categories)
        .get("/catalog/products", catalog_products)
        .get("/catalog/products/{id}", catalog_product)
        .get("/member-levels", member_levels)
        .get("/payment-channels", payment_channels)
        .get("/payment-methods", payment_channels)
}

/// The authenticated channel client, stored as a request extension.
#[derive(Debug, Clone)]
pub struct ChannelClientCtx {
    pub id: Id,
    pub key: String,
    pub channel_type: String,
}

/// A channel error: real HTTP status, envelope code, `error_code` and message key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Failure {
    pub http: StatusCode,
    pub error_code: &'static str,
    pub key: &'static str,
}

impl Failure {
    const fn new(http: StatusCode, error_code: &'static str, key: &'static str) -> Self {
        Self {
            http,
            error_code,
            key,
        }
    }
}

const VALIDATION: Failure = Failure::new(
    StatusCode::BAD_REQUEST,
    "validation_error",
    "error.bad_request",
);
const INTERNAL: Failure = Failure::new(
    StatusCode::INTERNAL_SERVER_ERROR,
    "internal_error",
    "error.internal_error",
);
const UNAUTHORIZED: Failure = Failure::new(
    StatusCode::UNAUTHORIZED,
    "channel_client_unauthorized",
    "error.unauthorized",
);
const DISABLED: Failure = Failure::new(
    StatusCode::FORBIDDEN,
    "channel_client_disabled",
    "error.forbidden",
);
const PRODUCT_NOT_FOUND: Failure = Failure::new(
    StatusCode::NOT_FOUND,
    "product_not_found",
    "error.product_not_found",
);

/// Builds the original `ChannelResponse` body (`data`, `error_code` and
/// `request_id` are omitted when empty).
fn envelope(
    http: StatusCode,
    msg: String,
    data: Option<Value>,
    error_code: &str,
    request_id: &str,
) -> Response {
    let mut body = Map::new();
    let code = if http.is_success() { 0 } else { http.as_u16() };
    body.insert("status_code".into(), code.into());
    body.insert("msg".into(), msg.into());
    if let Some(data) = data.filter(|d| !d.is_null()) {
        body.insert("data".into(), data);
    }
    if !error_code.is_empty() {
        body.insert("error_code".into(), error_code.into());
    }
    if !request_id.is_empty() {
        body.insert("request_id".into(), request_id.into());
    }
    (http, axum::Json(Value::Object(body))).into_response()
}

fn request_id_of(extensions: &axum::http::Extensions) -> String {
    extensions
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_default()
}

fn failure_response(f: Failure, locale: &str, request_id: &str) -> Response {
    envelope(
        f.http,
        i18n::translate(locale, f.key),
        None,
        f.error_code,
        request_id,
    )
}

/// Channel HMAC authentication (original `ChannelAPIAuthMiddleware`), preceded by
/// the 600/min limit keyed by IP + channel key.
pub async fn require_channel(State(s): State<AppState>, req: Request, next: Next) -> Response {
    let locale = i18n::resolve_locale(req.uri().query(), req.headers());
    let request_id = request_id_of(req.extensions());
    let (key, timestamp, signature, ip) = {
        let headers = req.headers();
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default()
                .to_owned()
        };
        let peer = req
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|c| c.0.ip());
        let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
        (
            header(sign::HEADER_CHANNEL_KEY),
            header(sign::HEADER_CHANNEL_TIMESTAMP),
            header(sign::HEADER_CHANNEL_SIGNATURE),
            crate::client::resolve_ip(peer, xff, &s.trusted_proxies),
        )
    };
    let limit_key = rate_limit::key_by_ip_and_header(&ip, &key);
    if let Err(e) = rate_limit::check(&s.svc.notify.channel_limiter, &limit_key, MSG_RATE_LIMITED) {
        return e.into_response();
    }

    let path = req
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| req.uri().path().to_owned(), |u| u.0.path().to_owned());
    let method = req.method().as_str().to_owned();
    let (parts, body) = req.into_parts();
    let Ok(bytes) = axum::body::to_bytes(body, MAX_BODY_BYTES).await else {
        return failure_response(VALIDATION, locale, &request_id);
    };
    let auth = s
        .svc
        .notify
        .clients
        .authenticate(SignedRequest {
            key: &key,
            timestamp: &timestamp,
            signature: &signature,
            method: &method,
            path: &path,
            body: &bytes,
        })
        .await;
    let client = match auth {
        Ok(c) => c,
        Err(ChannelAuthError::Rejected(AuthFailure::Disabled)) => {
            return failure_response(DISABLED, locale, &request_id);
        }
        Err(ChannelAuthError::Rejected(reason)) => {
            tracing::debug!(?reason, "channel auth rejected");
            return failure_response(UNAUTHORIZED, locale, &request_id);
        }
        Err(ChannelAuthError::Internal(error)) => {
            tracing::error!(%error, "channel auth error");
            return failure_response(INTERNAL, locale, &request_id);
        }
    };
    let mut req = Request::from_parts(parts, Body::from(bytes));
    req.extensions_mut().insert(ChannelClientCtx {
        id: client.id,
        key: client.channel_key,
        channel_type: client.channel_type,
    });
    next.run(req).await
}

/// Per-request channel context: locale, request id and the authenticated client.
#[derive(Debug, Clone)]
pub struct Ctx {
    pub locale: &'static str,
    pub request_id: String,
    pub client: ChannelClientCtx,
}

impl<S: Send + Sync> FromRequestParts<S> for Ctx {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let locale = i18n::resolve_locale(parts.uri.query(), &parts.headers);
        let request_id = request_id_of(&parts.extensions);
        match parts.extensions.get::<ChannelClientCtx>() {
            Some(client) => Ok(Self {
                locale,
                request_id,
                client: client.clone(),
            }),
            None => Err(failure_response(UNAUTHORIZED, locale, &request_id)),
        }
    }
}

impl Ctx {
    fn ok(&self, data: impl Serialize) -> Response {
        match serde_json::to_value(data) {
            Ok(v) => envelope(
                StatusCode::OK,
                "success".into(),
                Some(v),
                "",
                &self.request_id,
            ),
            Err(error) => self.internal(&Error::from(error)),
        }
    }

    fn fail(&self, f: Failure) -> Response {
        failure_response(f, self.locale, &self.request_id)
    }

    fn internal(&self, error: &Error) -> Response {
        tracing::error!(%error, "channel handler error");
        self.fail(INTERNAL)
    }

    /// Maps identity-flow errors like the original `respondChannelIdentityServiceError`.
    fn identity_error(&self, e: &Error) -> Response {
        let f = match e.key() {
            "error.bad_request" => VALIDATION,
            "error.email_invalid" => Failure::new(
                StatusCode::BAD_REQUEST,
                "validation_error",
                "error.email_invalid",
            ),
            "error.user_not_found" => Failure::new(
                StatusCode::NOT_FOUND,
                "user_not_found",
                "error.user_not_found",
            ),
            "error.verify_code_invalid" => Failure::new(
                StatusCode::BAD_REQUEST,
                "verify_code_invalid",
                "error.verify_code_invalid",
            ),
            "error.verify_code_expired" => Failure::new(
                StatusCode::BAD_REQUEST,
                "verify_code_expired",
                "error.verify_code_expired",
            ),
            "error.verify_code_attempts_exceeded" => Failure::new(
                StatusCode::BAD_REQUEST,
                "verify_code_invalid",
                "error.verify_code_attempts_exceeded",
            ),
            "error.user_disabled" => Failure::new(
                StatusCode::UNAUTHORIZED,
                "user_disabled",
                "error.user_disabled",
            ),
            "error.telegram_bind_conflict" => Failure::new(
                StatusCode::BAD_REQUEST,
                "channel_identity_conflict",
                "error.telegram_bind_conflict",
            ),
            "error.telegram_already_bound" => Failure::new(
                StatusCode::BAD_REQUEST,
                "channel_identity_conflict",
                "error.telegram_already_bound",
            ),
            _ => return self.internal(e),
        };
        self.fail(f)
    }
}

/// Parses a JSON body; malformed or empty bodies are `validation_error` (Gin `ShouldBindJSON`).
fn parse<T: DeserializeOwned>(body: &Bytes) -> Result<T, Failure> {
    serde_json::from_slice(body).map_err(|_| VALIDATION)
}

// ---------------------------------------------------------------------------
// telegram bot runtime
// ---------------------------------------------------------------------------

async fn telegram_config(ctx: Ctx, State(s): State<AppState>) -> Response {
    match s.svc.notify.channel.bot_config(ctx.client.id).await {
        Ok(v) => ctx.ok(v),
        Err(e) => ctx.internal(&e),
    }
}

async fn telegram_heartbeat(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let hb: Heartbeat = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    match s.svc.notify.channel.heartbeat(hb).await {
        Ok(v) => ctx.ok(v),
        Err(e) => ctx.internal(&e),
    }
}

// ---------------------------------------------------------------------------
// identities
// ---------------------------------------------------------------------------

/// Resolve / provision / bind request (`telegram_*` fields are legacy aliases).
#[derive(Debug, Default, serde::Deserialize)]
#[serde(default)]
struct IdentityRequest {
    channel_user_id: String,
    telegram_user_id: String,
    username: String,
    telegram_username: String,
    first_name: String,
    last_name: String,
    avatar_url: String,
    bind_mode: String,
    email: String,
    code: String,
}

fn first_non_empty(a: &str, b: &str) -> String {
    let a = a.trim();
    if a.is_empty() { b.trim() } else { a }.to_owned()
}

impl IdentityRequest {
    fn input(&self) -> TelegramIdentityInput {
        TelegramIdentityInput {
            channel_user_id: first_non_empty(&self.channel_user_id, &self.telegram_user_id),
            username: first_non_empty(&self.username, &self.telegram_username),
            first_name: self.first_name.trim().to_owned(),
            last_name: self.last_name.trim().to_owned(),
            avatar_url: self.avatar_url.trim().to_owned(),
        }
    }
}

/// Original `buildChannelIdentityResponse(true, created, user, identity)`.
fn identity_body(r: &Resolved) -> Map<String, Value> {
    let body = json!({
        "bound": true,
        "identity": {
            "provider": r.identity.provider,
            "provider_user_id": r.identity.provider_user_id,
            "username": r.identity.username,
            "avatar_url": r.identity.avatar_url,
        },
        "user": {
            "id": r.user.id,
            "email": r.user.email,
            "display_name": r.user.display_name,
            "status": r.user.status,
            "locale": r.user.locale,
            "email_verified": r.user.email_verified,
            "password_setup_required": r.user.password_setup_required,
        },
        "created": r.created,
    });
    body.as_object().cloned().unwrap_or_default()
}

async fn resolve_identity(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let req: IdentityRequest = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let input = req.input();
    if input.channel_user_id.is_empty() {
        return ctx.fail(VALIDATION);
    }
    match s.svc.notify.channel.resolve(&input).await {
        Ok(Some(r)) => ctx.ok(identity_body(&r)),
        Ok(None) => ctx.ok(json!({"bound": false})),
        Err(e) => ctx.identity_error(&e),
    }
}

async fn provision_identity(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let req: IdentityRequest = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let input = req.input();
    if input.channel_user_id.is_empty() {
        return ctx.fail(VALIDATION);
    }
    match s.svc.notify.channel.provision(&input).await {
        Ok(r) => ctx.ok(identity_body(&r)),
        Err(e) => ctx.identity_error(&e),
    }
}

async fn bind_identity(ctx: Ctx, State(s): State<AppState>, body: Bytes) -> Response {
    let req: IdentityRequest = match parse(&body) {
        Ok(v) => v,
        Err(f) => return ctx.fail(f),
    };
    let mode = req.bind_mode.trim().to_lowercase();
    if !mode.is_empty() && mode != "email_code" {
        return ctx.fail(VALIDATION);
    }
    match s
        .svc
        .notify
        .channel
        .bind(&req.input(), &req.email, &req.code)
        .await
    {
        Ok(r) => {
            let mut out = identity_body(&r);
            if r.previous_user_id != 0 {
                out.insert("previous_user_id".into(), r.previous_user_id.into());
            }
            ctx.ok(out)
        }
        Err(e) => ctx.identity_error(&e),
    }
}

fn channel_user_id(q: &HashMap<String, String>) -> String {
    let get = |k: &str| q.get(k).map(String::as_str).unwrap_or_default();
    first_non_empty(get("channel_user_id"), get("telegram_user_id"))
}

async fn current_identity(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let input = TelegramIdentityInput {
        channel_user_id: channel_user_id(&q),
        username: q
            .get("username")
            .map(|v| v.trim().to_owned())
            .unwrap_or_default(),
        avatar_url: q
            .get("avatar_url")
            .map(|v| v.trim().to_owned())
            .unwrap_or_default(),
        ..TelegramIdentityInput::default()
    };
    if input.channel_user_id.is_empty() {
        return ctx.fail(VALIDATION);
    }
    match s.svc.notify.channel.resolve(&input).await {
        Ok(Some(r)) => ctx.ok(identity_body(&r)),
        Ok(None) => ctx.ok(json!({"bound": false})),
        Err(e) => ctx.identity_error(&e),
    }
}

// ---------------------------------------------------------------------------
// catalog
// ---------------------------------------------------------------------------

fn locale_of(q: &HashMap<String, String>) -> String {
    q.get("locale")
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .unwrap_or_else(|| DEFAULT_LOCALE.to_owned())
}

/// Localized text of a `{locale: text}` JSON object (`locale`, zh-CN, then any).
fn localized(map: &Map<String, Value>, locale: &str) -> String {
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    };
    [locale, DEFAULT_LOCALE]
        .iter()
        .filter_map(|l| map.get(*l))
        .chain(map.values())
        .map(text)
        .find(|t| !t.is_empty())
        .unwrap_or_default()
}

/// Removes HTML tags and blank lines (original `stripHTML`).
fn strip_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut in_tag = false;
    for c in value.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// First `limit` characters plus `...` when longer (original `truncate`).
fn truncate(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        value.to_owned()
    } else {
        format!("{}...", value.chars().take(limit).collect::<String>())
    }
}

fn wholesale(tiers: &[WholesaleTier]) -> Option<Vec<Value>> {
    let items: Vec<Value> = tiers
        .iter()
        .filter(|t| t.min_quantity > 0 && t.unit_price.is_positive())
        .map(|t| {
            let mut m = Map::new();
            if t.sku_id != 0 {
                m.insert("sku_id".into(), t.sku_id.into());
            }
            if !t.sku_code.is_empty() {
                m.insert("sku_code".into(), t.sku_code.clone().into());
            }
            m.insert("min_quantity".into(), t.min_quantity.into());
            m.insert("unit_price".into(), t.unit_price.to_string().into());
            Value::Object(m)
        })
        .collect();
    (!items.is_empty()).then_some(items)
}

fn product_stock_count(p: &PublicProduct) -> i64 {
    if p.fulfillment_type == FulfillmentType::Auto {
        p.auto_stock_available
    } else {
        p.manual_stock_available
    }
}

fn sku_stock_count(p: &PublicProduct, sku: &PublicSku) -> i64 {
    if p.fulfillment_type == FulfillmentType::Auto {
        sku.auto_stock_available
    } else {
        sku.manual_stock_total
    }
}

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

async fn member_price(
    s: &AppState,
    level: Id,
    product: Id,
    sku: Id,
    base: Amount,
) -> Option<String> {
    if level <= 0 {
        return None;
    }
    let price = s
        .svc
        .marketing
        .member_level
        .resolve_price(level, product, sku, base)
        .await
        .ok()?
        .price;
    (price < base).then(|| price.to_string())
}

async fn currency(s: &AppState) -> String {
    match s.svc.content.settings.currency().await {
        Ok(c) if !c.trim().is_empty() => c,
        _ => DEFAULT_CURRENCY.to_owned(),
    }
}

async fn catalog_categories(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let locale = locale_of(&q);
    let categories = match s.svc.catalog.category.list_active().await {
        Ok(c) => c,
        Err(e) => return ctx.internal(&e),
    };
    let counts = s
        .svc
        .notify
        .channel
        .category_product_counts()
        .await
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "channel catalog product counts failed");
            HashMap::new()
        });
    let has_children: HashSet<Id> = categories
        .iter()
        .filter(|c| c.parent_id != 0)
        .map(|c| c.parent_id)
        .collect();
    let visible_roots: HashSet<Id> = categories
        .iter()
        .filter(|c| c.parent_id == 0)
        .filter(|c| counts.get(&c.id).copied().unwrap_or(0) > 0 || has_children.contains(&c.id))
        .map(|c| c.id)
        .collect();
    let items: Vec<Value> = categories
        .iter()
        .filter(|c| {
            visible_roots.contains(if c.parent_id == 0 {
                &c.id
            } else {
                &c.parent_id
            })
        })
        .map(|c| {
            json!({
                "id": c.id,
                "parent_id": c.parent_id,
                "name": c.name.resolve(&locale),
                "icon": c.icon,
                "slug": c.slug,
                "product_count": counts.get(&c.id).copied().unwrap_or(0),
            })
        })
        .collect();
    ctx.ok(json!({"items": if items.is_empty() { Value::Null } else { Value::Array(items) }}))
}

/// Public stock fields shared by products and SKUs.
struct StockView<'a> {
    status: Value,
    count: i64,
    mode: Value,
    display: &'a str,
    min: Option<i32>,
    max: Option<i32>,
    hidden: bool,
}

fn stock_fields(m: &mut Map<String, Value>, v: StockView<'_>) {
    m.insert("stock_status".into(), v.status);
    m.insert("stock_count".into(), v.count.into());
    m.insert("stock_display_mode".into(), v.mode);
    m.insert("stock_display".into(), v.display.into());
    m.insert("stock_range_min".into(), v.min.into());
    m.insert("stock_range_max".into(), v.max.into());
    m.insert("stock_quantity_hidden".into(), v.hidden.into());
}

async fn catalog_products(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let locale = locale_of(&q);
    let num = |k: &str| {
        q.get(k)
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|n| *n > 0)
    };
    let page = PageRequest {
        page: num("page").unwrap_or(1),
        page_size: num("page_size")
            .unwrap_or(PRODUCTS_DEFAULT_PAGE_SIZE)
            .min(PRODUCTS_MAX_PAGE_SIZE),
    };
    let category = q
        .get("category_id")
        .map(|c| c.trim().to_owned())
        .unwrap_or_default();
    let result = match s.svc.catalog.product.list_public(&category, "", page).await {
        Ok(r) => r,
        Err(e) => return ctx.internal(&e),
    };
    let currency = currency(&s).await;
    let level = s
        .svc
        .notify
        .channel
        .member_level_of(&channel_user_id(&q))
        .await;
    let mut items = Vec::with_capacity(result.items.len());
    for p in &result.items {
        let mut m = Map::new();
        m.insert("id".into(), p.id.into());
        m.insert("title".into(), localized(&p.title, &locale).into());
        m.insert(
            "summary".into(),
            truncate(
                &strip_html(&localized(&p.description, &locale)),
                SUMMARY_CHARS,
            )
            .into(),
        );
        m.insert(
            "image_url".into(),
            p.images.first().cloned().unwrap_or_default().into(),
        );
        m.insert("price_from".into(), p.price_amount.to_string().into());
        if let Some(price) = member_price(&s, level, p.id, 0, p.price_amount).await {
            m.insert("member_price_from".into(), price.into());
        }
        if let Some(w) = wholesale(&p.wholesale_prices) {
            m.insert("wholesale_prices".into(), w.into());
        }
        m.insert("currency".into(), currency.clone().into());
        stock_fields(
            &mut m,
            StockView {
                status: to_json(&p.stock_status),
                count: product_stock_count(p),
                mode: to_json(&p.stock_display_mode),
                display: &p.stock_display,
                min: p.stock_range_min,
                max: p.stock_range_max,
                hidden: p.stock_quantity_hidden,
            },
        );
        m.insert(
            "category_name".into(),
            p.category.name.resolve(&locale).into(),
        );
        items.push(Value::Object(m));
    }
    ctx.ok(json!({
        "items": items,
        "total": result.total,
        "page": page.page,
        "page_size": page.page_size,
        "total_page": result.total.div_ceil(page.page_size),
    }))
}

/// Localized manual form schema (original `normalizeChannelManualFormSchema`).
fn manual_form_schema(schema: &Map<String, Value>, locale: &str) -> Value {
    let Some(Value::Array(fields)) = schema.get("fields") else {
        return json!({"fields": []});
    };
    let text = |v: Option<&Value>| match v {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Object(m)) => localized(m, locale).trim().to_owned(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    };
    let out: Vec<Value> = fields
        .iter()
        .filter_map(Value::as_object)
        .map(|f| {
            let mut m = Map::new();
            for key in ["key", "type"] {
                if let Some(Value::String(v)) = f.get(key) {
                    m.insert(key.into(), v.clone().into());
                }
            }
            if let Some(Value::Bool(b)) = f.get("required") {
                m.insert("required".into(), (*b).into());
            }
            for key in ["label", "placeholder"] {
                let t = text(f.get(key));
                if !t.is_empty() {
                    m.insert(key.into(), t.into());
                }
            }
            if let Some(Value::String(r)) = f
                .get("regex")
                .filter(|r| r.as_str().is_some_and(|s| !s.trim().is_empty()))
            {
                m.insert("regex".into(), r.clone().into());
            }
            for key in ["min", "max", "max_len"] {
                if let Some(v) = f.get(key) {
                    m.insert(key.into(), v.clone());
                }
            }
            if let Some(Value::Array(options)) = f.get("options") {
                let opts: Vec<Value> = options
                    .iter()
                    .map(|o| text(Some(o)))
                    .filter(|o| !o.is_empty())
                    .map(Value::from)
                    .collect();
                if !opts.is_empty() {
                    m.insert("options".into(), opts.into());
                }
            }
            Value::Object(m)
        })
        .collect();
    json!({"fields": out})
}

async fn catalog_product(
    ctx: Ctx,
    State(s): State<AppState>,
    Path(raw): Path<String>,
    Query(q): Params,
) -> Response {
    let locale = locale_of(&q);
    let Ok(id) = raw.trim().parse::<Id>() else {
        return ctx.fail(PRODUCT_NOT_FOUND);
    };
    let slug = match s.svc.notify.channel.product_slug(id).await {
        Ok(Some(slug)) => slug,
        Ok(None) => return ctx.fail(PRODUCT_NOT_FOUND),
        Err(e) => return ctx.internal(&e),
    };
    let p = match s.svc.catalog.product.get_public(&slug).await {
        Ok(p) => p,
        Err(e) if e.is_not_found() => return ctx.fail(PRODUCT_NOT_FOUND),
        Err(e) => return ctx.internal(&e),
    };
    let currency = currency(&s).await;
    let level = s
        .svc
        .notify
        .channel
        .member_level_of(&channel_user_id(&q))
        .await;
    let mut skus = Vec::new();
    for sku in p.skus.iter().filter(|k| k.is_active) {
        let mut m = Map::new();
        m.insert("id".into(), sku.id.into());
        m.insert("sku_code".into(), sku.sku_code.clone().into());
        m.insert(
            "spec_values".into(),
            localized(&sku.spec_values, &locale).into(),
        );
        m.insert("price".into(), sku.price_amount.to_string().into());
        if let Some(price) = member_price(&s, level, p.id, sku.id, sku.price_amount).await {
            m.insert("member_price".into(), price.into());
        }
        stock_fields(
            &mut m,
            StockView {
                status: to_json(&sku.stock_status),
                count: sku_stock_count(&p, sku),
                mode: to_json(&sku.stock_display_mode),
                display: &sku.stock_display,
                min: sku.stock_range_min,
                max: sku.stock_range_max,
                hidden: sku.stock_quantity_hidden,
            },
        );
        skus.push(Value::Object(m));
    }
    let mut m = Map::new();
    m.insert("id".into(), p.id.into());
    m.insert("title".into(), localized(&p.title, &locale).into());
    m.insert(
        "description".into(),
        strip_html(&localized(&p.content, &locale)).into(),
    );
    m.insert(
        "image_url".into(),
        p.images.first().cloned().unwrap_or_default().into(),
    );
    m.insert("price_from".into(), p.price_amount.to_string().into());
    let member_from = member_price(&s, level, p.id, 0, p.price_amount)
        .await
        .unwrap_or_default();
    m.insert("member_price_from".into(), member_from.into());
    m.insert(
        "wholesale_prices".into(),
        wholesale(&p.wholesale_prices).map_or(Value::Null, Value::from),
    );
    m.insert("currency".into(), currency.into());
    stock_fields(
        &mut m,
        StockView {
            status: to_json(&p.stock_status),
            count: product_stock_count(&p),
            mode: to_json(&p.stock_display_mode),
            display: &p.stock_display,
            min: p.stock_range_min,
            max: p.stock_range_max,
            hidden: p.stock_quantity_hidden,
        },
    );
    m.insert(
        "category_name".into(),
        p.category.name.resolve(&locale).into(),
    );
    m.insert("fulfillment_type".into(), to_json(&p.fulfillment_type));
    m.insert(
        "min_purchase_quantity".into(),
        p.min_purchase_quantity.max(0).into(),
    );
    m.insert(
        "max_purchase_quantity".into(),
        p.max_purchase_quantity.max(0).into(),
    );
    m.insert(
        "manual_form_schema".into(),
        manual_form_schema(&p.manual_form_schema, &locale),
    );
    m.insert("purchase_note".into(), "".into());
    m.insert("skus".into(), skus.into());
    ctx.ok(Value::Object(m))
}

// ---------------------------------------------------------------------------
// member levels / payment channels
// ---------------------------------------------------------------------------

async fn member_levels(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let locale = locale_of(&q);
    let levels = match s.svc.marketing.member_level.list_public().await {
        Ok(l) => l,
        Err(e) => return ctx.internal(&e),
    };
    let items: Vec<Value> = levels
        .iter()
        .map(|l| {
            json!({
                "id": l.id,
                "name": localized(&l.name, &locale),
                "slug": l.slug,
                "icon": l.icon,
                "discount_rate": l.discount_rate,
                "recharge_threshold": l.recharge_threshold,
                "spend_threshold": l.spend_threshold,
                "is_default": l.is_default,
                "sort_order": l.sort_order,
            })
        })
        .collect();
    ctx.ok(json!({"items": items}))
}

async fn payment_channels(ctx: Ctx, State(s): State<AppState>, Query(q): Params) -> Response {
    let context = q.get("context").map(String::as_str).unwrap_or_default();
    let order_no = q.get("order_no").map(String::as_str).unwrap_or_default();
    let methods = match s
        .svc
        .notify
        .channel
        .payment_methods(context, order_no, &channel_user_id(&q))
        .await
    {
        Ok(m) => m,
        Err(e) => return ctx.internal(&e),
    };
    let items: Vec<Value> = methods
        .items
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "provider_type": c.provider_type,
                "channel_type": c.channel_type,
                "interaction_mode": c.interaction_mode,
                "fee_rate": c.fee_rate.to_string(),
                "fixed_fee": c.fixed_fee.to_string(),
            })
        })
        .collect();
    let mut out = json!({"items": items});
    if methods.wallet_only_payment {
        out["wallet_only_payment"] = true.into();
    }
    ctx.ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_and_blank_lines() {
        assert_eq!(
            strip_html("<p>Hello <b>World</b></p>\n\n  <br/> line2 "),
            "Hello World\nline2"
        );
        assert_eq!(truncate("abcdef", 3), "abc...");
        assert_eq!(truncate("abc", 3), "abc");
    }

    #[test]
    fn manual_form_schema_is_localized() {
        let schema = json!({"fields": [{"key": "acc", "type": "text", "required": true,
            "label": {"zh-CN": "账号", "en-US": "Account"}, "options": ["a", "", 1], "regex": " "}]});
        let out = manual_form_schema(schema.as_object().unwrap_or(&Map::new()), "en-US");
        assert_eq!(
            out,
            json!({"fields": [{"key": "acc", "type": "text", "required": true, "label": "Account", "options": ["a", "1"]}]})
        );
        assert_eq!(
            manual_form_schema(&Map::new(), "zh-CN"),
            json!({"fields": []})
        );
    }
}
