//! Recharge persistence ([`RechargeStore`], [`RechargeLookup`]), the gateway adapter, the
//! post-success hooks and the recharge-aware [`PaymentSettlement`].

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection, DatabaseTransaction,
    EntityTrait, QueryFilter, QuerySelect, Set, TransactionTrait,
};
use serde_json::Value;
use zs_app::marketing::member_level::MemberLevelService;
use zs_app::wallet::RechargeService;
use zs_domain::notify::center::{DispatchPayload, biz_types, events};
use zs_domain::notify::channel::{BotNotifyPayload, bot_events};
use zs_domain::payment::callback::{
    CallbackInput, CallbackOutcome, apply_callback, merge_provider_payload, validate_callback_facts,
};
use zs_domain::payment::channel::{ChannelFilter, ChannelRepo, PaymentChannel};
use zs_domain::payment::eligibility::Payer;
use zs_domain::payment::errors::keys as pay_keys;
use zs_domain::payment::gateway::{GatewayCreateInput, GatewayRegistry};
use zs_domain::payment::model::Payment;
use zs_domain::payment::returns::{
    ReturnContext, build_return_query, resolve_gateway_order_no, return_marker,
};
use zs_domain::payment::settlement::PaymentSettlement;
use zs_domain::payment::types::{InteractionMode, PaymentStatus};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::wallet::ports::{BalanceChangeRequest, RechargeDraft};
use zs_domain::wallet::rules::{
    RECHARGE_CREDIT_REMARK, can_apply_recharge_callback, can_expire_recharge, clean_remark,
    recharge_reference, recharge_status_after,
};
use zs_domain::wallet::{
    RechargeGateway, RechargeHooks, RechargeLookup, RechargeOrder, RechargeSettled, RechargeStatus,
    RechargeStore, keys, txn_type,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use super::ledger;
use super::repo::recharge_to_domain;
use crate::db::entity::{payments, user_oauth_identities, users, wallet_recharge_orders};
use crate::db::repo::payment::channel::SeaChannelRepo;
use crate::db::repo::payment::records::to_domain as payment_to_domain;
use crate::db::repo::payment::settlement::save_callback_columns;
use crate::db::repo::support::{DbResultExt, to_json};

/// OAuth provider name of Telegram identities.
const TELEGRAM_PROVIDER: &str = "telegram";

/// SeaORM implementation of [`RechargeStore`] and [`RechargeLookup`].
#[derive(Debug, Clone)]
pub struct SeaRechargeStore {
    db: DatabaseConnection,
    channels: SeaChannelRepo,
}

impl SeaRechargeStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self {
            channels: SeaChannelRepo::new(db.clone()),
            db,
        }
    }
}

async fn lock_payment<C: ConnectionTrait>(conn: &C, id: Id) -> Result<Option<Payment>> {
    Ok(payments::Entity::find_by_id(id)
        .filter(payments::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()?
        .map(payment_to_domain))
}

async fn lock_recharge<C: ConnectionTrait>(
    conn: &C,
    payment_id: Id,
) -> Result<Option<wallet_recharge_orders::Model>> {
    wallet_recharge_orders::Entity::find()
        .filter(wallet_recharge_orders::Column::PaymentId.eq(payment_id))
        .filter(wallet_recharge_orders::Column::DeletedAt.is_null())
        .lock_exclusive()
        .one(conn)
        .await
        .dom()
}

/// Moves a recharge from `from` to `to` with a conditional update (exactly one row).
async fn set_recharge_status<C: ConnectionTrait>(
    conn: &C,
    id: Id,
    from: &str,
    to: RechargeStatus,
    paid_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<()> {
    let mut q = wallet_recharge_orders::Entity::update_many()
        .col_expr(
            wallet_recharge_orders::Column::Status,
            Expr::value(to.as_str()),
        )
        .col_expr(wallet_recharge_orders::Column::UpdatedAt, Expr::value(now));
    if paid_at.is_some() {
        q = q.col_expr(wallet_recharge_orders::Column::PaidAt, Expr::value(paid_at));
    }
    let affected = q
        .filter(wallet_recharge_orders::Column::Id.eq(id))
        .filter(wallet_recharge_orders::Column::Status.eq(from))
        .exec(conn)
        .await
        .dom()?
        .rows_affected;
    if affected != 1 {
        return Err(Error::internal_msg("recharge changed concurrently"));
    }
    Ok(())
}

/// Updates only the callback metadata of a payment whose status must not change
/// (`updateCallbackMetaWithRepo` with the current status).
fn touch_callback_meta(payment: &mut Payment, input: &CallbackInput, now: DateTime<Utc>) {
    if !input.provider_ref.is_empty() && payment.provider_ref.is_empty() {
        payment.provider_ref.clone_from(&input.provider_ref);
    }
    payment.provider_payload = merge_provider_payload(&payment.provider_payload, &input.payload);
    payment.callback_at = Some(now);
    payment.updated_at = now;
}

/// Applies a verified recharge callback inside `txn` (WAL-01): locks payment and recharge,
/// re-validates the facts under the lock, applies the terminal-state matrix and credits
/// the wallet only on the recharge's transition to success.
pub async fn settle_recharge_in(
    txn: &DatabaseTransaction,
    input: &CallbackInput,
    now: DateTime<Utc>,
) -> Result<RechargeSettled> {
    let mut payment = lock_payment(txn, input.payment_id)
        .await?
        .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
    if payment.order_id != 0 {
        return Err(Error::bad_request(keys::PAYMENT_INVALID));
    }
    let recharge = lock_recharge(txn, payment.id)
        .await?
        .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
    validate_callback_facts(&payment, &recharge.recharge_no, input)?;
    let recharge_status =
        RechargeStatus::parse(&recharge.status).unwrap_or(RechargeStatus::Pending);
    let previous = payment.status;

    if previous == PaymentStatus::Success || previous == input.status {
        // Idempotent: metadata only (duplicate success never credits twice).
        apply_callback(&mut payment, input, now);
        save_callback_columns(txn, &payment, None).await?;
        return Ok(RechargeSettled {
            payment,
            recharge: Some(recharge_to_domain(recharge)),
            newly_succeeded: false,
        });
    }
    if !can_apply_recharge_callback(previous, recharge_status, input.status) {
        tracing::info!(
            payment_id = payment.id,
            payment_status = %previous,
            recharge_status = recharge_status.as_str(),
            target = %input.status,
            "wallet_recharge_callback_ignored_terminal_transition"
        );
        touch_callback_meta(&mut payment, input, now);
        save_callback_columns(txn, &payment, None).await?;
        return Ok(RechargeSettled {
            payment,
            recharge: Some(recharge_to_domain(recharge)),
            newly_succeeded: false,
        });
    }

    let outcome = apply_callback(&mut payment, input, now);
    if matches!(outcome, CallbackOutcome::Transitioned { .. })
        && save_callback_columns(txn, &payment, Some(previous)).await? != 1
    {
        return Err(Error::internal_msg("payment row changed concurrently"));
    }
    let mut newly_succeeded = false;
    if recharge_status != RechargeStatus::Success {
        let target = recharge_status_after(input.status);
        let mut paid_at = None;
        if target == RechargeStatus::Success {
            let change = BalanceChangeRequest {
                user_id: recharge.user_id,
                delta: Amount::new(recharge.amount),
                kind: txn_type::RECHARGE.to_owned(),
                reference: recharge_reference(recharge.id),
                remark: clean_remark(&recharge.remark, RECHARGE_CREDIT_REMARK),
                currency: recharge.currency.clone(),
                operator_admin_id: None,
                order_id: None,
            };
            ledger::credit(txn, &change, now).await?;
            paid_at = Some(payment.paid_at.unwrap_or(now));
            newly_succeeded = true;
        }
        if target != recharge_status {
            set_recharge_status(txn, recharge.id, &recharge.status, target, paid_at, now).await?;
        }
    }
    let recharge = wallet_recharge_orders::Entity::find_by_id(recharge.id)
        .one(txn)
        .await
        .dom()?
        .map(recharge_to_domain);
    Ok(RechargeSettled {
        payment,
        recharge,
        newly_succeeded,
    })
}

#[async_trait]
impl RechargeStore for SeaRechargeStore {
    async fn create(&self, d: &RechargeDraft) -> Result<(RechargeOrder, Payment)> {
        let txn = self.db.begin().await.dom()?;
        let payment = payments::ActiveModel {
            order_id: Set(0),
            channel_id: Set(d.channel.id),
            provider_type: Set(d.channel.provider_type.clone()),
            channel_type: Set(d.channel.channel_type.clone()),
            interaction_mode: Set(d.channel.interaction_mode.clone()),
            amount: Set(d.payable_amount.decimal()),
            fee_rate: Set(d.fee_rate.decimal()),
            fixed_fee: Set(d.fixed_fee.decimal()),
            fee_amount: Set(d.fee_amount.decimal()),
            fee_policy: Set(d.fee_policy.as_str().to_owned()),
            currency: Set(d.currency.clone()),
            status: Set(PaymentStatus::Initiated.as_str().to_owned()),
            exception_code: Set(String::new()),
            provider_ref: Set(String::new()),
            gateway_order_no: Set(String::new()),
            provider_payload: Set(None),
            pay_url: Set(String::new()),
            qr_code: Set(String::new()),
            created_at: Set(d.now),
            updated_at: Set(d.now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        let recharge = wallet_recharge_orders::ActiveModel {
            recharge_no: Set(d.recharge_no.clone()),
            user_id: Set(d.user_id),
            payment_id: Set(payment.id),
            channel_id: Set(d.channel.id),
            provider_type: Set(d.channel.provider_type.clone()),
            channel_type: Set(d.channel.channel_type.clone()),
            interaction_mode: Set(d.channel.interaction_mode.clone()),
            amount: Set(d.amount.decimal()),
            payable_amount: Set(d.payable_amount.decimal()),
            fee_rate: Set(d.fee_rate.decimal()),
            fee_amount: Set(d.fee_amount.decimal()),
            currency: Set(d.currency.clone()),
            status: Set(RechargeStatus::Pending.as_str().to_owned()),
            remark: Set(d.remark.clone()),
            paid_at: Set(None),
            created_at: Set(d.now),
            updated_at: Set(d.now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        txn.commit().await.dom()?;
        Ok((recharge_to_domain(recharge), payment_to_domain(payment)))
    }

    async fn save_started(&self, p: &Payment) -> Result<()> {
        payments::Entity::update_many()
            .col_expr(payments::Column::Status, Expr::value(p.status.as_str()))
            .col_expr(payments::Column::Amount, Expr::value(p.amount.decimal()))
            .col_expr(payments::Column::Currency, Expr::value(p.currency.clone()))
            .col_expr(
                payments::Column::ProviderRef,
                Expr::value(p.provider_ref.clone()),
            )
            .col_expr(
                payments::Column::GatewayOrderNo,
                Expr::value(p.gateway_order_no.clone()),
            )
            .col_expr(
                payments::Column::ProviderPayload,
                Expr::value(to_json(&p.provider_payload)?),
            )
            .col_expr(payments::Column::PayUrl, Expr::value(p.pay_url.clone()))
            .col_expr(payments::Column::QrCode, Expr::value(p.qr_code.clone()))
            .col_expr(payments::Column::UpdatedAt, Expr::value(p.updated_at))
            .filter(payments::Column::Id.eq(p.id))
            .filter(payments::Column::Status.eq(PaymentStatus::Initiated.as_str()))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn mark_failed(&self, payment_id: Id, now: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        let Some(payment) = lock_payment(&txn, payment_id).await? else {
            return Ok(());
        };
        if payment.status.is_open() {
            payments::Entity::update_many()
                .col_expr(
                    payments::Column::Status,
                    Expr::value(PaymentStatus::Failed.as_str()),
                )
                .col_expr(payments::Column::UpdatedAt, Expr::value(now))
                .filter(payments::Column::Id.eq(payment_id))
                .filter(payments::Column::Status.eq(payment.status.as_str()))
                .exec(&txn)
                .await
                .dom()?;
        }
        if let Some(recharge) = lock_recharge(&txn, payment_id).await?
            && recharge.status == RechargeStatus::Pending.as_str()
        {
            set_recharge_status(
                &txn,
                recharge.id,
                &recharge.status,
                RechargeStatus::Failed,
                None,
                now,
            )
            .await?;
        }
        txn.commit().await.dom()?;
        Ok(())
    }

    async fn expire(&self, payment_id: Id, now: DateTime<Utc>) -> Result<Option<Payment>> {
        let txn = self.db.begin().await.dom()?;
        let Some(mut payment) = lock_payment(&txn, payment_id).await? else {
            return Err(Error::not_found(keys::PAYMENT_NOT_FOUND));
        };
        if payment.order_id != 0 {
            // Order payments are the order group's business (WAL-01).
            return Ok(Some(payment));
        }
        let recharge = lock_recharge(&txn, payment_id)
            .await?
            .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
        let recharge_status =
            RechargeStatus::parse(&recharge.status).unwrap_or(RechargeStatus::Pending);
        if !can_expire_recharge(payment.status, recharge_status) {
            return Ok(Some(payment));
        }
        let previous = payment.status;
        payment.status = PaymentStatus::Expired;
        payment.expired_at = Some(now);
        payment.updated_at = now;
        if save_callback_columns(&txn, &payment, Some(previous)).await? != 1 {
            return Err(Error::internal_msg("payment row changed concurrently"));
        }
        set_recharge_status(
            &txn,
            recharge.id,
            &recharge.status,
            RechargeStatus::Expired,
            None,
            now,
        )
        .await?;
        txn.commit().await.dom()?;
        Ok(Some(payment))
    }

    async fn settle(&self, input: &CallbackInput, now: DateTime<Utc>) -> Result<RechargeSettled> {
        let txn = self.db.begin().await.dom()?;
        let settled = settle_recharge_in(&txn, input, now).await?;
        txn.commit().await.dom()?;
        Ok(settled)
    }
}

#[async_trait]
impl RechargeLookup for SeaRechargeStore {
    async fn payment(&self, id: Id) -> Result<Option<Payment>> {
        if id <= 0 {
            return Ok(None);
        }
        Ok(payments::Entity::find_by_id(id)
            .filter(payments::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(payment_to_domain))
    }

    async fn channel(&self, id: Id) -> Result<Option<PaymentChannel>> {
        if id <= 0 {
            return Ok(None);
        }
        self.channels.get(id).await
    }

    async fn active_channels(&self) -> Result<Vec<PaymentChannel>> {
        let filter = ChannelFilter {
            active_only: true,
            ..ChannelFilter::default()
        };
        Ok(self.channels.list(&filter).await?.0)
    }

    async fn payer(&self, user_id: Id) -> Result<Option<Payer>> {
        if user_id <= 0 {
            return Ok(None);
        }
        let row: Option<(Id, Id)> = users::Entity::find_by_id(user_id)
            .select_only()
            .columns([users::Column::Id, users::Column::MemberLevelId])
            .filter(users::Column::DeletedAt.is_null())
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(|(user_id, member_level_id)| Payer {
            user_id,
            member_level_id,
        }))
    }
}

/// [`RechargeGateway`] over the payment group's gateway registry.
///
/// TODO(payment-group): once the payment group exposes a create/capture payment service
/// (`applyProviderPayment` with tenant return URLs), delegate to it instead of calling the
/// registry directly.
#[derive(Clone)]
pub struct RegistryRechargeGateway {
    registry: Arc<GatewayRegistry>,
    clock: Arc<dyn zs_shared::clock::Clock>,
}

impl std::fmt::Debug for RegistryRechargeGateway {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RegistryRechargeGateway")
    }
}

impl RegistryRechargeGateway {
    pub fn new(registry: Arc<GatewayRegistry>, clock: Arc<dyn zs_shared::clock::Clock>) -> Self {
        Self { registry, clock }
    }
}

#[async_trait]
impl RechargeGateway for RegistryRechargeGateway {
    async fn start(
        &self,
        channel: &PaymentChannel,
        payment: &Payment,
        recharge: &RechargeOrder,
        client_ip: &str,
    ) -> Result<Payment> {
        let gateway = self
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .ok_or_else(|| Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED))?;
        let now = self.clock.now();
        let gateway_order_no = resolve_gateway_order_no(&payment.gateway_order_no, now);
        let ctx = ReturnContext {
            biz_type: "recharge".to_owned(),
            business_no: recharge.recharge_no.clone(),
            guest: false,
        };
        let input = GatewayCreateInput {
            payment_id: payment.id,
            order_id: 0,
            order_no: gateway_order_no.clone(),
            subject: recharge.recharge_no.clone(),
            amount: payment.amount,
            currency: payment.currency.clone(),
            client_ip: client_ip.trim().to_owned(),
            channel_type: channel.channel_type.clone(),
            interaction_mode: InteractionMode::parse(&channel.interaction_mode),
            order_user_key: recharge.user_id.to_string(),
            return_url_query: build_return_query(
                &ctx,
                &return_marker(&channel.provider_type, &channel.channel_type),
                "",
            ),
            ..GatewayCreateInput::default()
        };
        let result = gateway.create_payment(&channel.config_json, &input).await?;
        let mut started = payment.clone();
        started.gateway_order_no = gateway_order_no.clone();
        zs_domain::payment::callback::apply_create_result(
            &mut started,
            &result,
            &gateway_order_no,
            now,
        );
        Ok(started)
    }

    async fn query(
        &self,
        channel: &PaymentChannel,
        payment: &Payment,
    ) -> Result<Option<CallbackInput>> {
        let gateway = self
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .ok_or_else(|| Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED))?;
        if !gateway.capabilities().query {
            return Err(Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED));
        }
        let reference = if payment.provider_ref.is_empty() {
            payment.gateway_order_no.clone()
        } else {
            payment.provider_ref.clone()
        };
        let result = gateway
            .query_payment(&channel.config_json, &reference)
            .await?;
        Ok(result.status.map(|status| CallbackInput {
            payment_id: payment.id,
            order_no: String::new(),
            channel_id: channel.id,
            status,
            provider_ref: result.provider_ref,
            amount: result.amount,
            currency: result.currency,
            paid_at: result.paid_at,
            payload: result.payload,
            verified_legacy_currency: String::new(),
        }))
    }
}

/// Post-success side effects: member level upgrade, notification center event and the
/// Telegram bot notification (only for users with a Telegram identity).
#[derive(Clone)]
pub struct QueuedRechargeHooks {
    db: DatabaseConnection,
    queue: Arc<dyn JobQueue>,
    member_levels: MemberLevelService,
}

impl std::fmt::Debug for QueuedRechargeHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueuedRechargeHooks")
    }
}

impl QueuedRechargeHooks {
    pub fn new(
        db: DatabaseConnection,
        queue: Arc<dyn JobQueue>,
        member_levels: MemberLevelService,
    ) -> Self {
        Self {
            db,
            queue,
            member_levels,
        }
    }

    async fn notification(
        &self,
        recharge: &RechargeOrder,
        payment: &Payment,
    ) -> Result<DispatchPayload> {
        let user: Option<(String, String)> = users::Entity::find_by_id(recharge.user_id)
            .select_only()
            .columns([users::Column::Email, users::Column::DisplayName])
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        let (email, name) = user.unwrap_or_default();
        let label = if name.trim().is_empty() {
            email.clone()
        } else {
            name
        };
        let provider = recharge.provider_type.trim();
        let channel = recharge.channel_type.trim();
        let payment_channel = match (provider.is_empty(), channel.is_empty()) {
            (false, false) => format!("{provider}/{channel}"),
            (true, _) => channel.to_owned(),
            (false, true) => provider.to_owned(),
        };
        let mut data = serde_json::Map::new();
        let mut set = |k: &str, v: String| {
            data.insert(k.to_owned(), Value::String(v));
        };
        set("user_id", recharge.user_id.to_string());
        set("recharge_id", recharge.id.to_string());
        set("recharge_no", recharge.recharge_no.trim().to_owned());
        set("amount", recharge.amount.to_string());
        set("currency", recharge.currency.trim().to_ascii_uppercase());
        set("provider_type", provider.to_owned());
        set("channel_type", channel.to_owned());
        set("payment_channel", payment_channel);
        set("customer_email", email);
        set("customer_label", label);
        set("payment_id", payment.id.to_string());
        Ok(DispatchPayload {
            event_type: events::WALLET_RECHARGE_SUCCESS.to_owned(),
            biz_type: biz_types::WALLET_RECHARGE.to_owned(),
            biz_id: recharge.id,
            locale: String::new(),
            force: false,
            data,
        })
    }

    async fn telegram_user_id(&self, user_id: Id) -> Result<Option<String>> {
        let row: Option<String> = user_oauth_identities::Entity::find()
            .select_only()
            .column(user_oauth_identities::Column::ProviderUserId)
            .filter(user_oauth_identities::Column::UserId.eq(user_id))
            .filter(user_oauth_identities::Column::Provider.eq(TELEGRAM_PROVIDER))
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        Ok(row.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()))
    }
}

#[async_trait]
impl RechargeHooks for QueuedRechargeHooks {
    async fn on_succeeded(&self, recharge: &RechargeOrder, payment: &Payment) {
        if let Err(error) = self
            .member_levels
            .on_recharge_completed(recharge.user_id, recharge.amount)
            .await
        {
            tracing::warn!(payment_id = payment.id, user_id = recharge.user_id, %error, "member_level_recharge_completed_failed");
        }
        let job = self
            .notification(recharge, payment)
            .await
            .and_then(|p| NewJob::new(kinds::NOTIFICATION_DISPATCH, p));
        match job {
            Ok(job) => {
                if let Err(error) = self.queue.enqueue(job).await {
                    tracing::warn!(recharge_id = recharge.id, %error, "notification_enqueue_wallet_recharge_failed");
                }
            }
            Err(error) => {
                tracing::warn!(recharge_id = recharge.id, %error, "notification_build_wallet_recharge_failed");
            }
        }
        match self.telegram_user_id(recharge.user_id).await {
            Ok(Some(telegram_user_id)) => {
                let payload = BotNotifyPayload {
                    event_type: bot_events::WALLET_RECHARGE_SUCCEEDED.to_owned(),
                    order_id: 0,
                    telegram_user_id,
                    recharge_no: recharge.recharge_no.trim().to_owned(),
                    amount: recharge.amount.to_string(),
                    currency: recharge.currency.trim().to_ascii_uppercase(),
                };
                let enqueued = match NewJob::new(kinds::BOT_NOTIFY, payload) {
                    Ok(job) => self.queue.enqueue(job).await,
                    Err(e) => Err(e),
                };
                if let Err(error) = enqueued {
                    tracing::warn!(recharge_id = recharge.id, %error, "wallet_recharge_notify_bot_enqueue_failed");
                }
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(recharge_id = recharge.id, %error, "wallet_recharge_notify_bot_fetch_identity_failed");
            }
        }
    }
}

/// [`PaymentSettlement`] that settles wallet recharges (`order_id = 0`) through
/// [`RechargeService::settle`] and delegates every other payment to `inner`.
///
/// Whoever wires the payment callbacks (the order group's settlement) should wrap its
/// settlement with this so gateway notifications credit recharges.
#[derive(Clone)]
pub struct RechargeAwareSettlement {
    recharges: Arc<RechargeService>,
    inner: Arc<dyn PaymentSettlement>,
    db: DatabaseConnection,
}

impl std::fmt::Debug for RechargeAwareSettlement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RechargeAwareSettlement")
    }
}

impl RechargeAwareSettlement {
    pub fn new(
        recharges: Arc<RechargeService>,
        inner: Arc<dyn PaymentSettlement>,
        db: DatabaseConnection,
    ) -> Self {
        Self {
            recharges,
            inner,
            db,
        }
    }
}

#[async_trait]
impl PaymentSettlement for RechargeAwareSettlement {
    async fn settle(&self, input: CallbackInput) -> Result<Payment> {
        let order_id: Option<Id> = payments::Entity::find_by_id(input.payment_id)
            .select_only()
            .column(payments::Column::OrderId)
            .filter(payments::Column::DeletedAt.is_null())
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        match order_id {
            Some(0) => Ok(self.recharges.settle(&input).await?.payment),
            _ => self.inner.settle(input).await,
        }
    }
}
