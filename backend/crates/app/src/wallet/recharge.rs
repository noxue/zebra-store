//! Wallet recharges: channel listing, creation, status capture, timeout expiry and the
//! completion path (credit + member level + notifications).

use std::sync::Arc;

use chrono::Duration;
use rust_decimal::Decimal;
use zs_domain::payment::callback::CallbackInput;
use zs_domain::payment::eligibility::{
    AvailabilityFilter, AvailableChannel, available_channels, check_amount, check_currency,
    check_wallet_channel,
};
use zs_domain::payment::errors::keys as pay_keys;
use zs_domain::payment::fee::calculate_payment_amounts;
use zs_domain::payment::model::Payment;
use zs_domain::payment::types::{PaymentStatus, SITE_CURRENCY_DEFAULT, payment_type};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::settings::schema::site::{OrderSetting, PaymentFeeSetting, WalletSetting};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::wallet::ports::RechargeDraft;
use zs_domain::wallet::rules::{DEFAULT_RECHARGE_REMARK, clean_remark, recharge_no};
use zs_domain::wallet::{
    Account, RechargeGateway, RechargeHooks, RechargeLookup, RechargeOrder, RechargeSettled,
    RechargeStore, WalletRepo, keys,
};
use zs_domain::{Error, ErrorKind, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::money::Amount;

/// Largest accepted fixed fee (original `fixedFee >= 10000` is a config error).
const FIXED_FEE_LIMIT: i64 = 10_000;
/// Largest fee rate in percent.
const FEE_RATE_LIMIT: i64 = 100;

/// A recharge request from the storefront.
#[derive(Debug, Clone, Default)]
pub struct RechargeRequest {
    pub user_id: Id,
    pub channel_id: Id,
    /// Raw amount as sent by the client.
    pub amount: String,
    pub currency: String,
    pub remark: String,
    pub client_ip: String,
}

/// A recharge with its payment and the user's account (`WalletRechargePaymentPayload`).
#[derive(Debug, Clone)]
pub struct RechargeView {
    pub recharge: RechargeOrder,
    pub payment: Option<Payment>,
    pub account: Account,
}

/// Payload of the `wallet_recharge:timeout_expire` job.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExpirePayload {
    pub payment_id: Id,
}

/// Recharge use cases.
#[derive(Clone)]
pub struct RechargeService {
    wallets: Arc<dyn WalletRepo>,
    store: Arc<dyn RechargeStore>,
    lookup: Arc<dyn RechargeLookup>,
    gateway: Arc<dyn RechargeGateway>,
    hooks: Arc<dyn RechargeHooks>,
    queue: Arc<dyn JobQueue>,
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
    /// `config.yml` fallback of the payment window.
    order_fallback: OrderSetting,
}

impl std::fmt::Debug for RechargeService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RechargeService")
    }
}

/// Dependencies of [`RechargeService`].
#[derive(Clone)]
pub struct RechargeDeps {
    pub wallets: Arc<dyn WalletRepo>,
    pub store: Arc<dyn RechargeStore>,
    pub lookup: Arc<dyn RechargeLookup>,
    pub gateway: Arc<dyn RechargeGateway>,
    pub hooks: Arc<dyn RechargeHooks>,
    pub queue: Arc<dyn JobQueue>,
    pub settings: Arc<dyn SettingsStore>,
    pub clock: Arc<dyn Clock>,
    pub order_fallback: OrderSetting,
}

impl std::fmt::Debug for RechargeDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RechargeDeps")
    }
}

/// Parses a client amount (`decimal.NewFromString`), rounded to two decimals.
fn parse_amount(raw: &str) -> Result<Amount> {
    raw.trim()
        .parse::<Decimal>()
        .map(Amount::new)
        .map_err(|_| Error::invalid())
}

fn create_failed(e: Error) -> Error {
    e.or_internal(keys::PAYMENT_CREATE_FAILED)
}

impl RechargeService {
    pub fn new(deps: RechargeDeps) -> Self {
        Self {
            wallets: deps.wallets,
            store: deps.store,
            lookup: deps.lookup,
            gateway: deps.gateway,
            hooks: deps.hooks,
            queue: deps.queue,
            settings: deps.settings,
            clock: deps.clock,
            order_fallback: deps.order_fallback,
        }
    }

    async fn setting(&self, key: &str) -> Option<serde_json::Value> {
        self.settings.get(key).await.ok().flatten()
    }

    /// Channels usable for a top-up of `amount` (`GetAvailableChannels` with payment type
    /// `wallet`); the same rules are re-checked on creation (PAY-05).
    pub async fn channels(&self, user_id: Id, amount: &str) -> Result<Vec<AvailableChannel>> {
        let amount = parse_amount(amount)?;
        if !amount.is_positive() {
            return Err(Error::invalid());
        }
        let payer = self.lookup.payer(user_id).await.ok().flatten();
        let channels = self
            .lookup
            .active_channels()
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?;
        let fee =
            PaymentFeeSetting::decode(self.setting(setting_keys::PAYMENT_CONFIG).await.as_ref());
        let filter = AvailabilityFilter {
            target_amount: Some(amount),
            payer,
            payment_type: payment_type::WALLET,
        };
        Ok(available_channels(
            &channels,
            &filter,
            fee.customer_fee_enabled,
        ))
    }

    /// Payment window in minutes (`resolveExpireMinutes`: setting → config → 15).
    async fn expire_minutes(&self) -> i64 {
        OrderSetting::decode(
            self.setting(setting_keys::ORDER_CONFIG).await.as_ref(),
            self.order_fallback,
        )
        .payment_expire_minutes
    }

    /// Creates a recharge order and its payment (`CreateWalletRechargePayment`).
    pub async fn create(&self, req: &RechargeRequest) -> Result<RechargeView> {
        let amount = parse_amount(&req.amount)?;
        if req.user_id <= 0 || req.channel_id <= 0 {
            return Err(Error::bad_request(keys::PAYMENT_INVALID));
        }
        if !amount.is_positive() {
            return Err(Error::invalid());
        }
        let channel = self
            .lookup
            .channel(req.channel_id)
            .await
            .map_err(create_failed)?
            .ok_or_else(|| Error::not_found(pay_keys::CHANNEL_NOT_FOUND))?;
        if !channel.is_active {
            return Err(Error::bad_request(pay_keys::CHANNEL_INACTIVE));
        }
        let payer = self
            .lookup
            .payer(req.user_id)
            .await
            .map_err(create_failed)?;
        check_wallet_channel(&channel, payer)?;
        let wallet =
            WalletSetting::decode(self.setting(setting_keys::WALLET_CONFIG).await.as_ref());
        if !wallet.recharge_channel_ids.is_empty()
            && !wallet.recharge_channel_ids.contains(&channel.id)
        {
            return Err(Error::bad_request(
                pay_keys::CHANNEL_NOT_ALLOWED_FOR_RECHARGE,
            ));
        }
        let fee_rate = channel.fee_rate;
        let fixed_fee = channel.fixed_fee;
        if fee_rate.is_negative()
            || fee_rate > Amount::from(FEE_RATE_LIMIT)
            || fixed_fee.is_negative()
            || fixed_fee >= Amount::from(FIXED_FEE_LIMIT)
        {
            return Err(Error::bad_request(pay_keys::CHANNEL_CONFIG_INVALID));
        }
        check_amount(&channel, amount)?;
        let fee =
            PaymentFeeSetting::decode(self.setting(setting_keys::PAYMENT_CONFIG).await.as_ref());
        let amounts = calculate_payment_amounts(
            amount,
            fee_rate.decimal(),
            fixed_fee.decimal(),
            fee.customer_fee_enabled,
        );
        // The handler defaults an empty currency to the site currency (CNY by default).
        let mut currency = req.currency.trim().to_ascii_uppercase();
        if currency.is_empty() {
            currency = super::account::site_currency(self.settings.as_ref()).await;
        }
        if currency.is_empty() {
            currency = SITE_CURRENCY_DEFAULT.to_owned();
        }
        check_currency(&channel, &currency)?;

        let now = self.clock.now();
        let draft = RechargeDraft {
            recharge_no: recharge_no(now),
            user_id: req.user_id,
            channel: channel.clone(),
            amount,
            payable_amount: amounts.payable,
            fee_rate,
            fixed_fee,
            fee_amount: amounts.fee,
            fee_policy: amounts.policy,
            currency,
            remark: clean_remark(&req.remark, DEFAULT_RECHARGE_REMARK),
            now,
        };
        let (recharge, payment) = self.store.create(&draft).await.map_err(create_failed)?;

        // The gateway is called outside any database transaction (DB-01).
        let started = match self
            .gateway
            .start(&channel, &payment, &recharge, &req.client_ip)
            .await
        {
            Ok(p) => self.store.save_started(&p).await.map(|()| p),
            Err(e) => Err(e),
        };
        let payment = match started {
            Ok(p) => p,
            Err(e) => {
                if let Err(mark) = self.store.mark_failed(payment.id, self.clock.now()).await {
                    tracing::warn!(payment_id = payment.id, error = %mark, "wallet_recharge_mark_failed_failed");
                }
                return Err(create_failed(e));
            }
        };

        let delay = Duration::minutes(self.expire_minutes().await);
        let job = NewJob::new(
            kinds::WALLET_RECHARGE_EXPIRE,
            ExpirePayload {
                payment_id: payment.id,
            },
        )?
        .at(self.clock.now() + delay)
        .unique(format!("wallet_recharge_expire:{}", payment.id));
        if let Err(error) = self.queue.enqueue(job).await {
            // WAL-01: a recharge without its timeout job must not stay pending forever.
            tracing::error!(payment_id = payment.id, recharge_no = %recharge.recharge_no, %error, "wallet_recharge_enqueue_timeout_expire_failed");
            if let Err(mark) = self.store.mark_failed(payment.id, self.clock.now()).await {
                tracing::warn!(payment_id = payment.id, error = %mark, "wallet_recharge_mark_failed_failed");
            }
            return Err(
                Error::internal_msg("queue unavailable").or_internal(keys::PAYMENT_CREATE_FAILED)
            );
        }

        let recharge = self
            .wallets
            .recharge_by_payment(payment.id, req.user_id)
            .await
            .ok()
            .flatten()
            .unwrap_or(recharge);
        let account = self.account(req.user_id).await?;
        Ok(RechargeView {
            recharge,
            payment: Some(payment),
            account,
        })
    }

    async fn account(&self, user_id: Id) -> Result<Account> {
        self.wallets
            .account(user_id)
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))
    }

    async fn payment(&self, payment_id: Id) -> Result<Payment> {
        self.lookup
            .payment(payment_id)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_CALLBACK_FAILED))?
            .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))
    }

    /// A user's recharge with its payment (`GET /wallet/recharges/:recharge_no`).
    pub async fn detail(&self, user_id: Id, recharge_no: &str) -> Result<RechargeView> {
        let recharge_no = recharge_no.trim();
        if recharge_no.is_empty() {
            return Err(Error::invalid());
        }
        let recharge = self
            .wallets
            .recharge_by_no(user_id, recharge_no)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
        let payment = self.payment(recharge.payment_id).await?;
        let account = self.account(user_id).await?;
        Ok(RechargeView {
            recharge,
            payment: Some(payment),
            account,
        })
    }

    /// "Check payment status" (WAL-03): actively queries the gateway when it can, and
    /// otherwise returns the stored payment. The payment must belong to the user.
    pub async fn capture(&self, user_id: Id, payment_id: Id) -> Result<RechargeView> {
        let recharge = self
            .wallets
            .recharge_by_payment(payment_id, user_id)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
        let payment = self.payment(payment_id).await?;
        let payment = if payment.status.is_open() {
            self.query_and_settle(payment).await?
        } else {
            payment
        };
        let recharge = self
            .wallets
            .recharge_by_no(user_id, &recharge.recharge_no)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?
            .unwrap_or(recharge);
        let account = self.account(user_id).await?;
        Ok(RechargeView {
            recharge,
            payment: Some(payment),
            account,
        })
    }

    async fn query_and_settle(&self, payment: Payment) -> Result<Payment> {
        let Some(channel) = self
            .lookup
            .channel(payment.channel_id)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_CALLBACK_FAILED))?
        else {
            return Ok(payment);
        };
        match self.gateway.query(&channel, &payment).await {
            Ok(Some(input)) => Ok(self.settle(&input).await?.payment),
            Ok(None) => Ok(payment),
            Err(e) if e.key() == pay_keys::PROVIDER_NOT_SUPPORTED => Ok(payment),
            Err(e) => Err(e.or_internal(keys::PAYMENT_CALLBACK_FAILED)),
        }
    }

    /// Recharge completion: applies a verified gateway result (WAL-01 matrix, one credit
    /// on the first success) and then runs the member-level / notification hooks once.
    pub async fn settle(&self, input: &CallbackInput) -> Result<RechargeSettled> {
        let settled = self.store.settle(input, self.clock.now()).await?;
        if settled.newly_succeeded
            && let Some(recharge) = &settled.recharge
        {
            self.hooks.on_succeeded(recharge, &settled.payment).await;
        }
        Ok(settled)
    }

    /// `wallet_recharge:timeout_expire` job: missing payments are skipped (no retry).
    pub async fn expire(&self, payment_id: Id) -> Result<()> {
        if payment_id <= 0 {
            return Ok(());
        }
        match self.store.expire(payment_id, self.clock.now()).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                tracing::debug!(payment_id, error = %e, "wallet_recharge_expire_skip");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    /// Current status of a payment (used by other groups' status pages).
    pub fn is_terminal(status: PaymentStatus) -> bool {
        !status.is_open()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, TimeZone, Utc};
    use std::collections::{BTreeMap, HashMap};
    use std::sync::Mutex;
    use zs_domain::payment::channel::PaymentChannel;
    use zs_domain::payment::eligibility::Payer;
    use zs_domain::payment::types::FeePolicy;
    use zs_domain::wallet::ports::{BalanceChangeRequest, GiftCardRedemption, LedgerError};
    use zs_domain::wallet::{RechargeFilter, Transaction, TransactionFilter};
    use zs_shared::clock::FixedClock;
    use zs_shared::page::{Page, PageRequest};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()
    }

    fn channel(id: Id) -> PaymentChannel {
        PaymentChannel {
            id,
            name: "c".into(),
            icon: String::new(),
            provider_type: "epay".into(),
            channel_type: "alipay".into(),
            interaction_mode: "redirect".into(),
            fee_rate: Amount::from(1),
            fixed_fee: Amount::ZERO,
            min_amount: Amount::ZERO,
            max_amount: Amount::ZERO,
            hide_amount_out_range: false,
            payment_roles: vec![],
            member_levels: vec![],
            payment_types: vec!["wallet".into()],
            config_json: Default::default(),
            is_active: true,
            sort_order: 0,
            created_at: now(),
            updated_at: now(),
        }
    }

    fn payment(id: Id, draft: &RechargeDraft) -> Payment {
        Payment {
            id,
            order_id: 0,
            channel_id: draft.channel.id,
            provider_type: draft.channel.provider_type.clone(),
            channel_type: draft.channel.channel_type.clone(),
            interaction_mode: draft.channel.interaction_mode.clone(),
            amount: draft.payable_amount,
            fee_rate: draft.fee_rate,
            fixed_fee: draft.fixed_fee,
            fee_amount: draft.fee_amount,
            fee_policy: draft.fee_policy.as_str().into(),
            currency: draft.currency.clone(),
            status: PaymentStatus::Initiated,
            exception_code: String::new(),
            provider_ref: String::new(),
            gateway_order_no: String::new(),
            provider_payload: Default::default(),
            pay_url: String::new(),
            qr_code: String::new(),
            created_at: now(),
            updated_at: now(),
            paid_at: None,
            expired_at: None,
            superseded_at: None,
            superseded_by_payment_id: None,
            callback_at: None,
        }
    }

    fn recharge(draft: &RechargeDraft) -> RechargeOrder {
        RechargeOrder {
            id: 1,
            recharge_no: draft.recharge_no.clone(),
            user_id: draft.user_id,
            payment_id: 7,
            channel_id: draft.channel.id,
            provider_type: draft.channel.provider_type.clone(),
            channel_type: draft.channel.channel_type.clone(),
            interaction_mode: draft.channel.interaction_mode.clone(),
            amount: draft.amount,
            payable_amount: draft.payable_amount,
            fee_rate: draft.fee_rate,
            fee_amount: draft.fee_amount,
            currency: draft.currency.clone(),
            status: "pending".into(),
            remark: draft.remark.clone(),
            paid_at: None,
            created_at: now(),
            updated_at: now(),
        }
    }

    #[derive(Default)]
    struct World {
        drafts: Mutex<Vec<RechargeDraft>>,
        failed: Mutex<Vec<Id>>,
        jobs: Mutex<Vec<NewJob>>,
        queue_down: bool,
        gateway_down: bool,
        settings: HashMap<String, serde_json::Value>,
    }

    #[async_trait]
    impl WalletRepo for World {
        async fn account(&self, user_id: Id) -> Result<Account> {
            Ok(Account {
                id: 1,
                user_id,
                balance: Amount::ZERO,
                created_at: now(),
                updated_at: now(),
            })
        }
        async fn balances(&self, _: &[Id]) -> Result<HashMap<Id, Amount>> {
            Ok(HashMap::new())
        }
        async fn list_transactions(
            &self,
            _: &TransactionFilter,
            _: PageRequest,
        ) -> Result<Page<Transaction>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
        async fn change_balance(
            &self,
            _: &BalanceChangeRequest,
        ) -> std::result::Result<(Account, Transaction), LedgerError> {
            Err(LedgerError::InvalidAmount)
        }
        async fn list_recharges(
            &self,
            _: &RechargeFilter,
            _: PageRequest,
        ) -> Result<Page<RechargeOrder>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
        async fn recharge_stats(&self, _: Id, _: &str) -> Result<BTreeMap<String, i64>> {
            Ok(BTreeMap::new())
        }
        async fn recharge_by_no(&self, _: Id, _: &str) -> Result<Option<RechargeOrder>> {
            Ok(None)
        }
        async fn recharge_by_payment(&self, _: Id, _: Id) -> Result<Option<RechargeOrder>> {
            Ok(None)
        }
        async fn redeem_gift_card(
            &self,
            _: Id,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<GiftCardRedemption> {
            Err(Error::invalid())
        }
    }

    #[async_trait]
    impl RechargeStore for World {
        async fn create(&self, draft: &RechargeDraft) -> Result<(RechargeOrder, Payment)> {
            self.drafts.lock().unwrap().push(draft.clone());
            Ok((recharge(draft), payment(7, draft)))
        }
        async fn save_started(&self, _: &Payment) -> Result<()> {
            Ok(())
        }
        async fn mark_failed(&self, payment_id: Id, _: DateTime<Utc>) -> Result<()> {
            self.failed.lock().unwrap().push(payment_id);
            Ok(())
        }
        async fn expire(&self, _: Id, _: DateTime<Utc>) -> Result<Option<Payment>> {
            Ok(None)
        }
        async fn settle(&self, _: &CallbackInput, _: DateTime<Utc>) -> Result<RechargeSettled> {
            Err(Error::invalid())
        }
    }

    #[async_trait]
    impl RechargeLookup for World {
        async fn payment(&self, _: Id) -> Result<Option<Payment>> {
            Ok(None)
        }
        async fn channel(&self, id: Id) -> Result<Option<PaymentChannel>> {
            Ok(match id {
                1 => Some(channel(1)),
                2 => Some(PaymentChannel {
                    is_active: false,
                    ..channel(2)
                }),
                3 => Some(PaymentChannel {
                    payment_types: vec!["order".into()],
                    ..channel(3)
                }),
                _ => None,
            })
        }
        async fn active_channels(&self) -> Result<Vec<PaymentChannel>> {
            Ok(vec![
                channel(1),
                PaymentChannel {
                    payment_types: vec!["order".into()],
                    ..channel(3)
                },
            ])
        }
        async fn payer(&self, user_id: Id) -> Result<Option<Payer>> {
            Ok(Some(Payer {
                user_id,
                member_level_id: 0,
            }))
        }
    }

    #[async_trait]
    impl RechargeGateway for World {
        async fn start(
            &self,
            _: &PaymentChannel,
            p: &Payment,
            _: &RechargeOrder,
            _: &str,
        ) -> Result<Payment> {
            if self.gateway_down {
                return Err(Error::bad_request(pay_keys::GATEWAY_REQUEST_FAILED));
            }
            Ok(Payment {
                status: PaymentStatus::Pending,
                pay_url: "https://pay".into(),
                ..p.clone()
            })
        }
        async fn query(&self, _: &PaymentChannel, _: &Payment) -> Result<Option<CallbackInput>> {
            Err(Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED))
        }
    }

    #[async_trait]
    impl RechargeHooks for World {
        async fn on_succeeded(&self, _: &RechargeOrder, _: &Payment) {}
    }

    #[async_trait]
    impl JobQueue for World {
        async fn enqueue(&self, job: NewJob) -> Result<()> {
            if self.queue_down {
                return Err(Error::internal_msg("down"));
            }
            self.jobs.lock().unwrap().push(job);
            Ok(())
        }
    }

    #[async_trait]
    impl SettingsStore for World {
        async fn get(&self, key: &str) -> Result<Option<serde_json::Value>> {
            Ok(self.settings.get(key).cloned())
        }
        async fn set(&self, _: &str, _: &serde_json::Value) -> Result<()> {
            Ok(())
        }
    }

    fn service(world: Arc<World>) -> RechargeService {
        RechargeService::new(RechargeDeps {
            wallets: world.clone(),
            store: world.clone(),
            lookup: world.clone(),
            gateway: world.clone(),
            hooks: world.clone(),
            queue: world.clone(),
            settings: world,
            clock: Arc::new(FixedClock(now())),
            order_fallback: OrderSetting::default(),
        })
    }

    fn req(channel_id: Id, amount: &str) -> RechargeRequest {
        RechargeRequest {
            user_id: 5,
            channel_id,
            amount: amount.into(),
            ..RechargeRequest::default()
        }
    }

    #[tokio::test]
    async fn create_snapshots_fees_and_schedules_expiry() {
        let mut world = World::default();
        world.settings.insert(
            "payment_config".into(),
            serde_json::json!({"customer_fee_enabled": true}),
        );
        world.settings.insert(
            "order_config".into(),
            serde_json::json!({"payment_expire_minutes": 30}),
        );
        let world = Arc::new(world);
        let view = service(world.clone()).create(&req(1, "100")).await.unwrap();
        let draft = world.drafts.lock().unwrap()[0].clone();
        assert!(draft.recharge_no.starts_with("WR"));
        assert_eq!(draft.payable_amount, Amount::from(101));
        assert_eq!(draft.fee_amount, Amount::from(1));
        assert_eq!(draft.fee_policy, FeePolicy::CustomerSurcharge);
        assert_eq!(draft.currency, "CNY");
        assert_eq!(draft.remark, "余额充值");
        assert_eq!(view.payment.unwrap().pay_url, "https://pay");
        let jobs = world.jobs.lock().unwrap();
        assert_eq!(jobs[0].kind, "wallet_recharge:timeout_expire");
        assert_eq!(jobs[0].payload["payment_id"], 7);
        assert_eq!(jobs[0].run_at, Some(now() + Duration::minutes(30)));
    }

    #[tokio::test]
    async fn create_validations() {
        let svc = service(Arc::new(World::default()));
        let key = |r: Result<RechargeView>| r.unwrap_err().key().to_owned();
        assert_eq!(key(svc.create(&req(1, "abc")).await), "error.bad_request");
        assert_eq!(key(svc.create(&req(1, "0")).await), "error.bad_request");
        assert_eq!(key(svc.create(&req(0, "1")).await), "error.payment_invalid");
        assert_eq!(
            key(svc.create(&req(9, "1")).await),
            "error.payment_channel_not_found"
        );
        assert_eq!(
            key(svc.create(&req(2, "1")).await),
            "error.payment_channel_inactive"
        );
        assert_eq!(
            key(svc.create(&req(3, "1")).await),
            "error.payment_channel_not_allowed_for_recharge"
        );
        let mut world = World::default();
        world.settings.insert(
            "wallet_config".into(),
            serde_json::json!({"recharge_channel_ids": [4]}),
        );
        let svc = service(Arc::new(world));
        assert_eq!(
            key(svc.create(&req(1, "1")).await),
            "error.payment_channel_not_allowed_for_recharge"
        );
        let mut r = req(1, "1");
        r.currency = "usdx".into();
        assert_eq!(
            key(service(Arc::new(World::default())).create(&r).await),
            "error.payment_currency_mismatch"
        );
    }

    /// WAL-01: queue unavailable → creation fails and the recharge is marked failed.
    #[tokio::test]
    async fn wal_01_queue_unavailable_marks_failed() {
        let world = Arc::new(World {
            queue_down: true,
            ..World::default()
        });
        let err = service(world.clone())
            .create(&req(1, "10"))
            .await
            .unwrap_err();
        assert_eq!(err.key(), "error.payment_create_failed");
        assert_eq!(*world.failed.lock().unwrap(), vec![7]);

        let world = Arc::new(World {
            gateway_down: true,
            ..World::default()
        });
        let err = service(world.clone())
            .create(&req(1, "10"))
            .await
            .unwrap_err();
        assert_eq!(err.key(), "error.payment_gateway_request_failed");
        assert_eq!(*world.failed.lock().unwrap(), vec![7]);
        assert!(world.jobs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn channels_are_filtered_by_payment_type() {
        let svc = service(Arc::new(World::default()));
        let list = svc.channels(5, "10").await.unwrap();
        assert_eq!(list.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1]);
        assert!(list[0].fee_rate.is_none());
        assert_eq!(
            svc.channels(5, "-1").await.unwrap_err().key(),
            "error.bad_request"
        );
    }
}
