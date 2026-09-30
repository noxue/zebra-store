//! Order payments: channel list, payment creation (wallet first, fee snapshot, gateway),
//! latest payment and active capture (`payment_service_create.go`, `…_capture.go`).

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Map;
use zs_domain::order::model::{Order, OrderStatus, keys};
use zs_domain::order::ports::{BeginOutcome, BeginPayment};
use zs_domain::payment::callback::{CallbackInput, apply_create_result};
use zs_domain::payment::channel::PaymentChannel;
use zs_domain::payment::eligibility::{
    AvailabilityFilter, AvailableChannel, Payer, available_channels, product_allows,
};
use zs_domain::payment::errors::keys as pay_keys;
use zs_domain::payment::gateway::{GatewayCreateInput, GatewayError};
use zs_domain::payment::model::Payment;
use zs_domain::payment::returns::{
    ReturnContext, TenantSite, build_return_query, return_marker, tenant_return_url,
};
use zs_domain::payment::types::{InteractionMode, PaymentStatus, payment_type, provider};
use zs_domain::payment::wallet_info::extract_crypto_wallet_info;
use zs_domain::reseller::tenant::ResellerTenant;
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use super::OrderService;
use super::query::Viewer;

/// Timeout of one outbound gateway call (PAY-13).
const GATEWAY_TIMEOUT: Duration = Duration::from_secs(15);

/// Result of a payment creation (`CreatePaymentResp`).
#[derive(Debug, Clone, PartialEq, Serialize, Default)]
pub struct PaymentView {
    pub order_paid: bool,
    pub wallet_paid_amount: Amount,
    pub online_pay_amount: Amount,
    pub payable_amount: Amount,
    pub currency: String,
    pub fee_amount: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fee_policy: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<Id>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub provider_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub channel_type: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub interaction_mode: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub pay_url: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub qr_code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub wallet_address: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub chain_amount: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub chain: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub channel_name: String,
}

impl PaymentView {
    fn new(
        payment: Option<&Payment>,
        channel: Option<&PaymentChannel>,
        order: &Order,
        paid: bool,
    ) -> Self {
        let mut view = Self {
            order_paid: paid,
            wallet_paid_amount: order.wallet_paid_amount,
            online_pay_amount: if paid {
                Amount::ZERO
            } else {
                order.online_paid_amount
            },
            ..Self::default()
        };
        if let Some(p) = payment.filter(|_| !paid) {
            let info = extract_crypto_wallet_info(
                &p.provider_type,
                &p.interaction_mode,
                &p.provider_payload,
            );
            view.payment_id = Some(p.id);
            view.channel_id = Some(p.channel_id);
            view.provider_type.clone_from(&p.provider_type);
            view.channel_type.clone_from(&p.channel_type);
            view.interaction_mode.clone_from(&p.interaction_mode);
            view.pay_url.clone_from(&p.pay_url);
            view.qr_code.clone_from(&p.qr_code);
            view.payable_amount = p.amount;
            view.currency.clone_from(&p.currency);
            view.fee_amount = p.fee_amount;
            view.fee_policy.clone_from(&p.fee_policy);
            view.expires_at = p.expired_at;
            view.wallet_address = info.address;
            view.chain_amount = info.chain_amount;
            view.chain = info.chain;
            view.token_id = info.token_id;
        }
        if let Some(c) = channel.filter(|_| !paid) {
            view.channel_name.clone_from(&c.name);
        }
        view
    }
}

/// Latest open payment of an order (`LatestPaymentResp`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LatestPaymentView {
    pub payment_id: Id,
    pub order_no: String,
    pub channel_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub channel_name: String,
    pub provider_type: String,
    pub channel_type: String,
    pub interaction_mode: String,
    pub pay_url: String,
    pub qr_code: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub wallet_address: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub chain_amount: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub chain: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub token_id: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub payable_amount: Amount,
    pub currency: String,
    pub fee_amount: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fee_policy: String,
}

/// A payment request (`POST /payments`, `/guest/payments`, create-and-pay).
#[derive(Debug, Clone)]
pub struct PayRequest {
    pub order_id: Id,
    pub channel_id: Id,
    pub channel_type: String,
    pub use_balance: bool,
    pub client_ip: String,
    pub user_agent: String,
    pub tenant: ResellerTenant,
    /// `http` / `https` of the request (tenant return URLs, PAY-06).
    pub scheme: String,
}

fn tenant_site(t: &ResellerTenant) -> TenantSite {
    TenantSite {
        is_main: !t.is_reseller(),
        unavailable: t.unavailable,
        host: t.host.clone(),
        primary_domain: t.primary_domain.clone(),
    }
}

impl OrderService {
    /// Channels usable for an order amount / product set (`POST /order/payment-channels`).
    /// Wallet-only mode lists nothing (PAY-23); the product whitelist applies (PAY-11).
    pub async fn order_channels(
        &self,
        payer: Option<Payer>,
        amount: Amount,
        product_ids: &[Id],
    ) -> Result<Vec<AvailableChannel>> {
        if !amount.is_positive() {
            return Ok(Vec::new());
        }
        if self.wallet_setting().await.wallet_only_payment {
            return Ok(Vec::new());
        }
        let fee = self.fee_setting().await;
        let channels = self
            .deps
            .repo
            .active_channels()
            .await
            .map_err(|e| e.or_internal(pay_keys::PAYMENT_FETCH_FAILED))?;
        let mut list = available_channels(
            &channels,
            &AvailabilityFilter {
                target_amount: Some(amount),
                payer,
                payment_type: payment_type::ORDER,
            },
            fee.customer_fee_enabled,
        );
        let allowed = self.allowed_channel_ids(product_ids).await?;
        list.retain(|c| product_allows(&allowed, c.id));
        Ok(list)
    }

    /// Payer seen by channel rules for a logged-in user.
    pub async fn payer_of(&self, user_id: Id) -> Result<Option<Payer>> {
        if user_id <= 0 {
            return Ok(None);
        }
        Ok(Some(Payer {
            user_id,
            member_level_id: self.deps.repo.member_level_of(user_id).await?,
        }))
    }

    /// Creates (or reuses) a payment for an order (`CreatePayment`).
    pub async fn pay(&self, req: &PayRequest) -> Result<PaymentView> {
        let wallet_only = self.wallet_setting().await.wallet_only_payment;
        let mut use_balance = req.use_balance;
        if wallet_only {
            use_balance = true;
            if req.channel_id != 0 {
                return Err(Error::bad_request(keys::WALLET_ONLY_PAYMENT_REQUIRED));
            }
        }
        let fee = self.fee_setting().await;
        let now = self.deps.clock.now();
        let outcome = self
            .deps
            .payments
            .begin(&BeginPayment {
                order_id: req.order_id,
                channel_id: req.channel_id,
                channel_type: req.channel_type.clone(),
                use_balance,
                wallet_only,
                customer_fee_enabled: fee.customer_fee_enabled,
                reuse_legacy_fee_payment: fee.reuse_legacy_order_fee_payment,
                reseller_confirm_days: self.deps.reseller_confirm_days,
                now,
            })
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_CREATE_FAILED))?;
        match outcome {
            BeginOutcome::Reused {
                payment,
                channel,
                order,
            } => {
                self.deps
                    .payments
                    .supersede_others(order.id, payment.id, now)
                    .await?;
                Ok(PaymentView::new(
                    Some(&payment),
                    Some(&channel),
                    &order,
                    false,
                ))
            }
            BeginOutcome::PaidByWallet { payment, order } => {
                self.after_order_paid(&order, Some(&payment)).await;
                Ok(PaymentView::new(None, None, &order, true))
            }
            BeginOutcome::Created {
                payment,
                channel,
                order,
            } => {
                let started = self
                    .start_gateway(req, &order, &channel, payment.clone())
                    .await;
                match started {
                    Ok(payment) => {
                        self.deps
                            .payments
                            .supersede_others(order.id, payment.id, self.deps.clock.now())
                            .await?;
                        Ok(PaymentView::new(
                            Some(&payment),
                            Some(&channel),
                            &order,
                            false,
                        ))
                    }
                    Err(e) => {
                        if let Err(error) = self
                            .deps
                            .payments
                            .fail_started(payment.id, order.id, self.deps.clock.now())
                            .await
                        {
                            tracing::error!(%error, payment_id = payment.id, "payment_create_provider_failed_with_rollback_error");
                        }
                        Err(e)
                    }
                }
            }
        }
    }

    /// Calls the gateway in a detached task so a client disconnect cannot abort it half
    /// way; the result is persisted inside the same task (PAY-13).
    async fn start_gateway(
        &self,
        req: &PayRequest,
        order: &Order,
        channel: &PaymentChannel,
        mut payment: Payment,
    ) -> Result<Payment> {
        let gateway = self
            .deps
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .ok_or_else(|| Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED))?;
        let email = if order.user_id > 0 {
            self.deps
                .repo
                .users(&[order.user_id])
                .await?
                .remove(&order.user_id)
                .map(|u| u.email)
                .unwrap_or_default()
        } else {
            order.guest_email.clone()
        };
        let order_user_key = if order.user_id > 0 {
            order.user_id.to_string()
        } else if !order.guest_email.trim().is_empty() {
            order.guest_email.trim().to_owned()
        } else {
            order.order_no.clone()
        };
        let provider_order_no = payment.gateway_order_no.clone();
        let input = GatewayCreateInput {
            payment_id: payment.id,
            order_id: order.id,
            order_no: provider_order_no.clone(),
            subject: order.order_no.clone(),
            amount: payment.amount,
            currency: payment.currency.clone(),
            email: email.trim().to_owned(),
            notify_url: String::new(),
            return_url: tenant_return_url(
                Some(&tenant_site(&req.tenant)),
                &req.scheme,
                &channel.config_json,
            ),
            return_url_query: build_return_query(
                &ReturnContext {
                    biz_type: "order".into(),
                    business_no: order.order_no.clone(),
                    guest: order.user_id == 0,
                },
                &return_marker(&channel.provider_type, &channel.channel_type),
                "",
            ),
            client_ip: req.client_ip.trim().to_owned(),
            user_agent: req.user_agent.trim().to_owned(),
            channel_type: channel.channel_type.clone(),
            interaction_mode: InteractionMode::parse(&channel.interaction_mode),
            order_user_key,
            cancel_url: String::new(),
        };
        let config = channel.config_json.clone();
        let store = self.deps.payments.clone();
        let clock = self.deps.clock.clone();
        let task = tokio::spawn(async move {
            let created =
                tokio::time::timeout(GATEWAY_TIMEOUT, gateway.create_payment(&config, &input))
                    .await
                    .map_err(|_| GatewayError::request("gateway timeout"))??;
            apply_create_result(&mut payment, &created, &provider_order_no, clock.now());
            store
                .save_started(&payment)
                .await
                .map_err(|e| GatewayError::request(e.to_string()))?;
            Ok::<Payment, GatewayError>(payment)
        });
        match task.await {
            Ok(Ok(payment)) => Ok(payment),
            Ok(Err(e)) => {
                tracing::error!(error = %e, order_id = order.id, "payment_provider_apply_failed");
                Err(Error::from(e))
            }
            Err(join) => Err(Error::internal(join).or_internal(keys::PAYMENT_CREATE_FAILED)),
        }
    }

    /// `GET /payments/latest`: the latest payable link of a pending, unexpired parent order.
    pub async fn latest_payment(&self, order: &Order) -> Result<LatestPaymentView> {
        if order.parent_id.is_some() {
            return Err(Error::bad_request(pay_keys::PAYMENT_INVALID));
        }
        let now = self.deps.clock.now();
        if order.status != OrderStatus::PendingPayment || order.expires_at.is_some_and(|e| e <= now)
        {
            return Err(Error::bad_request(keys::ORDER_STATUS_INVALID));
        }
        let payment = self
            .deps
            .repo
            .latest_pending_payment(order.id, now)
            .await
            .map_err(|e| e.or_internal(pay_keys::PAYMENT_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(pay_keys::PAYMENT_NOT_FOUND))?;
        let channel_name = self
            .deps
            .repo
            .channel(payment.channel_id)
            .await?
            .map(|c| c.name)
            .unwrap_or_default();
        let info = extract_crypto_wallet_info(
            &payment.provider_type,
            &payment.interaction_mode,
            &payment.provider_payload,
        );
        Ok(LatestPaymentView {
            payment_id: payment.id,
            order_no: order.order_no.clone(),
            channel_id: payment.channel_id,
            channel_name,
            provider_type: payment.provider_type.clone(),
            channel_type: payment.channel_type.clone(),
            interaction_mode: payment.interaction_mode.clone(),
            pay_url: payment.pay_url.clone(),
            qr_code: payment.qr_code.clone(),
            wallet_address: info.address,
            chain_amount: info.chain_amount,
            chain: info.chain,
            token_id: info.token_id,
            expires_at: payment.expired_at,
            payable_amount: payment.amount,
            currency: payment.currency.clone(),
            fee_amount: payment.fee_amount,
            fee_policy: payment.fee_policy.clone(),
        })
    }

    /// The payment of an order owned by the viewer (capture ownership, 404 otherwise).
    pub async fn owned_payment(
        &self,
        viewer: &Viewer,
        tenant: &ResellerTenant,
        payment_id: Id,
    ) -> Result<Payment> {
        let payment = self
            .deps
            .repo
            .payment(payment_id)
            .await
            .map_err(|e| e.or_internal(pay_keys::PAYMENT_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(pay_keys::PAYMENT_NOT_FOUND))?;
        self.get_order_by_id(viewer, tenant, payment.order_id)
            .await?;
        Ok(payment)
    }

    /// Actively queries the gateway and settles the result (`CapturePayment`); only
    /// official gateways support it.
    pub async fn capture(&self, payment: Payment) -> Result<Payment> {
        if payment.status == PaymentStatus::Success {
            return Ok(payment);
        }
        let channel = self
            .deps
            .repo
            .channel(payment.channel_id)
            .await?
            .ok_or_else(|| Error::not_found(pay_keys::CHANNEL_NOT_FOUND))?;
        if channel.provider() != provider::OFFICIAL {
            return Err(Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED));
        }
        if payment.provider_ref.trim().is_empty() {
            return Err(Error::bad_request(pay_keys::PAYMENT_INVALID));
        }
        let gateway = self
            .deps
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .ok_or_else(|| Error::bad_request(pay_keys::PROVIDER_NOT_SUPPORTED))?;
        let config = channel.config_json.clone();
        let provider_ref = payment.provider_ref.clone();
        let result = tokio::spawn(async move {
            tokio::time::timeout(
                GATEWAY_TIMEOUT,
                gateway.query_payment(&config, &provider_ref),
            )
            .await
            .map_err(|_| GatewayError::request("gateway timeout"))?
        })
        .await
        .map_err(|e| Error::internal(e).or_internal(pay_keys::PAYMENT_CALLBACK_FAILED))??;
        let input = CallbackInput {
            payment_id: payment.id,
            order_no: String::new(),
            channel_id: channel.id,
            status: result.status.unwrap_or(PaymentStatus::Pending),
            provider_ref: if result.provider_ref.trim().is_empty() {
                payment.provider_ref.clone()
            } else {
                result.provider_ref.clone()
            },
            amount: result.amount,
            currency: result.currency.trim().to_uppercase(),
            paid_at: result.paid_at,
            payload: if result.payload.is_empty() {
                Map::new()
            } else {
                result.payload
            },
            verified_legacy_currency: String::new(),
        };
        self.settle_payment(input).await
    }
}
