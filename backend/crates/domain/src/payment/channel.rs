//! Payment channels: model, persistence port, validation and config redaction.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use serde_json::{Map, Value};
use zs_shared::money::Amount;
use zs_shared::page::PageRequest;

use super::types::{InteractionMode, order_mode, provider};
use crate::{Id, Result};

/// Per-gateway JSON configuration (`config_json`).
pub type ChannelConfig = Map<String, Value>;

/// Placeholder returned instead of secrets (`redactedPaymentConfigValue`).
pub const REDACTED_VALUE: &str = "••••••••";

/// Config keys never echoed back to the admin UI (`sensitivePaymentConfigKeys`).
pub const SENSITIVE_CONFIG_KEYS: [&str; 11] = [
    "api_secret",
    "api_v3_key",
    "auth_token",
    "client_secret",
    "merchant_key",
    "merchant_private_key",
    "merchant_token",
    "notify_secret",
    "private_key",
    "secret_key",
    "webhook_secret",
];

/// Largest fee rate in percent.
const MAX_FEE_RATE: i64 = 100;
/// Fixed fees must fit `decimal(6,2)` (≤ 9999.99, PAY-12).
const FIXED_FEE_LIMIT: i64 = 10_000;
/// Amounts must fit `decimal(20,2)` (the original's `amountOverflow20_2`).
pub const AMOUNT_OVERFLOW: i64 = 1_000_000_000_000_000_000;

/// A payment channel as returned by the admin API (JSON shape of the original model).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PaymentChannel {
    pub id: Id,
    pub name: String,
    pub icon: String,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    pub fee_rate: Amount,
    pub fixed_fee: Amount,
    pub min_amount: Amount,
    pub max_amount: Amount,
    pub hide_amount_out_range: bool,
    pub payment_roles: Vec<String>,
    pub member_levels: Vec<Id>,
    pub payment_types: Vec<String>,
    pub config_json: ChannelConfig,
    pub is_active: bool,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PaymentChannel {
    /// Lower-cased trimmed provider type.
    pub fn provider(&self) -> String {
        self.provider_type.trim().to_ascii_lowercase()
    }

    /// Lower-cased trimmed channel type.
    pub fn channel(&self) -> String {
        self.channel_type.trim().to_ascii_lowercase()
    }

    /// True for `official` + the given channel type.
    pub fn is_official(&self, channel_type: &str) -> bool {
        self.provider() == provider::OFFICIAL && self.channel() == channel_type
    }
}

/// Fields written on create/update.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChannelDraft {
    pub name: String,
    pub icon: String,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    pub fee_rate: Amount,
    pub fixed_fee: Amount,
    pub min_amount: Amount,
    pub max_amount: Amount,
    pub hide_amount_out_range: bool,
    pub payment_roles: Vec<String>,
    pub member_levels: Vec<Id>,
    pub payment_types: Vec<String>,
    pub config_json: ChannelConfig,
    pub is_active: bool,
    pub sort_order: i32,
}

impl From<&PaymentChannel> for ChannelDraft {
    fn from(c: &PaymentChannel) -> Self {
        Self {
            name: c.name.clone(),
            icon: c.icon.clone(),
            provider_type: c.provider_type.clone(),
            channel_type: c.channel_type.clone(),
            interaction_mode: c.interaction_mode.clone(),
            fee_rate: c.fee_rate,
            fixed_fee: c.fixed_fee,
            min_amount: c.min_amount,
            max_amount: c.max_amount,
            hide_amount_out_range: c.hide_amount_out_range,
            payment_roles: c.payment_roles.clone(),
            member_levels: c.member_levels.clone(),
            payment_types: c.payment_types.clone(),
            config_json: c.config_json.clone(),
            is_active: c.is_active,
            sort_order: c.sort_order,
        }
    }
}

/// Channel list filter (`ChannelListFilter`); `page = None` lists everything.
#[derive(Debug, Clone, Default)]
pub struct ChannelFilter {
    pub page: Option<PageRequest>,
    pub provider_type: String,
    pub channel_type: String,
    pub active_only: bool,
}

/// Persistence port for payment channels. Soft-deleted rows are never returned.
#[async_trait]
pub trait ChannelRepo: Send + Sync {
    /// Ordered by `sort_order DESC, id ASC`; returns the total before pagination.
    async fn list(&self, filter: &ChannelFilter) -> Result<(Vec<PaymentChannel>, u64)>;
    async fn get(&self, id: Id) -> Result<Option<PaymentChannel>>;
    async fn list_by_ids(&self, ids: &[Id]) -> Result<Vec<PaymentChannel>>;
    async fn create(&self, draft: &ChannelDraft) -> Result<PaymentChannel>;
    async fn update(&self, id: Id, draft: &ChannelDraft) -> Result<PaymentChannel>;
    /// Soft delete; deleting a missing channel is not an error.
    async fn delete(&self, id: Id) -> Result<()>;
}

/// Why a channel draft is rejected before its gateway config is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelRuleError {
    /// `error.payment_channel_config_invalid`
    ConfigInvalid,
}

/// Outcome of the gateway-independent channel rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelCheck {
    /// Wallet channels have no gateway config.
    Wallet,
    /// The gateway must validate its config with this parameter
    /// (interaction mode for `official`, channel type otherwise).
    Gateway { validate_param: String },
}

/// Gateway-independent rules of `ValidateChannel`: fee/amount ranges and interaction modes.
pub fn check_channel_rules(
    draft: &ChannelDraft,
) -> std::result::Result<ChannelCheck, ChannelRuleError> {
    let invalid = Err(ChannelRuleError::ConfigInvalid);
    let fee_rate = draft.fee_rate.decimal();
    if fee_rate < Decimal::ZERO || fee_rate > Decimal::from(MAX_FEE_RATE) {
        return invalid;
    }
    let fixed_fee = draft.fixed_fee.decimal();
    if fixed_fee < Decimal::ZERO || fixed_fee >= Decimal::from(FIXED_FEE_LIMIT) {
        return invalid;
    }
    let (min, max) = (draft.min_amount.decimal(), draft.max_amount.decimal());
    let overflow = Decimal::from(AMOUNT_OVERFLOW);
    if min < Decimal::ZERO || min >= overflow || max < Decimal::ZERO || max >= overflow {
        return invalid;
    }
    if max > Decimal::ZERO && min > max {
        return invalid;
    }
    let provider_type = draft.provider_type.trim().to_ascii_lowercase();
    if provider_type.is_empty() {
        return invalid;
    }
    if provider_type == provider::WALLET {
        return Ok(ChannelCheck::Wallet);
    }
    let mode = draft.interaction_mode.trim().to_ascii_lowercase();
    if provider_type != provider::OFFICIAL {
        let mode_ok = matches!(
            InteractionMode::parse(&mode),
            Some(InteractionMode::Qr | InteractionMode::Redirect)
        );
        if !mode_ok {
            return invalid;
        }
        // PAY-44: BEpusdt cashier orders cannot be rendered as a QR code.
        if provider_type == provider::BEPUSDT
            && mode == "qr"
            && config_str(&draft.config_json, "order_mode").to_ascii_lowercase()
                == order_mode::CASHIER
        {
            return invalid;
        }
    }
    let validate_param = if provider_type == provider::OFFICIAL {
        mode
    } else {
        draft.channel_type.trim().to_ascii_lowercase()
    };
    Ok(ChannelCheck::Gateway { validate_param })
}

/// Reads a config value as a trimmed string (`fmt.Sprint` of non-strings is not needed here).
pub fn config_str(config: &ChannelConfig, key: &str) -> String {
    match config.get(key) {
        Some(Value::String(s)) => s.trim().to_owned(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn is_sensitive(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase();
    SENSITIVE_CONFIG_KEYS.contains(&key.as_str())
}

fn non_empty_string(value: &Value) -> bool {
    value.as_str().is_some_and(|s| !s.trim().is_empty())
}

/// Replaces non-empty secrets with [`REDACTED_VALUE`] and drops empty ones (`redactPaymentChannel`).
pub fn redact_config(config: &ChannelConfig) -> ChannelConfig {
    config
        .iter()
        .filter_map(|(k, v)| {
            if is_sensitive(k) {
                non_empty_string(v).then(|| (k.clone(), Value::String(REDACTED_VALUE.to_owned())))
            } else {
                Some((k.clone(), v.clone()))
            }
        })
        .collect()
}

/// Returns a copy of the channel with secrets redacted.
pub fn redact_channel(channel: &PaymentChannel) -> PaymentChannel {
    PaymentChannel {
        config_json: redact_config(&channel.config_json),
        ..channel.clone()
    }
}

/// Merges an edited config (`mergePaymentChannelConfig`): the incoming object replaces every
/// ordinary key (so cleared fields really disappear, PAY-19), while blank/redacted secrets keep
/// their stored value and `null` explicitly clears a secret.
pub fn merge_config(existing: &ChannelConfig, incoming: &ChannelConfig) -> ChannelConfig {
    let mut merged = ChannelConfig::new();
    let mut cleared = Vec::new();
    for (key, value) in incoming {
        if is_sensitive(key) {
            if value.is_null() {
                cleared.push(key.clone());
                continue;
            }
            let text = value.as_str().map(str::trim).unwrap_or_default();
            if text.is_empty() || text == REDACTED_VALUE {
                if let Some(current) = existing.get(key) {
                    merged.insert(key.clone(), current.clone());
                }
                continue;
            }
        }
        merged.insert(key.clone(), value.clone());
    }
    for (key, value) in existing {
        if is_sensitive(key) && !cleared.contains(key) && !merged.contains_key(key) {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draft() -> ChannelDraft {
        ChannelDraft {
            name: "ch".into(),
            provider_type: "epay".into(),
            channel_type: "alipay".into(),
            interaction_mode: "qr".into(),
            ..ChannelDraft::default()
        }
    }

    fn obj(v: Value) -> ChannelConfig {
        v.as_object().cloned().unwrap_or_default()
    }

    /// PAY-12: fee and amount ranges are enforced on save.
    #[test]
    fn pay_12_fee_ranges() {
        let ok = draft();
        assert!(check_channel_rules(&ok).is_ok());
        for bad in [
            ChannelDraft {
                fixed_fee: Amount::from_cents(-100),
                ..draft()
            },
            ChannelDraft {
                fixed_fee: Amount::from(10_000),
                ..draft()
            },
            ChannelDraft {
                fee_rate: Amount::from_cents(10_001),
                ..draft()
            },
            ChannelDraft {
                fee_rate: Amount::from_cents(-1),
                ..draft()
            },
            ChannelDraft {
                min_amount: Amount::from(10),
                max_amount: Amount::from(5),
                ..draft()
            },
        ] {
            assert_eq!(
                check_channel_rules(&bad),
                Err(ChannelRuleError::ConfigInvalid)
            );
        }
        let fixed_max = ChannelDraft {
            fixed_fee: Amount::from_cents(999_999),
            max_amount: Amount::ZERO,
            min_amount: Amount::from(10),
            ..draft()
        };
        assert!(check_channel_rules(&fixed_max).is_ok());
    }

    /// PAY-37 / PAY-44: interaction modes of aggregated gateways.
    #[test]
    fn pay_37_pay_44_mode_rules() {
        let foo = ChannelDraft {
            interaction_mode: "foo".into(),
            ..draft()
        };
        assert_eq!(
            check_channel_rules(&foo),
            Err(ChannelRuleError::ConfigInvalid)
        );
        let cashier_qr = ChannelDraft {
            provider_type: "bepusdt".into(),
            channel_type: "bepusdt".into(),
            config_json: obj(json!({"order_mode": "cashier"})),
            ..draft()
        };
        assert_eq!(
            check_channel_rules(&cashier_qr),
            Err(ChannelRuleError::ConfigInvalid)
        );
        let official = ChannelDraft {
            provider_type: "official".into(),
            channel_type: "alipay".into(),
            interaction_mode: "WAP".into(),
            ..draft()
        };
        assert_eq!(
            check_channel_rules(&official),
            Ok(ChannelCheck::Gateway {
                validate_param: "wap".into()
            })
        );
        let wallet = ChannelDraft {
            provider_type: "wallet".into(),
            interaction_mode: String::new(),
            ..draft()
        };
        assert_eq!(check_channel_rules(&wallet), Ok(ChannelCheck::Wallet));
    }

    #[test]
    fn redacts_secrets() {
        let cfg =
            obj(json!({"merchant_key": "abc", "private_key": " ", "gateway_url": "https://x"}));
        assert_eq!(
            redact_config(&cfg),
            obj(json!({"merchant_key": REDACTED_VALUE, "gateway_url": "https://x"}))
        );
    }

    /// PAY-19: cleared ordinary fields disappear; blank/redacted secrets are kept; null clears.
    #[test]
    fn pay_19_merge_config() {
        let existing = obj(json!({
            "exchange_rate": "7.2", "target_currency": "CNY", "merchant_key": "k1", "secret_key": "s1", "api_secret": "a1"
        }));
        let incoming = obj(json!({
            "gateway_url": "https://x", "merchant_key": REDACTED_VALUE, "secret_key": null, "api_secret": "new"
        }));
        assert_eq!(
            merge_config(&existing, &incoming),
            obj(json!({"gateway_url": "https://x", "merchant_key": "k1", "api_secret": "new"}))
        );
        let omitted = merge_config(&existing, &obj(json!({"gateway_url": "https://y"})));
        assert_eq!(omitted.get("merchant_key"), Some(&json!("k1")));
        assert_eq!(omitted.get("exchange_rate"), None);
    }
}
