//! The `zebra-store` protocol v1 we serve as a supplier (`/api/v1/zs/*`, docs/protocol/
//! zebra-store-v1.md): handshake, cursor catalog, change feed (snapshot-diff job),
//! webhooks + signed event outbox, quotes, idempotent multi-item orders with
//! encrypted deliveries, and the connection code of the credential page.
//!
//! Ordering and pricing go through the order group ([`UpstreamOrdering`]); catalog
//! reads through [`SupplierCatalog`] / [`CatalogSnapshotSource`].

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use zs_domain::catalog::manual_form::validate_and_normalize;
use zs_domain::catalog::product::{FulfillmentType, JsonMap, validate_purchase_quantity};
use zs_domain::integration::credential::CredentialSecurityRepo;
use zs_domain::integration::downstream::OrderRefRepo;
use zs_domain::integration::procurement::LocalOrders;
use zs_domain::integration::protocol::RemoteFulfillment;
use zs_domain::integration::supplier::{
    LinePrice, PlaceLine, PlaceOutcome, PlaceUpstreamLines, PriceLine, SupplierCatalog,
    UpstreamOrderDetail, UpstreamOrderError, UpstreamOrdering,
};
use zs_domain::integration::zs::{
    self as proto, CatalogSnapshotSource, ErrorCode, EventSender, EventStatus, ItemRequest,
    OrderRequest, ProductSnapshot, QuotedLine, StoredQuote, Webhook, ZsFailure, ZsResult, ZsStore,
};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;

use super::credential::{Caller, CredentialService, ZsAuthFailure, ZsSignedRequest};
use super::supplier::{SupplierService, validate_callback_url};

/// Page size of the snapshot-diff scan.
const SNAPSHOT_PAGE: u64 = 200;
/// Default webhook subscriptions (spec §6).
const DEFAULT_EVENTS: [&str; 3] = ["catalog.changed", "order.*", "account.balance_low"];
/// Longest `downstream_order_no` / `trace_id` (`varchar(64)`).
const MAX_REF_LEN: usize = 64;

/// Payload of `zs:deliver_event`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DeliverEventJob {
    pub event_row_id: Id,
}

/// `POST /orders` body (parsed).
#[derive(Debug, Clone, Default)]
pub struct OrderBody {
    pub quote_id: Option<String>,
    pub items: Vec<ItemRequest>,
    pub downstream_order_no: String,
    pub trace_id: String,
    pub callback: bool,
}

/// Dependencies of [`ZsSupplier`].
#[derive(Clone)]
pub struct ZsSupplierDeps {
    pub supplier: SupplierService,
    pub credentials: CredentialService,
    pub catalog: Arc<dyn SupplierCatalog>,
    pub source: Arc<dyn CatalogSnapshotSource>,
    pub store: Arc<dyn ZsStore>,
    pub security: Arc<dyn CredentialSecurityRepo>,
    pub ordering: Arc<dyn UpstreamOrdering>,
    pub refs: Arc<dyn OrderRefRepo>,
    pub orders: Arc<dyn LocalOrders>,
    pub sender: Arc<dyn EventSender>,
    pub queue: Arc<dyn JobQueue>,
    pub clock: Arc<dyn Clock>,
    /// Test-only: webhook URLs may point at private addresses.
    pub allow_private_urls: bool,
}

impl std::fmt::Debug for ZsSupplierDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZsSupplierDeps")
    }
}

/// The served protocol.
#[derive(Clone)]
pub struct ZsSupplier {
    d: ZsSupplierDeps,
    snapshot_lock: Arc<tokio::sync::Mutex<()>>,
}

impl std::fmt::Debug for ZsSupplier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ZsSupplier")
    }
}

fn internal(error: &Error) -> ZsFailure {
    tracing::error!(%error, "zs request failed");
    ZsFailure::internal()
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Delivery plaintext of spec §7.
fn delivery_plain(f: &RemoteFulfillment) -> Value {
    json!({
        "type": f.kind,
        "payload": f.payload,
        "delivery_data": f.delivery_data.clone().unwrap_or_default(),
        "delivered_at": f.delivered_at.map(rfc3339),
    })
}

/// The order object of spec §7 (deliveries sealed with the credential secret).
pub fn order_json(
    detail: &UpstreamOrderDetail,
    downstream_no: &str,
    secret: &str,
) -> Result<Value> {
    let mut items = Vec::with_capacity(detail.lines.len());
    for l in &detail.lines {
        let delivery = match &l.fulfillment {
            Some(f) => {
                let plain = serde_json::to_vec(&delivery_plain(f))?;
                let sealed =
                    zs_shared::zs::seal_delivery(secret, &plain).map_err(Error::internal)?;
                serde_json::to_value(sealed)?
            }
            None => Value::Null,
        };
        items.push(json!({
            "sku_id": l.sku_id,
            "product_id": l.product_id,
            "quantity": l.quantity,
            "unit_price": l.unit_price.to_string(),
            "subtotal": l.total_price.to_string(),
            "status": l.status,
            "delivery": delivery,
        }));
    }
    Ok(json!({
        "order_no": detail.order_no,
        "downstream_order_no": downstream_no,
        "status": detail.status,
        "currency": detail.currency,
        "total": detail.total.to_string(),
        "items": items,
        "created_at": rfc3339(detail.created_at),
    }))
}

fn order_failure(e: UpstreamOrderError) -> ZsFailure {
    match e {
        UpstreamOrderError::InsufficientBalance => ZsFailure::new(
            ErrorCode::InsufficientBalance,
            "wallet balance is insufficient",
        ),
        UpstreamOrderError::InsufficientStock => {
            ZsFailure::new(ErrorCode::ItemUnavailable, "stock is insufficient")
        }
        UpstreamOrderError::ProductUnavailable | UpstreamOrderError::SkuUnavailable => {
            ZsFailure::new(ErrorCode::ItemUnavailable, "item is not available")
        }
        UpstreamOrderError::InvalidItem => ZsFailure::invalid("items: invalid order item"),
        UpstreamOrderError::ManualFormInvalid(m) => {
            ZsFailure::invalid(format!("items: manual_form_data invalid: {m}"))
        }
        UpstreamOrderError::NotFound => ZsFailure::not_found("order"),
        UpstreamOrderError::CancelNotAllowed => ZsFailure::new(
            ErrorCode::OrderNotCancelable,
            "order cannot be canceled in its current status",
        ),
        UpstreamOrderError::DuplicateDownstreamNo => ZsFailure::new(
            ErrorCode::IdempotencyConflict,
            "downstream_order_no already used",
        ),
        UpstreamOrderError::Internal(error) => internal(&error),
    }
}

/// Reason code of a line refused by the order group (spec §7 quote `reason`).
fn line_reason(e: &UpstreamOrderError) -> Option<&'static str> {
    match e {
        UpstreamOrderError::InsufficientStock => Some("out_of_stock"),
        UpstreamOrderError::ProductUnavailable
        | UpstreamOrderError::SkuUnavailable
        | UpstreamOrderError::InvalidItem
        | UpstreamOrderError::NotFound => Some("inactive"),
        UpstreamOrderError::ManualFormInvalid(_) => Some("form_invalid"),
        _ => None,
    }
}

/// One resolved request line.
#[derive(Debug, Clone)]
struct Line {
    sku_id: Id,
    product_id: Id,
    quantity: i32,
    manual_form_data: Option<JsonMap>,
    base_price: String,
    reason: Option<&'static str>,
    unit_price: Option<Amount>,
}

impl ZsSupplier {
    pub fn new(deps: ZsSupplierDeps) -> Self {
        Self {
            d: deps,
            snapshot_lock: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.d.clock.now()
    }

    // ------------------------------------------------------------------ auth

    /// Authenticates a signed request (spec §2).
    pub async fn authenticate(&self, req: ZsSignedRequest<'_>) -> ZsResult<Caller> {
        self.d
            .credentials
            .authenticate_zs(req)
            .await
            .map_err(|f| match f {
                ZsAuthFailure::Unauthorized => ZsFailure::unauthorized(),
                ZsAuthFailure::Forbidden => ZsFailure::new(
                    ErrorCode::Forbidden,
                    "credential is not approved, disabled, or its user is disabled",
                ),
                ZsAuthFailure::Internal => ZsFailure::internal(),
            })
    }

    async fn secret_of(&self, credential_id: Id) -> Result<Option<(String, String)>> {
        let Some(c) = self.d.credentials.find(credential_id).await? else {
            return Ok(None);
        };
        if !c.usable() {
            return Ok(None);
        }
        let secret = self.d.credentials.current_secret(&c).await?;
        Ok(Some((c.api_key, secret)))
    }

    // ------------------------------------------------------------------ handshake

    /// `GET /handshake` (spec §4); `origin` is the fallback site URL.
    pub async fn handshake(&self, caller: &Caller, origin: &str) -> ZsResult<Value> {
        let site = self.d.supplier.site().await.map_err(|e| internal(&e))?;
        let buyer = self
            .d
            .catalog
            .buyer(caller.user_id)
            .await
            .map_err(|e| internal(&e))?;
        let latest = self
            .d
            .store
            .change_bounds()
            .await
            .map_err(|e| internal(&e))?
            .map_or(0, |b| b.1);
        let url = if site.site_url.trim().is_empty() {
            origin.trim_end_matches('/').to_owned()
        } else {
            site.site_url.trim().trim_end_matches('/').to_owned()
        };
        let member_level = buyer
            .as_ref()
            .and_then(|b| b.member_level.as_ref())
            .map(|l| json!({"id": l.id, "name": l.name, "slug": l.slug, "icon": l.icon}));
        Ok(json!({
            "protocol": proto::PROTOCOL_ID,
            "version": proto::VERSION,
            "site": {"name": site.site_name, "url": url, "currency": site.currency},
            "features": proto::FEATURES,
            "limits": {
                "requests_per_minute": proto::REQUESTS_PER_MINUTE,
                "max_items_per_order": proto::MAX_ITEMS_PER_ORDER,
                "quote_ttl_seconds": proto::QUOTE_TTL_SECS,
                "changes_retention_days": proto::CHANGES_RETENTION_DAYS,
            },
            "account": {
                "user_id": caller.user_id,
                "balance": buyer.as_ref().map_or_else(|| "0.00".to_owned(), |b| b.balance.to_string()),
                "currency": site.currency,
                "member_level": member_level,
            },
            "catalog": {"latest_seq": latest},
            "server_time": rfc3339(self.now()),
        }))
    }

    // ------------------------------------------------------------------ catalog

    /// `GET /catalog/categories`.
    pub async fn categories(&self) -> ZsResult<Value> {
        let list = self
            .d
            .supplier
            .categories()
            .await
            .map_err(|e| internal(&e))?;
        Ok(Value::Array(
            list.into_iter()
                .map(|c| {
                    json!({
                        "id": c.id,
                        "parent_id": c.parent_id,
                        "slug": c.slug,
                        "name": c.name,
                        "icon": c.icon,
                        "sort_order": c.sort_order,
                    })
                })
                .collect(),
        ))
    }

    fn with_version(p: &zs_domain::integration::protocol::RemoteProduct) -> Value {
        let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
        if let Some(obj) = v.as_object_mut() {
            obj.insert("version".into(), Value::String(proto::product_version(p)));
        }
        v
    }

    /// `GET /catalog/products?cursor=&limit=` (by id; inactive products included).
    pub async fn products(
        &self,
        caller: &Caller,
        cursor: &str,
        limit: Option<u64>,
    ) -> ZsResult<Value> {
        let after: Id = if cursor.trim().is_empty() {
            0
        } else {
            cursor
                .trim()
                .parse()
                .map_err(|_| ZsFailure::invalid("cursor: invalid"))?
        };
        let limit = proto::list_limit(limit);
        let (mut items, total) = self
            .d
            .catalog
            .products_after(after, limit + 1)
            .await
            .map_err(|e| internal(&e))?;
        let has_more = items.len() as u64 > limit;
        items.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next = if has_more {
            items.last().map(|p| p.id.to_string()).unwrap_or_default()
        } else {
            String::new()
        };
        let projected = self
            .d
            .supplier
            .project(caller, &items)
            .await
            .map_err(|e| internal(&e))?;
        Ok(json!({
            "items": projected.iter().map(Self::with_version).collect::<Vec<_>>(),
            "next_cursor": next,
            "has_more": has_more,
            "total": total,
        }))
    }

    /// `GET /catalog/products/{id}` (delisted: `is_active=false`; deleted: 404).
    pub async fn product(&self, caller: &Caller, id: Id) -> ZsResult<Value> {
        match self.d.supplier.product(caller, id).await {
            Ok(Some(p)) => Ok(Self::with_version(&p)),
            Ok(None) => Err(ZsFailure::not_found("product")),
            Err(e) => Err(internal(&e)),
        }
    }

    /// `GET /catalog/changes?since=&limit=` (spec §5).
    pub async fn changes(&self, since: Option<&str>, limit: Option<u64>) -> ZsResult<Value> {
        let limit = proto::list_limit(limit);
        let bounds = self
            .d
            .store
            .change_bounds()
            .await
            .map_err(|e| internal(&e))?;
        let since = match since.map(str::trim).filter(|s| !s.is_empty()) {
            Some(raw) => {
                let v: i64 = raw
                    .parse()
                    .ok()
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| ZsFailure::invalid("since: invalid cursor"))?;
                if proto::cursor_expired(v, bounds.map(|b| b.0)) {
                    return Err(ZsFailure::new(
                        ErrorCode::CursorExpired,
                        "cursor is older than the retention window; resync with /catalog/products",
                    ));
                }
                v
            }
            None => bounds.map_or(0, |b| b.0 - 1).max(0),
        };
        let mut rows = self
            .d
            .store
            .changes_after(since, limit + 1)
            .await
            .map_err(|e| internal(&e))?;
        let has_more = rows.len() as u64 > limit;
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let next = rows.last().map_or(since, |r| r.seq);
        Ok(json!({
            "changes": rows,
            "next_cursor": next.to_string(),
            "has_more": has_more,
        }))
    }

    // ------------------------------------------------------------------ webhooks

    fn webhook_json(h: &Webhook) -> Value {
        json!({
            "url": h.url,
            "events": h.events,
            "balance_low_threshold": h.balance_low_threshold.map(|a| a.to_string()),
            "created_at": rfc3339(h.created_at),
            "updated_at": rfc3339(h.updated_at),
        })
    }

    /// `PUT /webhooks`.
    pub async fn put_webhook(
        &self,
        caller: &Caller,
        url: &str,
        events: &[String],
        threshold: Option<Amount>,
    ) -> ZsResult<Value> {
        let url = url.trim();
        if !(url.starts_with("https://") || url.starts_with("http://")) || url.len() > 500 {
            return Err(ZsFailure::invalid("url: must be an absolute http(s) URL"));
        }
        if !self.d.allow_private_urls
            && let Err(reason) = validate_callback_url(url)
        {
            return Err(ZsFailure::invalid(format!("url: {reason}")));
        }
        if let Some(bad) = events.iter().find(|e| !proto::valid_event_pattern(e)) {
            return Err(ZsFailure::invalid(format!("events: unknown event {bad}")));
        }
        if threshold.is_some_and(|t| t.is_negative()) {
            return Err(ZsFailure::invalid(
                "balance_low_threshold: must not be negative",
            ));
        }
        let events: Vec<String> = if events.is_empty() {
            DEFAULT_EVENTS.iter().map(|e| (*e).to_owned()).collect()
        } else {
            events.iter().map(|e| e.trim().to_owned()).collect()
        };
        let now = self.now();
        let hook = Webhook {
            credential_id: caller.credential_id,
            url: url.to_owned(),
            events,
            balance_low_threshold: threshold,
            last_balance_low_at: None,
            created_at: now,
            updated_at: now,
        };
        let saved = self
            .d
            .store
            .put_webhook(&hook, now)
            .await
            .map_err(|e| internal(&e))?;
        Ok(Self::webhook_json(&saved))
    }

    /// `GET /webhooks` (`null` when none).
    pub async fn get_webhook(&self, caller: &Caller) -> ZsResult<Value> {
        Ok(self
            .d
            .store
            .webhook(caller.credential_id)
            .await
            .map_err(|e| internal(&e))?
            .map_or(Value::Null, |h| Self::webhook_json(&h)))
    }

    /// `DELETE /webhooks`.
    pub async fn delete_webhook(&self, caller: &Caller) -> ZsResult<Value> {
        let deleted = self
            .d
            .store
            .delete_webhook(caller.credential_id)
            .await
            .map_err(|e| internal(&e))?;
        Ok(json!({"deleted": deleted}))
    }

    // ------------------------------------------------------------------ quote

    /// Resolves request lines: availability (active, quantity limits, projected stock,
    /// manual form) then prices through the order group.
    async fn resolve_lines(&self, caller: &Caller, items: &[ItemRequest]) -> ZsResult<Vec<Line>> {
        let mut lines = Vec::with_capacity(items.len());
        for item in items {
            let found = self
                .d
                .catalog
                .product_of_sku(item.sku_id)
                .await
                .map_err(|e| internal(&e))?;
            let mut line = Line {
                sku_id: item.sku_id,
                product_id: 0,
                quantity: item.quantity,
                manual_form_data: item.manual_form_data.clone(),
                base_price: String::new(),
                reason: None,
                unit_price: None,
            };
            let Some((product, sku_active)) = found else {
                line.reason = Some("inactive");
                lines.push(line);
                continue;
            };
            line.product_id = product.id;
            let projected = self
                .d
                .supplier
                .project(caller, std::slice::from_ref(&product))
                .await
                .map_err(|e| internal(&e))?;
            let sku = projected
                .first()
                .and_then(|p| p.skus.iter().find(|s| s.id == item.sku_id));
            if let Some(s) = sku {
                line.base_price.clone_from(&s.price_amount);
            }
            line.reason = if !sku_active || !product.is_active || sku.is_none() {
                Some("inactive")
            } else if validate_purchase_quantity(
                product.min_purchase_quantity,
                product.max_purchase_quantity,
                item.quantity,
            )
            .is_err()
            {
                Some("quantity_limit")
            } else if sku.is_some_and(|s| {
                s.stock_quantity >= 0 && s.stock_quantity < i64::from(item.quantity)
            }) {
                Some("out_of_stock")
            } else if (product.fulfillment_type == FulfillmentType::Manual
                || (product.fulfillment_type == FulfillmentType::Upstream
                    && !product.manual_form_schema.is_empty()))
                && validate_and_normalize(
                    &product.manual_form_schema,
                    &item.manual_form_data.clone().unwrap_or_default(),
                )
                .is_err()
            {
                Some("form_invalid")
            } else {
                None
            };
            lines.push(line);
        }
        self.price(caller, &mut lines).await?;
        Ok(lines)
    }

    /// Prices the available lines together (cross-line wholesale); when the whole
    /// set is refused, line by line to attribute the reasons.
    async fn price(&self, caller: &Caller, lines: &mut [Line]) -> ZsResult<()> {
        let request = |ls: &[&Line]| -> Vec<PriceLine> {
            ls.iter()
                .map(|l| PriceLine {
                    product_id: l.product_id,
                    sku_id: l.sku_id,
                    quantity: l.quantity,
                })
                .collect()
        };
        let apply = |lines: &mut [Line], prices: &[LinePrice]| {
            let by_key: HashMap<(Id, Id), Amount> = prices
                .iter()
                .map(|p| ((p.product_id, p.sku_id), p.unit_price))
                .collect();
            for l in lines.iter_mut().filter(|l| l.reason.is_none()) {
                l.unit_price = by_key.get(&(l.product_id, l.sku_id)).copied();
            }
        };
        let available: Vec<&Line> = lines.iter().filter(|l| l.reason.is_none()).collect();
        if available.is_empty() {
            return Ok(());
        }
        match self
            .d
            .ordering
            .price_lines(caller.user_id, &request(&available))
            .await
        {
            Ok(prices) => {
                apply(lines, &prices);
                return Ok(());
            }
            Err(UpstreamOrderError::Internal(e)) => return Err(internal(&e)),
            Err(_) => {}
        }
        for l in lines.iter_mut().filter(|l| l.reason.is_none()) {
            match self
                .d
                .ordering
                .price_lines(caller.user_id, &request(&[&*l]))
                .await
            {
                Ok(prices) => l.unit_price = prices.first().map(|p| p.unit_price),
                Err(UpstreamOrderError::Internal(e)) => return Err(internal(&e)),
                Err(e) => l.reason = Some(line_reason(&e).unwrap_or("inactive")),
            }
        }
        Ok(())
    }

    /// `POST /orders/quote` (spec §7): locks unit prices until `expires_at`; no stock
    /// is reserved.
    pub async fn quote(&self, caller: &Caller, items: &[ItemRequest]) -> ZsResult<Value> {
        proto::validate_items(items)?;
        let lines = self.resolve_lines(caller, items).await?;
        let site = self.d.supplier.site().await.map_err(|e| internal(&e))?;
        let balance = self
            .d
            .catalog
            .buyer(caller.user_id)
            .await
            .map_err(|e| internal(&e))?
            .map_or(Amount::ZERO, |b| b.balance);
        let mut total = Amount::ZERO;
        let mut quoted = Vec::new();
        let mut out = Vec::new();
        for l in &lines {
            let qty = i64::from(l.quantity);
            let (available, unit) = match (l.reason, l.unit_price) {
                (None, Some(unit)) => (true, unit),
                _ => (false, l.base_price.parse().unwrap_or(Amount::ZERO)),
            };
            let subtotal = unit * qty;
            if available {
                total += subtotal;
                quoted.push(QuotedLine {
                    sku_id: l.sku_id,
                    product_id: l.product_id,
                    quantity: l.quantity,
                    unit_price: unit,
                });
            }
            out.push(json!({
                "sku_id": l.sku_id,
                "quantity": l.quantity,
                "unit_price": unit.to_string(),
                "subtotal": subtotal.to_string(),
                "available": available,
                "reason": if available { None } else { Some(l.reason.unwrap_or("inactive")) },
            }));
        }
        let expires_at = self.now() + Duration::seconds(proto::QUOTE_TTL_SECS);
        let quote_id = format!("q_{}", uuid::Uuid::new_v4().simple());
        self.d
            .store
            .save_quote(
                &StoredQuote {
                    quote_id: quote_id.clone(),
                    credential_id: caller.credential_id,
                    lines: quoted,
                    total,
                    currency: site.currency.clone(),
                    expires_at,
                },
                self.now(),
            )
            .await
            .map_err(|e| internal(&e))?;
        Ok(json!({
            "quote_id": quote_id,
            "expires_at": rfc3339(expires_at),
            "currency": site.currency,
            "items": out,
            "total": total.to_string(),
            "balance": balance.to_string(),
            "sufficient_balance": balance >= total,
        }))
    }

    // ------------------------------------------------------------------ orders

    async fn order_object(&self, caller: &Caller, order_id: Id) -> ZsResult<Value> {
        let detail = self
            .d
            .ordering
            .detail(caller.user_id, order_id)
            .await
            .map_err(|e| internal(&e))?
            .ok_or_else(|| ZsFailure::not_found("order"))?;
        let downstream = self
            .d
            .refs
            .get_by_order(order_id)
            .await
            .map_err(|e| internal(&e))?
            .map(|r| r.downstream_order_no)
            .unwrap_or_default();
        let (_, secret) = self
            .secret_of(caller.credential_id)
            .await
            .map_err(|e| internal(&e))?
            .ok_or_else(ZsFailure::unauthorized)?;
        order_json(&detail, &downstream, &secret).map_err(|e| internal(&e))
    }

    async fn existing_by_downstream(&self, caller: &Caller, no: &str) -> ZsResult<Option<Value>> {
        if no.is_empty() {
            return Ok(None);
        }
        let found = self
            .d
            .refs
            .find_by_downstream_no(caller.credential_id, no)
            .await
            .map_err(|e| internal(&e))?;
        match found {
            Some(r) => Ok(Some(self.order_object(caller, r.order_id).await?)),
            None => Ok(None),
        }
    }

    /// `POST /orders` (spec §7): `Idempotency-Key` (24 h, body hash checked), the
    /// downstream order number is idempotent too; the quote locks prices; paid from
    /// the buyer's wallet as one parent order with a child per line.
    pub async fn create_order(
        &self,
        caller: &Caller,
        key: &str,
        body_hash: &str,
        body: &OrderBody,
        client_ip: &str,
    ) -> ZsResult<Value> {
        if !proto::valid_idempotency_key(key) {
            return Err(ZsFailure::invalid(
                "Idempotency-Key: required, 1-64 visible characters",
            ));
        }
        proto::validate_items(&body.items)?;
        let downstream = body.downstream_order_no.trim().to_owned();
        if downstream.len() > MAX_REF_LEN || body.trace_id.len() > MAX_REF_LEN {
            return Err(ZsFailure::invalid(
                "downstream_order_no / trace_id: at most 64 characters",
            ));
        }
        let now = self.now();
        let reservation = OrderRequest {
            credential_id: caller.credential_id,
            idempotency_key: key.to_owned(),
            request_hash: body_hash.to_owned(),
            order_id: 0,
            order_no: String::new(),
            callback: body.callback,
            created_at: now,
        };
        if let Some(existing) = self
            .d
            .store
            .reserve_request(&reservation, now)
            .await
            .map_err(|e| internal(&e))?
        {
            if existing.request_hash != body_hash {
                return Err(ZsFailure::new(
                    ErrorCode::IdempotencyConflict,
                    "Idempotency-Key was used with a different request body",
                ));
            }
            if existing.order_id > 0 {
                return self.order_object(caller, existing.order_id).await;
            }
            return Err(ZsFailure::new(
                ErrorCode::RequestInProgress,
                "a request with this Idempotency-Key is still being processed",
            ));
        }
        let result = self.place(caller, key, body, &downstream, client_ip).await;
        if result.is_err()
            && let Err(error) = self
                .d
                .store
                .release_request(caller.credential_id, key)
                .await
        {
            tracing::warn!(%error, "release idempotency key failed");
        }
        result
    }

    async fn place(
        &self,
        caller: &Caller,
        key: &str,
        body: &OrderBody,
        downstream: &str,
        client_ip: &str,
    ) -> ZsResult<Value> {
        if let Some(existing) = self.existing_by_downstream(caller, downstream).await? {
            return Ok(existing);
        }
        let mut caps: HashMap<Id, Amount> = HashMap::new();
        let mut estimate = None;
        if let Some(quote_id) = body.quote_id.as_deref().filter(|q| !q.trim().is_empty()) {
            let quote = self
                .d
                .store
                .quote(quote_id.trim())
                .await
                .map_err(|e| internal(&e))?
                .filter(|q| q.credential_id == caller.credential_id)
                .ok_or_else(|| ZsFailure::new(ErrorCode::QuoteMismatch, "quote not found"))?;
            if quote.expires_at <= self.now() {
                return Err(ZsFailure::new(ErrorCode::QuoteExpired, "quote has expired"));
            }
            if !proto::quote_matches(&quote, &body.items) {
                return Err(ZsFailure::new(
                    ErrorCode::QuoteMismatch,
                    "items differ from the quote",
                ));
            }
            caps = quote
                .lines
                .iter()
                .map(|l| (l.sku_id, l.unit_price))
                .collect();
            estimate = Some(quote.total);
        }
        let lines = self.resolve_lines(caller, &body.items).await?;
        if let Some(bad) = lines.iter().find(|l| l.reason.is_some()) {
            return Err(ZsFailure::new(
                ErrorCode::ItemUnavailable,
                format!(
                    "sku {} is not available: {}",
                    bad.sku_id,
                    bad.reason.unwrap_or("inactive")
                ),
            ));
        }
        let current: Amount = lines
            .iter()
            .map(|l| l.unit_price.unwrap_or(Amount::ZERO) * i64::from(l.quantity))
            .sum();
        let total = estimate.map_or(current, |q| q.min(current));
        let balance = self
            .d
            .catalog
            .buyer(caller.user_id)
            .await
            .map_err(|e| internal(&e))?
            .map_or(Amount::ZERO, |b| b.balance);
        if balance < total {
            return Err(ZsFailure::new(
                ErrorCode::InsufficientBalance,
                "wallet balance is insufficient",
            ));
        }
        let req = PlaceUpstreamLines {
            user_id: caller.user_id,
            credential_id: caller.credential_id,
            lines: lines
                .iter()
                .map(|l| PlaceLine {
                    product_id: l.product_id,
                    sku_id: l.sku_id,
                    quantity: l.quantity,
                    manual_form_data: l.manual_form_data.clone(),
                    price_cap: caps.get(&l.sku_id).copied(),
                })
                .collect(),
            downstream_order_no: downstream.to_owned(),
            trace_id: body.trace_id.clone(),
            client_ip: client_ip.to_owned(),
        };
        match self.d.ordering.place_lines(&req).await {
            Ok(PlaceOutcome::Placed(s)) => {
                self.d
                    .store
                    .complete_request(caller.credential_id, key, s.order_id, &s.order_no)
                    .await
                    .map_err(|e| internal(&e))?;
                self.check_balance_low(caller).await;
                self.order_object(caller, s.order_id).await
            }
            Ok(PlaceOutcome::PaymentFailed { message, .. }) => Err(ZsFailure::new(
                ErrorCode::InsufficientBalance,
                format!("wallet payment failed: {message}"),
            )),
            Err(UpstreamOrderError::DuplicateDownstreamNo) => self
                .existing_by_downstream(caller, downstream)
                .await?
                .ok_or_else(|| order_failure(UpstreamOrderError::DuplicateDownstreamNo)),
            Err(e) => Err(order_failure(e)),
        }
    }

    async fn order_id_of(&self, order_no: &str) -> ZsResult<Id> {
        self.d
            .store
            .order_id_by_no(order_no.trim())
            .await
            .map_err(|e| internal(&e))?
            .ok_or_else(|| ZsFailure::not_found("order"))
    }

    /// `GET /orders/{order_no}` (own orders only).
    pub async fn get_order(&self, caller: &Caller, order_no: &str) -> ZsResult<Value> {
        let id = self.order_id_of(order_no).await?;
        self.order_object(caller, id).await
    }

    /// `GET /orders?downstream_order_no=` or the credential's orders by cursor.
    pub async fn list_orders(
        &self,
        caller: &Caller,
        downstream_no: &str,
        cursor: &str,
        limit: Option<u64>,
    ) -> ZsResult<Value> {
        if !downstream_no.trim().is_empty() {
            let items: Vec<Value> = self
                .existing_by_downstream(caller, downstream_no.trim())
                .await?
                .into_iter()
                .collect();
            return Ok(json!({"items": items, "next_cursor": "", "has_more": false}));
        }
        let before = if cursor.trim().is_empty() {
            None
        } else {
            Some(
                cursor
                    .trim()
                    .parse::<Id>()
                    .map_err(|_| ZsFailure::invalid("cursor: invalid"))?,
            )
        };
        let limit = proto::list_limit(limit);
        let mut rows = self
            .d
            .store
            .list_requests(caller.credential_id, before, limit + 1)
            .await
            .map_err(|e| internal(&e))?;
        let has_more = rows.len() as u64 > limit;
        rows.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
        let mut items = Vec::with_capacity(rows.len());
        for r in &rows {
            match self.order_object(caller, r.order_id).await {
                Ok(v) => items.push(v),
                Err(f) if f.code == ErrorCode::NotFound => {}
                Err(f) => return Err(f),
            }
        }
        let next = if has_more {
            rows.last()
                .map(|r| r.order_id.to_string())
                .unwrap_or_default()
        } else {
            String::new()
        };
        Ok(json!({"items": items, "next_cursor": next, "has_more": has_more}))
    }

    /// `POST /orders/{order_no}/cancel` (unpaid orders only; paid API orders are
    /// refunded by the supplier's admin).
    pub async fn cancel_order(&self, caller: &Caller, order_no: &str) -> ZsResult<Value> {
        let id = self.order_id_of(order_no).await?;
        self.d
            .ordering
            .cancel(caller.user_id, id)
            .await
            .map_err(order_failure)?;
        self.order_object(caller, id).await
    }

    // ------------------------------------------------------------------ events

    /// Queues an event for the credential's webhook when subscribed.
    pub async fn emit(&self, credential_id: Id, kind: &str, data: Value) -> Result<()> {
        let Some(hook) = self.d.store.webhook(credential_id).await? else {
            return Ok(());
        };
        if !proto::subscribed(&hook.events, kind) {
            return Ok(());
        }
        let event_id = format!("evt_{}", uuid::Uuid::new_v4().simple());
        let now = self.now();
        let body = json!({
            "id": event_id,
            "type": kind,
            "created_at": rfc3339(now),
            "data": data,
        });
        let row = self
            .d
            .store
            .create_event(credential_id, &event_id, kind, &body, now)
            .await?;
        self.enqueue_delivery(row.id, None).await;
        Ok(())
    }

    async fn enqueue_delivery(&self, row_id: Id, at: Option<DateTime<Utc>>) {
        let job = NewJob::new(
            kinds::ZS_DELIVER_EVENT,
            DeliverEventJob {
                event_row_id: row_id,
            },
        )
        .map(|j| j.attempts(1));
        let r = match (job, at) {
            (Ok(j), Some(at)) => self.d.queue.enqueue(j.at(at)).await,
            (Ok(j), None) => self.d.queue.enqueue(j).await,
            (Err(e), _) => Err(e),
        };
        if let Err(error) = r {
            tracing::warn!(%error, event_row_id = row_id, "enqueue zs event failed");
        }
    }

    /// `zs:deliver_event`: signed POST to the webhook; failures are retried after
    /// 30 s / 2 min / 10 min / 1 h / 6 h, then the event is marked failed (spec §6).
    pub async fn deliver(&self, row_id: Id) -> Result<()> {
        let Some(event) = self.d.store.event(row_id).await? else {
            return Ok(());
        };
        if event.status != EventStatus::Pending {
            return Ok(());
        }
        let now = self.now();
        let hook = self.d.store.webhook(event.credential_id).await?;
        let secret = self.secret_of(event.credential_id).await?;
        let (Some(hook), Some((api_key, secret))) = (hook, secret) else {
            self.d
                .store
                .update_event(
                    row_id,
                    EventStatus::Failed,
                    event.attempts,
                    None,
                    "webhook or credential no longer available",
                    now,
                )
                .await?;
            return Ok(());
        };
        let body = serde_json::to_vec(&event.body)?;
        let attempts = event.attempts + 1;
        match self
            .d
            .sender
            .send(&hook.url, &api_key, &secret, &event.event_id, &body)
            .await
        {
            Ok(()) => {
                self.d
                    .store
                    .update_event(row_id, EventStatus::Sent, attempts, None, "", now)
                    .await
            }
            Err(error) => {
                let message = error.to_string();
                tracing::warn!(%error, event_id = %event.event_id, attempts, "zs event delivery failed");
                match proto::webhook_retry_delay(attempts) {
                    Some(delay) => {
                        let next = now + delay;
                        self.d
                            .store
                            .update_event(
                                row_id,
                                EventStatus::Pending,
                                attempts,
                                Some(next),
                                &message,
                                now,
                            )
                            .await?;
                        self.enqueue_delivery(row_id, Some(next)).await;
                        Ok(())
                    }
                    None => {
                        self.d
                            .store
                            .update_event(
                                row_id,
                                EventStatus::Failed,
                                attempts,
                                None,
                                &message,
                                now,
                            )
                            .await
                    }
                }
            }
        }
    }

    /// Order status hook: `order.*` event for orders placed through this protocol
    /// (children resolve their parent).
    pub async fn order_changed(&self, order_id: Id) -> Result<()> {
        let root = match self.d.orders.get(order_id).await? {
            Some(o) => o.parent_id.unwrap_or(o.id),
            None => return Ok(()),
        };
        let Some(req) = self.d.store.request_by_order(root).await? else {
            return Ok(());
        };
        if !req.callback {
            return Ok(());
        }
        let Some(credential) = self.d.credentials.find(req.credential_id).await? else {
            return Ok(());
        };
        let Some(detail) = self.d.ordering.detail(credential.user_id, root).await? else {
            return Ok(());
        };
        let Some((_, secret)) = self.secret_of(credential.id).await? else {
            return Ok(());
        };
        let downstream = self
            .d
            .refs
            .get_by_order(root)
            .await?
            .map(|r| r.downstream_order_no)
            .unwrap_or_default();
        let data = order_json(&detail, &downstream, &secret)?;
        self.emit(credential.id, proto::order_event_type(&detail.status), data)
            .await
    }

    /// `account.balance_low` when the buyer's balance fell below the webhook
    /// threshold (at most once per 24 h).
    async fn check_balance_low(&self, caller: &Caller) {
        let r: Result<()> = async {
            let Some(hook) = self.d.store.webhook(caller.credential_id).await? else {
                return Ok(());
            };
            let Some(threshold) = hook.balance_low_threshold else {
                return Ok(());
            };
            let now = self.now();
            if hook
                .last_balance_low_at
                .is_some_and(|t| now - t < Duration::hours(proto::BALANCE_LOW_REPEAT_HOURS))
            {
                return Ok(());
            }
            let balance = self
                .d
                .catalog
                .buyer(caller.user_id)
                .await?
                .map_or(Amount::ZERO, |b| b.balance);
            if balance >= threshold {
                return Ok(());
            }
            self.d
                .store
                .mark_balance_low(caller.credential_id, now)
                .await?;
            self.emit(
                caller.credential_id,
                "account.balance_low",
                json!({"balance": balance.to_string(), "threshold": threshold.to_string()}),
            )
            .await
        }
        .await;
        if let Err(error) = r {
            tracing::warn!(%error, "balance_low check failed");
        }
    }

    // ------------------------------------------------------------------ change feed

    /// `zs:catalog_snapshot` (every 15 s): diffs the projected catalog against the
    /// stored snapshots, appends changes, notifies webhooks, purges expired rows.
    pub async fn snapshot_tick(&self) -> Result<Option<i64>> {
        let Ok(_guard) = self.snapshot_lock.try_lock() else {
            return Ok(None);
        };
        let old: HashMap<Id, ProductSnapshot> = self
            .d
            .store
            .snapshots()
            .await?
            .into_iter()
            .map(|s| (s.product_id, s))
            .collect();
        let mut changes = Vec::new();
        let mut upserts = Vec::new();
        let mut seen = HashSet::new();
        let mut after = 0;
        loop {
            let page = self.d.source.page(after, SNAPSHOT_PAGE).await?;
            for p in &page {
                let snap = proto::snapshot_of(p);
                let before = old.get(&p.id);
                changes.extend(proto::diff_product(before, &snap, p));
                if before != Some(&snap) {
                    upserts.push(snap);
                }
                seen.insert(p.id);
                after = after.max(p.id);
            }
            if (page.len() as u64) < SNAPSHOT_PAGE {
                break;
            }
        }
        let mut deletes: Vec<Id> = old
            .keys()
            .filter(|id| !seen.contains(id))
            .copied()
            .collect();
        deletes.sort_unstable();
        changes.extend(deletes.iter().map(|id| proto::deleted_change(*id)));
        let now = self.now();
        let latest = if changes.is_empty() && upserts.is_empty() {
            None
        } else {
            self.d
                .store
                .apply_snapshot_diff(&changes, &upserts, &deletes, now)
                .await?
        };
        if let Some(seq) = latest {
            for hook in self.d.store.webhooks().await? {
                if let Err(error) = self
                    .emit(
                        hook.credential_id,
                        "catalog.changed",
                        json!({"latest_seq": seq}),
                    )
                    .await
                {
                    tracing::warn!(%error, "emit catalog.changed failed");
                }
            }
        }
        self.d
            .store
            .purge_changes(now - Duration::days(proto::CHANGES_RETENTION_DAYS))
            .await?;
        self.d
            .security
            .purge_nonces(now - Duration::seconds(zs_shared::zs::NONCE_TTL_SECS))
            .await?;
        self.d.store.purge_expired(now).await?;
        Ok(latest)
    }

    // ------------------------------------------------------------------ credential page

    /// Connection code for the owner (spec §3): generates a new secret as a pending
    /// rotation and embeds it with the site URL (brand URL, else `origin`), key and
    /// name. Returns `(code, rotation_expires_at)`.
    pub async fn connection_code(
        &self,
        user_id: Id,
        origin: &str,
    ) -> Result<(String, DateTime<Utc>)> {
        let (credential, secret, expires_at) = self.d.credentials.rotate_mine(user_id).await?;
        let site = self.d.supplier.site().await?;
        let url = if site.site_url.trim().is_empty() {
            origin.trim().trim_end_matches('/').to_owned()
        } else {
            site.site_url.trim().trim_end_matches('/').to_owned()
        };
        let code = zs_shared::zs::encode_connection_code(&zs_shared::zs::ConnectionCode {
            v: 1,
            url,
            key: credential.api_key,
            secret,
            name: site.site_name,
        });
        Ok((code, expires_at))
    }
}
