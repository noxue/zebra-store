//! `zebra-store` protocol v1 adapter (docs/protocol/zebra-store-v1.md): the buyer-side
//! client of `/api/v1/zs/*` (canonical HMAC-SHA256 signing with nonce, cursor catalog,
//! change feed, quotes, idempotent orders, encrypted deliveries), verification of the
//! events pushed to our `/api/v1/zs/events`, connection codes, and the SSRF-safe sender
//! of the events we push as a supplier.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rand::Rng;
use rand::distr::Alphanumeric;
use serde_json::{Value, json};
use zs_domain::integration::adapter::{
    AdapterMeta, Capabilities, Capability, CatalogChange, ChangeKind, ChangePage, ConnectionCode,
    HandshakeInfo, InboundError, InboundEvent, InboundKind, InboundRequest, OrderLine, OrderNotice,
    OrderRef, PlaceOrder, Quote, QuoteLine, SupplierAdapter, text3,
};
use zs_domain::integration::connection::Endpoint;
use zs_domain::integration::protocol::{
    CategoryList, CreateOrderResponse, Download, PingInfo, ProductPage, ProductQuery,
    RemoteCategory, RemoteFulfillment, RemoteOrder, RemoteProduct, UpstreamClient, UpstreamError,
};
use zs_domain::integration::zs::{EventSender, PROTOCOL_ID};
use zs_domain::notify::ports::KEY_FORBIDDEN_ADDRESS;
use zs_domain::{Error, Id};
use zs_shared::clock::Clock;
use zs_shared::zs::{self as proto, Sealed, Signed};

use super::client::{
    HttpConnector, MAX_RESPONSE_BYTES, body_lost, key_secret_fields, transport, unreadable,
};
use super::http::{AddressPolicy, build_client, check_url, is_forbidden, read_limited};

/// Path of our event receiver.
pub const EVENTS_PATH: &str = "/api/v1/zs/events";
/// API prefix of the protocol.
const PREFIX: &str = "/api/v1/zs";
/// Page size used when walking cursor pages for a page number.
const MAX_LIMIT: i64 = 200;
/// Events requested when registering our receiver.
const SUBSCRIBED: [&str; 3] = ["catalog.changed", "order.*", "account.balance_low"];
/// Whole-request timeout of event deliveries.
const EVENT_TIMEOUT: Duration = Duration::from_secs(15);
/// Bytes of an event answer read.
const EVENT_RESPONSE_LIMIT: usize = 4096;

fn random_nonce() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

/// Capability of a supplier feature name (spec §4).
fn capability_of(feature: &str) -> Option<Capability> {
    Some(match feature {
        "changes" => Capability::IncrementalChanges,
        "webhooks" => Capability::PushEvents,
        "quote" => Capability::Quote,
        "multi_item" => Capability::MultiItem,
        "idempotency" => Capability::Idempotency,
        "encrypted_delivery" => Capability::EncryptedDelivery,
        _ => return None,
    })
}

/// Opens a sealed delivery into a fulfillment.
fn open_delivery(secret: &str, v: &Value) -> Result<Option<RemoteFulfillment>, String> {
    if v.is_null() {
        return Ok(None);
    }
    let fulfillment_of = |plain: &Value| -> Result<RemoteFulfillment, String> {
        let mut f: RemoteFulfillment =
            serde_json::from_value(plain.clone()).map_err(|e| e.to_string())?;
        if f.status.is_empty() {
            "delivered".clone_into(&mut f.status);
        }
        Ok(f)
    };
    if v.get("encrypted").and_then(Value::as_bool) == Some(true) {
        let sealed: Sealed = serde_json::from_value(v.clone()).map_err(|e| e.to_string())?;
        let plain = proto::open_delivery(secret, &sealed).map_err(|e| e.to_string())?;
        let plain: Value = serde_json::from_slice(&plain).map_err(|e| e.to_string())?;
        return fulfillment_of(&plain).map(Some);
    }
    fulfillment_of(v).map(Some)
}

/// First delivered line of an order object.
fn order_delivery(secret: &str, order: &Value) -> Result<Option<RemoteFulfillment>, String> {
    for item in order
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(f) = open_delivery(secret, item.get("delivery").unwrap_or(&Value::Null))? {
            return Ok(Some(f));
        }
    }
    Ok(None)
}

fn text(v: &Value, key: &str) -> String {
    match v.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// The `zebra-store` system.
#[derive(Debug, Clone)]
pub struct ZebraStoreAdapter {
    ctx: HttpConnector,
}

impl ZebraStoreAdapter {
    pub fn new(ctx: HttpConnector) -> Self {
        Self { ctx }
    }

    fn capabilities() -> Capabilities {
        Capabilities::of(&Capability::ALL)
    }
}

impl SupplierAdapter for ZebraStoreAdapter {
    fn meta(&self) -> AdapterMeta {
        AdapterMeta {
            id: PROTOCOL_ID,
            name: text3("Zebra Store", "Zebra Store", "Zebra Store"),
            description: text3(
                "Zebra Store 对接协议 v1：连接码一键接入、握手协商、变更流增量同步 + Webhook 推送、报价锁价、多商品幂等下单、卡密加密交付。",
                "Zebra Store 對接協議 v1：連接碼一鍵接入、握手協商、變更流增量同步 + Webhook 推送、報價鎖價、多商品冪等下單、卡密加密交付。",
                "Zebra Store protocol v1: one-paste connection codes, handshake, change feed + webhooks, price-locking quotes, idempotent multi-item orders, encrypted deliveries.",
            ),
            fields: key_secret_fields(),
            capabilities: Self::capabilities(),
            supports_connection_code: true,
            inbound_path: EVENTS_PATH,
        }
    }

    fn open(&self, endpoint: &Endpoint) -> zs_domain::Result<Arc<dyn UpstreamClient>> {
        let all = Self::capabilities();
        let capabilities = match &endpoint.features {
            // Negotiated capability ids (categories are always served).
            Some(ids) => {
                let mut list = Capabilities::from_ids(ids).list().to_vec();
                list.push(Capability::Categories);
                all.intersect(&Capabilities::of(&list))
            }
            None => all,
        };
        Ok(Arc::new(ZebraStoreClient {
            ctx: self.ctx.clone(),
            base_url: endpoint.base_url.trim_end_matches('/').to_owned(),
            api_key: endpoint.api_key.clone(),
            api_secret: endpoint.api_secret.clone(),
            capabilities,
        }))
    }

    fn inbound_key(
        &self,
        req: &InboundRequest<'_>,
        now: DateTime<Utc>,
    ) -> Result<String, InboundError> {
        let key = req.header(proto::HEADER_KEY);
        let ts = req.header(proto::HEADER_TIMESTAMP);
        let nonce = req.header(proto::HEADER_NONCE);
        if key.is_empty()
            || ts.is_empty()
            || nonce.is_empty()
            || req.header(proto::HEADER_SIGNATURE).is_empty()
        {
            return Err(InboundError::MissingHeaders);
        }
        let ts: i64 = ts.parse().map_err(|_| InboundError::InvalidTimestamp)?;
        if !proto::valid_nonce(&nonce) {
            return Err(InboundError::InvalidSignature);
        }
        if !proto::timestamp_valid(ts, now.timestamp()) {
            return Err(InboundError::TimestampExpired);
        }
        Ok(key)
    }

    fn parse_inbound(
        &self,
        req: &InboundRequest<'_>,
        secret: &str,
        _now: DateTime<Utc>,
    ) -> Result<Vec<InboundEvent>, InboundError> {
        let ts: i64 = req
            .header(proto::HEADER_TIMESTAMP)
            .parse()
            .map_err(|_| InboundError::InvalidTimestamp)?;
        let nonce = req.header(proto::HEADER_NONCE);
        let signed = Signed {
            method: req.method,
            path: req.path,
            query: req.query,
            timestamp: ts,
            nonce: &nonce,
            body: req.body,
        };
        if !signed.verify(secret, &req.header(proto::HEADER_SIGNATURE)) {
            return Err(InboundError::InvalidSignature);
        }
        let event: Value =
            serde_json::from_slice(req.body).map_err(|_| InboundError::InvalidBody)?;
        let id = text(&event, "id");
        let kind = text(&event, "type");
        if id.is_empty() || kind.is_empty() {
            return Err(InboundError::MissingFields);
        }
        let header_id = req.header(proto::HEADER_EVENT_ID);
        if !header_id.is_empty() && header_id != id {
            return Err(InboundError::InvalidBody);
        }
        let data = event.get("data").cloned().unwrap_or(Value::Null);
        let kind = match kind.as_str() {
            "catalog.changed" => InboundKind::CatalogChanged,
            "account.balance_low" => InboundKind::BalanceLow {
                balance: text(&data, "balance"),
                threshold: text(&data, "threshold"),
            },
            k if k.starts_with("order.") => {
                let fulfillment =
                    order_delivery(secret, &data).map_err(|_| InboundError::InvalidBody)?;
                let notice = OrderNotice {
                    downstream_order_no: text(&data, "downstream_order_no"),
                    upstream: OrderRef {
                        id: 0,
                        no: text(&data, "order_no"),
                    },
                    status: text(&data, "status"),
                    fulfillment,
                };
                if notice.downstream_order_no.is_empty() || notice.status.is_empty() {
                    return Err(InboundError::MissingFields);
                }
                InboundKind::Order(notice)
            }
            other => InboundKind::Other(other.to_owned()),
        };
        Ok(vec![InboundEvent {
            event_id: Some(id),
            kind,
        }])
    }

    fn parse_connection_code(&self, code: &str) -> Option<Result<ConnectionCode, String>> {
        if !code.trim().starts_with("zsc") {
            return None;
        }
        Some(
            proto::decode_connection_code(code)
                .map(|c| ConnectionCode {
                    name: c.name,
                    base_url: c.url.trim_end_matches('/').to_owned(),
                    api_key: c.key,
                    api_secret: c.secret,
                    protocol: PROTOCOL_ID.to_owned(),
                })
                .map_err(|e| e.to_string()),
        )
    }
}

/// Client bound to one connection.
struct ZebraStoreClient {
    ctx: HttpConnector,
    base_url: String,
    api_key: String,
    api_secret: String,
    capabilities: Capabilities,
}

impl ZebraStoreClient {
    /// Signed request; returns `data` of `{ok:true}` answers, [`UpstreamError::Http`]
    /// with the error object otherwise (`410 cursor_expired` → [`UpstreamError::CursorExpired`]).
    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        query: &str,
        body: Option<&Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value, UpstreamError> {
        let full_path = format!("{PREFIX}{path}");
        let target = if query.is_empty() {
            format!("{}{full_path}", self.base_url)
        } else {
            format!("{}{full_path}?{query}", self.base_url)
        };
        let url = check_url(self.ctx.policy, &target).map_err(UpstreamError::Forbidden)?;
        let bytes = match body {
            Some(b) => serde_json::to_vec(b).map_err(|e| UpstreamError::Protocol(e.to_string()))?,
            None => Vec::new(),
        };
        let ts = self.ctx.clock.now().timestamp();
        let nonce = random_nonce();
        let signature = Signed {
            method: method.as_str(),
            path: url.path(),
            query: url.query().unwrap_or_default(),
            timestamp: ts,
            nonce: &nonce,
            body: &bytes,
        }
        .sign(&self.api_secret);
        let mut req = self
            .ctx
            .http
            .request(method.clone(), url)
            .header(proto::HEADER_KEY, &self.api_key)
            .header(proto::HEADER_TIMESTAMP, ts.to_string())
            .header(proto::HEADER_NONCE, &nonce)
            .header(proto::HEADER_SIGNATURE, signature);
        if let Some(key) = idempotency_key {
            req = req.header(proto::HEADER_IDEMPOTENCY_KEY, key);
        }
        if body.is_some() {
            req = req
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/json; charset=utf-8",
                )
                .body(bytes);
        }
        let res = req.send().await.map_err(|e| transport(&e))?;
        let status = res.status().as_u16();
        let raw = read_limited(res, MAX_RESPONSE_BYTES)
            .await
            .map_err(body_lost)?;
        let Ok(parsed) = serde_json::from_slice::<Value>(&raw) else {
            if (200..300).contains(&status) || status >= 500 {
                // Possibly executed, answer unusable.
                return Err(unreadable(&raw));
            }
            return Err(UpstreamError::Http {
                status,
                code: String::new(),
                message: String::from_utf8_lossy(&raw).chars().take(500).collect(),
            });
        };
        if (200..300).contains(&status) && parsed.get("ok").and_then(Value::as_bool) == Some(true) {
            return Ok(parsed.get("data").cloned().unwrap_or(Value::Null));
        }
        let error = parsed.get("error").cloned().unwrap_or(Value::Null);
        let code = text(&error, "code");
        if status == 410 || code == "cursor_expired" {
            return Err(UpstreamError::CursorExpired);
        }
        let mut message = text(&error, "message");
        if message.is_empty() {
            message = String::from_utf8_lossy(&raw).chars().take(500).collect();
        }
        tracing::warn!(method = %method, path = %full_path, status, code, "zebra-store request error");
        Err(UpstreamError::Http {
            status,
            code,
            message,
        })
    }

    async fn get(&self, path: &str, query: &str) -> Result<Value, UpstreamError> {
        self.call(reqwest::Method::GET, path, query, None, None)
            .await
    }

    async fn products_page(&self, cursor: &str, limit: i64) -> Result<ProductPage, UpstreamError> {
        let query = format!(
            "cursor={}&limit={}",
            proto::percent_encode(cursor),
            limit.clamp(1, MAX_LIMIT)
        );
        let data = self.get("/catalog/products", &query).await?;
        let items: Vec<RemoteProduct> = data
            .get("items")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|p| serde_json::from_value(p.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(ProductPage {
            total: data.get("total").and_then(Value::as_i64).unwrap_or(0),
            items,
            includes_inactive: true,
            next_cursor: text(&data, "next_cursor"),
            has_more: data
                .get("has_more")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        })
    }

    fn order_of(&self, data: &Value) -> Result<RemoteOrder, UpstreamError> {
        let fulfillment = order_delivery(&self.api_secret, data)
            .map_err(|e| UpstreamError::Protocol(format!("delivery decryption failed: {e}")))?;
        Ok(RemoteOrder {
            order_id: 0,
            order_no: text(data, "order_no"),
            status: text(data, "status"),
            amount: text(data, "total"),
            refunded_amount: String::new(),
            currency: text(data, "currency"),
            fulfillment,
            refund_records: Vec::new(),
        })
    }
}

fn lines_json(lines: &[OrderLine]) -> Value {
    Value::Array(
        lines
            .iter()
            .map(|l| {
                json!({
                    "sku_id": l.sku_id,
                    "quantity": l.quantity,
                    "manual_form_data": l.manual_form_data,
                })
            })
            .collect(),
    )
}

#[async_trait]
impl UpstreamClient for ZebraStoreClient {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }

    async fn handshake(&self) -> Result<HandshakeInfo, UpstreamError> {
        let d = self.get("/handshake", "").await?;
        let site = d.get("site").cloned().unwrap_or(Value::Null);
        let account = d.get("account").cloned().unwrap_or(Value::Null);
        let latest = d
            .get("catalog")
            .map(|c| text(c, "latest_seq"))
            .filter(|s| !s.is_empty());
        Ok(HandshakeInfo {
            protocol: text(&d, "protocol"),
            version: text(&d, "version"),
            site_name: text(&site, "name"),
            site_url: text(&site, "url"),
            currency: text(&site, "currency"),
            // Wire feature names (spec §4) mapped to capability ids.
            features: std::iter::once(Capability::Categories)
                .chain(
                    d.get("features")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .filter_map(capability_of),
                )
                .map(|c| c.as_str().to_owned())
                .collect(),
            limits: d
                .get("limits")
                .and_then(Value::as_object)
                .cloned()
                .unwrap_or_default(),
            user_id: account.get("user_id").and_then(Value::as_i64).unwrap_or(0),
            balance: text(&account, "balance"),
            account_currency: text(&account, "currency"),
            member_level: account
                .get("member_level")
                .cloned()
                .filter(|v| !v.is_null()),
            change_head: latest,
            server_time: d
                .get("server_time")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc)),
        })
    }

    async fn ping(&self) -> Result<PingInfo, UpstreamError> {
        let h = self.handshake().await?;
        Ok(PingInfo {
            site_name: h.site_name,
            protocol_version: h.version,
            user_id: h.user_id,
            balance: h.balance,
            currency: h.account_currency,
            member_level: h.member_level,
        })
    }

    async fn list_categories(&self) -> Result<CategoryList, UpstreamError> {
        let d = self.get("/catalog/categories", "").await?;
        let categories: Vec<RemoteCategory> = d
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| serde_json::from_value(c.clone()).ok())
                    .collect()
            })
            .unwrap_or_default();
        Ok(CategoryList {
            supported: true,
            categories,
        })
    }

    /// Cursor listing; a page number without cursor walks the cursor pages (admin
    /// browsing by page).
    async fn list_products(&self, q: &ProductQuery) -> Result<ProductPage, UpstreamError> {
        let limit = if q.page_size > 0 { q.page_size } else { 50 };
        if let Some(cursor) = &q.cursor {
            return self.products_page(cursor, limit).await;
        }
        let mut cursor = String::new();
        let mut page = self.products_page(&cursor, limit).await?;
        for _ in 1..q.page.max(1) {
            if !page.has_more || page.next_cursor.is_empty() {
                return Ok(ProductPage {
                    items: Vec::new(),
                    has_more: false,
                    next_cursor: String::new(),
                    ..page
                });
            }
            cursor.clone_from(&page.next_cursor);
            page = self.products_page(&cursor, limit).await?;
        }
        Ok(page)
    }

    async fn get_product(&self, id: Id) -> Result<RemoteProduct, UpstreamError> {
        match self.get(&format!("/catalog/products/{id}"), "").await {
            Ok(v) => serde_json::from_value(v).map_err(|e| UpstreamError::Protocol(e.to_string())),
            Err(UpstreamError::Http { status: 404, .. }) => Err(UpstreamError::ProductDeleted),
            Err(e) => Err(e),
        }
    }

    /// Business refusals (4xx except 429) come back as `ok = false` with the protocol
    /// error code so the caller classifies them (UPS-11).
    async fn place_order(&self, req: &PlaceOrder) -> Result<CreateOrderResponse, UpstreamError> {
        let body = json!({
            "quote_id": req.quote_id,
            "items": lines_json(&req.lines),
            "downstream_order_no": req.downstream_order_no,
            "trace_id": req.trace_id,
            "callback": true,
        });
        let key = if req.idempotency_key.is_empty() {
            format!("order:{}", req.downstream_order_no)
        } else {
            req.idempotency_key.clone()
        };
        match self
            .call(
                reqwest::Method::POST,
                "/orders",
                "",
                Some(&body),
                Some(&key),
            )
            .await
        {
            Ok(d) => Ok(CreateOrderResponse {
                ok: true,
                order_id: 0,
                order_no: text(&d, "order_no"),
                status: text(&d, "status"),
                amount: text(&d, "total"),
                currency: text(&d, "currency"),
                ..CreateOrderResponse::default()
            }),
            Err(UpstreamError::Http {
                status,
                code,
                message,
            }) if (400..500).contains(&status) && status != 429 => Ok(CreateOrderResponse {
                ok: false,
                error_code: code,
                error_message: message,
                ..CreateOrderResponse::default()
            }),
            Err(e) => Err(e),
        }
    }

    fn addressable(&self, order: &OrderRef) -> bool {
        !order.no.is_empty()
    }

    async fn get_order(&self, order: &OrderRef) -> Result<RemoteOrder, UpstreamError> {
        let d = self
            .get(&format!("/orders/{}", proto::percent_encode(&order.no)), "")
            .await?;
        self.order_of(&d)
    }

    async fn cancel_order(&self, order: &OrderRef) -> Result<(), UpstreamError> {
        self.call(
            reqwest::Method::POST,
            &format!("/orders/{}/cancel", proto::percent_encode(&order.no)),
            "",
            Some(&json!({})),
            None,
        )
        .await
        .map(|_| ())
    }

    async fn download(&self, raw: &str) -> Result<Download, UpstreamError> {
        self.ctx.download(&self.base_url, raw).await
    }

    async fn changes(&self, since: Option<&str>, limit: i64) -> Result<ChangePage, UpstreamError> {
        let mut query = format!("limit={}", limit.clamp(1, MAX_LIMIT));
        if let Some(s) = since.filter(|s| !s.is_empty()) {
            query.push_str("&since=");
            query.push_str(&proto::percent_encode(s));
        }
        let d = self.get("/catalog/changes", &query).await?;
        let changes = d
            .get("changes")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|c| {
                        let kind = match text(c, "type").as_str() {
                            "product.upserted" => ChangeKind::ProductUpserted,
                            "product.deleted" => ChangeKind::ProductDeleted,
                            "sku.stock" => ChangeKind::SkuStock,
                            "sku.price" => ChangeKind::SkuPrice,
                            _ => return None,
                        };
                        Some(CatalogChange {
                            cursor: text(c, "seq"),
                            kind,
                            product_id: c.get("product_id").and_then(Value::as_i64)?,
                            sku_id: c.get("sku_id").and_then(Value::as_i64),
                            product: c
                                .get("data")
                                .and_then(|d| d.get("product"))
                                .and_then(|p| serde_json::from_value(p.clone()).ok()),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(ChangePage {
            changes,
            next_cursor: text(&d, "next_cursor"),
            has_more: d.get("has_more").and_then(Value::as_bool).unwrap_or(false),
        })
    }

    async fn change_head(&self) -> Result<String, UpstreamError> {
        self.handshake()
            .await?
            .change_head
            .ok_or(UpstreamError::Unsupported)
    }

    async fn quote(&self, lines: &[OrderLine]) -> Result<Quote, UpstreamError> {
        let d = self
            .call(
                reqwest::Method::POST,
                "/orders/quote",
                "",
                Some(&json!({"items": lines_json(lines)})),
                None,
            )
            .await?;
        Ok(Quote {
            quote_id: text(&d, "quote_id"),
            expires_at: d
                .get("expires_at")
                .and_then(Value::as_str)
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|t| t.with_timezone(&Utc)),
            currency: text(&d, "currency"),
            lines: d
                .get("items")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|l| QuoteLine {
                            sku_id: l.get("sku_id").and_then(Value::as_i64).unwrap_or(0),
                            quantity: l
                                .get("quantity")
                                .and_then(Value::as_i64)
                                .and_then(|q| i32::try_from(q).ok())
                                .unwrap_or(0),
                            unit_price: text(l, "unit_price"),
                            available: l.get("available").and_then(Value::as_bool) == Some(true),
                            reason: l.get("reason").and_then(Value::as_str).map(str::to_owned),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            total: text(&d, "total"),
            balance: text(&d, "balance"),
            sufficient_balance: d.get("sufficient_balance").and_then(Value::as_bool) == Some(true),
        })
    }

    async fn register_events(&self, url: &str) -> Result<(), UpstreamError> {
        self.call(
            reqwest::Method::PUT,
            "/webhooks",
            "",
            Some(&json!({"url": url, "events": SUBSCRIBED})),
            None,
        )
        .await
        .map(|_| ())
    }
}

/// Signed, SSRF-safe delivery of the events we push as a supplier (UPS-01): public
/// addresses only (resolver-level check), no redirects, 2xx = delivered.
#[derive(Clone)]
pub struct HttpEventSender {
    http: reqwest::Client,
    policy: AddressPolicy,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for HttpEventSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpEventSender")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl HttpEventSender {
    pub fn new(policy: AddressPolicy, clock: Arc<dyn Clock>) -> Self {
        Self {
            http: build_client(policy, EVENT_TIMEOUT),
            policy,
            clock,
        }
    }
}

#[async_trait]
impl EventSender for HttpEventSender {
    async fn send(
        &self,
        url: &str,
        api_key: &str,
        secret: &str,
        event_id: &str,
        body: &[u8],
    ) -> zs_domain::Result<()> {
        let url = check_url(self.policy, url)
            .map_err(|d| Error::forbidden(KEY_FORBIDDEN_ADDRESS).arg(d))?;
        let ts = self.clock.now().timestamp();
        let nonce = random_nonce();
        let signature = Signed {
            method: "POST",
            path: url.path(),
            query: url.query().unwrap_or_default(),
            timestamp: ts,
            nonce: &nonce,
            body,
        }
        .sign(secret);
        let res = self
            .http
            .post(url)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/json; charset=utf-8",
            )
            .header(proto::HEADER_KEY, api_key)
            .header(proto::HEADER_TIMESTAMP, ts.to_string())
            .header(proto::HEADER_NONCE, nonce)
            .header(proto::HEADER_SIGNATURE, signature)
            .header(proto::HEADER_EVENT_ID, event_id)
            .body(body.to_vec())
            .send()
            .await
            .map_err(|e| {
                if is_forbidden(&e) {
                    Error::forbidden(KEY_FORBIDDEN_ADDRESS).arg("resolved to a non-public address")
                } else {
                    Error::internal(e.without_url())
                }
            })?;
        let status = res.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        let text = read_limited(res, EVENT_RESPONSE_LIMIT)
            .await
            .unwrap_or_default();
        Err(Error::internal_msg(format!(
            "event endpoint returned {status}: {}",
            String::from_utf8_lossy(&text).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn features_map_to_capabilities() {
        assert_eq!(
            capability_of("changes"),
            Some(Capability::IncrementalChanges)
        );
        assert_eq!(capability_of("webhooks"), Some(Capability::PushEvents));
        assert_eq!(capability_of("future_thing"), None);
    }

    #[test]
    fn deliveries_are_decrypted() {
        let plain = json!({"type": "auto", "payload": "CARD-1", "delivery_data": {}, "delivered_at": "2026-09-25T06:00:00Z"});
        let sealed = proto::seal_delivery("s", plain.to_string().as_bytes())
            .unwrap_or_else(|_| unreachable!("encryption with a valid key cannot fail"));
        let order = json!({"items": [{"delivery": null}, {"delivery": sealed}]});
        let f = order_delivery("s", &order).ok().flatten();
        assert_eq!(f.as_ref().map(|f| f.payload.as_str()), Some("CARD-1"));
        assert_eq!(f.map(|f| f.status), Some("delivered".to_owned()));
        assert!(order_delivery("wrong", &order).is_err());
    }
}
