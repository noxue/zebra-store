//! Payment gateway port (`contract/gateway.go`) and the provider registry.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value};
use zs_shared::money::Amount;

use super::channel::ChannelConfig;
use super::form::FormMap;
use super::types::{InteractionMode, PaymentStatus};
use crate::Id;

/// Normalized gateway failure (`contract/gateway_errors.go`); the message is for logs only.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GatewayError {
    #[error("payment provider config invalid: {0}")]
    ConfigInvalid(String),
    #[error("payment provider request failed: {0}")]
    RequestFailed(String),
    #[error("payment provider response invalid: {0}")]
    ResponseInvalid(String),
    #[error("payment provider signature invalid: {0}")]
    SignatureInvalid(String),
    #[error("payment provider auth failed: {0}")]
    AuthFailed(String),
    /// The provider explicitly reports that this merchant lacks the requested product access.
    #[error("payment provider permission missing: {0}")]
    ProviderPermissionMissing(String),
    #[error("payment channel type not supported by provider: {0}")]
    UnsupportedChannel(String),
    #[error("payment provider not found in registry")]
    ProviderNotFound,
    /// The provider lacks the requested capability (query / webhook / callback / security test).
    #[error("payment provider capability not supported")]
    Unsupported,
}

impl GatewayError {
    pub fn config(msg: impl Into<String>) -> Self {
        Self::ConfigInvalid(msg.into())
    }
    pub fn request(msg: impl Into<String>) -> Self {
        Self::RequestFailed(msg.into())
    }
    pub fn response(msg: impl Into<String>) -> Self {
        Self::ResponseInvalid(msg.into())
    }
    pub fn signature(msg: impl Into<String>) -> Self {
        Self::SignatureInvalid(msg.into())
    }
}

/// Request headers with case-insensitive lookup (first value per name).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Headers(BTreeMap<String, String>);

impl Headers {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a header (names are stored lower-cased; the first value wins).
    pub fn insert(&mut self, name: &str, value: &str) {
        self.0
            .entry(name.trim().to_ascii_lowercase())
            .or_insert_with(|| value.to_owned());
    }

    /// Trimmed header value, or `""` when absent.
    pub fn get(&self, name: &str) -> String {
        self.0
            .get(&name.trim().to_ascii_lowercase())
            .map(|v| v.trim().to_owned())
            .unwrap_or_default()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.0.iter()
    }
}

impl<const N: usize> From<[(&str, &str); N]> for Headers {
    fn from(items: [(&str, &str); N]) -> Self {
        let mut h = Self::new();
        for (k, v) in items {
            h.insert(k, v);
        }
        h
    }
}

/// Input of [`PaymentGateway::create_payment`] (`GatewayCreateInput`).
#[derive(Debug, Clone, Default)]
pub struct GatewayCreateInput {
    pub payment_id: Id,
    pub order_id: Id,
    /// Merchant order number sent to the gateway — the payment's `gateway_order_no` (PAY-09).
    pub order_no: String,
    /// Display subject; always the business order number (PAY-22).
    pub subject: String,
    pub amount: Amount,
    pub currency: String,
    pub email: String,
    /// Overrides the channel's notify URL when non-empty.
    pub notify_url: String,
    /// Tenant return URL (PAY-06); empty = channel config.
    pub return_url: String,
    /// Parameters appended to the return URL (PAY-16/PAY-20).
    pub return_url_query: BTreeMap<String, String>,
    pub client_ip: String,
    /// Original browser user agent, used by gateways with distinct H5/PC flows.
    pub user_agent: String,
    pub channel_type: String,
    /// The channel's interaction mode (typed to avoid the PAY-08 parameter mix-up).
    pub interaction_mode: Option<InteractionMode>,
    /// TokenPay `OrderUserKey` (user id, guest e-mail or order number).
    pub order_user_key: String,
    /// PayPal/Stripe/DujiaoPay cancel URL override.
    pub cancel_url: String,
}

/// Result of a created gateway payment (`GatewayCreateResult`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GatewayCreateResult {
    pub provider_ref: String,
    pub redirect_url: String,
    pub qr_code_url: String,
    pub payload: Map<String, Value>,
    pub display_channel_type: String,
    /// Amount actually sent to the gateway (after conversion); empty = unchanged.
    pub amount_sent: String,
    /// Currency actually sent to the gateway; empty = unchanged.
    pub currency_sent: String,
}

/// Result of an active status query (`GatewayQueryResult`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GatewayQueryResult {
    pub provider_ref: String,
    pub status: Option<PaymentStatus>,
    pub amount: Amount,
    pub currency: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub payload: Map<String, Value>,
}

/// Input for an original-route refund.
#[derive(Debug, Clone, Default)]
pub struct GatewayRefundInput {
    pub provider_ref: String,
    /// Stable request date stored before contacting the provider.
    pub request_date: String,
    pub refund_no: String,
    pub amount: Amount,
    pub notify_url: String,
    pub remark: String,
    pub client_ip: String,
}

/// Result of a refund request. `status = Pending` means the request was accepted and must be
/// confirmed by a signed notification or an active refund query.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GatewayRefundResult {
    pub provider_ref: String,
    pub status: Option<PaymentStatus>,
    pub payload: Map<String, Value>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GatewayTradeBillFile {
    pub file_date: String,
    pub file_id: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GatewayTradeBillTask {
    pub data_date: String,
    pub task_stat: String,
    pub task_start_time: String,
    pub task_end_time: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GatewayTradeBillQuery {
    pub files: Vec<GatewayTradeBillFile>,
    pub tasks: Vec<GatewayTradeBillTask>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GatewayTradeBillDownload {
    pub file_name: String,
    pub body: Vec<u8>,
}

/// A verified callback/webhook, normalized (`GatewayCallbackResult`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GatewayCallbackResult {
    /// Merchant order number echoed by the gateway (gateway order number or business number).
    pub order_no: String,
    pub provider_ref: String,
    /// `None` = event without a payment state change (ignored).
    pub status: Option<PaymentStatus>,
    /// Zero when the gateway sent no (parsable) amount.
    pub amount: Amount,
    pub currency: String,
    pub paid_at: Option<DateTime<Utc>>,
    pub payload: Map<String, Value>,
}

/// Read-only security diagnosis (`GatewaySecurityTestResult`); contains no secrets.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GatewaySecurityTestResult {
    pub verification_mode: String,
    pub response_serial: String,
    pub request_signature_accepted: bool,
    pub response_signature_valid: bool,
    pub echo_message_matched: bool,
}

/// Optional capabilities of a gateway.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GatewayCapabilities {
    pub query: bool,
    pub webhook: bool,
    pub callback: bool,
    pub security_test: bool,
    pub trade_bills: bool,
}

/// A payment provider adapter.
#[async_trait]
pub trait PaymentGateway: Send + Sync + std::fmt::Debug {
    /// Registry key such as `official:alipay` or `epay:`.
    fn key(&self) -> &'static str;

    fn capabilities(&self) -> GatewayCapabilities;

    /// Validates the channel config. `param` is the interaction mode for official gateways and
    /// the channel type for aggregators (see [`super::channel::check_channel_rules`]).
    fn validate_config(&self, config: &ChannelConfig, param: &str) -> Result<(), GatewayError>;

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError>;

    /// Actively queries (or captures) a payment by provider reference / gateway order number.
    async fn query_payment(
        &self,
        _config: &ChannelConfig,
        _provider_ref: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    async fn refund_payment(
        &self,
        _config: &ChannelConfig,
        _input: &GatewayRefundInput,
    ) -> Result<GatewayRefundResult, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    async fn query_refund(
        &self,
        _config: &ChannelConfig,
        _provider_ref: &str,
    ) -> Result<GatewayRefundResult, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    async fn query_trade_bill(
        &self,
        _config: &ChannelConfig,
        _file_date: &str,
    ) -> Result<GatewayTradeBillQuery, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    async fn download_trade_bill(
        &self,
        _config: &ChannelConfig,
        _file_date: &str,
        _file_id: &str,
    ) -> Result<GatewayTradeBillDownload, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    /// Verifies and parses an asynchronous webhook from its raw body.
    async fn parse_webhook(
        &self,
        _config: &ChannelConfig,
        _headers: &Headers,
        _body: &[u8],
        _now: DateTime<Utc>,
    ) -> Result<GatewayCallbackResult, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    /// Verifies and parses a synchronous callback (form fields or raw JSON body).
    fn verify_callback(
        &self,
        _config: &ChannelConfig,
        _form: &FormMap,
        _body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        Err(GatewayError::Unsupported)
    }

    /// Non-transactional security diagnosis (WeChat Pay public key test).
    async fn test_security(
        &self,
        _config: &ChannelConfig,
    ) -> Result<GatewaySecurityTestResult, GatewayError> {
        Err(GatewayError::Unsupported)
    }
}

/// Registry of gateways by `(provider_type, channel_type)` with a `(provider_type, "")` fallback.
#[derive(Debug, Clone, Default)]
pub struct GatewayRegistry {
    providers: HashMap<String, Arc<dyn PaymentGateway>>,
}

fn registry_key(provider_type: &str, channel_type: &str) -> String {
    format!(
        "{}:{}",
        provider_type.trim().to_ascii_lowercase(),
        channel_type.trim().to_ascii_lowercase()
    )
}

impl GatewayRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers (or replaces) a gateway; aggregators register with an empty channel type.
    pub fn register(
        &mut self,
        provider_type: &str,
        channel_type: &str,
        gateway: Arc<dyn PaymentGateway>,
    ) {
        self.providers
            .insert(registry_key(provider_type, channel_type), gateway);
    }

    /// Exact match first, then the provider-wide fallback.
    pub fn lookup(
        &self,
        provider_type: &str,
        channel_type: &str,
    ) -> Option<Arc<dyn PaymentGateway>> {
        self.providers
            .get(&registry_key(provider_type, channel_type))
            .or_else(|| self.providers.get(&registry_key(provider_type, "")))
            .cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Dummy(&'static str);

    #[async_trait]
    impl PaymentGateway for Dummy {
        fn key(&self) -> &'static str {
            self.0
        }
        fn capabilities(&self) -> GatewayCapabilities {
            GatewayCapabilities::default()
        }
        fn validate_config(&self, _: &ChannelConfig, _: &str) -> Result<(), GatewayError> {
            Ok(())
        }
        async fn create_payment(
            &self,
            _: &ChannelConfig,
            _: &GatewayCreateInput,
        ) -> Result<GatewayCreateResult, GatewayError> {
            Err(GatewayError::Unsupported)
        }
    }

    #[test]
    fn registry_falls_back_to_provider_wide_entry() {
        let mut r = GatewayRegistry::new();
        r.register("official", "stripe", Arc::new(Dummy("official:stripe")));
        r.register("epay", "", Arc::new(Dummy("epay:")));
        assert_eq!(
            r.lookup("OFFICIAL", " Stripe ").map(|g| g.key()),
            Some("official:stripe")
        );
        assert!(r.lookup("official", "paypal").is_none());
        assert_eq!(r.lookup("epay", "wechat").map(|g| g.key()), Some("epay:"));
    }

    #[test]
    fn headers_are_case_insensitive() {
        let h = Headers::from([("Stripe-Signature", " t=1 "), ("stripe-signature", "x")]);
        assert_eq!(h.get("STRIPE-SIGNATURE"), "t=1");
        assert_eq!(h.get("missing"), "");
    }
}
