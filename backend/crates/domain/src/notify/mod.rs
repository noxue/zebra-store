//! `notify` domain: the notification center, channel clients (Telegram bot
//! authentication), Telegram identities, broadcasts and bot callbacks.

pub mod broadcast;
pub mod center;
pub mod channel;
pub mod inventory;
pub mod log;
pub mod ports;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serializer;

/// Serializes a timestamp as RFC 3339 with seconds (original `time.Time` JSON).
pub fn rfc3339<S: Serializer>(t: &DateTime<Utc>, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

/// Optional variant of [`rfc3339`] (`null` when absent).
pub fn rfc3339_opt<S: Serializer>(t: &Option<DateTime<Utc>>, s: S) -> Result<S::Ok, S::Error> {
    match t {
        Some(t) => rfc3339(t, s),
        None => s.serialize_none(),
    }
}
