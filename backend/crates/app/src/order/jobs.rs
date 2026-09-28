//! Job handlers of the order group (`order:auto_fulfill`, `order:timeout_cancel`,
//! `order:status_email`).

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use zs_domain::queue::JobHandler;
use zs_domain::{Id, Result};

use super::OrderService;
use super::email::StatusEmailPayload;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct OrderPayload {
    order_id: Id,
}

fn order_id_of(payload: Value) -> Id {
    serde_json::from_value::<OrderPayload>(payload)
        .map(|p| p.order_id)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "order_job_payload_malformed");
            0
        })
}

/// `order:auto_fulfill`.
#[derive(Debug, Clone)]
pub struct AutoFulfillJob(pub OrderService);

#[async_trait]
impl JobHandler for AutoFulfillJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        self.0.auto_fulfill(order_id_of(payload)).await
    }
}

/// `order:timeout_cancel`.
#[derive(Debug, Clone)]
pub struct TimeoutCancelJob(pub OrderService);

#[async_trait]
impl JobHandler for TimeoutCancelJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let order_id = order_id_of(payload);
        if order_id <= 0 {
            return Ok(());
        }
        self.0.cancel_expired(order_id).await
    }
}

/// `order:status_email`.
#[derive(Debug, Clone)]
pub struct StatusEmailJob(pub OrderService);

#[async_trait]
impl JobHandler for StatusEmailJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let payload: StatusEmailPayload = serde_json::from_value(payload).unwrap_or_default();
        self.0.send_status_email(&payload).await
    }
}
