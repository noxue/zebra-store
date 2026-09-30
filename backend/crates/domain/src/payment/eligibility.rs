//! Channel eligibility: amount limits, payer role, member level, payment type,
//! product channel restrictions and currency (PAY-05, PAY-11, PAY-15).
//!
//! The same functions back both the channel list and the payment-creation
//! write path, so hidden channels can never be used by submitting their id.

use std::collections::BTreeSet;

use rust_decimal::Decimal;
use serde::Serialize;
use zs_shared::money::Amount;

use super::channel::{AMOUNT_OVERFLOW, PaymentChannel};
use super::types::{FeePolicy, channel_type, payment_type, provider, role};
use crate::Id;

/// The paying customer as seen by channel rules (`None` = guest).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Payer {
    pub user_id: Id,
    /// `0` when the member has no level.
    pub member_level_id: Id,
}

/// Why a channel cannot be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EligibilityError {
    /// `error.payment_channel_not_allowed_for_product`
    NotAllowedForProduct,
    /// `error.payment_channel_not_allowed_for_recharge`
    NotAllowedForRecharge,
    /// `error.payment_invalid` (e.g. a wallet recharge without a user)
    Invalid,
    AmountTooSmall,
    AmountTooLarge,
    /// `error.payment_currency_mismatch`
    CurrencyMismatch,
}

/// `matchesChannelAmount`: only hides channels with `hide_amount_out_range`.
pub fn matches_amount(channel: &PaymentChannel, target: Option<Amount>) -> bool {
    let Some(amount) = target else { return true };
    if !channel.hide_amount_out_range {
        return true;
    }
    (!channel.min_amount.is_positive() || amount >= channel.min_amount)
        && (!channel.max_amount.is_positive() || amount <= channel.max_amount)
}

/// `matchesChannelRole`: guests are `guest`, logged-in users `member`.
pub fn matches_role(channel: &PaymentChannel, payer: Option<Payer>) -> bool {
    if channel.payment_roles.is_empty() {
        return true;
    }
    let target = if payer.is_some() {
        role::MEMBER
    } else {
        role::GUEST
    };
    channel.payment_roles.iter().any(|r| r == target)
}

/// `matchesChannelMemberLevel`: level-restricted channels exclude guests and unlevelled users.
pub fn matches_member_level(channel: &PaymentChannel, payer: Option<Payer>) -> bool {
    if channel.member_levels.is_empty() {
        return true;
    }
    match payer {
        Some(p) if p.member_level_id != 0 => channel.member_levels.contains(&p.member_level_id),
        _ => false,
    }
}

/// `matchesChannelPaymentType`: empty type (listing without context) matches everything.
pub fn matches_payment_type(channel: &PaymentChannel, kind: &str) -> bool {
    channel.payment_types.is_empty()
        || kind.is_empty()
        || channel.payment_types.iter().any(|t| t == kind)
}

/// Filter of the storefront channel list (`AvailablePaymentChannelFilter`).
#[derive(Debug, Clone, Copy, Default)]
pub struct AvailabilityFilter<'a> {
    pub target_amount: Option<Amount>,
    pub payer: Option<Payer>,
    pub payment_type: &'a str,
}

/// True when the channel is listed for this filter (`GetAvailableChannels`).
pub fn is_available(channel: &PaymentChannel, filter: &AvailabilityFilter<'_>) -> bool {
    matches_amount(channel, filter.target_amount)
        && matches_role(channel, filter.payer)
        && matches_member_level(channel, filter.payer)
        && matches_payment_type(channel, filter.payment_type)
}

/// Public channel entry of the storefront list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AvailableChannel {
    pub id: Id,
    pub name: String,
    pub provider_type: String,
    pub channel_type: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_channel_types: Vec<String>,
    pub interaction_mode: String,
    pub min_amount: Amount,
    pub max_amount: Amount,
    pub hide_amount_out_range: bool,
    /// Fee fields are only disclosed when customers pay the fee (PAY-03).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_policy: Option<FeePolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee_rate: Option<Amount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_fee: Option<Amount>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub icon: String,
}

/// Filters active channels for the storefront (`GetAvailableChannels`).
pub fn available_channels(
    channels: &[PaymentChannel],
    filter: &AvailabilityFilter<'_>,
    customer_fee_enabled: bool,
) -> Vec<AvailableChannel> {
    channels
        .iter()
        .filter(|c| is_available(c, filter))
        .map(|c| {
            let supported = c.supported_channel_types();
            let supported_channel_types =
                if supported.len() > 1 || supported.first().is_some_and(|t| t != &c.channel()) {
                    supported
                } else {
                    Vec::new()
                };
            AvailableChannel {
                id: c.id,
                name: c.name.clone(),
                provider_type: c.provider_type.clone(),
                channel_type: c.channel_type.clone(),
                supported_channel_types,
                interaction_mode: c.interaction_mode.clone(),
                min_amount: c.min_amount,
                max_amount: c.max_amount,
                hide_amount_out_range: c.hide_amount_out_range,
                fee_policy: customer_fee_enabled.then_some(FeePolicy::CustomerSurcharge),
                fee_rate: customer_fee_enabled.then_some(c.fee_rate),
                fixed_fee: customer_fee_enabled.then_some(c.fixed_fee),
                icon: c.icon.clone(),
            }
        })
        .collect()
}

/// Write-path check for order payments (`validateOrderChannelEligibility`), using the
/// order snapshot's user and member level — never client-supplied values.
pub fn check_order_channel(
    channel: &PaymentChannel,
    payer: Option<Payer>,
) -> Result<(), EligibilityError> {
    if matches_role(channel, payer)
        && matches_member_level(channel, payer)
        && matches_payment_type(channel, payment_type::ORDER)
    {
        Ok(())
    } else {
        Err(EligibilityError::NotAllowedForProduct)
    }
}

/// Write-path check for wallet recharges (`validateWalletChannelEligibility`).
pub fn check_wallet_channel(
    channel: &PaymentChannel,
    payer: Option<Payer>,
) -> Result<(), EligibilityError> {
    let Some(payer) = payer.filter(|p| p.user_id != 0) else {
        return Err(EligibilityError::Invalid);
    };
    if matches_role(channel, Some(payer))
        && matches_member_level(channel, Some(payer))
        && matches_payment_type(channel, payment_type::WALLET)
    {
        Ok(())
    } else {
        Err(EligibilityError::NotAllowedForRecharge)
    }
}

/// `validatePaymentAmountForChannel`: hard min/max limits on payment creation.
pub fn check_amount(channel: &PaymentChannel, amount: Amount) -> Result<(), EligibilityError> {
    if amount.decimal() >= Decimal::from(AMOUNT_OVERFLOW) {
        return Err(EligibilityError::AmountTooLarge);
    }
    if channel.min_amount.is_positive() && amount < channel.min_amount {
        return Err(EligibilityError::AmountTooSmall);
    }
    if channel.max_amount.is_positive() && amount > channel.max_amount {
        return Err(EligibilityError::AmountTooLarge);
    }
    Ok(())
}

/// Three upper-case ASCII letters (`settingsapp.IsCurrencyCode`).
pub fn is_currency_code(code: &str) -> bool {
    code.len() == 3 && code.bytes().all(|b| b.is_ascii_uppercase())
}

/// `validatePaymentCurrencyForChannel` (PAY-15): official WeChat/Alipay only take CNY.
pub fn check_currency(channel: &PaymentChannel, currency: &str) -> Result<(), EligibilityError> {
    let normalized = currency.trim().to_ascii_uppercase();
    if !is_currency_code(&normalized) {
        return Err(EligibilityError::CurrencyMismatch);
    }
    let cny_only = (channel.provider() == provider::OFFICIAL
        || channel.provider() == provider::HUIFU)
        && matches!(
            channel.channel().as_str(),
            channel_type::WECHAT | channel_type::ALIPAY
        );
    if cny_only && normalized != super::types::SITE_CURRENCY_DEFAULT {
        return Err(EligibilityError::CurrencyMismatch);
    }
    Ok(())
}

/// Allowed channel ids of a set of products (`computeProductChannelIntersection`, PAY-11):
/// `None` = unrestricted; `Some(empty)` = no online channel can be used.
/// Each product's list is its decoded `payment_channel_ids` (empty = unrestricted).
pub fn product_channel_intersection(products: &[Vec<Id>]) -> Option<Vec<Id>> {
    let mut intersection: Option<BTreeSet<Id>> = None;
    for allowed in products {
        let allowed: BTreeSet<Id> = allowed.iter().copied().filter(|id| *id > 0).collect();
        if allowed.is_empty() {
            continue;
        }
        intersection = Some(match intersection {
            None => allowed,
            Some(current) => current.intersection(&allowed).copied().collect(),
        });
    }
    intersection.map(|set| set.into_iter().collect())
}

/// Decodes a product's `payment_channel_ids` JSON text; garbage means unrestricted (PAY-11).
pub fn decode_channel_ids(raw: &str) -> Vec<Id> {
    serde_json::from_str::<Vec<serde_json::Value>>(raw.trim())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| {
                    v.as_i64()
                        .or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
                })
                .filter(|id| *id > 0)
                .collect()
        })
        .unwrap_or_default()
}

/// True when `channel_id` may pay for products restricted by `allowed`.
pub fn product_allows(allowed: &Option<Vec<Id>>, channel_id: Id) -> bool {
    allowed.as_ref().is_none_or(|ids| ids.contains(&channel_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    pub(crate) fn channel() -> PaymentChannel {
        PaymentChannel {
            id: 1,
            name: "ch".into(),
            icon: String::new(),
            provider_type: "epay".into(),
            channel_type: "alipay".into(),
            interaction_mode: "qr".into(),
            fee_rate: Amount::ZERO,
            fixed_fee: Amount::ZERO,
            min_amount: Amount::ZERO,
            max_amount: Amount::ZERO,
            hide_amount_out_range: false,
            payment_roles: vec![],
            member_levels: vec![],
            payment_types: vec![],
            config_json: serde_json::Map::new(),
            is_active: true,
            sort_order: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn payment_methods_expand_only_configured_aggregate_methods() {
        let mut huifu = channel();
        huifu.provider_type = "huifu".into();
        huifu.config_json =
            serde_json::json!({"supported_channel_types": ["wechat", "alipay", "invalid"]})
                .as_object()
                .cloned()
                .unwrap_or_default();
        assert_eq!(huifu.supported_channel_types(), ["wechat", "alipay"]);

        huifu.config_json.clear();
        assert_eq!(huifu.supported_channel_types(), ["alipay"]);

        let mut epusdt = channel();
        epusdt.provider_type = "epusdt".into();
        epusdt.channel_type = "usdt.tron".into();
        epusdt.config_json = serde_json::json!({"token": "USDT", "network": "TRON"})
            .as_object()
            .cloned()
            .unwrap_or_default();
        assert_eq!(epusdt.supported_channel_types(), ["usdt.tron"]);

        epusdt
            .config_json
            .insert("order_mode".into(), "cashier".into());
        assert_eq!(epusdt.supported_channel_types(), ["usdt.tron"]);
    }

    const MEMBER_L1: Payer = Payer {
        user_id: 9,
        member_level_id: 1,
    };

    /// PAY-05 (1): role, level, type and amount limits enforced on the write path.
    #[test]
    fn pay_05_write_path_rules() {
        let members_only = PaymentChannel {
            payment_roles: vec!["member".into()],
            ..channel()
        };
        assert_eq!(
            check_order_channel(&members_only, None),
            Err(EligibilityError::NotAllowedForProduct)
        );
        assert_eq!(check_order_channel(&members_only, Some(MEMBER_L1)), Ok(()));
        let vip = PaymentChannel {
            member_levels: vec![2],
            ..channel()
        };
        assert_eq!(
            check_order_channel(&vip, Some(MEMBER_L1)),
            Err(EligibilityError::NotAllowedForProduct)
        );
        let order_only = PaymentChannel {
            payment_types: vec!["order".into()],
            ..channel()
        };
        assert_eq!(
            check_wallet_channel(&order_only, Some(MEMBER_L1)),
            Err(EligibilityError::NotAllowedForRecharge)
        );
        let wallet_only = PaymentChannel {
            payment_types: vec!["wallet".into()],
            ..channel()
        };
        assert_eq!(
            check_order_channel(&wallet_only, Some(MEMBER_L1)),
            Err(EligibilityError::NotAllowedForProduct)
        );
        assert_eq!(
            check_wallet_channel(&channel(), None),
            Err(EligibilityError::Invalid)
        );
        let min10 = PaymentChannel {
            min_amount: Amount::from(10),
            ..channel()
        };
        assert_eq!(
            check_amount(&min10, Amount::from_cents(999)),
            Err(EligibilityError::AmountTooSmall)
        );
        assert_eq!(check_amount(&channel(), Amount::from(1_000_000)), Ok(()));
    }

    /// PAY-05: list filtering only hides out-of-range channels when asked to.
    #[test]
    fn pay_05_list_filter() {
        let ranged = PaymentChannel {
            min_amount: Amount::from(10),
            max_amount: Amount::from(100),
            ..channel()
        };
        assert!(matches_amount(&ranged, Some(Amount::from(5))));
        let hidden = PaymentChannel {
            hide_amount_out_range: true,
            ..ranged
        };
        assert!(!matches_amount(&hidden, Some(Amount::from(5))));
        assert!(matches_amount(&hidden, Some(Amount::from(100))));
        assert!(!matches_amount(&hidden, Some(Amount::from_cents(10_001))));
        assert!(matches_amount(&hidden, None));
        let filter = AvailabilityFilter {
            target_amount: None,
            payer: None,
            payment_type: "wallet",
        };
        let fee = PaymentChannel {
            fee_rate: Amount::from(3),
            icon: "/i.png".into(),
            payment_types: vec!["order".into()],
            ..channel()
        };
        assert!(available_channels(std::slice::from_ref(&fee), &filter, true).is_empty());
        let list = available_channels(&[fee], &AvailabilityFilter::default(), true);
        let json = serde_json::to_value(&list).unwrap_or_default();
        assert_eq!(json[0]["fee_policy"], "customer_surcharge");
        assert_eq!(json[0]["fee_rate"], "3.00");
        assert_eq!(json[0]["icon"], "/i.png");
        let hidden_fee = available_channels(&[channel()], &AvailabilityFilter::default(), false);
        let json = serde_json::to_value(&hidden_fee).unwrap_or_default();
        assert!(json[0].get("fee_rate").is_none());
    }

    /// PAY-11: product channel intersection.
    #[test]
    fn pay_11_intersection() {
        let allowed = product_channel_intersection(&[vec![1, 2], vec![2, 3]]);
        assert_eq!(allowed, Some(vec![2]));
        assert!(product_allows(&allowed, 2));
        assert!(!product_allows(&allowed, 1));
        assert_eq!(
            product_channel_intersection(&[vec![1], vec![3]]),
            Some(vec![])
        );
        assert_eq!(
            product_channel_intersection(&[vec![], vec![3]]),
            Some(vec![3])
        );
        assert_eq!(product_channel_intersection(&[vec![], vec![]]), None);
        assert!(product_allows(&None, 99));
        assert_eq!(decode_channel_ids("garbage"), Vec::<Id>::new());
        assert_eq!(decode_channel_ids("[1, 0, \"3\"]"), vec![1, 3]);
    }

    /// PAY-15: official WeChat/Alipay are CNY-only.
    #[test]
    fn pay_15_currency() {
        let alipay = PaymentChannel {
            provider_type: "official".into(),
            ..channel()
        };
        assert_eq!(
            check_currency(&alipay, "USD"),
            Err(EligibilityError::CurrencyMismatch)
        );
        assert_eq!(check_currency(&alipay, "cny"), Ok(()));
        assert_eq!(check_currency(&channel(), "USD"), Ok(()));
        assert_eq!(
            check_currency(&channel(), "US"),
            Err(EligibilityError::CurrencyMismatch)
        );
    }
}
