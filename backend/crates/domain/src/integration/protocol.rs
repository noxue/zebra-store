//! Supplier wire types (the compatibility JSON exchanged on `/api/v1/upstream/*`,
//! which is also the neutral product/order shape) and the outbound client port.
//!
//! Deserialization is lenient (every field defaults, prices are kept as text) so a
//! malformed field of a peer never fails a whole page; prices are parsed with
//! [`super::pricing::parse_upstream_price`] (UPS-08).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::adapter::{
    Capabilities, ChangePage, HandshakeInfo, OrderLine, OrderRef, PlaceOrder, Quote,
    SupplierAdapter,
};
use super::connection::Endpoint;
use crate::catalog::product::JsonMap;
use crate::{Id, Result};

/// Accepts a string, a number or null as text (prices, codes).
pub fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<String, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => s,
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => String::new(),
    })
}

/// Accepts an object; anything else becomes empty.
pub fn lenient_map<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<JsonMap, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => m,
        _ => JsonMap::new(),
    })
}

/// Accepts an object; null / anything else is `None` (optional sync fields, UPS-13).
pub fn lenient_opt_map<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<JsonMap>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Object(m) => Some(m),
        _ => None,
    })
}

fn lenient_i64<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<i64, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Number(n) => n.as_i64().unwrap_or_default(),
        Value::String(s) => s.trim().parse().unwrap_or_default(),
        _ => 0,
    })
}

fn lenient_bool<'de, D: Deserializer<'de>>(d: D) -> std::result::Result<bool, D::Error> {
    Ok(matches!(Value::deserialize(d)?, Value::Bool(true)))
}

fn lenient_time<'de, D: Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<DateTime<Utc>>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::String(s) => DateTime::parse_from_rfc3339(s.trim())
            .ok()
            .map(|t| t.with_timezone(&Utc)),
        _ => None,
    })
}

fn lenient_list<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    d: D,
) -> std::result::Result<Vec<T>, D::Error> {
    Ok(match Value::deserialize(d)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect(),
        _ => Vec::new(),
    })
}

fn is_empty_str(s: &str) -> bool {
    s.is_empty()
}

/// A wholesale tier as sent by a supplier (`unit_price` text).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteTier {
    #[serde(deserialize_with = "lenient_i64", skip_serializing_if = "is_zero")]
    pub sku_id: i64,
    #[serde(
        deserialize_with = "lenient_string",
        skip_serializing_if = "String::is_empty"
    )]
    pub sku_code: String,
    #[serde(deserialize_with = "lenient_i64")]
    pub min_quantity: i64,
    #[serde(deserialize_with = "lenient_string")]
    pub unit_price: String,
}

fn is_zero(v: &i64) -> bool {
    *v == 0
}

/// A supplier SKU.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteSku {
    #[serde(deserialize_with = "lenient_i64")]
    pub id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub sku_code: String,
    #[serde(deserialize_with = "lenient_map")]
    pub spec_values: JsonMap,
    #[serde(deserialize_with = "lenient_string")]
    pub price_amount: String,
    #[serde(
        deserialize_with = "lenient_string",
        skip_serializing_if = "is_empty_str"
    )]
    pub original_price: String,
    #[serde(
        deserialize_with = "lenient_string",
        skip_serializing_if = "is_empty_str"
    )]
    pub member_price: String,
    #[serde(deserialize_with = "lenient_string")]
    pub stock_status: String,
    /// Real available quantity, `-1` unlimited (UPS-14).
    #[serde(deserialize_with = "lenient_i64")]
    pub stock_quantity: i64,
    #[serde(deserialize_with = "lenient_bool")]
    pub is_active: bool,
}

/// A supplier product.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteProduct {
    #[serde(deserialize_with = "lenient_i64")]
    pub id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub slug: String,
    #[serde(deserialize_with = "lenient_map")]
    pub seo_meta: JsonMap,
    #[serde(deserialize_with = "lenient_map")]
    pub title: JsonMap,
    #[serde(deserialize_with = "lenient_map")]
    pub description: JsonMap,
    #[serde(deserialize_with = "lenient_map")]
    pub content: JsonMap,
    #[serde(deserialize_with = "lenient_list")]
    pub images: Vec<String>,
    #[serde(deserialize_with = "lenient_list")]
    pub tags: Vec<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub price_amount: String,
    #[serde(
        deserialize_with = "lenient_string",
        skip_serializing_if = "is_empty_str"
    )]
    pub original_price: String,
    #[serde(
        deserialize_with = "lenient_string",
        skip_serializing_if = "is_empty_str"
    )]
    pub member_price: String,
    #[serde(
        deserialize_with = "lenient_list",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub wholesale_prices: Vec<RemoteTier>,
    #[serde(
        deserialize_with = "lenient_string",
        skip_serializing_if = "is_empty_str"
    )]
    pub currency: String,
    #[serde(deserialize_with = "lenient_string")]
    pub fulfillment_type: String,
    /// `None` when the supplier did not send a schema (sync keeps the local one).
    #[serde(deserialize_with = "lenient_opt_map")]
    pub manual_form_schema: Option<JsonMap>,
    #[serde(deserialize_with = "lenient_bool")]
    pub is_active: bool,
    #[serde(deserialize_with = "lenient_i64")]
    pub category_id: Id,
    #[serde(deserialize_with = "lenient_list")]
    pub skus: Vec<RemoteSku>,
    #[serde(
        deserialize_with = "lenient_time",
        skip_serializing_if = "Option::is_none"
    )]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(
        deserialize_with = "lenient_time",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<DateTime<Utc>>,
}

/// A supplier category.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteCategory {
    #[serde(deserialize_with = "lenient_i64")]
    pub id: Id,
    #[serde(deserialize_with = "lenient_i64")]
    pub parent_id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub slug: String,
    #[serde(deserialize_with = "lenient_map")]
    pub name: JsonMap,
    #[serde(deserialize_with = "lenient_string")]
    pub icon: String,
    #[serde(deserialize_with = "lenient_i64")]
    pub sort_order: i64,
}

/// One page of supplier products.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProductPage {
    #[serde(deserialize_with = "lenient_i64")]
    pub total: i64,
    #[serde(deserialize_with = "lenient_list")]
    pub items: Vec<RemoteProduct>,
    /// Echo of `include_inactive` (old suppliers omit it, UPS-14).
    #[serde(deserialize_with = "lenient_bool")]
    pub includes_inactive: bool,
    /// Cursor of the next page for cursor-paginated suppliers (empty otherwise).
    #[serde(skip)]
    pub next_cursor: String,
    /// Cursor-paginated suppliers: more pages follow.
    #[serde(skip)]
    pub has_more: bool,
}

/// Supplier category list; `supported = false` for suppliers without the endpoint.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CategoryList {
    pub supported: bool,
    pub categories: Vec<RemoteCategory>,
}

/// Supplier ping answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PingInfo {
    #[serde(deserialize_with = "lenient_string")]
    pub site_name: String,
    #[serde(deserialize_with = "lenient_string")]
    pub protocol_version: String,
    #[serde(deserialize_with = "lenient_i64")]
    pub user_id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub balance: String,
    #[serde(deserialize_with = "lenient_string")]
    pub currency: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_level: Option<Value>,
}

/// Products list query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProductQuery {
    pub page: i64,
    pub page_size: i64,
    pub updated_after: Option<DateTime<Utc>>,
    pub include_inactive: bool,
    /// Continue a cursor-paginated listing (takes precedence over `page`).
    pub cursor: Option<String>,
}

/// `POST /upstream/orders` body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub sku_id: Id,
    pub quantity: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manual_form_data: Option<JsonMap>,
    pub downstream_order_no: String,
    pub trace_id: String,
    pub callback_url: String,
}

/// `POST /upstream/orders` answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CreateOrderResponse {
    #[serde(deserialize_with = "lenient_bool")]
    pub ok: bool,
    #[serde(deserialize_with = "lenient_i64")]
    pub order_id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub order_no: String,
    #[serde(deserialize_with = "lenient_string")]
    pub status: String,
    #[serde(deserialize_with = "lenient_string")]
    pub amount: String,
    #[serde(deserialize_with = "lenient_string")]
    pub currency: String,
    #[serde(deserialize_with = "lenient_string")]
    pub error_code: String,
    #[serde(deserialize_with = "lenient_string")]
    pub error_message: String,
    /// Delivery handed over by the order call itself (suppliers that deliver
    /// synchronously). The procurement core persists it together with the supplier
    /// order number and applies it from there, so a restart never loses it. Not part
    /// of any wire format.
    #[serde(skip)]
    pub fulfillment: Option<RemoteFulfillment>,
    /// Set when the supplier executed (and charged) the purchase but its result needs a
    /// human decision (e.g. paid without goods): the purchase order is held in
    /// `manual_review` with this reason instead of being delivered or rolled back. Not
    /// part of any wire format.
    #[serde(skip)]
    pub review: Option<String>,
}

/// Delivery details of a supplier order / callback.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteFulfillment {
    #[serde(rename = "type", deserialize_with = "lenient_string")]
    pub kind: String,
    #[serde(deserialize_with = "lenient_string")]
    pub status: String,
    #[serde(deserialize_with = "lenient_string")]
    pub payload: String,
    #[serde(deserialize_with = "lenient_opt_map")]
    pub delivery_data: Option<JsonMap>,
    #[serde(
        deserialize_with = "lenient_time",
        skip_serializing_if = "Option::is_none"
    )]
    pub delivered_at: Option<DateTime<Utc>>,
}

/// `GET /upstream/orders/:id` answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RemoteOrder {
    #[serde(deserialize_with = "lenient_i64")]
    pub order_id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub order_no: String,
    #[serde(deserialize_with = "lenient_string")]
    pub status: String,
    #[serde(deserialize_with = "lenient_string")]
    pub amount: String,
    #[serde(deserialize_with = "lenient_string")]
    pub refunded_amount: String,
    #[serde(deserialize_with = "lenient_string")]
    pub currency: String,
    pub fulfillment: Option<RemoteFulfillment>,
    #[serde(deserialize_with = "lenient_list")]
    pub refund_records: Vec<JsonMap>,
}

/// Signed callback body sent to a downstream shop / received from a supplier.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CallbackPayload {
    #[serde(deserialize_with = "lenient_string")]
    pub event: String,
    #[serde(deserialize_with = "lenient_i64")]
    pub order_id: Id,
    #[serde(deserialize_with = "lenient_string")]
    pub order_no: String,
    #[serde(deserialize_with = "lenient_string")]
    pub downstream_order_no: String,
    #[serde(deserialize_with = "lenient_string")]
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fulfillment: Option<RemoteFulfillment>,
    #[serde(deserialize_with = "lenient_i64")]
    pub timestamp: i64,
}

/// A downloaded file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Download {
    pub filename: String,
    pub bytes: Vec<u8>,
}

/// Failure of an outbound supplier call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamError {
    /// The product was deleted at the supplier (`product_deleted` / `product_not_found`).
    ProductDeleted,
    /// Old suppliers answer `product_unavailable` for delisted products.
    ProductUnavailable,
    /// Non-200 answer with the parsed `{error_code, error_message}`.
    Http {
        status: u16,
        code: String,
        message: String,
    },
    /// Target refused by the SSRF guard (UPS-01).
    Forbidden(String),
    /// Network failure before the request could reach the supplier (DNS, connection
    /// refused / connect timeout, TLS handshake): nothing was executed there.
    Transport(String),
    /// The request may have reached the supplier but its answer was lost or unusable
    /// (timeout while waiting, connection reset mid-answer, truncated / unreadable
    /// body): a non-idempotent call may or may not have been executed.
    Uncertain(String),
    /// Unparseable answer or `ok:false`.
    Protocol(String),
    /// The system has no such operation (optional capability).
    Unsupported,
    /// The change-feed cursor fell out of the supplier's retention window.
    CursorExpired,
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ProductDeleted => f.write_str("upstream product deleted"),
            Self::ProductUnavailable => f.write_str("upstream product unavailable"),
            Self::Http {
                status,
                code,
                message,
            } if !code.is_empty() => {
                write!(
                    f,
                    "upstream responded with status {status} ({code}): {message}"
                )
            }
            Self::Http {
                status, message, ..
            } => write!(f, "upstream responded with status {status}: {message}"),
            Self::Forbidden(d) => write!(f, "forbidden address: {d}"),
            Self::Transport(d) => write!(f, "send request: {d}"),
            Self::Uncertain(d) => write!(f, "answer lost (request may have been executed): {d}"),
            Self::Protocol(d) => write!(f, "upstream protocol error: {d}"),
            Self::Unsupported => f.write_str("operation not supported by the supplier"),
            Self::CursorExpired => f.write_str("change cursor expired"),
        }
    }
}

impl UpstreamError {
    /// True when the failed call may nevertheless have been executed by the supplier:
    /// the answer was lost / unusable, a 5xx (gateway) status with an ambiguous commit,
    /// or a 2xx answer we could not interpret. Failures before sending, 4xx refusals
    /// and the SSRF guard are safe (nothing happened there).
    pub fn may_have_executed(&self) -> bool {
        match self {
            Self::Uncertain(_) => true,
            Self::Http { status, .. } => *status >= 500 || (200..300).contains(status),
            _ => false,
        }
    }

    /// For failures of read-only calls made before an order is sent (catalog, stock,
    /// price checks): their uncertainty cannot have bought anything, so it is reported
    /// as a plain transport failure.
    ///
    /// LQA-I4: the message says what happened (no usable answer, nothing placed) and
    /// never repeats the "may have been executed" wording of [`Self::Uncertain`], which
    /// contradicted the resulting `rejected` status.
    #[must_use]
    pub fn not_executed(self) -> Self {
        match self {
            Self::Uncertain(d) => Self::Transport(format!(
                "no usable answer before ordering (nothing placed): {d}"
            )),
            Self::Http { status, .. } if self.may_have_executed() => Self::Transport(format!(
                "unusable answer (HTTP {status}) before ordering (nothing placed)"
            )),
            other => other,
        }
    }
}

impl std::error::Error for UpstreamError {}

#[cfg(test)]
mod upstream_error_tests {
    use super::UpstreamError as E;

    fn http(status: u16) -> E {
        E::Http {
            status,
            code: String::new(),
            message: String::new(),
        }
    }

    #[test]
    fn classifies_whether_a_failed_call_may_have_executed() {
        assert!(E::Uncertain("reset".into()).may_have_executed());
        assert!(http(502).may_have_executed());
        assert!(http(500).may_have_executed());
        assert!(http(200).may_have_executed(), "2xx we could not interpret");
        assert!(!http(429).may_have_executed());
        assert!(!http(400).may_have_executed());
        assert!(!E::Transport("connection refused".into()).may_have_executed());
        assert!(!E::Forbidden("10.0.0.1".into()).may_have_executed());
        assert!(!E::Protocol("bad request".into()).may_have_executed());
        let settled = E::Uncertain("timeout".into()).not_executed();
        assert!(matches!(settled, E::Transport(_)));
        // LQA-I4: a pre-order failure never reads "may have been executed".
        let text = E::Uncertain("unreadable response: <html>".into())
            .not_executed()
            .to_string();
        assert!(!text.contains("may have been executed"), "{text}");
        assert!(text.contains("nothing placed"), "{text}");
        assert!(!http(200).not_executed().to_string().contains("may have"));
        assert!(!http(503).not_executed().may_have_executed());
        assert_eq!(http(404).not_executed(), http(404));
    }
}

impl From<UpstreamError> for crate::Error {
    fn from(e: UpstreamError) -> Self {
        crate::Error::internal(e)
    }
}

/// Outbound supplier client bound to one connection (signed requests, SSRF-safe),
/// expressed in protocol-neutral terms; optional operations default to
/// [`UpstreamError::Unsupported`] and are only called when [`Self::capabilities`] says so.
#[async_trait]
pub trait UpstreamClient: Send + Sync {
    /// Capabilities of this connection (system capabilities narrowed by negotiation).
    fn capabilities(&self) -> Capabilities;
    async fn handshake(&self) -> std::result::Result<HandshakeInfo, UpstreamError>;
    async fn ping(&self) -> std::result::Result<PingInfo, UpstreamError>;
    async fn list_categories(&self) -> std::result::Result<CategoryList, UpstreamError>;
    async fn list_products(
        &self,
        query: &ProductQuery,
    ) -> std::result::Result<ProductPage, UpstreamError>;
    async fn get_product(&self, id: Id) -> std::result::Result<RemoteProduct, UpstreamError>;
    /// Places an order; business refusals come back as `ok = false` with an
    /// `error_code` (see [`super::procurement::is_retryable_error_code`]).
    async fn place_order(
        &self,
        req: &PlaceOrder,
    ) -> std::result::Result<CreateOrderResponse, UpstreamError>;
    /// True when this system can address the order by the identity we stored (numeric
    /// id by default).
    fn addressable(&self, order: &OrderRef) -> bool {
        order.id > 0
    }
    async fn get_order(&self, order: &OrderRef) -> std::result::Result<RemoteOrder, UpstreamError>;
    async fn cancel_order(&self, order: &OrderRef) -> std::result::Result<(), UpstreamError>;
    /// Downloads an image of the supplier (same origin as the base URL only, size-capped).
    async fn download(&self, url: &str) -> std::result::Result<Download, UpstreamError>;

    /// [`Capability::IncrementalChanges`]: changes after `since` (`None` = earliest).
    async fn changes(
        &self,
        _since: Option<&str>,
        _limit: i64,
    ) -> std::result::Result<ChangePage, UpstreamError> {
        Err(UpstreamError::Unsupported)
    }

    /// [`Capability::IncrementalChanges`]: the latest cursor (start point after a full sync).
    async fn change_head(&self) -> std::result::Result<String, UpstreamError> {
        Err(UpstreamError::Unsupported)
    }

    /// [`Capability::Quote`]: price-locking quote of order lines.
    async fn quote(&self, _lines: &[OrderLine]) -> std::result::Result<Quote, UpstreamError> {
        Err(UpstreamError::Unsupported)
    }

    /// [`Capability::PushEvents`]: registers our inbound endpoint with the supplier.
    async fn register_events(&self, _url: &str) -> std::result::Result<(), UpstreamError> {
        Err(UpstreamError::Unsupported)
    }
}

/// Builds clients for connections (one per call; cheap) through the registered
/// [`SupplierAdapter`]s.
pub trait UpstreamConnector: Send + Sync {
    fn open(&self, endpoint: &Endpoint) -> Result<Arc<dyn UpstreamClient>>;
    /// Every registered supplier system.
    fn adapters(&self) -> Vec<Arc<dyn SupplierAdapter>>;
    /// The adapter of a protocol id.
    fn adapter(&self, protocol: &str) -> Option<Arc<dyn SupplierAdapter>> {
        self.adapters()
            .into_iter()
            .find(|a| a.meta().id == protocol.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn lenient_product_parse() {
        let p: RemoteProduct = serde_json::from_value(json!({
            "id": "7",
            "title": {"zh-CN": "T"},
            "description": null,
            "price_amount": 12.5,
            "wholesale_prices": [{"min_quantity": 5, "unit_price": "8.00"}, "junk"],
            "manual_form_schema": null,
            "is_active": true,
            "skus": [{"id": 1, "price_amount": "abc", "stock_quantity": -1, "is_active": true}],
            "updated_at": "2026-01-02T03:04:05+08:00"
        }))
        .unwrap();
        assert_eq!(p.id, 7);
        assert_eq!(p.price_amount, "12.5");
        assert!(p.description.is_empty());
        assert_eq!(p.wholesale_prices.len(), 1);
        assert!(p.manual_form_schema.is_none());
        assert_eq!(p.skus[0].price_amount, "abc");
        assert_eq!(p.skus[0].stock_quantity, -1);
        assert!(p.updated_at.is_some());
    }
}
