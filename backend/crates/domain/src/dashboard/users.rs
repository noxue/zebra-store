//! Admin user directory (port of `identity/user/transport/http/admin`):
//! list/filter/sort, edit, batch status, OAuth unbind and coupon usages.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::identity::user::{STATUS_ACTIVE, STATUS_DISABLED, User};
use crate::{Error, Id, Result};

/// OAuth provider name of Telegram identities.
pub const PROVIDER_TELEGRAM: &str = "telegram";
/// OAuth provider name of Google identities.
pub const PROVIDER_GOOGLE: &str = "google";

/// Error keys of this module (identical to the original).
pub mod keys {
    pub const USER_ID_INVALID: &str = "error.user_id_invalid";
    pub const USER_NOT_FOUND: &str = "error.user_not_found";
    pub const USER_FETCH_FAILED: &str = "error.user_fetch_failed";
    pub const USER_UPDATE_FAILED: &str = "error.user_update_failed";
    pub const USER_DISABLED: &str = "error.user_disabled";
    pub const EMAIL_EXISTS: &str = "error.email_exists";
    pub const TELEGRAM_NOT_BOUND: &str = "error.telegram_not_bound";
    pub const GOOGLE_NOT_BOUND: &str = "error.google_not_bound";
    pub const TELEGRAM_UNBIND_REQUIRES_EMAIL: &str = "error.telegram_unbind_requires_email";
    pub const GOOGLE_UNBIND_LOCKED: &str = "error.google_unbind_locked";
}

/// Whitelisted `sort_by` values (DB-10 / user-list lesson: never interpolated).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum UserSort {
    #[default]
    Id,
    CreatedAt,
    LastLoginAt,
    WalletBalance,
}

impl UserSort {
    /// Unknown values fall back to `id DESC`.
    pub fn parse(raw: &str) -> Self {
        match raw.trim() {
            "created_at" => Self::CreatedAt,
            "last_login_at" => Self::LastLoginAt,
            "wallet_balance" => Self::WalletBalance,
            _ => Self::Id,
        }
    }
}

/// Filters of `GET /admin/users`.
#[derive(Debug, Clone, Default)]
pub struct UserListFilter {
    pub user_id: Option<Id>,
    pub keyword: String,
    pub status: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
    pub last_login_from: Option<DateTime<Utc>>,
    pub last_login_to: Option<DateTime<Utc>>,
    pub sort: UserSort,
    /// Only `asc` (case-insensitive) sorts ascending.
    pub ascending: bool,
}

/// A linked third-party identity.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OAuthIdentity {
    pub id: Id,
    pub provider: String,
    pub provider_user_id: String,
    pub username: String,
    pub avatar_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

/// List row: the user plus its wallet balance.
#[derive(Debug, Clone, Serialize)]
pub struct AdminUserItem {
    #[serde(flatten)]
    pub user: User,
    pub wallet_balance: Amount,
}

/// Detail: the user, wallet balance and OAuth identities.
#[derive(Debug, Clone, Serialize)]
pub struct AdminUserDetail {
    #[serde(flatten)]
    pub user: User,
    pub wallet_balance: Amount,
    pub oauth_identities: Vec<OAuthIdentity>,
}

/// A coupon usage row as stored.
#[derive(Debug, Clone)]
pub struct CouponUsage {
    pub id: Id,
    pub coupon_id: Id,
    pub order_id: Id,
    pub discount_amount: Amount,
    pub created_at: DateTime<Utc>,
}

/// The coupon fields shown next to a usage.
#[derive(Debug, Clone)]
pub struct CouponBrief {
    pub id: Id,
    pub code: String,
    pub kind: String,
    /// JSON array of product ids (`coupons.scope_ref_ids`).
    pub scope_ref_ids: String,
}

/// A product referenced by a coupon scope.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ScopeProduct {
    pub id: Id,
    pub title: Option<Value>,
}

/// `GET /admin/users/:id/coupon-usages` row.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UserCouponUsage {
    pub id: Id,
    pub coupon_id: Id,
    pub coupon_code: String,
    pub coupon_type: String,
    pub order_id: Id,
    pub discount_amount: Amount,
    pub created_at: DateTime<Utc>,
    pub scope_ref_ids: Option<Vec<Id>>,
    pub scope_products: Option<Vec<ScopeProduct>>,
}

/// Decodes `coupons.scope_ref_ids`; blank or malformed → `None`.
pub fn decode_scope_ids(raw: &str) -> Option<Vec<Id>> {
    if raw.trim().is_empty() {
        return None;
    }
    serde_json::from_str::<Vec<Id>>(raw).ok()
}

/// Joins usages with their coupons and scope products (original handler logic).
pub fn assemble_coupon_usages(
    usages: Vec<CouponUsage>,
    coupons: &HashMap<Id, CouponBrief>,
    products: &HashMap<Id, ScopeProduct>,
) -> Vec<UserCouponUsage> {
    usages
        .into_iter()
        .map(|u| {
            let mut row = UserCouponUsage {
                id: u.id,
                coupon_id: u.coupon_id,
                coupon_code: String::new(),
                coupon_type: String::new(),
                order_id: u.order_id,
                discount_amount: u.discount_amount,
                created_at: u.created_at,
                scope_ref_ids: None,
                scope_products: None,
            };
            if let Some(c) = coupons.get(&u.coupon_id) {
                row.coupon_code.clone_from(&c.code);
                row.coupon_type.clone_from(&c.kind);
                let ids = decode_scope_ids(&c.scope_ref_ids);
                if let Some(ids) = ids.as_ref().filter(|ids| !ids.is_empty()) {
                    row.scope_products = Some(
                        ids.iter()
                            .filter_map(|id| products.get(id).cloned())
                            .collect(),
                    );
                }
                row.scope_ref_ids = ids;
            }
            row
        })
        .collect()
}

/// `PUT /admin/users/:id` body (every field optional).
#[derive(Debug, Clone, Default)]
pub struct UserPatch {
    pub nickname: Option<String>,
    pub locale: Option<String>,
    pub status: Option<String>,
    /// Already normalized email.
    pub email: Option<String>,
    /// Already hashed password.
    pub password_hash: Option<String>,
    pub admin_note: Option<String>,
    pub email_verified: Option<bool>,
}

/// Outcome of [`apply_patch`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchOutcome {
    pub updated: bool,
    pub revoke_tokens: bool,
}

/// Normalizes a requested status: `active` / `disabled`, otherwise `None`.
pub fn parse_status(raw: &str) -> Option<&'static str> {
    match raw.trim().to_lowercase().as_str() {
        STATUS_ACTIVE => Some(STATUS_ACTIVE),
        STATUS_DISABLED => Some(STATUS_DISABLED),
        _ => None,
    }
}

/// Applies an admin edit to `user` (original `UpdateAdminUser` rules):
/// blank nickname/password/locale are ignored, unknown statuses are ignored,
/// a new password or a `disabled` status revokes every issued token.
pub fn apply_patch(user: &mut User, patch: UserPatch, now: DateTime<Utc>) -> PatchOutcome {
    let mut updated = false;
    let mut revoke = false;
    if let Some(email) = patch.email
        && email != user.email
    {
        user.email = email;
        updated = true;
    }
    if let Some(name) = patch
        .nickname
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        user.display_name = name.to_owned();
        updated = true;
    }
    if let Some(hash) = patch.password_hash {
        user.password_hash = hash;
        updated = true;
        revoke = true;
    }
    if let Some(locale) = patch
        .locale
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        user.locale = locale.to_owned();
        updated = true;
    }
    if let Some(status) = patch.status.as_deref().and_then(parse_status) {
        if user.status != status {
            status.clone_into(&mut user.status);
            updated = true;
        }
        if status == STATUS_DISABLED {
            revoke = true;
        }
    }
    if let Some(note) = patch.admin_note {
        user.admin_note = note;
        updated = true;
    }
    match patch.email_verified {
        Some(true) if user.email_verified_at.is_none() => {
            user.email_verified_at = Some(now);
            updated = true;
        }
        Some(false) if user.email_verified_at.is_some() => {
            user.email_verified_at = None;
            updated = true;
        }
        _ => {}
    }
    if updated && revoke {
        user.token_version += 1;
        user.token_invalid_before = Some(now);
    }
    PatchOutcome {
        updated,
        revoke_tokens: revoke,
    }
}

/// True when `hash` parses as a bcrypt hash (Go `bcrypt.Cost` succeeds).
pub fn is_bcrypt_hash(hash: &str) -> bool {
    let h = hash.trim().as_bytes();
    // `$2?$NN$` + 53 characters of salt and digest.
    const LEN: usize = 60;
    if h.len() < LEN - 1 || h.first() != Some(&b'$') || h.get(1) != Some(&b'2') {
        return false;
    }
    let rest = match h.get(2) {
        Some(b'$') => &h[3..],
        Some(b'a' | b'b' | b'x' | b'y') if h.get(3) == Some(&b'$') => &h[4..],
        _ => return false,
    };
    let (Some(d1), Some(d2), Some(b'$')) = (rest.first(), rest.get(1), rest.get(2)) else {
        return false;
    };
    if !d1.is_ascii_digit() || !d2.is_ascii_digit() {
        return false;
    }
    let cost = (d1 - b'0') * 10 + (d2 - b'0');
    (4..=31).contains(&cost)
}

/// Providers that currently work as a login method (enabled and configured).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsableProviders {
    pub google: bool,
    pub telegram: bool,
}

impl UsableProviders {
    fn allows(self, provider: &str) -> bool {
        match provider.trim().to_lowercase().as_str() {
            PROVIDER_GOOGLE => self.google,
            PROVIDER_TELEGRAM => self.telegram,
            // Unknown providers are not assumed to be a usable recovery method.
            _ => false,
        }
    }
}

/// True when the account keeps a usable login after removing `removed`:
/// a local bcrypt password (not pending setup) or another usable identity.
pub fn keeps_usable_login(
    user: &User,
    removed: Id,
    identities: &[OAuthIdentity],
    usable: UsableProviders,
) -> bool {
    if !user.password_setup_required && is_bcrypt_hash(&user.password_hash) {
        return true;
    }
    identities
        .iter()
        .any(|i| i.id != removed && usable.allows(&i.provider))
}

/// Result of an unbind attempt performed atomically by the adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnbindOutcome {
    Unbound,
    UserNotFound,
    UserDisabled,
    NotBound,
    /// Removing it would leave the account without any login method.
    Locked,
}

impl UnbindOutcome {
    /// Maps the outcome to the original error keys of `provider`.
    pub fn into_result(self, provider: &str) -> Result<()> {
        let google = provider == PROVIDER_GOOGLE;
        match self {
            Self::Unbound => Ok(()),
            Self::UserNotFound => Err(Error::not_found(keys::USER_NOT_FOUND)),
            Self::UserDisabled => Err(Error::bad_request(keys::USER_DISABLED)),
            Self::NotBound if google => Err(Error::bad_request(keys::GOOGLE_NOT_BOUND)),
            Self::NotBound => Err(Error::bad_request(keys::TELEGRAM_NOT_BOUND)),
            Self::Locked if google => Err(Error::bad_request(keys::GOOGLE_UNBIND_LOCKED)),
            Self::Locked => Err(Error::bad_request(keys::TELEGRAM_UNBIND_REQUIRES_EMAIL)),
        }
    }
}

/// Persistence port of the admin user directory (soft-deleted users excluded).
#[async_trait]
pub trait AdminUserRepo: Send + Sync {
    async fn list(&self, filter: &UserListFilter, page: PageRequest) -> Result<Page<User>>;
    async fn get(&self, id: Id) -> Result<Option<User>>;
    async fn get_by_email(&self, email: &str) -> Result<Option<User>>;
    /// Writes the admin-editable columns of `user` (email, display name,
    /// password, locale, status, note, email verification, token revocation).
    async fn save_admin_edit(&self, user: &User) -> Result<()>;
    /// Sets `status` of every listed user; disabling also bumps
    /// `token_version` and sets `token_invalid_before = now`.
    async fn batch_status(&self, ids: &[Id], status: &str, now: DateTime<Utc>) -> Result<()>;
    /// Wallet balances of the given users (missing accounts are absent).
    async fn balances(&self, ids: &[Id]) -> Result<HashMap<Id, Amount>>;
    async fn identities(&self, user_id: Id) -> Result<Vec<OAuthIdentity>>;
    /// Atomically checks the account and removes its `provider` identity
    /// when [`keeps_usable_login`] allows it.
    async fn unbind(
        &self,
        user_id: Id,
        provider: &str,
        usable: UsableProviders,
    ) -> Result<UnbindOutcome>;
    async fn coupon_usages(&self, user_id: Id, page: PageRequest) -> Result<Page<CouponUsage>>;
    async fn coupons(&self, ids: &[Id]) -> Result<Vec<CouponBrief>>;
    async fn products(&self, ids: &[Id]) -> Result<Vec<ScopeProduct>>;
}

/// Reads which OAuth providers are currently usable (settings-backed).
#[async_trait]
pub trait LoginProviders: Send + Sync {
    async fn usable(&self) -> Result<UsableProviders>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user() -> User {
        let t = DateTime::parse_from_rfc3339("2026-09-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        User {
            id: 1,
            email: "a@example.com".into(),
            password_hash: "$2b$10$abcdefghijklmnopqrstuuFb8i7yUgAr9nvsgk6HjWJVxBm6mOZ3C".into(),
            password_setup_required: false,
            display_name: "A".into(),
            locale: "zh-CN".into(),
            status: STATUS_ACTIVE.into(),
            member_level_id: 0,
            total_recharged: Amount::default(),
            total_spent: Amount::default(),
            admin_note: String::new(),
            token_version: 3,
            token_invalid_before: None,
            totp_secret: String::new(),
            totp_enabled_at: None,
            totp_pending_secret: String::new(),
            totp_pending_expires_at: None,
            recovery_codes: String::new(),
            email_verified_at: None,
            last_login_at: None,
            created_at: t,
            updated_at: t,
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-09-24T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn sort_whitelist() {
        assert_eq!(UserSort::parse("wallet_balance"), UserSort::WalletBalance);
        assert_eq!(UserSort::parse("last_login_at"), UserSort::LastLoginAt);
        assert_eq!(UserSort::parse("id;DROP TABLE users"), UserSort::Id);
    }

    #[test]
    fn disabling_revokes_tokens() {
        let mut u = user();
        let out = apply_patch(
            &mut u,
            UserPatch {
                status: Some(" Disabled ".into()),
                ..UserPatch::default()
            },
            now(),
        );
        assert_eq!(
            out,
            PatchOutcome {
                updated: true,
                revoke_tokens: true
            }
        );
        assert_eq!(u.status, STATUS_DISABLED);
        assert_eq!(u.token_version, 4);
        assert_eq!(u.token_invalid_before, Some(now()));
    }

    #[test]
    fn blank_and_unknown_values_are_ignored() {
        let mut u = user();
        let out = apply_patch(
            &mut u,
            UserPatch {
                nickname: Some("  ".into()),
                locale: Some(String::new()),
                status: Some("banned".into()),
                email: Some("a@example.com".into()),
                email_verified: Some(false),
                ..UserPatch::default()
            },
            now(),
        );
        assert!(!out.updated, "nothing changed → error.bad_request upstream");
        assert_eq!(u.token_version, 3);
    }

    #[test]
    fn password_and_verification_changes() {
        let mut u = user();
        let out = apply_patch(
            &mut u,
            UserPatch {
                password_hash: Some("new-hash".into()),
                email_verified: Some(true),
                admin_note: Some(String::new()),
                ..UserPatch::default()
            },
            now(),
        );
        assert!(out.updated && out.revoke_tokens);
        assert_eq!(u.email_verified_at, Some(now()));
        assert_eq!(u.token_version, 4);
        // Already verified: the timestamp is kept.
        let before = u.email_verified_at;
        apply_patch(
            &mut u,
            UserPatch {
                email_verified: Some(true),
                nickname: Some("B".into()),
                ..UserPatch::default()
            },
            now() + chrono::Duration::days(1),
        );
        assert_eq!(u.email_verified_at, before);
        assert_eq!(u.token_version, 4, "renaming does not revoke");
    }

    #[test]
    fn bcrypt_detection() {
        assert!(is_bcrypt_hash(&user().password_hash));
        assert!(is_bcrypt_hash(
            "$2y$10$65NFOY77jA4fEN6IINV0x.IzxS3MuaxHBoljdzkj4KY9h1VDa1iia"
        ));
        assert!(!is_bcrypt_hash(""));
        assert!(!is_bcrypt_hash(
            "plain-password-not-a-hash-at-all-but-quite-long-enough-xx"
        ));
        assert!(!is_bcrypt_hash(
            "$2b$99$abcdefghijklmnopqrstuuFb8i7yUgAr9nvsgk6HjWJVxBm6mOZ3C"
        ));
    }

    #[test]
    fn unbind_guard() {
        let identity = |id: Id, provider: &str| OAuthIdentity {
            id,
            provider: provider.into(),
            provider_user_id: "x".into(),
            username: String::new(),
            avatar_url: String::new(),
            auth_at: None,
            created_at: now(),
        };
        let mut u = user();
        let ids = [identity(1, "telegram"), identity(2, "google")];
        assert!(
            keeps_usable_login(&u, 1, &ids, UsableProviders::default()),
            "local password"
        );
        u.password_setup_required = true;
        assert!(!keeps_usable_login(&u, 1, &ids, UsableProviders::default()));
        let google = UsableProviders {
            google: true,
            telegram: false,
        };
        assert!(
            keeps_usable_login(&u, 1, &ids, google),
            "google still usable"
        );
        assert!(
            !keeps_usable_login(&u, 2, &ids, google),
            "removing the only usable one"
        );
        assert_eq!(
            UnbindOutcome::Locked
                .into_result(PROVIDER_TELEGRAM)
                .unwrap_err()
                .key(),
            keys::TELEGRAM_UNBIND_REQUIRES_EMAIL
        );
        assert_eq!(
            UnbindOutcome::NotBound
                .into_result(PROVIDER_GOOGLE)
                .unwrap_err()
                .key(),
            keys::GOOGLE_NOT_BOUND
        );
    }

    #[test]
    fn coupon_usage_assembly() {
        let usage = |id: Id, coupon_id: Id| CouponUsage {
            id,
            coupon_id,
            order_id: 9,
            discount_amount: Amount::from_cents(150),
            created_at: now(),
        };
        let coupons = HashMap::from([
            (
                1,
                CouponBrief {
                    id: 1,
                    code: "SAVE".into(),
                    kind: "fixed".into(),
                    scope_ref_ids: "[5,6]".into(),
                },
            ),
            (
                2,
                CouponBrief {
                    id: 2,
                    code: "ALL".into(),
                    kind: "percent".into(),
                    scope_ref_ids: String::new(),
                },
            ),
        ]);
        let products = HashMap::from([(
            5,
            ScopeProduct {
                id: 5,
                title: Some(json!({"zh-CN": "五"})),
            },
        )]);
        let rows = assemble_coupon_usages(
            vec![usage(10, 1), usage(11, 2), usage(12, 3)],
            &coupons,
            &products,
        );
        let json = serde_json::to_value(&rows).unwrap();
        assert_eq!(json[0]["coupon_code"], "SAVE");
        assert_eq!(json[0]["discount_amount"], "1.50");
        assert_eq!(json[0]["scope_ref_ids"], json!([5, 6]));
        assert_eq!(
            json[0]["scope_products"],
            json!([{"id": 5, "title": {"zh-CN": "五"}}])
        );
        assert_eq!(json[1]["coupon_type"], "percent");
        assert!(json[1]["scope_ref_ids"].is_null() && json[1]["scope_products"].is_null());
        assert_eq!(json[2]["coupon_code"], "");
    }
}
