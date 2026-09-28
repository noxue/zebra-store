//! Channel clients (external bots authenticating against `/api/v1/channel/*`)
//! and the Telegram identities they map to shop users (port of
//! `modules/channelclient` and `userauth/telegram_channel.go`).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::{Error, Id, Result};

/// Channel type of the Telegram bot client.
pub const CHANNEL_TYPE_TELEGRAM_BOT: &str = "telegram_bot";
/// Active status value.
pub const STATUS_ACTIVE: i32 = 1;
/// Disabled status value.
pub const STATUS_DISABLED: i32 = 0;
/// Random bytes of generated keys and secrets (hex-encoded to 64 chars).
pub const CREDENTIAL_BYTES: usize = 32;
/// OAuth provider of Telegram identities.
pub const PROVIDER_TELEGRAM: &str = "telegram";

/// Message keys.
pub mod keys {
    pub const LIST_FAILED: &str = "error.channel_clients_fetch_failed";
    pub const FETCH_FAILED: &str = "error.channel_client_fetch_failed";
    pub const CREATE_FAILED: &str = "error.channel_client_create_failed";
    pub const UPDATE_FAILED: &str = "error.channel_client_update_failed";
    pub const RESET_FAILED: &str = "error.channel_client_reset_secret_failed";
    pub const DELETE_FAILED: &str = "error.channel_client_delete_failed";
}

/// A stored channel client (secret and bot token encrypted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelClient {
    pub id: Id,
    pub name: String,
    pub channel_type: String,
    pub channel_key: String,
    /// AES-GCM ciphertext.
    pub channel_secret: String,
    /// AES-GCM ciphertext, empty when unset.
    pub bot_token: String,
    pub callback_url: String,
    pub status: i32,
    pub description: String,
    pub last_used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl ChannelClient {
    pub fn is_active(&self) -> bool {
        self.status == STATUS_ACTIVE
    }
}

/// Admin view with the decrypted secret and a masked bot token (original `ClientDetail`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClientDetail {
    pub id: Id,
    pub name: String,
    pub channel_type: String,
    pub channel_key: String,
    pub channel_secret: String,
    pub bot_token: String,
    pub bot_token_set: bool,
    pub callback_url: String,
    pub description: String,
    pub status: i32,
}

/// Masks a bot token: first and last 4 characters kept (all masked when ≤ 12 chars).
pub fn mask_bot_token(token: &str) -> String {
    let chars: Vec<char> = token.chars().collect();
    let n = chars.len();
    if n == 0 {
        return String::new();
    }
    if n <= 12 {
        return "*".repeat(n);
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[n - 4..].iter().collect();
    format!("{head}{}{tail}", "*".repeat(n - 8))
}

/// Fields of a new channel client (ciphertexts already computed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChannelClient {
    pub name: String,
    pub channel_type: String,
    pub channel_key: String,
    pub channel_secret: String,
    pub bot_token: String,
    pub callback_url: String,
    pub description: String,
}

/// Persistence of channel clients (soft-deleted rows are invisible).
#[async_trait]
pub trait ChannelClientRepo: Send + Sync {
    async fn create(&self, client: &NewChannelClient, now: DateTime<Utc>) -> Result<ChannelClient>;
    async fn get(&self, id: Id) -> Result<Option<ChannelClient>>;
    async fn find_by_key(&self, key: &str) -> Result<Option<ChannelClient>>;
    /// First active client of the type.
    async fn find_active_by_type(&self, channel_type: &str) -> Result<Option<ChannelClient>>;
    /// Ordered by `created_at DESC`.
    async fn list(&self) -> Result<Vec<ChannelClient>>;
    /// Saves name, description, callback url, bot token, secret and status.
    async fn update(&self, client: &ChannelClient, now: DateTime<Utc>) -> Result<()>;
    async fn touch(&self, id: Id, at: DateTime<Utc>) -> Result<()>;
    async fn soft_delete(&self, id: Id, at: DateTime<Utc>) -> Result<()>;
}

/// Why channel authentication failed. Every reason except `Disabled` is
/// reported identically to the caller (NTF-06: no key probing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthFailure {
    Missing,
    Expired,
    UnknownKey,
    BadSignature,
    Disabled,
}

/// Outcome of channel authentication.
pub type AuthResult = std::result::Result<ChannelClient, ChannelAuthError>;

/// Channel authentication error.
#[derive(Debug)]
pub enum ChannelAuthError {
    Rejected(AuthFailure),
    Internal(Error),
}

impl From<Error> for ChannelAuthError {
    fn from(e: Error) -> Self {
        Self::Internal(e)
    }
}

// ---------------------------------------------------------------------------
// Telegram identities
// ---------------------------------------------------------------------------

/// Telegram identity fields sent by the bot.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TelegramIdentityInput {
    pub channel_user_id: String,
    pub username: String,
    pub first_name: String,
    pub last_name: String,
    pub avatar_url: String,
}

/// A shop user as seen by the channel API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelUser {
    pub id: Id,
    pub email: String,
    pub display_name: String,
    pub status: String,
    pub locale: String,
    pub email_verified: bool,
    pub password_setup_required: bool,
    pub member_level_id: Id,
}

impl ChannelUser {
    pub fn is_active(&self) -> bool {
        self.status.trim().eq_ignore_ascii_case("active")
    }
}

/// A `user_oauth_identities` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OAuthIdentity {
    pub id: Id,
    pub user_id: Id,
    pub provider: String,
    pub provider_user_id: String,
    pub username: String,
    pub avatar_url: String,
    pub auth_at: Option<DateTime<Utc>>,
}

/// A new Telegram placeholder user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChannelUser {
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
}

/// Users and OAuth identities needed by the channel identity flows.
#[async_trait]
pub trait ChannelIdentityRepo: Send + Sync {
    async fn user_by_id(&self, id: Id) -> Result<Option<ChannelUser>>;
    async fn user_by_email(&self, email: &str) -> Result<Option<ChannelUser>>;
    /// Creates a user and assigns the default member level (a551e8f8).
    async fn create_user(&self, user: &NewChannelUser, now: DateTime<Utc>) -> Result<ChannelUser>;
    async fn identity_by_provider_user(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<OAuthIdentity>>;
    async fn identity_by_user(&self, user_id: Id, provider: &str) -> Result<Option<OAuthIdentity>>;
    /// Inserts; fails on the `(provider, provider_user_id)` unique key.
    async fn create_identity(
        &self,
        identity: &OAuthIdentity,
        now: DateTime<Utc>,
    ) -> Result<OAuthIdentity>;
    async fn update_identity(&self, identity: &OAuthIdentity, now: DateTime<Utc>) -> Result<()>;
    /// Whether self-registration is enabled (Telegram sign-up follows it, 803f11a1).
    async fn registration_enabled(&self) -> Result<bool>;
}

/// Checks a `telegram_bind` email verification code (consumes it on success).
#[async_trait]
pub trait BindCodeVerifier: Send + Sync {
    async fn verify(&self, email: &str, code: &str) -> Result<()>;
}

/// Message keys of the identity flows (original userauth errors).
pub mod identity_keys {
    pub const USER_NOT_FOUND: &str = "error.user_not_found";
    pub const USER_DISABLED: &str = "error.user_disabled";
    pub const BIND_CONFLICT: &str = "error.telegram_bind_conflict";
    pub const ALREADY_BOUND: &str = "error.telegram_already_bound";
    pub const REGISTRATION_DISABLED: &str = "error.registration_disabled";
}

/// Placeholder email of a Telegram-only account.
pub fn placeholder_email(provider_user_id: &str) -> String {
    let id = provider_user_id.trim();
    format!(
        "telegram_{}@login.local",
        if id.is_empty() { "unknown" } else { id }
    )
}

/// Display name of a Telegram account (full name, username, `telegram_<id>`).
pub fn telegram_display_name(
    provider_user_id: &str,
    username: &str,
    first: &str,
    last: &str,
) -> String {
    let full = format!("{} {}", first.trim(), last.trim())
        .trim()
        .to_owned();
    if !full.is_empty() {
        return full;
    }
    if !username.trim().is_empty() {
        return username.trim().to_owned();
    }
    if !provider_user_id.trim().is_empty() {
        return format!("telegram_{}", provider_user_id.trim());
    }
    "Telegram User".into()
}

// ---------------------------------------------------------------------------
// channel catalog / payment lookups
// ---------------------------------------------------------------------------

/// A payment method offered to the bot (original channel `channelItem`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PaymentMethod {
    pub id: Id,
    pub name: String,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    pub fee_rate: zs_shared::money::Amount,
    pub fixed_fee: zs_shared::money::Amount,
}

/// Read-only lookups the channel API needs from catalog, payment and order tables.
#[async_trait]
pub trait ChannelLookup: Send + Sync {
    /// Slug of a live product.
    async fn product_slug(&self, id: Id) -> Result<Option<String>>;
    /// Active, live products per category id.
    async fn category_product_counts(&self) -> Result<std::collections::HashMap<Id, i64>>;
    /// Active payment channels (`sort_order DESC, id ASC`), at most `limit`.
    async fn active_payment_channels(&self, limit: u64) -> Result<Vec<PaymentMethod>>;
    /// Product ids of a user's order (parent and children); `None` when not found.
    async fn order_product_ids(&self, user_id: Id, order_no: &str) -> Result<Option<Vec<Id>>>;
    /// Raw `payment_channel_ids` of the products.
    async fn product_payment_channel_ids(&self, product_ids: &[Id]) -> Result<Vec<String>>;
}

/// Intersection of the products' allowed channels (`None` = unrestricted).
pub fn allowed_channel_intersection(raw_lists: &[String]) -> Option<Vec<Id>> {
    let mut acc: Option<Vec<Id>> = None;
    for raw in raw_lists {
        let allowed = crate::catalog::product::decode_payment_channel_ids(raw);
        if allowed.is_empty() {
            continue;
        }
        acc = Some(match acc {
            None => allowed,
            Some(prev) => prev.into_iter().filter(|id| allowed.contains(id)).collect(),
        });
    }
    acc
}

// ---------------------------------------------------------------------------
// bot:notify
// ---------------------------------------------------------------------------

/// Bot event types (original `BotNotifyEvent*`).
pub mod bot_events {
    pub const ORDER_PAID: &str = "order_paid";
    pub const ORDER_FULFILLED: &str = "order_fulfilled";
    pub const WALLET_RECHARGE_SUCCEEDED: &str = "wallet_recharge_succeeded";
}

/// Retry budget of `bot:notify` jobs.
pub const BOT_NOTIFY_MAX_RETRY: i32 = 3;

/// Payload of the `bot:notify` job (original `BotNotifyPayload`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BotNotifyPayload {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub event_type: String,
    pub order_id: Id,
    pub telegram_user_id: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub recharge_no: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub amount: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub currency: String,
}

/// A signed request to send to the bot.
#[derive(Debug, Clone, PartialEq)]
pub struct BotRequest {
    /// Path used for both the URL and the signature.
    pub path: &'static str,
    pub body: serde_json::Value,
}

/// Builds the bot request of a payload; `None` when the event must be skipped
/// (unknown event, missing ids).
pub fn bot_request(p: &BotNotifyPayload) -> Option<BotRequest> {
    if p.telegram_user_id.trim().is_empty() {
        return None;
    }
    let order_body =
        || serde_json::json!({"order_id": p.order_id, "telegram_user_id": p.telegram_user_id});
    match p.event_type.as_str() {
        bot_events::ORDER_PAID if p.order_id > 0 => Some(BotRequest {
            path: "/internal/order-paid",
            body: order_body(),
        }),
        "" | bot_events::ORDER_FULFILLED if p.order_id > 0 => Some(BotRequest {
            path: "/internal/order-fulfilled",
            body: order_body(),
        }),
        bot_events::WALLET_RECHARGE_SUCCEEDED if !p.recharge_no.trim().is_empty() => {
            Some(BotRequest {
                path: "/internal/wallet-recharge-succeeded",
                body: serde_json::json!({
                    "recharge_no": p.recharge_no,
                    "telegram_user_id": p.telegram_user_id,
                    "amount": p.amount,
                    "currency": p.currency,
                }),
            })
        }
        _ => None,
    }
}

/// Rebuilds the callback URL with `path`, dropping query and fragment (NTF-06).
/// `None` when the URL has no `http(s)` scheme or host.
pub fn bot_request_url(callback_url: &str, path: &str) -> Option<String> {
    let raw = callback_url.trim();
    let (scheme, rest) = raw.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() || authority.starts_with(':') || authority.contains('@') {
        return None;
    }
    Some(format!("{scheme}://{authority}{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_tokens() {
        assert_eq!(mask_bot_token(""), "");
        assert_eq!(mask_bot_token("123456789012"), "************");
        assert_eq!(mask_bot_token("1234567890abcdef"), "1234********cdef");
    }

    // NTF-06 (2): the callback URL is rebuilt, not concatenated.
    #[test]
    fn ntf06_bot_url_is_rebuilt() {
        assert_eq!(
            bot_request_url(
                "https://bot.x/cb?a=1",
                "/internal/wallet-recharge-succeeded"
            )
            .as_deref(),
            Some("https://bot.x/internal/wallet-recharge-succeeded")
        );
        assert_eq!(
            bot_request_url(
                "http://127.0.0.1:8444/internal/order-fulfilled#x",
                "/internal/order-paid"
            )
            .as_deref(),
            Some("http://127.0.0.1:8444/internal/order-paid")
        );
        assert_eq!(bot_request_url("bot.x/cb", "/p"), None);
        assert_eq!(bot_request_url("ftp://bot.x/cb", "/p"), None);
        assert_eq!(bot_request_url("https:///cb", "/p"), None);
        assert_eq!(bot_request_url("https://u@evil/cb", "/p"), None);
    }

    #[test]
    fn bot_request_by_event() {
        let mut p = BotNotifyPayload {
            order_id: 5,
            telegram_user_id: "42".into(),
            ..BotNotifyPayload::default()
        };
        assert_eq!(
            bot_request(&p).map(|r| r.path),
            Some("/internal/order-fulfilled")
        );
        p.event_type = bot_events::ORDER_PAID.into();
        assert_eq!(
            bot_request(&p).map(|r| r.path),
            Some("/internal/order-paid")
        );
        p.event_type = bot_events::WALLET_RECHARGE_SUCCEEDED.into();
        assert!(bot_request(&p).is_none(), "recharge needs a recharge_no");
        p.recharge_no = "RC1".into();
        let r = bot_request(&p);
        assert_eq!(
            r.as_ref().map(|r| r.path),
            Some("/internal/wallet-recharge-succeeded")
        );
        assert_eq!(r.map(|r| r.body["recharge_no"].clone()), Some("RC1".into()));
        p.event_type = "unknown".into();
        assert!(bot_request(&p).is_none());
        p.event_type = String::new();
        p.telegram_user_id = " ".into();
        assert!(bot_request(&p).is_none());
    }

    #[test]
    fn channel_intersection() {
        assert_eq!(
            allowed_channel_intersection(&["".into(), "[]".into()]),
            None
        );
        assert_eq!(
            allowed_channel_intersection(&["[1,2,3]".into(), "".into(), "[2,3,4]".into()]),
            Some(vec![2, 3])
        );
        assert_eq!(
            allowed_channel_intersection(&["[1]".into(), "[2]".into()]),
            Some(vec![])
        );
    }

    #[test]
    fn display_names() {
        assert_eq!(telegram_display_name("1", "u", "A", "B"), "A B");
        assert_eq!(telegram_display_name("1", "u", "", ""), "u");
        assert_eq!(telegram_display_name("1", "", "", ""), "telegram_1");
        assert_eq!(placeholder_email(" 9 "), "telegram_9@login.local");
    }
}
