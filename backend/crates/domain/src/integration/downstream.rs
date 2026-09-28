//! Callbacks to downstream shops about orders they placed through our upstream API
//! (original `modules/downstreamcallback`, UPS-01/UPS-12).

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::protocol::{CallbackPayload, RemoteFulfillment};
use crate::{Id, Result};

/// Retries before a callback is marked failed (original `maxCallbackRetries`).
pub const MAX_CALLBACK_RETRIES: i32 = 5;
/// Delays between attempts (original `callbackRetryDelays`, last one repeated).
pub const CALLBACK_RETRY_DELAYS_SECS: [i64; 4] = [30, 60, 120, 300];
/// Path signed by callbacks (the receiver verifies against its fixed route).
pub const CALLBACK_SIGN_PATH: &str = "/api/v1/upstream/callback";

/// Callback delivery state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallbackStatus {
    Pending,
    Sent,
    Failed,
}

impl CallbackStatus {
    pub fn from_stored(raw: &str) -> Self {
        match raw.trim() {
            "sent" => Self::Sent,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Sent => "sent",
            Self::Failed => "failed",
        }
    }
}

/// `downstream_order_refs` row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrderRef {
    pub id: Id,
    pub order_id: Id,
    pub api_credential_id: Id,
    pub downstream_order_no: String,
    pub callback_url: String,
    pub trace_id: String,
    pub callback_status: CallbackStatus,
    pub callback_retry_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_callback_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Minimal order projection for callbacks.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OrderSnapshot {
    pub id: Id,
    pub order_no: String,
    pub parent_id: Option<Id>,
    pub status: String,
    pub fulfillment: Option<RemoteFulfillment>,
    pub children: Vec<OrderSnapshot>,
}

/// The delivered fulfillment of an order, else of its first child with one (UPS-12).
pub fn delivered_fulfillment(order: &OrderSnapshot) -> Option<RemoteFulfillment> {
    let source = order
        .fulfillment
        .as_ref()
        .or_else(|| order.children.iter().find_map(|c| c.fulfillment.as_ref()))?;
    (source.status == "delivered").then(|| source.clone())
}

/// Builds the callback body for the current order state.
pub fn build_payload(order: &OrderSnapshot, r: &OrderRef, now: DateTime<Utc>) -> CallbackPayload {
    let event = if matches!(order.status.as_str(), "delivered" | "completed") {
        "order.fulfilled"
    } else {
        "order.status_changed"
    };
    CallbackPayload {
        event: event.to_owned(),
        order_id: order.id,
        order_no: order.order_no.clone(),
        downstream_order_no: r.downstream_order_no.clone(),
        status: order.status.clone(),
        fulfillment: delivered_fulfillment(order),
        timestamp: now.timestamp(),
    }
}

/// Delay before the next attempt after `retry_count` failures, `None` when exhausted.
pub fn next_retry_delay(retry_count: i32) -> Option<Duration> {
    if retry_count >= MAX_CALLBACK_RETRIES {
        return None;
    }
    let idx = usize::try_from((retry_count - 1).max(0)).unwrap_or(0);
    let secs = CALLBACK_RETRY_DELAYS_SECS
        .get(idx)
        .or(CALLBACK_RETRY_DELAYS_SECS.last())
        .copied()
        .unwrap_or(300);
    Some(Duration::seconds(secs))
}

/// A new reference (written by the order group inside its order transaction).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewOrderRef {
    pub order_id: Id,
    pub api_credential_id: Id,
    pub downstream_order_no: String,
    pub callback_url: String,
    pub trace_id: String,
}

/// Persistence of `downstream_order_refs`.
#[async_trait]
pub trait OrderRefRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<OrderRef>>;
    async fn get_by_order(&self, order_id: Id) -> Result<Option<OrderRef>>;
    async fn find_by_downstream_no(
        &self,
        credential_id: Id,
        downstream_order_no: &str,
    ) -> Result<Option<OrderRef>>;
    /// Sets status / retry count / last attempt.
    async fn update_delivery(
        &self,
        id: Id,
        status: CallbackStatus,
        retry_count: i32,
        last_callback_at: Option<DateTime<Utc>>,
        now: DateTime<Utc>,
    ) -> Result<()>;
}

/// Order projection for callbacks (reads the order tables).
#[async_trait]
pub trait CallbackOrders: Send + Sync {
    async fn snapshot(&self, order_id: Id) -> Result<Option<OrderSnapshot>>;
}

/// Sends a signed callback; `Ok` only for HTTP 200 with `{"ok":true}`. Implementations
/// must be SSRF-safe (public addresses only, no redirects, timeouts, UPS-01).
#[async_trait]
pub trait CallbackSender: Send + Sync {
    async fn send(
        &self,
        url: &str,
        api_key: &str,
        api_secret: &str,
        payload: &CallbackPayload,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fulfillment(status: &str) -> RemoteFulfillment {
        RemoteFulfillment {
            kind: "auto".into(),
            status: status.into(),
            payload: "CARD".into(),
            ..RemoteFulfillment::default()
        }
    }

    // UPS-12: the parent's callback carries the child's delivery.
    #[test]
    fn ups12_child_fulfillment_is_used() {
        let order = OrderSnapshot {
            id: 1,
            order_no: "P1".into(),
            status: "delivered".into(),
            children: vec![
                OrderSnapshot {
                    id: 2,
                    ..OrderSnapshot::default()
                },
                OrderSnapshot {
                    id: 3,
                    fulfillment: Some(fulfillment("delivered")),
                    ..OrderSnapshot::default()
                },
            ],
            ..OrderSnapshot::default()
        };
        let r = OrderRef {
            id: 1,
            order_id: 1,
            api_credential_id: 1,
            downstream_order_no: "D1".into(),
            callback_url: "https://x".into(),
            trace_id: String::new(),
            callback_status: CallbackStatus::Pending,
            callback_retry_count: 0,
            last_callback_at: None,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        };
        let p = build_payload(&order, &r, DateTime::<Utc>::MIN_UTC);
        assert_eq!(p.event, "order.fulfilled");
        assert_eq!(p.downstream_order_no, "D1");
        assert_eq!(p.fulfillment.map(|f| f.payload), Some("CARD".into()));

        let pending = OrderSnapshot {
            status: "paid".into(),
            fulfillment: Some(fulfillment("pending")),
            ..order
        };
        let p = build_payload(&pending, &r, DateTime::<Utc>::MIN_UTC);
        assert_eq!(p.event, "order.status_changed");
        assert!(p.fulfillment.is_none());
    }

    #[test]
    fn retry_schedule() {
        assert_eq!(next_retry_delay(1), Some(Duration::seconds(30)));
        assert_eq!(next_retry_delay(4), Some(Duration::seconds(300)));
        assert_eq!(next_retry_delay(5), None);
    }
}
