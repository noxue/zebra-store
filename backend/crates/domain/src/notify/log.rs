//! Notification delivery log (`notification_logs`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

/// Log status values.
pub mod status {
    pub const SUCCESS: &str = "success";
    pub const FAILED: &str = "failed";
}

/// One delivery attempt (JSON shape of the original `NotificationLog`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NotificationLog {
    pub id: Id,
    pub event_type: String,
    pub biz_type: String,
    pub biz_id: i64,
    pub channel: String,
    pub recipient: String,
    pub locale: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub error_message: String,
    pub is_test: bool,
    pub variables: Value,
    #[serde(serialize_with = "crate::notify::rfc3339")]
    pub created_at: DateTime<Utc>,
}

/// A log row to insert.
#[derive(Debug, Clone, PartialEq)]
pub struct NewNotificationLog {
    pub event_type: String,
    pub biz_type: String,
    pub biz_id: i64,
    pub channel: String,
    pub recipient: String,
    pub locale: String,
    pub title: String,
    pub body: String,
    pub status: String,
    pub error_message: String,
    pub is_test: bool,
    pub variables: Value,
    pub created_at: DateTime<Utc>,
}

/// Admin log list filter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogFilter {
    pub page: PageRequest,
    pub channel: String,
    pub status: String,
    pub event_type: String,
    pub is_test: Option<bool>,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Persistence of notification logs.
#[async_trait]
pub trait NotificationLogRepo: Send + Sync {
    async fn create(&self, log: &NewNotificationLog) -> Result<()>;
    /// Ordered by `id DESC`.
    async fn list(&self, filter: &LogFilter) -> Result<Page<NotificationLog>>;
}
