//! Read ports the content group needs from other modules (consumer-defined):
//! public payment channels, sitemap catalogue entries, SMTP test sending and the
//! reseller overlay hook for `/public/config`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use zs_shared::money::Amount;

use crate::identity::mailer::Email;
use crate::settings::schema::smtp::SmtpSetting;
use crate::{Id, Result};

/// Projection of an active `payment_channels` row.
#[derive(Debug, Clone, PartialEq)]
pub struct PaymentChannelView {
    pub id: Id,
    pub name: String,
    pub icon: String,
    pub provider_type: String,
    pub channel_type: String,
    pub supported_channel_types: Vec<String>,
    pub interaction_mode: String,
    pub fee_rate: Amount,
    pub fixed_fee: Amount,
    pub min_amount: Amount,
    pub max_amount: Amount,
    pub hide_amount_out_range: bool,
    pub payment_roles: Vec<String>,
    pub member_levels: Vec<Id>,
    pub payment_types: Vec<String>,
}

impl PaymentChannelView {
    /// Visible to an anonymous visitor paying an order (role `guest`, no level, type `order`).
    pub fn visible_to_guest_order(&self) -> bool {
        (self.payment_roles.is_empty() || self.payment_roles.iter().any(|r| r == "guest"))
            && self.member_levels.is_empty()
            && (self.payment_types.is_empty() || self.payment_types.iter().any(|t| t == "order"))
    }

    /// Public JSON (`GetAvailableChannels` item).
    pub fn to_public(&self, customer_fee_enabled: bool) -> Value {
        let mut v = json!({
            "id": self.id,
            "name": self.name,
            "provider_type": self.provider_type,
            "channel_type": self.channel_type,
            "interaction_mode": self.interaction_mode,
            "min_amount": self.min_amount,
            "max_amount": self.max_amount,
            "hide_amount_out_range": self.hide_amount_out_range,
        });
        if !self.supported_channel_types.is_empty() {
            v["supported_channel_types"] = json!(self.supported_channel_types);
        }
        if customer_fee_enabled {
            v["fee_policy"] = json!("customer_surcharge");
            v["fee_rate"] = json!(self.fee_rate);
            v["fixed_fee"] = json!(self.fixed_fee);
        }
        if !self.icon.is_empty() {
            v["icon"] = json!(self.icon);
        }
        v
    }
}

/// Active payment channels ordered `sort_order DESC, id ASC` (max 200).
#[async_trait]
pub trait PaymentChannelReader: Send + Sync {
    async fn active_channels(&self) -> Result<Vec<PaymentChannelView>>;
}

/// A sitemap location with its last modification time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapEntry {
    pub id: crate::Id,
    pub slug: String,
    pub modified_at: DateTime<Utc>,
}

/// Indexable catalogue entries for the sitemap.
#[async_trait]
pub trait SitemapCatalogReader: Send + Sync {
    /// Active categories (`created_at` as modification time).
    async fn active_categories(&self) -> Result<Vec<SitemapEntry>>;
    /// Active products in active categories (`updated_at`), at most `limit`.
    async fn active_products(&self, limit: u64) -> Result<Vec<SitemapEntry>>;
}

/// Sends one e-mail with an explicit SMTP configuration (used by the SMTP test).
///
/// Errors: `error.email_recipient_not_found` (400) when the server rejects the
/// recipient; any other failure is internal.
#[async_trait]
pub trait SmtpSender: Send + Sync {
    async fn send(&self, setting: &SmtpSetting, email: &Email) -> Result<()>;
}

/// The tenant a `/public/config` request belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tenant {
    /// `None` on the main site.
    pub reseller_id: Option<Id>,
    pub host: String,
}

/// Extension hook: rewrites the main-site public config for a reseller site.
#[async_trait]
pub trait PublicConfigOverlay: Send + Sync {
    async fn apply(&self, tenant: &Tenant, base: Value) -> Result<Value>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel() -> PaymentChannelView {
        PaymentChannelView {
            id: 1,
            name: "Alipay".into(),
            icon: String::new(),
            provider_type: "epay".into(),
            channel_type: "alipay".into(),
            supported_channel_types: vec!["alipay".into()],
            interaction_mode: "redirect".into(),
            fee_rate: Amount::from_cents(150),
            fixed_fee: Amount::ZERO,
            min_amount: Amount::ZERO,
            max_amount: Amount::ZERO,
            hide_amount_out_range: false,
            payment_roles: vec![],
            member_levels: vec![],
            payment_types: vec![],
        }
    }

    #[test]
    fn guest_order_visibility() {
        assert!(channel().visible_to_guest_order());
        let member_only = PaymentChannelView {
            payment_roles: vec!["member".into()],
            ..channel()
        };
        assert!(!member_only.visible_to_guest_order());
        let leveled = PaymentChannelView {
            member_levels: vec![2],
            ..channel()
        };
        assert!(!leveled.visible_to_guest_order());
        let recharge = PaymentChannelView {
            payment_types: vec!["recharge".into()],
            ..channel()
        };
        assert!(!recharge.visible_to_guest_order());
    }

    #[test]
    fn public_shape() {
        let v = channel().to_public(false);
        assert_eq!(v["min_amount"], "0.00");
        assert!(v.get("fee_rate").is_none());
        assert!(v.get("icon").is_none());
        let v = channel().to_public(true);
        assert_eq!(v["fee_policy"], "customer_surcharge");
        assert_eq!(v["fee_rate"], "1.50");
    }
}
