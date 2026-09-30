//! Durable original-route gateway refund attempts.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Map;
use zs_shared::money::Amount;

use crate::{Id, Result};

pub mod status {
    pub const PENDING: &str = "pending";
    pub const FAILED: &str = "failed";
    pub const SUCCEEDED: &str = "succeeded";
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GatewayRefundAttempt {
    pub id: Id,
    pub order_id: Id,
    pub payment_id: Id,
    pub channel_id: Id,
    pub request_date: String,
    pub refund_no: String,
    pub provider_ref: String,
    pub amount: Amount,
    pub status: String,
    pub remark: String,
    pub payment_fee_refunded: bool,
    pub payload: Map<String, serde_json::Value>,
    pub error: String,
    pub refund_record_id: Option<Id>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct NewGatewayRefundAttempt {
    pub order_id: Id,
    pub payment_id: Id,
    pub channel_id: Id,
    pub request_date: String,
    pub refund_no: String,
    pub provider_ref: String,
    pub amount: Amount,
    pub remark: String,
    pub payment_fee_refunded: bool,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct GatewayRefundAttemptUpdate {
    pub status: String,
    pub payload: Map<String, serde_json::Value>,
    pub error: String,
    pub now: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct ReservedGatewayRefund {
    pub attempt: GatewayRefundAttempt,
    pub created: bool,
}

/// Persistence for gateway refund attempts. `reserve` serializes per-payment requests and
/// returns an existing pending attempt instead of creating another refund at the provider.
#[async_trait]
pub trait GatewayRefundStore: Send + Sync {
    async fn reserve(&self, input: &NewGatewayRefundAttempt) -> Result<ReservedGatewayRefund>;
    async fn get(&self, id: Id) -> Result<Option<GatewayRefundAttempt>>;
    async fn list_for_order(&self, order_id: Id) -> Result<Vec<GatewayRefundAttempt>>;
    async fn update(
        &self,
        id: Id,
        update: &GatewayRefundAttemptUpdate,
    ) -> Result<GatewayRefundAttempt>;
}
