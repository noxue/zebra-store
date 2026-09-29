//! Background job port (implemented by the database-backed queue in `zs-infra`).
//!
//! Job kinds mirror the original asynq task types (e.g. `order:timeout_cancel`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Result;

/// Well-known job kinds (same names as the original asynq tasks).
pub mod kinds {
    pub const ORDER_STATUS_EMAIL: &str = "order:status_email";
    pub const ORDER_AUTO_FULFILL: &str = "order:auto_fulfill";
    pub const ORDER_TIMEOUT_CANCEL: &str = "order:timeout_cancel";
    pub const WALLET_RECHARGE_EXPIRE: &str = "wallet_recharge:timeout_expire";
    pub const NOTIFICATION_DISPATCH: &str = "notification:dispatch";
    pub const AFFILIATE_CONFIRM: &str = "affiliate:confirm_commissions";
    pub const RESELLER_CONFIRM_LEDGER: &str = "reseller:confirm_ledger";
    pub const UPSTREAM_SYNC_STOCK: &str = "upstream:sync_stock";
    pub const PROCUREMENT_SUBMIT: &str = "procurement:submit";
    pub const PROCUREMENT_POLL: &str = "procurement:poll_status";
    pub const PROCUREMENT_SYNC_ACCEPTED: &str = "procurement:sync_accepted";
    pub const DOWNSTREAM_CALLBACK: &str = "downstream:callback";
    pub const RECONCILIATION_RUN: &str = "reconciliation:run";
    pub const BOT_NOTIFY: &str = "bot:notify";
    pub const TELEGRAM_BROADCAST: &str = "telegram:broadcast";
    pub const ALERT_CHECK: &str = "notification:alert_check";
    pub const UPSTREAM_SYNC_CONNECTION: &str = "upstream:sync_connection";
    pub const ZS_CATALOG_SNAPSHOT: &str = "zs:catalog_snapshot";
    pub const ZS_DELIVER_EVENT: &str = "zs:deliver_event";
    pub const CARD_CONVERTER_HEALTH_TICK: &str = "card_converter:health_tick";
}

/// A job to enqueue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub kind: String,
    pub payload: serde_json::Value,
    /// When to run; `None` = now.
    pub run_at: Option<DateTime<Utc>>,
    pub max_attempts: i32,
    /// Optional de-duplication key: a pending job with the same key is not enqueued twice.
    pub unique_key: Option<String>,
}

impl NewJob {
    /// Default retry budget for a job.
    pub const DEFAULT_ATTEMPTS: i32 = 3;

    pub fn new(kind: &str, payload: impl Serialize) -> Result<Self> {
        Ok(Self {
            kind: kind.to_owned(),
            payload: serde_json::to_value(payload)?,
            run_at: None,
            max_attempts: Self::DEFAULT_ATTEMPTS,
            unique_key: None,
        })
    }

    #[must_use]
    pub fn at(mut self, run_at: DateTime<Utc>) -> Self {
        self.run_at = Some(run_at);
        self
    }

    #[must_use]
    pub fn attempts(mut self, max: i32) -> Self {
        self.max_attempts = max;
        self
    }

    #[must_use]
    pub fn unique(mut self, key: impl Into<String>) -> Self {
        self.unique_key = Some(key.into());
        self
    }
}

/// Enqueues background jobs.
#[async_trait]
pub trait JobQueue: Send + Sync {
    async fn enqueue(&self, job: NewJob) -> Result<()>;
}

/// Handles one job kind. Returning an error schedules a retry (exponential backoff)
/// until `max_attempts` is reached.
#[async_trait]
pub trait JobHandler: Send + Sync {
    async fn handle(&self, payload: serde_json::Value) -> Result<()>;
}
