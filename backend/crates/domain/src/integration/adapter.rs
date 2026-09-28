//! Protocol-neutral supplier adapter port.
//!
//! Every supplier system we can buy from is one
//! [`SupplierAdapter`] registered by protocol id. Core services (sync, import,
//! procurement, inbound events) only use the neutral types of this module and
//! [`super::protocol::UpstreamClient`] plus its [`Capabilities`]; they never branch on a
//! protocol id. Adding a system = one adapter module + one registration line.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use super::connection::Endpoint;
use super::protocol::{RemoteFulfillment, RemoteProduct, UpstreamClient};
use crate::catalog::product::JsonMap;
use crate::{Id, Result};

/// One optional ability of a supplier system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Supplier category list.
    Categories,
    /// Cursor-driven change feed (incremental sync, deletions included).
    IncrementalChanges,
    /// Signed push events (catalog / order / account).
    PushEvents,
    /// Price-locking quotes before ordering.
    Quote,
    /// Several lines in one supplier order.
    MultiItem,
    /// Idempotency keys on order creation.
    Idempotency,
    /// Deliveries are encrypted end to end.
    EncryptedDelivery,
}

impl Capability {
    pub const ALL: [Self; 7] = [
        Self::Categories,
        Self::IncrementalChanges,
        Self::PushEvents,
        Self::Quote,
        Self::MultiItem,
        Self::Idempotency,
        Self::EncryptedDelivery,
    ];

    /// Capability of an id (the single vocabulary used by the API and storage).
    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.as_str() == raw.trim())
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Categories => "categories",
            Self::IncrementalChanges => "incremental_changes",
            Self::PushEvents => "push_events",
            Self::Quote => "quote",
            Self::MultiItem => "multi_item",
            Self::Idempotency => "idempotency",
            Self::EncryptedDelivery => "encrypted_delivery",
        }
    }
}

/// A set of [`Capability`] flags.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Capabilities(Vec<Capability>);

impl Capabilities {
    pub fn of(list: &[Capability]) -> Self {
        let mut v: Vec<Capability> = Vec::new();
        for c in list {
            if !v.contains(c) {
                v.push(*c);
            }
        }
        Self(v)
    }

    pub fn has(&self, c: Capability) -> bool {
        self.0.contains(&c)
    }

    /// Keeps only the capabilities also in `other`.
    pub fn intersect(&self, other: &Self) -> Self {
        Self(self.0.iter().copied().filter(|c| other.has(*c)).collect())
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.0.iter().map(|c| c.as_str()).collect()
    }

    /// Capabilities of a list of ids (unknown ids ignored).
    pub fn from_ids<S: AsRef<str>>(ids: &[S]) -> Self {
        let list: Vec<Capability> = ids
            .iter()
            .filter_map(|i| Capability::parse(i.as_ref()))
            .collect();
        Self::of(&list)
    }

    pub fn list(&self) -> &[Capability] {
        &self.0
    }
}

/// Input kind of a connection configuration field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    Text,
    Secret,
    Url,
    Select,
}

/// One configuration field an adapter needs (drives the admin form).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConfigField {
    pub key: &'static str,
    pub label: JsonMap,
    pub kind: FieldKind,
    pub required: bool,
    pub placeholder: JsonMap,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Value>,
}

/// Localized text helper (`{"zh-CN", "zh-TW", "en-US"}`).
pub fn text3(zh_cn: &str, zh_tw: &str, en_us: &str) -> JsonMap {
    let mut m = JsonMap::new();
    m.insert("zh-CN".into(), Value::String(zh_cn.into()));
    m.insert("zh-TW".into(), Value::String(zh_tw.into()));
    m.insert("en-US".into(), Value::String(en_us.into()));
    m
}

/// Static description of a supplier system.
#[derive(Debug, Clone, PartialEq)]
pub struct AdapterMeta {
    /// Protocol id stored in `site_connections.protocol`.
    pub id: &'static str,
    pub name: JsonMap,
    pub description: JsonMap,
    pub fields: Vec<ConfigField>,
    /// Everything the system can do (a connection may negotiate fewer).
    pub capabilities: Capabilities,
    pub supports_connection_code: bool,
    /// Path of our inbound endpoint for this system (callback / event URL suffix).
    pub inbound_path: &'static str,
}

/// A parsed connection code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionCode {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    pub protocol: String,
}

/// Neutral handshake answer.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
pub struct HandshakeInfo {
    pub protocol: String,
    pub version: String,
    pub site_name: String,
    pub site_url: String,
    pub currency: String,
    /// Capability ids the supplier offers ([`Capability::as_str`]; adapters map their
    /// wire vocabulary).
    pub features: Vec<String>,
    pub limits: JsonMap,
    /// Our account at the supplier.
    pub user_id: Id,
    pub balance: String,
    pub account_currency: String,
    pub member_level: Option<Value>,
    /// Latest change cursor when the supplier has a change feed.
    pub change_head: Option<String>,
    pub server_time: Option<DateTime<Utc>>,
}

/// What one catalog change is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    ProductUpserted,
    ProductDeleted,
    SkuStock,
    SkuPrice,
}

/// One catalog change from a supplier change feed.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogChange {
    pub cursor: String,
    pub kind: ChangeKind,
    pub product_id: Id,
    pub sku_id: Option<Id>,
    /// Full product for `ProductUpserted` when the supplier sends it.
    pub product: Option<RemoteProduct>,
}

/// One page of a change feed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChangePage {
    pub changes: Vec<CatalogChange>,
    /// Cursor to continue from (unchanged when empty).
    pub next_cursor: String,
    pub has_more: bool,
}

/// One requested order line (supplier SKU id).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrderLine {
    pub sku_id: Id,
    pub quantity: i32,
    pub manual_form_data: Option<JsonMap>,
}

/// A quote of order lines.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Quote {
    pub quote_id: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub currency: String,
    pub lines: Vec<QuoteLine>,
    pub total: String,
    pub balance: String,
    pub sufficient_balance: bool,
}

/// One quoted line.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct QuoteLine {
    pub sku_id: Id,
    pub quantity: i32,
    pub unit_price: String,
    pub available: bool,
    pub reason: Option<String>,
}

/// A supplier order to place.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlaceOrder {
    pub lines: Vec<OrderLine>,
    /// Our local order number (the supplier's `downstream_order_no`).
    pub downstream_order_no: String,
    pub trace_id: String,
    pub callback_url: String,
    /// Used by suppliers with [`Capability::Idempotency`].
    pub idempotency_key: String,
    /// Price lock from [`super::protocol::UpstreamClient::quote`].
    pub quote_id: Option<String>,
}

/// Identity of a supplier order (systems use a numeric id, a number, or both).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderRef {
    pub id: Id,
    pub no: String,
}

impl OrderRef {
    pub fn is_known(&self) -> bool {
        self.id > 0 || !self.no.is_empty()
    }
}

/// An inbound (supplier → us) request.
#[derive(Clone, Copy)]
pub struct InboundRequest<'a> {
    pub method: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    /// Header lookup (case-insensitive name).
    pub headers: &'a (dyn Fn(&str) -> Option<String> + Sync),
    pub body: &'a [u8],
}

impl std::fmt::Debug for InboundRequest<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InboundRequest")
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body_len", &self.body.len())
            .finish_non_exhaustive()
    }
}

impl InboundRequest<'_> {
    pub fn header(&self, name: &str) -> String {
        (self.headers)(name).unwrap_or_default().trim().to_owned()
    }
}

/// Why an inbound request was refused by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundError {
    MissingHeaders,
    InvalidTimestamp,
    TimestampExpired,
    InvalidSignature,
    InvalidBody,
    MissingFields,
}

/// A supplier order notice (callback or pushed order event).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrderNotice {
    /// Our local order number.
    pub downstream_order_no: String,
    pub upstream: OrderRef,
    pub status: String,
    /// Decrypted delivery when present.
    pub fulfillment: Option<RemoteFulfillment>,
}

/// What an inbound event is about.
#[derive(Debug, Clone, PartialEq)]
pub enum InboundKind {
    Order(OrderNotice),
    /// The supplier catalog changed: pull the change feed now.
    CatalogChanged,
    BalanceLow {
        balance: String,
        threshold: String,
    },
    /// Unknown / unhandled type (acknowledged).
    Other(String),
}

/// One verified inbound event.
#[derive(Debug, Clone, PartialEq)]
pub struct InboundEvent {
    /// Supplier event id (deduplicated per connection) when the system has one.
    pub event_id: Option<String>,
    pub kind: InboundKind,
}

/// A supplier system.
pub trait SupplierAdapter: Send + Sync {
    fn meta(&self) -> AdapterMeta;

    /// Outbound client for one connection.
    fn open(&self, endpoint: &Endpoint) -> Result<Arc<dyn UpstreamClient>>;

    /// Pre-authentication checks of an inbound request (headers, clock window); returns
    /// the API key identifying the connection.
    fn inbound_key(
        &self,
        req: &InboundRequest<'_>,
        now: DateTime<Utc>,
    ) -> std::result::Result<String, InboundError>;

    /// Verifies the signature with the connection secret and parses the events.
    fn parse_inbound(
        &self,
        req: &InboundRequest<'_>,
        secret: &str,
        now: DateTime<Utc>,
    ) -> std::result::Result<Vec<InboundEvent>, InboundError>;

    /// Parses a connection code of this system (`None` = not this system's format).
    fn parse_connection_code(
        &self,
        _code: &str,
    ) -> Option<std::result::Result<ConnectionCode, String>> {
        None
    }
}

/// Inbound event ids already processed per connection (deduplication).
#[async_trait::async_trait]
pub trait ProcessedEvents: Send + Sync {
    async fn seen(&self, connection_id: Id, event_id: &str) -> Result<bool>;
    /// Records a processed event (idempotent).
    async fn record(&self, connection_id: Id, event_id: &str, now: DateTime<Utc>) -> Result<()>;
}

impl std::fmt::Debug for dyn SupplierAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SupplierAdapter({})", self.meta().id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_sets() {
        let all = Capabilities::of(&Capability::ALL);
        let some = Capabilities::of(&[Capability::Quote, Capability::Quote, Capability::MultiItem]);
        assert_eq!(some.names(), vec!["quote", "multi_item"]);
        assert_eq!(all.intersect(&some), some);
        assert!(!some.has(Capability::PushEvents));
        assert!(
            OrderRef {
                id: 0,
                no: "N".into()
            }
            .is_known()
        );
        assert!(!OrderRef::default().is_known());
    }
}
