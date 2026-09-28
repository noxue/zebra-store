//! Telegram broadcasts (port of `modules/telegram/broadcast`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use zs_shared::page::{Page, PageRequest};

use crate::{Id, Result};

/// Recipient types.
pub mod recipient_types {
    pub const ALL: &str = "all";
    pub const SPECIFIC: &str = "specific";
}

/// Broadcast statuses.
pub mod statuses {
    pub const PENDING: &str = "pending";
    pub const RUNNING: &str = "running";
    pub const COMPLETED: &str = "completed";
    pub const FAILED: &str = "failed";
}

/// Error message of a broadcast without recipients (original `ErrNoRecipients`).
pub const MSG_NO_RECIPIENTS: &str = "telegram broadcast no recipients";
/// Error message when no bot token is configured (original `ErrTokenUnavailable`).
pub const MSG_TOKEN_UNAVAILABLE: &str = "telegram bot token unavailable";

/// A broadcast job (JSON shape of the original `Broadcast`; chat ids are hidden).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Broadcast {
    pub id: Id,
    pub title: String,
    pub recipient_type: String,
    #[serde(rename = "filters")]
    pub filters: Value,
    #[serde(skip)]
    pub recipient_chat_ids: Vec<String>,
    pub recipient_count: i32,
    pub success_count: i32,
    pub failed_count: i32,
    pub status: String,
    pub message_html: String,
    pub attachment_url: String,
    pub attachment_name: String,
    #[serde(serialize_with = "crate::notify::rfc3339_opt")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(serialize_with = "crate::notify::rfc3339_opt")]
    pub completed_at: Option<DateTime<Utc>>,
    pub last_error: String,
    #[serde(serialize_with = "crate::notify::rfc3339")]
    pub created_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::notify::rfc3339")]
    pub updated_at: DateTime<Utc>,
}

/// A Telegram-bound user selectable as a recipient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TelegramUser {
    pub user_id: Id,
    pub display_name: String,
    pub user_email: String,
    pub telegram_username: String,
    pub telegram_user_id: String,
    #[serde(serialize_with = "crate::notify::rfc3339")]
    pub bound_at: DateTime<Utc>,
    #[serde(serialize_with = "crate::notify::rfc3339")]
    pub user_created_at: DateTime<Utc>,
}

/// Recipient directory query; `page = None` returns every match.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TelegramUserQuery {
    pub page: Option<PageRequest>,
    pub user_ids: Vec<Id>,
    pub keyword: String,
    pub display_name: String,
    pub telegram_username: String,
    pub telegram_user_id: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Broadcast list filter (admin page filters).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BroadcastFilter {
    pub page: PageRequest,
    pub keyword: String,
    pub recipient_type: String,
    pub status: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Persistence of broadcasts.
#[async_trait]
pub trait BroadcastRepo: Send + Sync {
    async fn create(&self, b: &Broadcast) -> Result<Broadcast>;
    async fn get(&self, id: Id) -> Result<Option<Broadcast>>;
    /// Ordered by `created_at DESC`.
    async fn list(&self, filter: &BroadcastFilter) -> Result<Page<Broadcast>>;
    async fn update(&self, b: &Broadcast) -> Result<()>;
    async fn soft_delete(&self, id: Id, at: DateTime<Utc>) -> Result<bool>;
    async fn telegram_users(&self, q: &TelegramUserQuery) -> Result<Page<TelegramUser>>;
}

/// Trimmed, de-duplicated, non-empty values in first-seen order.
pub fn dedupe_strings<I: IntoIterator<Item = String>>(items: I) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let t = item.trim().to_owned();
        if !t.is_empty() && !out.contains(&t) {
            out.push(t);
        }
    }
    out
}

/// Positive, de-duplicated ids in first-seen order.
pub fn unique_ids(ids: &[Id]) -> Vec<Id> {
    let mut out = Vec::new();
    for id in ids {
        if *id > 0 && !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// True for image attachments, which are sent with `sendPhoto` (NTF-06).
pub fn is_photo_attachment(url: &str, name: &str) -> bool {
    [name, url].iter().any(|candidate| {
        let c = candidate.trim();
        if c.is_empty() {
            return false;
        }
        let path = c.split(['?', '#']).next().unwrap_or(c);
        let ext = path
            .rsplit_once('.')
            .map(|(_, e)| e.to_ascii_lowercase())
            .unwrap_or_default();
        matches!(
            ext.as_str(),
            "jpg"
                | "jpeg"
                | "png"
                | "webp"
                | "gif"
                | "bmp"
                | "svg"
                | "tif"
                | "tiff"
                | "avif"
                | "ico"
        )
    })
}

/// Local `uploads/...` path of an attachment (no scheme, no `..`), sent as a
/// multipart upload instead of a URL.
pub fn local_attachment_path(url: &str) -> Option<String> {
    let v = url.trim();
    if v.is_empty() || v.contains("://") {
        return None;
    }
    let normalized = v.trim_start_matches('/');
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|p| !p.is_empty() && *p != ".")
        .collect();
    if parts.first() != Some(&"uploads")
        || parts.contains(&"..")
        || parts.iter().any(|p| p.contains('\\'))
    {
        return None;
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_detection() {
        assert!(is_photo_attachment("https://x/a.JPG?x=1", ""));
        assert!(is_photo_attachment("https://x/file", "cover.webp"));
        assert!(!is_photo_attachment("https://x/a.zip", "a.zip"));
    }

    #[test]
    fn local_paths() {
        assert_eq!(
            local_attachment_path("/uploads/telegram/a.png").as_deref(),
            Some("uploads/telegram/a.png")
        );
        assert_eq!(local_attachment_path("https://x/uploads/a.png"), None);
        assert_eq!(local_attachment_path("/uploads/../etc/passwd"), None);
        assert_eq!(local_attachment_path("/etc/passwd"), None);
    }

    #[test]
    fn dedupes() {
        assert_eq!(
            dedupe_strings(vec![" 1 ".into(), "1".into(), "".into(), "2".into()]),
            vec!["1", "2"]
        );
        assert_eq!(unique_ids(&[3, 0, 3, -1, 2]), vec![3, 2]);
    }
}
