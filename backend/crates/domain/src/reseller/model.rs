//! Reseller entities, serialized in the original GORM JSON shape (used verbatim by
//! the admin list endpoints that return raw rows).

use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};
use serde_json::Value;
use zs_shared::money::Amount;

use crate::Id;

/// Declares a string-backed enum stored as `varchar` and serialized as its value.
macro_rules! string_enum {
    ($(#[$meta:meta])* $name:ident default $default:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl $name {
            /// Stored / serialized value.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }

            /// Parses an exact value (surrounding blanks ignored).
            pub fn parse(raw: &str) -> Option<Self> {
                match raw.trim() {
                    $($value => Some(Self::$variant),)+
                    _ => None,
                }
            }

            /// Parses a stored value, falling back to the column default.
            pub fn from_db(raw: &str) -> Self {
                Self::parse(raw).unwrap_or(Self::$default)
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }
    };
}

string_enum!(
    /// `reseller_profiles.status`.
    ProfileStatus default PendingReview {
        PendingReview => "pending_review",
        Active => "active",
        Rejected => "rejected",
        Disabled => "disabled",
    }
);

string_enum!(
    /// `reseller_profiles.settlement_status`.
    SettlementStatus default Normal {
        Normal => "normal",
        Frozen => "frozen",
    }
);

string_enum!(
    /// `reseller_domains.type`.
    DomainType default Custom {
        Subdomain => "subdomain",
        Custom => "custom",
    }
);

string_enum!(
    /// `reseller_domains.verification_status`.
    VerificationStatus default Pending {
        Pending => "pending",
        Verified => "verified",
        Failed => "failed",
    }
);

string_enum!(
    /// `reseller_domains.status`.
    DomainStatus default PendingReview {
        PendingReview => "pending_review",
        Active => "active",
        Disabled => "disabled",
    }
);

string_enum!(
    /// `reseller_product_settings.pricing_mode`.
    PricingMode default Inherit {
        Inherit => "inherit",
        MarkupPercent => "markup_percent",
        FixedMarkup => "fixed_markup",
        FixedPrice => "fixed_price",
    }
);

string_enum!(
    /// `reseller_ledger_entries.type`.
    LedgerType default OrderProfit {
        OrderProfit => "order_profit",
        RefundDeduct => "refund_deduct",
        ManualAdjust => "manual_adjust",
        WithdrawLock => "withdraw_lock",
        WithdrawPaid => "withdraw_paid",
    }
);

string_enum!(
    /// `reseller_ledger_entries.status`.
    LedgerStatus default PendingConfirm {
        PendingConfirm => "pending_confirm",
        Available => "available",
        Locked => "locked",
        Withdrawn => "withdrawn",
        Canceled => "canceled",
    }
);

string_enum!(
    /// `reseller_withdraw_requests.status`.
    WithdrawStatus default Pending {
        Pending => "pending",
        Rejected => "rejected",
        Paid => "paid",
    }
);

string_enum!(
    /// `reseller_balance_accounts.status`.
    BalanceStatus default Normal {
        Normal => "normal",
        NegativeBalance => "negative_balance",
        FrozenReview => "frozen_review",
        Disabled => "disabled",
    }
);

/// `reseller_related_accounts.status` of an account counted for self-dealing.
pub const RELATED_ACCOUNT_ACTIVE: &str = "active";

/// The user owning a profile (preloaded `Profile.User`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UserRef {
    pub id: Id,
    pub email: String,
    pub display_name: String,
}

/// The administrator who processed a withdrawal (preloaded `Processor`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdminRef {
    pub id: Id,
    pub username: String,
}

/// `reseller_profiles` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Profile {
    pub id: Id,
    pub user_id: Id,
    pub status: ProfileStatus,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub apply_reason: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reject_reason: String,
    pub default_markup_percent: Amount,
    pub max_markup_percent: Amount,
    pub settlement_status: SettlementStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_by: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserRef>,
}

impl Profile {
    pub fn is_active(&self) -> bool {
        self.status == ProfileStatus::Active
    }
}

/// `reseller_domains` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResellerDomain {
    pub id: Id,
    pub reseller_id: Id,
    pub domain: String,
    #[serde(rename = "type")]
    pub kind: DomainType,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub verification_token: String,
    pub verification_status: VerificationStatus,
    pub status: DomainStatus,
    pub is_primary: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Box<Profile>>,
}

impl ResellerDomain {
    /// Serves traffic: active and verified.
    pub fn is_live(&self) -> bool {
        self.status == DomainStatus::Active
            && self.verification_status == VerificationStatus::Verified
    }
}

/// `reseller_site_configs` row (JSON columns kept as raw values).
#[derive(Debug, Clone, PartialEq)]
pub struct SiteConfig {
    pub id: Id,
    pub reseller_id: Id,
    pub site_name: String,
    pub logo: String,
    pub favicon: String,
    pub announcement: Value,
    pub support: Value,
    pub seo: Value,
    pub footer_links: Value,
    pub nav_config: Value,
    pub theme: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub profile: Option<Profile>,
}

/// Product projection shown next to admin product-setting rows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProductBrief {
    pub id: Id,
    pub slug: String,
    pub title: Value,
    pub price_amount: Amount,
    pub is_active: bool,
}

/// `reseller_product_settings` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProductSetting {
    pub id: Id,
    pub reseller_id: Id,
    pub product_id: Id,
    /// `0` = product-level rule.
    pub sku_id: Id,
    pub is_listed: bool,
    pub pricing_mode: PricingMode,
    pub markup_percent: Amount,
    pub fixed_markup_amount: Amount,
    pub fixed_price_amount: Amount,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub profile: Option<Profile>,
    pub product: Option<ProductBrief>,
}

impl ProductSetting {
    /// A rule not yet persisted (id 0, timestamps at the epoch).
    pub fn draft(sku_id: Id, is_listed: bool, pricing_mode: PricingMode) -> Self {
        Self {
            id: 0,
            reseller_id: 0,
            product_id: 0,
            sku_id,
            is_listed,
            pricing_mode,
            markup_percent: Amount::ZERO,
            fixed_markup_amount: Amount::ZERO,
            fixed_price_amount: Amount::ZERO,
            sort_order: 0,
            created_at: DateTime::<Utc>::UNIX_EPOCH,
            updated_at: DateTime::<Utc>::UNIX_EPOCH,
            profile: None,
            product: None,
        }
    }
}

/// `reseller_order_snapshots` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderSnapshot {
    pub id: Id,
    pub order_id: Id,
    pub reseller_id: Id,
    pub domain: String,
    pub currency: String,
    pub reseller_user_id: Id,
    pub buyer_user_id: Id,
    pub base_amount: Amount,
    pub reseller_amount: Amount,
    pub profit_amount: Amount,
    pub profit_eligible: bool,
    pub profit_block_reason: String,
    pub pricing_snapshot_json: Value,
    pub risk_snapshot_json: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Order projection shown next to admin ledger rows (preloaded `Order`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderBrief {
    pub id: Id,
    pub order_no: String,
    pub status: String,
    pub currency: String,
    pub total_amount: Amount,
    pub created_at: DateTime<Utc>,
}

/// `reseller_ledger_entries` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LedgerEntry {
    pub id: Id,
    pub reseller_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<Id>,
    #[serde(rename = "type")]
    pub kind: LedgerType,
    pub amount: Amount,
    pub currency: String,
    pub idempotency_key: String,
    pub metadata_json: Value,
    pub status: LedgerStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub withdraw_request_id: Option<Id>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub remark: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<OrderBrief>,
}

/// `reseller_withdraw_requests` row.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct WithdrawRequest {
    pub id: Id,
    pub reseller_id: Id,
    pub amount: Amount,
    pub currency: String,
    pub channel: String,
    pub account: String,
    pub status: WithdrawStatus,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub reject_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_by: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processor: Option<AdminRef>,
}

/// `reseller_balance_accounts` row (amount caches derived from the ledger).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BalanceAccount {
    pub id: Id,
    pub reseller_id: Id,
    pub currency: String,
    pub status: BalanceStatus,
    pub available_amount_cache: Amount,
    pub locked_amount_cache: Amount,
    pub negative_amount_cache: Amount,
    pub last_ledger_entry_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub risk_note: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enums_roundtrip_and_fallback() {
        assert_eq!(ProfileStatus::parse("active"), Some(ProfileStatus::Active));
        assert_eq!(
            ProfileStatus::from_db("weird"),
            ProfileStatus::PendingReview
        );
        assert_eq!(LedgerStatus::PendingConfirm.as_str(), "pending_confirm");
        assert_eq!(
            serde_json::to_value(BalanceStatus::NegativeBalance).unwrap_or_default(),
            "negative_balance"
        );
        assert_eq!(
            PricingMode::parse(" fixed_price "),
            Some(PricingMode::FixedPrice)
        );
    }
}
