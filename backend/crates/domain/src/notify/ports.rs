//! Outbound delivery ports of the notify module.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::center::{InventoryAlertRow, NotifyEvent, PaymentOrderAlertCounts};
use crate::Result;

/// Message key of a failed delivery (original `error.notification_send_failed`).
pub const KEY_SEND_FAILED: &str = "error.notification_send_failed";
/// Raw message of an invalid notification config (original `ErrConfigInvalid`).
pub const MSG_CONFIG_INVALID: &str = "notification config invalid";

/// Plain-text email delivery that reports a disabled SMTP service as an error
/// (original `SendCustomEmail`).
#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<()>;
}

/// Telegram message options (original `SendOptions`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelegramMessage {
    pub chat_id: String,
    pub text: String,
    /// `HTML` for broadcasts, empty for plain text.
    pub parse_mode: String,
    pub disable_web_page_preview: bool,
    /// Remote URL or local `uploads/...` path.
    pub attachment_url: String,
    pub attachment_name: String,
}

/// Telegram Bot API delivery with an explicit bot token.
#[async_trait]
pub trait TelegramSender: Send + Sync {
    async fn send(&self, bot_token: &str, message: &TelegramMessage) -> Result<()>;
}

/// Feishu/Lark self-built app bot delivery.
#[async_trait]
pub trait FeishuSender: Send + Sync {
    async fn send(
        &self,
        app_id: &str,
        app_secret: &str,
        receive_id_type: &str,
        receive_id: &str,
        text: &str,
    ) -> Result<()>;
}

/// `SET NX EX` style guard used for de-duplication and alert intervals.
#[async_trait]
pub trait OnceGuard: Send + Sync {
    /// Returns `true` when `key` was free (and is now held for `ttl_seconds`).
    async fn acquire(&self, key: &str, ttl_seconds: i64) -> Result<bool>;
}

/// Data sources of the periodic alert check.
#[async_trait]
pub trait AlertSource: Send + Sync {
    async fn inventory_alerts(&self, low_stock_threshold: i64) -> Result<Vec<InventoryAlertRow>>;
    async fn payment_order_counts(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<PaymentOrderAlertCounts>;
}

/// Message key of an outbound request refused by the SSRF guard.
pub const KEY_FORBIDDEN_ADDRESS: &str = "forbidden address";

/// POSTs to peer-configured URLs. Implementations only connect to public IPs
/// and never follow redirects (UPS-01); a refused target is
/// `ErrorKind::Forbidden` with [`KEY_FORBIDDEN_ADDRESS`].
#[async_trait]
pub trait CallbackPoster: Send + Sync {
    /// Returns the HTTP status of the response.
    async fn post_json(&self, url: &str, headers: &[(String, String)], body: &[u8]) -> Result<u16>;
}

/// Entry point other modules use to raise admin notifications; implemented by
/// the notify application layer on top of the job queue (`notification:dispatch`).
#[async_trait]
pub trait Notifier: Send + Sync {
    /// Queues `event`; unsupported events are rejected with `error.bad_request`.
    async fn notify(&self, event: NotifyEvent) -> Result<()>;
}
