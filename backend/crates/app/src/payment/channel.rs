//! Admin payment-channel use cases (`admin_channel_handler.go` + `ValidateChannel`).

use std::sync::{Arc, PoisonError, RwLock};

use serde_json::Map;
use zs_domain::payment::channel::{
    ChannelCheck, ChannelConfig, ChannelDraft, ChannelFilter, ChannelRepo, PaymentChannel,
    check_channel_rules, merge_config, redact_channel,
};
use zs_domain::payment::errors::keys;
use zs_domain::payment::gateway::{
    GatewayError, GatewayRegistry, GatewaySecurityTestResult, GatewayTradeBillDownload,
    GatewayTradeBillQuery,
};
use zs_domain::payment::types::{channel_type, provider};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::Page;

/// Partial update of a channel (`UpdatePaymentChannelRequest`): `None` keeps the stored value,
/// empty strings keep the stored name/provider/channel/mode like the original.
#[derive(Debug, Clone, Default)]
pub struct ChannelPatch {
    pub name: String,
    pub icon: Option<String>,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    pub fee_rate: Option<Amount>,
    pub fixed_fee: Option<Amount>,
    pub min_amount: Option<Amount>,
    pub max_amount: Option<Amount>,
    pub hide_amount_out_range: Option<bool>,
    pub payment_roles: Option<Vec<String>>,
    pub member_levels: Option<Vec<Id>>,
    pub payment_types: Option<Vec<String>>,
    pub config_json: Option<ChannelConfig>,
    pub is_active: Option<bool>,
    pub sort_order: Option<i32>,
}

impl ChannelPatch {
    fn apply(self, draft: &mut ChannelDraft) {
        if !self.name.is_empty() {
            draft.name = self.name;
        }
        if let Some(icon) = self.icon {
            draft.icon = icon;
        }
        if !self.provider_type.is_empty() {
            draft.provider_type = self.provider_type;
        }
        if !self.channel_type.is_empty() {
            draft.channel_type = self.channel_type;
        }
        if !self.interaction_mode.is_empty() {
            draft.interaction_mode = self.interaction_mode;
        }
        macro_rules! set {
            ($($field:ident),*) => {$(if let Some(v) = self.$field { draft.$field = v; })*};
        }
        set!(
            fee_rate,
            fixed_fee,
            min_amount,
            max_amount,
            hide_amount_out_range,
            payment_roles,
            member_levels,
            payment_types,
            is_active,
            sort_order
        );
        if let Some(incoming) = self.config_json {
            draft.config_json = merge_config(&draft.config_json, &incoming);
        }
    }
}

/// Why a channel draft was rejected (mapped to the create/update error keys).
fn validation_error(err: &GatewayError) -> Error {
    match err {
        GatewayError::ConfigInvalid(_) => Error::bad_request(keys::CHANNEL_CONFIG_INVALID),
        GatewayError::UnsupportedChannel(_)
        | GatewayError::ProviderNotFound
        | GatewayError::Unsupported => Error::bad_request(keys::PROVIDER_NOT_SUPPORTED),
        _ => Error::bad_request(keys::CHANNEL_INVALID),
    }
}

/// Hook run after a channel is created, updated or deleted.
pub type ChannelsChanged = Arc<dyn Fn() + Send + Sync>;

/// Admin channel service.
#[derive(Clone)]
pub struct ChannelService {
    repo: Arc<dyn ChannelRepo>,
    registry: Arc<GatewayRegistry>,
    changed: Arc<RwLock<Option<ChannelsChanged>>>,
}

impl std::fmt::Debug for ChannelService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ChannelService")
    }
}

impl ChannelService {
    pub fn new(repo: Arc<dyn ChannelRepo>, registry: Arc<GatewayRegistry>) -> Self {
        Self {
            repo,
            registry,
            changed: Arc::new(RwLock::new(None)),
        }
    }

    /// Installs the change hook. `/public/config` embeds the storefront's payment channels
    /// and is cached (content group), so the wiring clears that cache here; otherwise a new
    /// channel stays invisible to guest checkout for up to a minute.
    pub fn on_change(&self, f: ChannelsChanged) {
        *self.changed.write().unwrap_or_else(PoisonError::into_inner) = Some(f);
    }

    fn notify_changed(&self) {
        let hook = self
            .changed
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(f) = hook {
            f();
        }
    }

    /// Lists channels (secrets redacted).
    pub async fn list(&self, filter: &ChannelFilter) -> Result<Page<PaymentChannel>> {
        let (items, total) = self
            .repo
            .list(filter)
            .await
            .map_err(|e| e.or_internal(keys::CHANNEL_FETCH_FAILED))?;
        Ok(Page {
            items: items.iter().map(redact_channel).collect(),
            total,
        })
    }

    async fn load(&self, id: Id, failure_key: &'static str) -> Result<PaymentChannel> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(failure_key))?
            .ok_or_else(|| Error::not_found(keys::CHANNEL_NOT_FOUND))
    }

    /// One channel (secrets redacted).
    pub async fn get(&self, id: Id) -> Result<PaymentChannel> {
        Ok(redact_channel(
            &self.load(id, keys::CHANNEL_FETCH_FAILED).await?,
        ))
    }

    /// `ValidateChannel`: gateway-independent rules, then the provider's own config check.
    pub fn validate(&self, draft: &ChannelDraft) -> Result<()> {
        let param = match check_channel_rules(draft) {
            Ok(ChannelCheck::Wallet) => return Ok(()),
            Ok(ChannelCheck::Gateway { validate_param }) => validate_param,
            Err(_) => return Err(Error::bad_request(keys::CHANNEL_CONFIG_INVALID)),
        };
        let gateway = self
            .registry
            .lookup(&draft.provider_type, &draft.channel_type)
            .ok_or_else(|| Error::bad_request(keys::CHANNEL_CONFIG_INVALID))?;
        gateway
            .validate_config(&draft.config_json, &param)
            .map_err(|e| validation_error(&e))
    }

    /// Creates a channel after validation.
    pub async fn create(&self, draft: ChannelDraft) -> Result<PaymentChannel> {
        self.validate(&draft)?;
        let created = self
            .repo
            .create(&draft)
            .await
            .map_err(|e| e.or_internal(keys::CHANNEL_CREATE_FAILED))?;
        self.notify_changed();
        Ok(redact_channel(&created))
    }

    /// Applies a patch (secrets merged, cleared fields removed) and saves after validation.
    pub async fn update(&self, id: Id, patch: ChannelPatch) -> Result<PaymentChannel> {
        let current = self.load(id, keys::CHANNEL_UPDATE_FAILED).await?;
        let mut draft = ChannelDraft::from(&current);
        patch.apply(&mut draft);
        self.validate(&draft)?;
        let updated = self
            .repo
            .update(id, &draft)
            .await
            .map_err(|e| e.or_internal(keys::CHANNEL_UPDATE_FAILED))?;
        self.notify_changed();
        Ok(redact_channel(&updated))
    }

    /// Soft-deletes a channel.
    pub async fn delete(&self, id: Id) -> Result<()> {
        self.repo
            .delete(id)
            .await
            .map_err(|e| e.or_internal(keys::CHANNEL_DELETE_FAILED))?;
        self.notify_changed();
        Ok(())
    }

    /// `TestChannelSecurity`: WeChat Pay public-key echo test.
    pub async fn test_security(&self, id: Id) -> Result<GatewaySecurityTestResult> {
        let channel = self.load(id, keys::WECHAT_KEY_TEST_FAILED).await?;
        if !channel.is_official(channel_type::WECHAT) {
            return Err(Error::bad_request(keys::WECHAT_KEY_TEST_UNSUPPORTED));
        }
        let gateway = self
            .registry
            .lookup(provider::OFFICIAL, channel_type::WECHAT)
            .filter(|g| g.capabilities().security_test)
            .ok_or_else(|| Error::bad_request(keys::WECHAT_KEY_TEST_UNSUPPORTED))?;
        let result = gateway
            .test_security(&channel.config_json)
            .await
            .map_err(|e| {
                tracing::warn!(channel_id = id, error = %e, "payment_channel_security_test_failed");
                match e {
                    GatewayError::ConfigInvalid(_) => {
                        Error::bad_request(keys::WECHAT_KEY_TEST_CONFIG_INVALID)
                    }
                    GatewayError::RequestFailed(_) | GatewayError::AuthFailed(_) => {
                        Error::bad_request(keys::WECHAT_KEY_TEST_REQUEST_FAILED)
                    }
                    GatewayError::ResponseInvalid(_) | GatewayError::SignatureInvalid(_) => {
                        Error::bad_request(keys::WECHAT_KEY_TEST_RESPONSE_INVALID)
                    }
                    GatewayError::UnsupportedChannel(_)
                    | GatewayError::ProviderNotFound
                    | GatewayError::Unsupported => {
                        Error::bad_request(keys::WECHAT_KEY_TEST_UNSUPPORTED)
                    }
                }
            })?;
        tracing::info!(
            channel_id = id,
            verification_mode = %result.verification_mode,
            response_serial = %result.response_serial,
            "payment_channel_security_test_success"
        );
        Ok(result)
    }

    async fn trade_bill_gateway(
        &self,
        id: Id,
    ) -> Result<(
        PaymentChannel,
        Arc<dyn zs_domain::payment::gateway::PaymentGateway>,
    )> {
        let channel = self.load(id, keys::CHANNEL_FETCH_FAILED).await?;
        if channel.provider() != provider::HUIFU {
            return Err(Error::bad_request(keys::PROVIDER_NOT_SUPPORTED));
        }
        let gateway = self
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .filter(|gateway| gateway.capabilities().trade_bills)
            .ok_or_else(|| Error::bad_request(keys::PROVIDER_NOT_SUPPORTED))?;
        Ok((channel, gateway))
    }

    pub async fn query_trade_bill(&self, id: Id, file_date: &str) -> Result<GatewayTradeBillQuery> {
        let (channel, gateway) = self.trade_bill_gateway(id).await?;
        gateway
            .query_trade_bill(&channel.config_json, file_date)
            .await
            .map_err(|error| {
                tracing::warn!(channel_id = id, %error, "payment_trade_bill_query_failed");
                Error::bad_request(keys::CHANNEL_INVALID)
            })
    }

    pub async fn download_trade_bill(
        &self,
        id: Id,
        file_date: &str,
        file_id: &str,
    ) -> Result<GatewayTradeBillDownload> {
        let (channel, gateway) = self.trade_bill_gateway(id).await?;
        gateway
            .download_trade_bill(&channel.config_json, file_date, file_id)
            .await
            .map_err(|error| {
                tracing::warn!(channel_id = id, %error, "payment_trade_bill_download_failed");
                Error::bad_request(keys::CHANNEL_INVALID)
            })
    }
}

/// Builds a create draft (`CreatePaymentChannelRequest` defaults: active unless told otherwise).
pub fn new_draft() -> ChannelDraft {
    ChannelDraft {
        is_active: true,
        config_json: Map::new(),
        ..ChannelDraft::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use serde_json::json;
    use std::sync::Mutex;
    use zs_domain::payment::gateway::{
        GatewayCapabilities, GatewayCreateInput, GatewayCreateResult, PaymentGateway,
    };

    #[derive(Debug, Default)]
    struct MemRepo {
        rows: Mutex<Vec<PaymentChannel>>,
    }

    fn to_channel(id: Id, d: &ChannelDraft) -> PaymentChannel {
        PaymentChannel {
            id,
            name: d.name.clone(),
            icon: d.icon.clone(),
            provider_type: d.provider_type.clone(),
            channel_type: d.channel_type.clone(),
            interaction_mode: d.interaction_mode.clone(),
            fee_rate: d.fee_rate,
            fixed_fee: d.fixed_fee,
            min_amount: d.min_amount,
            max_amount: d.max_amount,
            hide_amount_out_range: d.hide_amount_out_range,
            payment_roles: d.payment_roles.clone(),
            member_levels: d.member_levels.clone(),
            payment_types: d.payment_types.clone(),
            config_json: d.config_json.clone(),
            is_active: d.is_active,
            sort_order: d.sort_order,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[async_trait]
    impl ChannelRepo for MemRepo {
        async fn list(&self, _: &ChannelFilter) -> Result<(Vec<PaymentChannel>, u64)> {
            let rows = self.rows.lock().map(|r| r.clone()).unwrap_or_default();
            let n = rows.len() as u64;
            Ok((rows, n))
        }
        async fn get(&self, id: Id) -> Result<Option<PaymentChannel>> {
            Ok(self
                .rows
                .lock()
                .ok()
                .and_then(|r| r.iter().find(|c| c.id == id).cloned()))
        }
        async fn list_by_ids(&self, _: &[Id]) -> Result<Vec<PaymentChannel>> {
            Ok(vec![])
        }
        async fn create(&self, d: &ChannelDraft) -> Result<PaymentChannel> {
            let mut rows = self.rows.lock().map_err(|_| Error::internal_msg("lock"))?;
            let c = to_channel(rows.len() as Id + 1, d);
            rows.push(c.clone());
            Ok(c)
        }
        async fn update(&self, id: Id, d: &ChannelDraft) -> Result<PaymentChannel> {
            let mut rows = self.rows.lock().map_err(|_| Error::internal_msg("lock"))?;
            let c = to_channel(id, d);
            if let Some(slot) = rows.iter_mut().find(|r| r.id == id) {
                *slot = c.clone();
            }
            Ok(c)
        }
        async fn delete(&self, _: Id) -> Result<()> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct NeedsKey;

    #[async_trait]
    impl PaymentGateway for NeedsKey {
        fn key(&self) -> &'static str {
            "epay:"
        }
        fn capabilities(&self) -> GatewayCapabilities {
            GatewayCapabilities::default()
        }
        fn validate_config(
            &self,
            cfg: &ChannelConfig,
            channel: &str,
        ) -> std::result::Result<(), GatewayError> {
            if channel == "paypal" {
                return Err(GatewayError::UnsupportedChannel(channel.into()));
            }
            match cfg.get("merchant_key").and_then(|v| v.as_str()) {
                Some(k) if !k.is_empty() => Ok(()),
                _ => Err(GatewayError::config("merchant_key is required")),
            }
        }
        async fn create_payment(
            &self,
            _: &ChannelConfig,
            _: &GatewayCreateInput,
        ) -> std::result::Result<GatewayCreateResult, GatewayError> {
            Err(GatewayError::Unsupported)
        }
    }

    fn service() -> ChannelService {
        let mut registry = GatewayRegistry::new();
        registry.register("epay", "", Arc::new(NeedsKey));
        ChannelService::new(Arc::new(MemRepo::default()), Arc::new(registry))
    }

    fn draft(cfg: serde_json::Value) -> ChannelDraft {
        ChannelDraft {
            name: "Epay".into(),
            provider_type: "epay".into(),
            channel_type: "alipay".into(),
            interaction_mode: "qr".into(),
            config_json: cfg.as_object().cloned().unwrap_or_default(),
            ..new_draft()
        }
    }

    #[tokio::test]
    async fn create_redacts_and_validates() {
        let svc = service();
        let created = svc.create(draft(json!({"merchant_key": "k"}))).await;
        let created = created.ok().map(|c| c.config_json["merchant_key"].clone());
        assert_eq!(created, Some(json!("••••••••")));
        let missing = svc
            .create(draft(json!({})))
            .await
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(
            missing.as_deref(),
            Some("error.payment_channel_config_invalid")
        );
        let mut paypal = draft(json!({"merchant_key": "k"}));
        paypal.channel_type = "paypal".into();
        let err = svc.create(paypal).await.err().map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.payment_provider_not_supported"));
        let mut unknown = draft(json!({}));
        unknown.provider_type = "nope".into();
        let err = svc.create(unknown).await.err().map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.payment_channel_config_invalid"));
    }

    /// PAY-19: update keeps redacted secrets and drops cleared fields.
    #[tokio::test]
    async fn pay_19_update_merges_config() {
        let svc = service();
        let id = svc
            .create(draft(
                json!({"merchant_key": "secret-k", "exchange_rate": "7.2"}),
            ))
            .await
            .map(|c| c.id)
            .unwrap_or_default();
        let patch = ChannelPatch {
            config_json: json!({"merchant_key": "••••••••"}).as_object().cloned(),
            ..ChannelPatch::default()
        };
        assert!(svc.update(id, patch).await.is_ok());
        let stored = svc
            .repo
            .get(id)
            .await
            .ok()
            .flatten()
            .map(|c| c.config_json)
            .unwrap_or_default();
        assert_eq!(stored.get("merchant_key"), Some(&json!("secret-k")));
        assert_eq!(stored.get("exchange_rate"), None);
        let missing = svc
            .update(999, ChannelPatch::default())
            .await
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(missing.as_deref(), Some("error.payment_channel_not_found"));
    }

    /// Found by the E2E suite: a channel created in the admin must reach the (cached)
    /// storefront `/public/config` right away, so every successful write fires the hook
    /// and a rejected write does not.
    #[tokio::test]
    async fn channel_writes_fire_the_change_hook() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let svc = service();
        let hits = Arc::new(AtomicUsize::new(0));
        let counter = hits.clone();
        svc.on_change(Arc::new(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        }));
        assert!(svc.create(draft(json!({}))).await.is_err());
        assert_eq!(hits.load(Ordering::SeqCst), 0);
        let id = svc
            .create(draft(json!({"merchant_key": "k"})))
            .await
            .map(|c| c.id)
            .unwrap_or_default();
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        assert!(svc.update(id, ChannelPatch::default()).await.is_ok());
        assert_eq!(hits.load(Ordering::SeqCst), 2);
        assert!(svc.delete(id).await.is_ok());
        assert_eq!(hits.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn security_test_only_for_official_wechat() {
        let svc = service();
        let id = svc
            .create(draft(json!({"merchant_key": "k"})))
            .await
            .map(|c| c.id)
            .unwrap_or_default();
        let err = svc
            .test_security(id)
            .await
            .err()
            .map(|e| e.key().to_owned());
        assert_eq!(err.as_deref(), Some("error.wechatpay_key_test_unsupported"));
    }
}
