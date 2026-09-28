//! Site connections: upstream suppliers this shop buys from (secret stored encrypted).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
pub use rust_decimal::Decimal;
use serde::{Serialize, Serializer};
use zs_shared::page::{Page, PageRequest};

use super::keys;
use super::pricing::{Pricing, RoundingMode};
use crate::{Error, Id, Result};

/// Default compatibility protocol; every other protocol id
/// must be registered in the adapter registry (checked by the connection service).
pub const PROTOCOL_DUJIAO_NEXT: &str = "dujiao-next";
/// Longest protocol id.
const MAX_PROTOCOL_LEN: usize = 32;
/// Default retry budget of purchase order submission.
pub const DEFAULT_RETRY_MAX: i32 = 5;
/// Default retry intervals (seconds, JSON array text).
pub const DEFAULT_RETRY_INTERVALS: &str = "[30,60,300]";

/// Connection status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Pending,
    Active,
    Disabled,
}

impl ConnectionStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "pending" => Some(Self::Pending),
            "active" => Some(Self::Active),
            "disabled" => Some(Self::Disabled),
            _ => None,
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        Self::parse(raw).unwrap_or(Self::Pending)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }
}

/// Serializes a decimal the way shopspring's `decimal.Decimal` does (quoted, minimal digits).
pub fn serialize_decimal<S: Serializer>(v: &Decimal, s: S) -> std::result::Result<S::Ok, S::Error> {
    s.collect_str(&v.normalize())
}

/// A site connection (admin JSON shape; the secret is never serialized).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SiteConnection {
    pub id: Id,
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    /// AES-GCM encrypted secret.
    #[serde(skip)]
    pub api_secret: String,
    pub protocol: String,
    pub callback_url: String,
    pub status: ConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ping_at: Option<DateTime<Utc>>,
    pub last_ping_ok: bool,
    pub retry_max: i32,
    pub retry_intervals: String,
    #[serde(serialize_with = "serialize_decimal")]
    pub exchange_rate: Decimal,
    #[serde(serialize_with = "serialize_decimal")]
    pub price_markup_percent: Decimal,
    pub price_rounding_mode: RoundingMode,
    pub auto_sync_price: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Negotiated adapter state (extra table; flattened into the admin JSON).
    #[serde(flatten)]
    pub state: ConnectionState,
}

/// How a connection's catalog is kept in sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    /// Cursor-driven change feed.
    Incremental,
    /// Periodic paginated listing.
    #[default]
    Full,
}

/// Push-event registration of a connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WebhookStatus {
    /// Not attempted yet.
    #[default]
    None,
    Registered,
    Failed,
    /// The supplier (or this connection) has no push events.
    Unsupported,
}

impl WebhookStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Registered => "registered",
            Self::Failed => "failed",
            Self::Unsupported => "unsupported",
        }
    }

    pub fn from_stored(raw: &str) -> Self {
        match raw.trim() {
            "registered" => Self::Registered,
            "failed" => Self::Failed,
            "unsupported" => Self::Unsupported,
            _ => Self::None,
        }
    }
}

/// Adapter state negotiated with the supplier (handshake, change cursor, events).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct ConnectionState {
    /// Supplier feature names from the last handshake.
    pub features: Vec<String>,
    /// Neutral capabilities in effect for this connection.
    pub capabilities: Vec<String>,
    pub supplier_currency: String,
    pub sync_mode: SyncMode,
    /// Numeric view of the change cursor (0 when none).
    pub last_change_seq: i64,
    pub webhook_status: WebhookStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_handshake_at: Option<DateTime<Utc>>,
    /// Error of the last failed handshake (empty after a successful one).
    pub handshake_error: String,
    /// Opaque change-feed cursor (empty = no incremental sync yet).
    #[serde(skip)]
    pub change_cursor: String,
    /// True once a handshake stored `features`.
    #[serde(skip)]
    pub negotiated: bool,
    /// Adapter-specific configuration (JSON on the extra table; `secret` fields are
    /// masked in admin views).
    pub extra: crate::catalog::product::JsonMap,
}

impl ConnectionState {
    /// Sets the cursor and its numeric view.
    pub fn set_cursor(&mut self, cursor: &str) {
        self.change_cursor = cursor.to_owned();
        self.last_change_seq = cursor.trim().parse().unwrap_or(0);
    }
}

impl SiteConnection {
    pub fn pricing(&self) -> Pricing {
        Pricing {
            exchange_rate: self.exchange_rate,
            markup_percent: self.price_markup_percent,
            rounding: self.price_rounding_mode,
        }
    }
}

/// Create request (decimals already parsed; absent values are `None`).
#[derive(Debug, Clone, Default)]
pub struct ConnectionInput {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    pub protocol: String,
    pub callback_url: String,
    pub retry_max: i32,
    pub retry_intervals: String,
    pub exchange_rate: Option<Decimal>,
    pub price_markup_percent: Option<Decimal>,
    pub price_rounding_mode: Option<String>,
    pub auto_sync_price: Option<bool>,
    /// Adapter-specific configuration fields (`None` keeps the stored ones).
    pub extra: Option<crate::catalog::product::JsonMap>,
}

fn invalid() -> Error {
    Error::bad_request(keys::CONNECTION_INVALID)
}

/// Normalized base URL: trimmed, no trailing slash, `http(s)://host…` only.
pub fn normalize_base_url(raw: &str) -> Result<String> {
    let url = raw.trim().trim_end_matches('/').to_owned();
    let rest = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .ok_or_else(invalid)?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || host.contains('@') || url.contains(char::is_whitespace) {
        return Err(invalid());
    }
    Ok(url)
}

/// Callback URL handed to the supplier: empty or an absolute http(s) URL.
fn normalize_callback_url(raw: &str) -> Result<String> {
    let url = raw.trim();
    if url.is_empty() {
        return Ok(String::new());
    }
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(invalid());
    }
    Ok(url.to_owned())
}

/// Validates the retry intervals text (`[30,60,300]` or `30,60,300`) and returns it canonical.
pub fn normalize_retry_intervals(raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_RETRY_INTERVALS.to_owned());
    }
    let inner = trimmed.trim_start_matches('[').trim_end_matches(']');
    let mut values = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let v: u32 = part.parse().map_err(|_| invalid())?;
        if v == 0 {
            return Err(invalid());
        }
        values.push(v.to_string());
    }
    if values.is_empty() {
        return Ok(DEFAULT_RETRY_INTERVALS.to_owned());
    }
    Ok(format!("[{}]", values.join(",")))
}

/// Exchange rate rule: absent → keep default 1; must be positive (UPS-05).
fn check_rate(rate: Decimal) -> Result<Decimal> {
    if rate <= Decimal::ZERO {
        return Err(invalid());
    }
    Ok(rate.normalize())
}

/// Markup rule: strictly above -100 % so a local price can never drop to zero (UPS-05).
fn check_markup(markup: Decimal) -> Result<Decimal> {
    if markup <= -Decimal::ONE_HUNDRED {
        return Err(invalid());
    }
    Ok(markup.round_dp(4).normalize())
}

fn check_rounding(raw: &str) -> Result<RoundingMode> {
    RoundingMode::parse_input(raw).ok_or_else(invalid)
}

/// Protocol id syntax (`[a-z0-9-]`, empty selects the legacy default); whether an adapter exists
/// for it is checked by the connection service against the registry.
fn check_protocol(raw: &str) -> Result<String> {
    let p = raw.trim();
    if p.is_empty() {
        return Ok(PROTOCOL_DUJIAO_NEXT.to_owned());
    }
    if p.len() > MAX_PROTOCOL_LEN
        || !p
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(invalid());
    }
    Ok(p.to_owned())
}

/// A validated new connection (secret still plain; the service encrypts it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewConnection {
    pub name: String,
    pub base_url: String,
    pub api_key: String,
    pub api_secret_plain: String,
    pub protocol: String,
    pub callback_url: String,
    pub retry_max: i32,
    pub retry_intervals: String,
    pub pricing: Pricing,
    pub auto_sync_price: bool,
}

impl ConnectionInput {
    /// Create validation (original `Create` plus UPS-05 bounds).
    pub fn validate_new(&self) -> Result<NewConnection> {
        let name = self.name.trim();
        let key = self.api_key.trim();
        if name.is_empty() || key.is_empty() || self.api_secret.trim().is_empty() {
            return Err(invalid());
        }
        Ok(NewConnection {
            name: name.to_owned(),
            base_url: normalize_base_url(&self.base_url)?,
            api_key: key.to_owned(),
            api_secret_plain: self.api_secret.clone(),
            protocol: check_protocol(&self.protocol)?,
            callback_url: normalize_callback_url(&self.callback_url)?,
            retry_max: if self.retry_max > 0 {
                self.retry_max
            } else {
                DEFAULT_RETRY_MAX
            },
            retry_intervals: normalize_retry_intervals(&self.retry_intervals)?,
            pricing: Pricing {
                exchange_rate: match self.exchange_rate {
                    Some(r) => check_rate(r)?,
                    None => Decimal::ONE,
                },
                markup_percent: match self.price_markup_percent {
                    Some(m) => check_markup(m)?,
                    None => Decimal::ZERO,
                },
                rounding: check_rounding(self.price_rounding_mode.as_deref().unwrap_or(""))?,
            },
            auto_sync_price: self.auto_sync_price.unwrap_or(false),
        })
    }

    /// Update: blank strings / absent values keep the current value (original `Update`).
    /// Returns the new secret (plain) when one was supplied.
    pub fn apply_update(&self, conn: &mut SiteConnection) -> Result<Option<String>> {
        if !self.name.trim().is_empty() {
            conn.name = self.name.trim().to_owned();
        }
        if !self.base_url.trim().is_empty() {
            conn.base_url = normalize_base_url(&self.base_url)?;
        }
        if !self.api_key.trim().is_empty() {
            conn.api_key = self.api_key.trim().to_owned();
        }
        if !self.protocol.trim().is_empty() {
            conn.protocol = check_protocol(&self.protocol)?;
        }
        if !self.callback_url.is_empty() {
            conn.callback_url = normalize_callback_url(&self.callback_url)?;
        }
        if self.retry_max > 0 {
            conn.retry_max = self.retry_max;
        }
        if !self.retry_intervals.trim().is_empty() {
            conn.retry_intervals = normalize_retry_intervals(&self.retry_intervals)?;
        }
        if let Some(r) = self.exchange_rate {
            conn.exchange_rate = check_rate(r)?;
        }
        if let Some(m) = self.price_markup_percent {
            conn.price_markup_percent = check_markup(m)?;
        }
        if let Some(mode) = &self.price_rounding_mode {
            conn.price_rounding_mode = check_rounding(mode)?;
        }
        if let Some(a) = self.auto_sync_price {
            conn.auto_sync_price = a;
        }
        let secret = self.api_secret.trim();
        Ok((!secret.is_empty()).then(|| self.api_secret.clone()))
    }
}

/// True when a pricing parameter changed by value (decimal equality, UPS-05 (2)).
pub fn pricing_changed(before: &Pricing, after: &Pricing) -> bool {
    before.exchange_rate != after.exchange_rate
        || before.markup_percent != after.markup_percent
        || before.rounding != after.rounding
}

/// Connection plus its decrypted secret, ready for an outbound client (`Debug` redacts
/// the secret and the adapter configuration values).
#[derive(Clone, PartialEq, Eq, Default)]
pub struct Endpoint {
    pub connection_id: Id,
    pub base_url: String,
    pub api_key: String,
    pub api_secret: String,
    pub protocol: String,
    /// Supplier features negotiated by a handshake (`None` = never negotiated).
    pub features: Option<Vec<String>>,
    /// Adapter-specific configuration.
    pub extra: crate::catalog::product::JsonMap,
}

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint")
            .field("connection_id", &self.connection_id)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key)
            .field("api_secret", &"<redacted>")
            .field("protocol", &self.protocol)
            .field("features", &self.features)
            .field("extra_keys", &self.extra.keys().collect::<Vec<_>>())
            .finish()
    }
}

/// Persistence of `site_connections`.
#[async_trait]
pub trait ConnectionRepo: Send + Sync {
    async fn get(&self, id: Id) -> Result<Option<SiteConnection>>;
    /// Live connection by its API key (any status).
    async fn find_by_key(&self, api_key: &str) -> Result<Option<SiteConnection>>;
    async fn create(
        &self,
        c: &NewConnection,
        encrypted_secret: &str,
        now: DateTime<Utc>,
    ) -> Result<SiteConnection>;
    async fn save(&self, c: &SiteConnection, now: DateTime<Utc>) -> Result<()>;
    async fn delete(&self, id: Id, at: DateTime<Utc>) -> Result<()>;
    async fn list(&self, page: PageRequest, status: &str) -> Result<Page<SiteConnection>>;
    /// Stores the negotiated adapter state of a connection.
    async fn save_state(&self, id: Id, state: &ConnectionState, now: DateTime<Utc>) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ConnectionInput {
        ConnectionInput {
            name: " A ".into(),
            base_url: "https://a.example.com/".into(),
            api_key: "k".into(),
            api_secret: "s".into(),
            ..ConnectionInput::default()
        }
    }

    #[test]
    fn create_defaults() {
        let c = input().validate_new().unwrap();
        assert_eq!(c.name, "A");
        assert_eq!(c.base_url, "https://a.example.com");
        assert_eq!(c.protocol, PROTOCOL_DUJIAO_NEXT);
        assert_eq!(c.retry_max, 5);
        assert_eq!(c.retry_intervals, "[30,60,300]");
        assert_eq!(c.pricing, Pricing::default());
    }

    // UPS-05 (1): markup <= -100 %, non-positive rate and unknown rounding are refused.
    #[test]
    fn ups05_rejects_bad_pricing() {
        for bad in [
            ConnectionInput {
                price_markup_percent: Some("-150".parse().unwrap()),
                ..input()
            },
            ConnectionInput {
                price_markup_percent: Some("-100".parse().unwrap()),
                ..input()
            },
            ConnectionInput {
                exchange_rate: Some("0".parse().unwrap()),
                ..input()
            },
            ConnectionInput {
                price_rounding_mode: Some("floor".into()),
                ..input()
            },
            ConnectionInput {
                base_url: "ftp://x".into(),
                ..input()
            },
            ConnectionInput {
                retry_intervals: "[30,abc]".into(),
                ..input()
            },
            ConnectionInput {
                api_secret: " ".into(),
                ..input()
            },
        ] {
            let err = bad.validate_new().unwrap_err();
            assert_eq!(err.key(), keys::CONNECTION_INVALID);
        }
    }

    // UPS-05 (2): equal decimals (1 vs 1.0) are not a change.
    #[test]
    fn ups05_pricing_change_is_by_value() {
        let a = Pricing {
            exchange_rate: "1".parse().unwrap(),
            ..Pricing::default()
        };
        let b = Pricing {
            exchange_rate: "1.000000".parse().unwrap(),
            ..Pricing::default()
        };
        assert!(!pricing_changed(&a, &b));
        let c = Pricing {
            exchange_rate: "6.9".parse().unwrap(),
            ..Pricing::default()
        };
        assert!(pricing_changed(&a, &c));
    }

    #[test]
    fn base_url_rules() {
        assert_eq!(
            normalize_base_url(" http://h:8080/ ").unwrap(),
            "http://h:8080"
        );
        assert!(normalize_base_url("http://").is_err());
        assert!(normalize_base_url("http://u@h").is_err());
        assert_eq!(normalize_retry_intervals("10, 20").unwrap(), "[10,20]");
    }
}
