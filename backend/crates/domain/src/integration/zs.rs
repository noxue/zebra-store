//! The `zebra-store` protocol v1 as served by us (`/api/v1/zs/*`, docs/protocol/
//! zebra-store-v1.md): error codes, limits, catalog snapshot diffing (change feed),
//! quote / idempotency rules and the storage ports of the supplier side.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use zs_shared::money::Amount;

use super::protocol::RemoteProduct;
use crate::catalog::product::JsonMap;
use crate::{Id, Result};

/// Protocol id (`site_connections.protocol`).
pub const PROTOCOL_ID: &str = "zebra-store";
/// Protocol version reported by the handshake.
pub const VERSION: &str = "1.0";
/// Features we serve (spec §4).
pub const FEATURES: [&str; 6] = [
    "changes",
    "webhooks",
    "quote",
    "multi_item",
    "idempotency",
    "encrypted_delivery",
];
/// Requests per minute per API key (spec §4 `limits`).
pub const REQUESTS_PER_MINUTE: i64 = 120;
/// Lines per order / quote (spec §4 `limits`).
pub const MAX_ITEMS_PER_ORDER: usize = 20;
/// Quote lifetime (spec §7).
pub const QUOTE_TTL_SECS: i64 = 600;
/// Change-feed retention (spec §5).
pub const CHANGES_RETENTION_DAYS: i64 = 7;
/// Default / largest list `limit` (spec §1).
pub const LIST_DEFAULT: u64 = 50;
pub const LIST_MAX: u64 = 200;
/// Largest request body (spec §1: 1 MiB).
pub const MAX_BODY_BYTES: usize = 1 << 20;
/// Idempotency keys are remembered this long (spec §7).
pub const IDEMPOTENCY_TTL_HOURS: i64 = 24;
/// Longest idempotency key (spec §7).
pub const MAX_IDEMPOTENCY_KEY_LEN: usize = 64;
/// Webhook retry delays after the 1st..5th failure (spec §6: 30 s / 2 min / 10 min / 1 h / 6 h).
pub const WEBHOOK_RETRY_SECS: [i64; 5] = [30, 120, 600, 3600, 21600];
/// `account.balance_low` is sent at most once per this window (spec §6).
pub const BALANCE_LOW_REPEAT_HOURS: i64 = 24;
/// Snapshot-diff interval of the change feed producer (spec §5).
pub const SNAPSHOT_INTERVAL_SECS: u64 = 15;
/// Event types a webhook may subscribe to.
pub const EVENT_TYPES: [&str; 5] = [
    "catalog.changed",
    "order.status_changed",
    "order.delivered",
    "order.refunded",
    "account.balance_low",
];
/// Protocols this site serves as a supplier (user credential page).
pub const SERVED_PROTOCOLS: [&str; 2] = ["dujiao-next", PROTOCOL_ID];

/// Protocol error codes (spec §8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    InvalidRequest,
    Unauthorized,
    Forbidden,
    NotFound,
    QuoteExpired,
    OrderNotCancelable,
    /// Same idempotency key while the first request is still running.
    RequestInProgress,
    CursorExpired,
    InsufficientBalance,
    QuoteMismatch,
    IdempotencyConflict,
    ItemUnavailable,
    RateLimited,
    Internal,
    Unavailable,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InvalidRequest => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::QuoteExpired => "quote_expired",
            Self::OrderNotCancelable => "order_not_cancelable",
            Self::RequestInProgress => "request_in_progress",
            Self::CursorExpired => "cursor_expired",
            Self::InsufficientBalance => "insufficient_balance",
            Self::QuoteMismatch => "quote_mismatch",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::ItemUnavailable => "item_unavailable",
            Self::RateLimited => "rate_limited",
            Self::Internal => "internal_error",
            Self::Unavailable => "unavailable",
        }
    }

    pub fn http_status(self) -> u16 {
        match self {
            Self::InvalidRequest => 400,
            Self::Unauthorized => 401,
            Self::InsufficientBalance => 402,
            Self::Forbidden => 403,
            Self::NotFound => 404,
            Self::QuoteExpired | Self::OrderNotCancelable | Self::RequestInProgress => 409,
            Self::CursorExpired => 410,
            Self::QuoteMismatch | Self::IdempotencyConflict | Self::ItemUnavailable => 422,
            Self::RateLimited => 429,
            Self::Internal => 500,
            Self::Unavailable => 503,
        }
    }

    pub fn retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::Internal | Self::Unavailable | Self::RequestInProgress
        )
    }
}

/// A protocol failure with its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZsFailure {
    pub code: ErrorCode,
    pub message: String,
}

impl ZsFailure {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InvalidRequest, message)
    }

    pub fn unauthorized() -> Self {
        Self::new(ErrorCode::Unauthorized, "unauthorized")
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(ErrorCode::NotFound, format!("{what} not found"))
    }

    pub fn internal() -> Self {
        Self::new(ErrorCode::Internal, "internal error")
    }
}

pub type ZsResult<T> = std::result::Result<T, ZsFailure>;

/// Clamped list limit.
pub fn list_limit(raw: Option<u64>) -> u64 {
    raw.filter(|v| *v > 0).unwrap_or(LIST_DEFAULT).min(LIST_MAX)
}

/// Valid `Idempotency-Key`: 1–64 visible ASCII characters.
pub fn valid_idempotency_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= MAX_IDEMPOTENCY_KEY_LEN
        && key.bytes().all(|b| b.is_ascii_graphic())
}

/// Webhook retry delay after `failures` failed attempts (1-based); `None` = give up.
pub fn webhook_retry_delay(failures: i32) -> Option<Duration> {
    let idx = usize::try_from(failures.checked_sub(1)?).ok()?;
    WEBHOOK_RETRY_SECS.get(idx).map(|s| Duration::seconds(*s))
}

/// True when a webhook subscribed to `event_type` (`order.*` style wildcards; an
/// empty list means every event).
pub fn subscribed(events: &[String], event_type: &str) -> bool {
    events.is_empty()
        || events.iter().any(|e| {
            let e = e.trim();
            e == event_type
                || e == "*"
                || e.strip_suffix(".*")
                    .is_some_and(|prefix| event_type.starts_with(&format!("{prefix}.")))
        })
}

/// Validates webhook event patterns.
pub fn valid_event_pattern(pattern: &str) -> bool {
    let p = pattern.trim();
    p == "*"
        || EVENT_TYPES.contains(&p)
        || p.strip_suffix(".*").is_some_and(|prefix| {
            EVENT_TYPES
                .iter()
                .any(|t| t.starts_with(&format!("{prefix}.")))
        })
}

/// Event type of an order status (spec §6).
pub fn order_event_type(status: &str) -> &'static str {
    match status {
        "delivered" | "completed" => "order.delivered",
        "refunded" | "partially_refunded" => "order.refunded",
        _ => "order.status_changed",
    }
}

// ---------------------------------------------------------------------------
// Change feed: catalog snapshots
// ---------------------------------------------------------------------------

/// What we remember of a product to detect changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProductSnapshot {
    pub product_id: Id,
    /// Digest of the product content without SKU prices / stock (spec §5 `version`).
    pub version: String,
    /// `(sku_id, price, stock_quantity)` of the listed SKUs.
    pub skus: Vec<(Id, String, i64)>,
}

/// Content digest of a projected product: everything except volatile price / stock
/// fields and timestamps.
pub fn product_version(p: &RemoteProduct) -> String {
    let mut v = serde_json::to_value(p).unwrap_or(Value::Null);
    if let Some(obj) = v.as_object_mut() {
        for k in [
            "price_amount",
            "original_price",
            "member_price",
            "updated_at",
            "created_at",
        ] {
            obj.remove(k);
        }
        if let Some(Value::Array(skus)) = obj.get_mut("skus") {
            for s in skus.iter_mut().filter_map(Value::as_object_mut) {
                for k in [
                    "price_amount",
                    "original_price",
                    "member_price",
                    "stock_quantity",
                    "stock_status",
                ] {
                    s.remove(k);
                }
            }
        }
    }
    let text = v.to_string();
    zs_shared::zs::sha256_hex(text.as_bytes())[..16].to_owned()
}

/// Snapshot of a projected product.
pub fn snapshot_of(p: &RemoteProduct) -> ProductSnapshot {
    ProductSnapshot {
        product_id: p.id,
        version: product_version(p),
        skus: p
            .skus
            .iter()
            .map(|s| (s.id, s.price_amount.clone(), s.stock_quantity))
            .collect(),
    }
}

/// A change to append to the feed.
#[derive(Debug, Clone, PartialEq)]
pub struct NewChange {
    /// `product.upserted` / `product.deleted` / `sku.stock` / `sku.price`.
    pub kind: &'static str,
    pub product_id: Id,
    pub sku_id: Option<Id>,
    pub data: Value,
}

/// Changes between the stored snapshot and the current product.
pub fn diff_product(
    old: Option<&ProductSnapshot>,
    now: &ProductSnapshot,
    p: &RemoteProduct,
) -> Vec<NewChange> {
    let upserted = || NewChange {
        kind: "product.upserted",
        product_id: now.product_id,
        sku_id: None,
        data: json!({"product": p}),
    };
    let Some(old) = old else {
        return vec![upserted()];
    };
    if old.version != now.version {
        return vec![upserted()];
    }
    let before: HashMap<Id, (&String, i64)> = old
        .skus
        .iter()
        .map(|(id, pr, st)| (*id, (pr, *st)))
        .collect();
    let mut out = Vec::new();
    for (id, price, stock) in &now.skus {
        let Some((old_price, old_stock)) = before.get(id) else {
            continue;
        };
        if *old_price != price {
            out.push(NewChange {
                kind: "sku.price",
                product_id: now.product_id,
                sku_id: Some(*id),
                data: json!({"price_amount": price}),
            });
        }
        if *old_stock != *stock {
            out.push(NewChange {
                kind: "sku.stock",
                product_id: now.product_id,
                sku_id: Some(*id),
                data: json!({
                    "stock_quantity": stock,
                    "stock_status": crate::catalog::stock::StockPolicy::UPSTREAM.status(*stock).as_str(),
                }),
            });
        }
    }
    out
}

/// A deletion change.
pub fn deleted_change(product_id: Id) -> NewChange {
    NewChange {
        kind: "product.deleted",
        product_id,
        sku_id: None,
        data: json!({}),
    }
}

/// A stored change.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ChangeRow {
    pub seq: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub product_id: Id,
    pub sku_id: Option<Id>,
    pub at: DateTime<Utc>,
    pub data: Value,
}

/// True when `since` fell out of the retention window: rows up to `min_seq - 1`
/// were purged and the caller has not seen them (spec §5 `410 cursor_expired`).
pub fn cursor_expired(since: i64, min_seq: Option<i64>) -> bool {
    min_seq.is_some_and(|min| since + 1 < min)
}

// ---------------------------------------------------------------------------
// Webhooks and events
// ---------------------------------------------------------------------------

/// A registered push endpoint of a credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Webhook {
    pub credential_id: Id,
    pub url: String,
    pub events: Vec<String>,
    /// `account.balance_low` threshold (empty = never).
    pub balance_low_threshold: Option<Amount>,
    #[serde(skip)]
    pub last_balance_low_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Delivery state of an outgoing event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventStatus {
    Pending,
    Sent,
    Failed,
}

impl EventStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        match raw {
            "sent" => Self::Sent,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// An outgoing event (outbox row).
#[derive(Debug, Clone, PartialEq)]
pub struct OutEvent {
    pub id: Id,
    pub event_id: String,
    pub credential_id: Id,
    pub kind: String,
    pub body: Value,
    pub status: EventStatus,
    pub attempts: i32,
    pub last_error: String,
}

// ---------------------------------------------------------------------------
// Quotes and order requests
// ---------------------------------------------------------------------------

/// One requested line (`items[]` of quote / order).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemRequest {
    pub sku_id: Id,
    pub quantity: i32,
    pub manual_form_data: Option<JsonMap>,
}

/// Validates `items[]` (1..=20 lines, positive ids and quantities).
pub fn validate_items(items: &[ItemRequest]) -> ZsResult<()> {
    if items.is_empty() {
        return Err(ZsFailure::invalid("items: at least one item is required"));
    }
    if items.len() > MAX_ITEMS_PER_ORDER {
        return Err(ZsFailure::invalid(format!(
            "items: at most {MAX_ITEMS_PER_ORDER} items"
        )));
    }
    for (i, item) in items.iter().enumerate() {
        if item.sku_id <= 0 {
            return Err(ZsFailure::invalid(format!("items[{i}].sku_id: required")));
        }
        if item.quantity < 1 {
            return Err(ZsFailure::invalid(format!(
                "items[{i}].quantity: must be at least 1"
            )));
        }
    }
    Ok(())
}

/// A quoted line kept with the quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedLine {
    pub sku_id: Id,
    pub product_id: Id,
    pub quantity: i32,
    pub unit_price: Amount,
}

/// A stored quote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredQuote {
    pub quote_id: String,
    pub credential_id: Id,
    pub lines: Vec<QuotedLine>,
    pub total: Amount,
    pub currency: String,
    pub expires_at: DateTime<Utc>,
}

/// Same SKUs and quantities (order-insensitive, duplicates merged).
pub fn quote_matches(quote: &StoredQuote, items: &[ItemRequest]) -> bool {
    let merge = |pairs: Vec<(Id, i32)>| {
        let mut m: HashMap<Id, i32> = HashMap::new();
        for (sku, qty) in pairs {
            *m.entry(sku).or_default() += qty;
        }
        m
    };
    merge(quote.lines.iter().map(|l| (l.sku_id, l.quantity)).collect())
        == merge(items.iter().map(|i| (i.sku_id, i.quantity)).collect())
}

/// An idempotent order request (`(credential, Idempotency-Key)`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderRequest {
    pub credential_id: Id,
    pub idempotency_key: String,
    pub request_hash: String,
    /// `0` while the first request is still running.
    pub order_id: Id,
    pub order_no: String,
    pub callback: bool,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Ports
// ---------------------------------------------------------------------------

/// Storage of the supplier side of the protocol.
#[async_trait]
pub trait ZsStore: Send + Sync {
    // change feed
    async fn snapshots(&self) -> Result<Vec<ProductSnapshot>>;
    /// Appends `changes` and updates snapshots in one transaction; returns the latest
    /// sequence when something was appended.
    async fn apply_snapshot_diff(
        &self,
        changes: &[NewChange],
        upserts: &[ProductSnapshot],
        deletes: &[Id],
        now: DateTime<Utc>,
    ) -> Result<Option<i64>>;
    async fn changes_after(&self, since: i64, limit: u64) -> Result<Vec<ChangeRow>>;
    /// `(min_seq, max_seq)` of the retained changes.
    async fn change_bounds(&self) -> Result<Option<(i64, i64)>>;
    /// Drops changes older than `before`, always keeping the newest one.
    async fn purge_changes(&self, before: DateTime<Utc>) -> Result<u64>;

    // webhooks + outbox
    async fn webhook(&self, credential_id: Id) -> Result<Option<Webhook>>;
    async fn put_webhook(&self, hook: &Webhook, now: DateTime<Utc>) -> Result<Webhook>;
    async fn delete_webhook(&self, credential_id: Id) -> Result<bool>;
    async fn webhooks(&self) -> Result<Vec<Webhook>>;
    async fn mark_balance_low(&self, credential_id: Id, at: DateTime<Utc>) -> Result<()>;
    async fn create_event(
        &self,
        credential_id: Id,
        event_id: &str,
        kind: &str,
        body: &Value,
        now: DateTime<Utc>,
    ) -> Result<OutEvent>;
    async fn event(&self, id: Id) -> Result<Option<OutEvent>>;
    async fn update_event(
        &self,
        id: Id,
        status: EventStatus,
        attempts: i32,
        next_attempt_at: Option<DateTime<Utc>>,
        last_error: &str,
        now: DateTime<Utc>,
    ) -> Result<()>;

    // quotes + idempotency
    async fn save_quote(&self, quote: &StoredQuote, now: DateTime<Utc>) -> Result<()>;
    async fn quote(&self, quote_id: &str) -> Result<Option<StoredQuote>>;
    /// Inserts the request unless the key exists; returns the existing row when it did.
    async fn reserve_request(
        &self,
        req: &OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<Option<OrderRequest>>;
    async fn complete_request(
        &self,
        credential_id: Id,
        key: &str,
        order_id: Id,
        order_no: &str,
    ) -> Result<()>;
    async fn release_request(&self, credential_id: Id, key: &str) -> Result<()>;
    async fn request_by_order(&self, order_id: Id) -> Result<Option<OrderRequest>>;
    /// Completed requests of a credential, newest first, `order_id < before` when set.
    async fn list_requests(
        &self,
        credential_id: Id,
        before: Option<Id>,
        limit: u64,
    ) -> Result<Vec<OrderRequest>>;
    /// Expired quotes / idempotency records.
    async fn purge_expired(&self, now: DateTime<Utc>) -> Result<()>;
    /// Root order id of an order number.
    async fn order_id_by_no(&self, order_no: &str) -> Result<Option<Id>>;
}

/// Projected catalog pages for the change-feed producer (all live products, active or
/// not, with the same stock decoration as the APIs; no member prices).
#[async_trait]
pub trait CatalogSnapshotSource: Send + Sync {
    async fn page(&self, after_id: Id, limit: u64) -> Result<Vec<RemoteProduct>>;
}

/// Sends one signed event to a buyer (SSRF-safe, no redirects, UPS-01).
#[async_trait]
pub trait EventSender: Send + Sync {
    async fn send(
        &self,
        url: &str,
        api_key: &str,
        secret: &str,
        event_id: &str,
        body: &[u8],
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::protocol::RemoteSku;

    fn product(price: &str, stock: i64, title: &str) -> RemoteProduct {
        let mut t = JsonMap::new();
        t.insert("zh-CN".into(), Value::String(title.into()));
        RemoteProduct {
            id: 7,
            title: t,
            price_amount: price.into(),
            skus: vec![RemoteSku {
                id: 70,
                price_amount: price.into(),
                stock_quantity: stock,
                is_active: true,
                ..RemoteSku::default()
            }],
            ..RemoteProduct::default()
        }
    }

    #[test]
    fn snapshot_diff_kinds() {
        let a = product("10.00", 5, "A");
        let sa = snapshot_of(&a);
        assert_eq!(diff_product(None, &sa, &a)[0].kind, "product.upserted");
        assert!(diff_product(Some(&sa), &sa, &a).is_empty());
        // price + stock only → sku changes, same version
        let b = product("9.90", 4, "A");
        let sb = snapshot_of(&b);
        assert_eq!(sa.version, sb.version);
        let kinds: Vec<_> = diff_product(Some(&sa), &sb, &b)
            .into_iter()
            .map(|c| (c.kind, c.sku_id))
            .collect();
        assert_eq!(
            kinds,
            vec![("sku.price", Some(70)), ("sku.stock", Some(70))]
        );
        // content change → upserted only
        let c = product("9.90", 4, "B");
        let sc = snapshot_of(&c);
        let out = diff_product(Some(&sb), &sc, &c);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "product.upserted");
        assert_eq!(out[0].data["product"]["id"], 7);
    }

    #[test]
    fn cursor_expiry_and_limits() {
        assert!(!cursor_expired(0, None));
        assert!(!cursor_expired(0, Some(1)));
        assert!(!cursor_expired(10, Some(11)));
        assert!(cursor_expired(5, Some(11)));
        assert_eq!(list_limit(None), 50);
        assert_eq!(list_limit(Some(500)), 200);
        assert_eq!(webhook_retry_delay(1), Some(Duration::seconds(30)));
        assert_eq!(webhook_retry_delay(5), Some(Duration::hours(6)));
        assert_eq!(webhook_retry_delay(6), None);
        assert_eq!(webhook_retry_delay(0), None);
    }

    #[test]
    fn subscriptions_and_keys() {
        let events = vec!["order.*".to_owned()];
        assert!(subscribed(&events, "order.delivered"));
        assert!(!subscribed(&events, "catalog.changed"));
        assert!(subscribed(&[], "catalog.changed"));
        assert!(valid_event_pattern("order.*"));
        assert!(valid_event_pattern("account.balance_low"));
        assert!(!valid_event_pattern("user.*"));
        assert!(valid_idempotency_key("procurement:12"));
        assert!(!valid_idempotency_key(""));
        assert!(!valid_idempotency_key(&"k".repeat(65)));
        assert!(!valid_idempotency_key("a b"));
        assert_eq!(order_event_type("completed"), "order.delivered");
        assert_eq!(order_event_type("paid"), "order.status_changed");
        assert_eq!(ErrorCode::QuoteExpired.http_status(), 409);
        assert!(ErrorCode::RateLimited.retryable());
        assert!(!ErrorCode::InsufficientBalance.retryable());
    }

    #[test]
    fn quote_matching_is_by_multiset() {
        let q = StoredQuote {
            quote_id: "q".into(),
            credential_id: 1,
            lines: vec![
                QuotedLine {
                    sku_id: 1,
                    product_id: 1,
                    quantity: 2,
                    unit_price: Amount::ZERO,
                },
                QuotedLine {
                    sku_id: 2,
                    product_id: 1,
                    quantity: 1,
                    unit_price: Amount::ZERO,
                },
            ],
            total: Amount::ZERO,
            currency: "CNY".into(),
            expires_at: DateTime::<Utc>::MIN_UTC,
        };
        let item = |sku, quantity| ItemRequest {
            sku_id: sku,
            quantity,
            manual_form_data: None,
        };
        assert!(quote_matches(&q, &[item(2, 1), item(1, 1), item(1, 1)]));
        assert!(!quote_matches(&q, &[item(1, 2)]));
        assert!(validate_items(&[]).is_err());
        assert!(validate_items(&[item(1, 0)]).is_err());
        assert!(validate_items(&[item(1, 1)]).is_ok());
    }
}
