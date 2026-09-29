//! Payment callbacks and webhooks: provider detection on the shared endpoint, payment/channel
//! lookup, signature verification, fact checks and hand-off to [`PaymentSettlement`].
//!
//! Mirrors `transport/http/callback/*`, `webhook_handler.go` and `payment_service_webhook.go`.
//! There is no unsigned "generic" callback path (PAY-14): unrecognized requests get a 404.

use std::sync::Arc;

use serde::Serialize;
use serde_json::{Map, Value};
use zs_domain::payment::alert::{PaymentAlert, PaymentAlerts};
use zs_domain::payment::callback::{CallbackInput, validate_callback_facts};
use zs_domain::payment::channel::{ChannelFilter, ChannelRepo, PaymentChannel};
use zs_domain::payment::errors::keys;
use zs_domain::payment::form::{FormMap, form_raw, pairs_to_form, parse_query};
use zs_domain::payment::gateway::{GatewayCallbackResult, GatewayError, GatewayRegistry, Headers};
use zs_domain::payment::model::{Payment, PaymentRepo};
use zs_domain::payment::routes::{CallbackKind, CallbackRoutes};
use zs_domain::payment::settlement::PaymentSettlement;
use zs_domain::payment::types::{
    PAYLOAD_FIAT_CURRENCY_SENT, PaymentStatus, channel_type, provider,
};
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, ErrorKind, Id, Result};
use zs_shared::clock::Clock;

/// Plain-text/JSON acknowledgements expected by each gateway.
pub mod replies {
    pub const EPAY_SUCCESS: &str = "success";
    pub const EPAY_FAIL: &str = "fail";
    pub const ALIPAY_SUCCESS: &str = "success";
    pub const ALIPAY_FAIL: &str = "fail";
    pub const BEPUSDT_SUCCESS: &str = "success";
    pub const BEPUSDT_FAIL: &str = "fail";
    pub const EPUSDT_SUCCESS: &str = "ok";
    pub const EPUSDT_FAIL: &str = "fail";
    pub const TOKENPAY_SUCCESS: &str = "ok";
    pub const TOKENPAY_FAIL: &str = "fail";
    pub const OKPAY_SUCCESS: &str = r#"{"status":"success"}"#;
    pub const OKPAY_FAIL: &str = r#"{"status":"fail"}"#;
    pub const WECHAT_SUCCESS: &str = r#"{"code":"SUCCESS","message":"成功"}"#;
    pub const WECHAT_FAIL: &str = r#"{"code":"FAIL","message":"失败"}"#;
    pub const HUIFU_FAIL: &str = "fail";
}

/// Content type of plain-text replies (gin `c.String`).
pub const TEXT_PLAIN: &str = "text/plain; charset=utf-8";
/// Content type of JSON replies.
pub const APPLICATION_JSON: &str = "application/json";
/// Content type of gin `c.JSON` replies.
pub const APPLICATION_JSON_UTF8: &str = "application/json; charset=utf-8";

/// A raw callback request.
#[derive(Debug, Clone, Default)]
pub struct CallbackRequest {
    pub method: String,
    pub path: String,
    pub raw_query: String,
    pub content_type: String,
    pub client_ip: String,
    pub headers: Headers,
    pub body: Vec<u8>,
}

/// Reply of the shared callback endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallbackReply {
    pub status: u16,
    pub content_type: &'static str,
    pub body: String,
}

impl CallbackReply {
    fn text(body: &str) -> Self {
        Self {
            status: 200,
            content_type: TEXT_PLAIN,
            body: body.to_owned(),
        }
    }

    /// Unrecognized callback (no provider matched).
    pub fn not_found() -> Self {
        Self {
            status: 404,
            content_type: TEXT_PLAIN,
            body: String::new(),
        }
    }
}

/// Envelope data of a processed webhook (`respondWebhookSuccess`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WebhookAck {
    pub accepted: bool,
    pub event_type: String,
    pub updated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<PaymentStatus>,
}

/// Dedicated webhook endpoints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebhookKind {
    DujiaoPay,
    Paypal,
    Stripe,
}

/// Callback use cases.
#[derive(Clone)]
pub struct CallbackService {
    payments: Arc<dyn PaymentRepo>,
    channels: Arc<dyn ChannelRepo>,
    registry: Arc<GatewayRegistry>,
    settlement: Arc<dyn PaymentSettlement>,
    alerts: Arc<dyn PaymentAlerts>,
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for CallbackService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CallbackService")
    }
}

/// Dependencies of [`CallbackService`].
pub struct CallbackDeps {
    pub payments: Arc<dyn PaymentRepo>,
    pub channels: Arc<dyn ChannelRepo>,
    pub registry: Arc<GatewayRegistry>,
    pub settlement: Arc<dyn PaymentSettlement>,
    pub alerts: Arc<dyn PaymentAlerts>,
    pub settings: Arc<dyn SettingsStore>,
    pub clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for CallbackDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CallbackDeps")
    }
}

/// `normalizeCallbackRequestQuery`: `&amp;` and `;` separators become `&` (PAY-18).
pub fn normalize_query(raw: &str) -> String {
    raw.replace("&amp;", "&").replace(';', "&")
}

/// `parseCallbackForm`: POST form body when present, else the (normalized) query.
/// Any malformed pair makes the whole form unusable (Go `ParseForm` error).
pub fn parse_callback_form(req: &CallbackRequest) -> Option<FormMap> {
    let (query, query_err) = parse_query(&normalize_query(&req.raw_query));
    let is_form_body = matches!(req.method.as_str(), "POST" | "PUT" | "PATCH")
        && req.content_type.split(';').next().is_some_and(|m| {
            m.trim()
                .eq_ignore_ascii_case("application/x-www-form-urlencoded")
        });
    let (post, post_err) = if is_form_body {
        parse_query(&String::from_utf8_lossy(&req.body))
    } else {
        (Vec::new(), None)
    };
    if query_err.is_some() || post_err.is_some() {
        return None;
    }
    if post.is_empty() {
        Some(pairs_to_form(query))
    } else {
        Some(pairs_to_form(post))
    }
}

/// Case-insensitive JSON member lookup (Go `encoding/json` struct field matching).
fn member<'a>(obj: &'a Map<String, Value>, key: &str) -> Option<&'a Value> {
    obj.get(key).or_else(|| {
        obj.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v)
    })
}

/// A string field of a probe struct: absent/null → empty, non-string → decode error.
fn probe_str(obj: &Map<String, Value>, key: &str) -> Option<String> {
    match member(obj, key) {
        None | Some(Value::Null) => Some(String::new()),
        Some(Value::String(s)) => Some(s.clone()),
        Some(_) => None,
    }
}

fn body_object(body: &[u8]) -> Option<Map<String, Value>> {
    match serde_json::from_slice::<Value>(body).ok()? {
        Value::Object(m) => Some(m),
        _ => None,
    }
}

/// Body callback descriptor (`bodyCallback`).
struct BodyCallback {
    provider: &'static str,
    order_no: String,
    provider_ref: String,
    success: &'static str,
    fail: &'static str,
    content_type: &'static str,
    alert: Option<(&'static str, Map<String, Value>)>,
}

impl CallbackService {
    pub fn new(deps: CallbackDeps) -> Self {
        Self {
            payments: deps.payments,
            channels: deps.channels,
            registry: deps.registry,
            settlement: deps.settlement,
            alerts: deps.alerts,
            settings: deps.settings,
            clock: deps.clock,
        }
    }

    /// Custom callback routes from settings (`callback_routes_config`).
    pub async fn routes(&self) -> CallbackRoutes {
        match self
            .settings
            .get(setting_keys::CALLBACK_ROUTES_CONFIG)
            .await
        {
            Ok(Some(value)) => CallbackRoutes::decode(&value),
            Ok(None) => CallbackRoutes::default(),
            Err(e) => {
                tracing::warn!(error = %e, "callback_routes_load_failed");
                CallbackRoutes::default()
            }
        }
    }

    /// True when the default path of `kind` is hidden by a custom route.
    pub async fn default_path_hidden(&self, kind: CallbackKind) -> bool {
        self.routes().await.default_hidden(kind)
    }

    async fn alert(&self, req: &CallbackRequest, alert: PaymentAlert) {
        let alert = PaymentAlert {
            method: req.method.clone(),
            path: req.path.clone(),
            client_ip: req.client_ip.clone(),
            ..alert
        };
        if let Err(e) = self.alerts.alert(alert).await {
            tracing::warn!(error = %e, "enqueue_payment_exception_alert_failed");
        }
    }

    /// Shared endpoint `GET|POST /payments/callback`: tries WeChat, OKPay, Alipay, epay,
    /// TokenPay, epusdt, BEpusdt in that order.
    pub async fn handle_shared(&self, req: &CallbackRequest) -> CallbackReply {
        tracing::info!(method = %req.method, client_ip = %req.client_ip, content_type = %req.content_type, "payment_callback_received");
        if let Some(reply) = self.try_wechat(req).await {
            return reply;
        }
        if let Some(reply) = self.try_okpay(req).await {
            return reply;
        }
        let form = parse_callback_form(req);
        if let Some(form) = &form {
            if let Some(reply) = self.try_huifu(req, form).await {
                return reply;
            }
            if let Some(reply) = self.try_alipay(req, form).await {
                return reply;
            }
            if let Some(reply) = self.try_epay(req, form).await {
                return reply;
            }
        } else {
            tracing::warn!("callback_form_parse_failed");
        }
        for probe in [
            Self::probe_tokenpay,
            Self::probe_epusdt,
            Self::probe_bepusdt,
        ] {
            if let Some(cb) = probe(&req.body) {
                return self.process_body_callback(req, cb).await;
            }
        }
        tracing::warn!(method = %req.method, client_ip = %req.client_ip, "payment_callback_unrecognized");
        let alert = PaymentAlert::new(
            "callback_unrecognized",
            "warning",
            "支付回调请求无法匹配已支持的回调格式",
        );
        self.alert(req, alert).await;
        CallbackReply::not_found()
    }

    async fn try_huifu(&self, req: &CallbackRequest, form: &FormMap) -> Option<CallbackReply> {
        let sign = form_raw(form, "sign");
        let resp_data = form_raw(form, "resp_data");
        if sign.trim().is_empty() || resp_data.trim().is_empty() {
            return None;
        }
        let data = serde_json::from_str::<Map<String, Value>>(&resp_data).ok()?;
        let order_no = data
            .get("req_seq_id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim()
            .to_owned();
        let merchant = data
            .get("huifu_id")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if order_no.is_empty() || merchant.trim().is_empty() {
            return None;
        }
        let Some(payment) = self.find_by_gateway_order_no(&order_no).await else {
            tracing::warn!(%order_no, "huifu_callback_payment_not_found");
            return Some(CallbackReply::text(replies::HUIFU_FAIL));
        };
        let Some(channel) = self.channel_of(&payment).await else {
            return Some(CallbackReply::text(replies::HUIFU_FAIL));
        };
        if channel.provider() != provider::HUIFU {
            return None;
        }
        Some(match self.handle_sync_callback(&channel, form, &[]).await {
            Ok(_) => CallbackReply::text(&format!("RECV_ORD_ID_{order_no}")),
            Err(error) => {
                tracing::warn!(payment_id = payment.id, %order_no, %error, "huifu_callback_handle_failed");
                let alert =
                    PaymentAlert::new("huifu_callback_handle_failed", "error", &error.to_string())
                        .with("payment_id", payment.id.to_string())
                        .with("provider", provider::HUIFU);
                self.alert(req, alert).await;
                CallbackReply::text(replies::HUIFU_FAIL)
            }
        })
    }

    fn query_channel_id(req: &CallbackRequest) -> std::result::Result<Id, ()> {
        let (pairs, _) = parse_query(&normalize_query(&req.raw_query));
        match pairs.iter().find(|(k, _)| k == "channel_id") {
            None => Ok(0),
            Some((_, v)) if v.trim().is_empty() => Ok(0),
            Some((_, v)) => v
                .trim()
                .parse::<u64>()
                .ok()
                .and_then(|u| Id::try_from(u).ok())
                .ok_or(()),
        }
    }

    async fn try_wechat(&self, req: &CallbackRequest) -> Option<CallbackReply> {
        let has_headers = [
            "Wechatpay-Signature",
            "Wechatpay-Timestamp",
            "Wechatpay-Nonce",
            "Wechatpay-Serial",
        ]
        .iter()
        .all(|h| !req.headers.get(h).is_empty());
        let is_wechat = has_headers
            && body_object(&req.body)
                .is_some_and(|m| matches!(m.get("resource"), Some(Value::Object(_))));
        if !is_wechat {
            return None;
        }
        let channel_id = Self::query_channel_id(req).unwrap_or(0);
        tracing::info!(
            channel_id,
            body_size = req.body.len(),
            "wechat_callback_received"
        );
        let result = self
            .webhook_via_registry(channel_id, provider::OFFICIAL, channel_type::WECHAT, req)
            .await;
        Some(match result {
            Ok(_) => CallbackReply {
                status: 200,
                content_type: APPLICATION_JSON_UTF8,
                body: replies::WECHAT_SUCCESS.to_owned(),
            },
            Err(e) => {
                tracing::warn!(channel_id, error = %e, "wechat_callback_handle_failed");
                let alert =
                    PaymentAlert::new("wechat_callback_handle_failed", "error", &e.to_string())
                        .with("provider", channel_type::WECHAT);
                self.alert(req, alert).await;
                CallbackReply {
                    status: 400,
                    content_type: APPLICATION_JSON_UTF8,
                    body: replies::WECHAT_FAIL.to_owned(),
                }
            }
        })
    }

    async fn try_okpay(&self, req: &CallbackRequest) -> Option<CallbackReply> {
        let text = String::from_utf8_lossy(&req.body);
        if !text.trim().starts_with('{') {
            return None;
        }
        let obj = body_object(&req.body)?;
        let sign = probe_str(&obj, "sign")?;
        let data = match member(&obj, "data") {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(d)) => d.clone(),
            Some(_) => return None,
        };
        let order_id = probe_str(&data, "order_id")?.trim().to_owned();
        let unique_id = probe_str(&data, "unique_id")?.trim().to_owned();
        if sign.trim().is_empty() || (order_id.is_empty() && unique_id.is_empty()) {
            return None;
        }
        let mut alert_data = Map::new();
        alert_data.insert("unique_id".into(), Value::String(unique_id.clone()));
        Some(
            self.process_body_callback(
                req,
                BodyCallback {
                    provider: provider::OKPAY,
                    order_no: unique_id,
                    provider_ref: order_id,
                    success: replies::OKPAY_SUCCESS,
                    fail: replies::OKPAY_FAIL,
                    content_type: APPLICATION_JSON,
                    alert: Some(("okpay_callback_handle_failed", alert_data)),
                },
            )
            .await,
        )
    }

    fn probe_tokenpay(body: &[u8]) -> Option<BodyCallback> {
        let obj = body_object(body)?;
        let signature = probe_str(&obj, "Signature")?;
        let order_id = probe_str(&obj, "OutOrderId")?;
        let token_id = probe_str(&obj, "Id")?;
        if signature.trim().is_empty() || order_id.trim().is_empty() || token_id.trim().is_empty() {
            return None;
        }
        Some(BodyCallback {
            provider: provider::TOKENPAY,
            order_no: order_id,
            provider_ref: token_id,
            success: replies::TOKENPAY_SUCCESS,
            fail: replies::TOKENPAY_FAIL,
            content_type: TEXT_PLAIN,
            alert: None,
        })
    }

    fn probe_epusdt(body: &[u8]) -> Option<BodyCallback> {
        let obj = body_object(body)?;
        let pid = probe_str(&obj, "pid")?;
        let trade_id = probe_str(&obj, "trade_id")?;
        let order_id = probe_str(&obj, "order_id")?;
        // PAY-42: `pid` is the discriminator between epusdt (GMPay) and BEpusdt.
        if pid.trim().is_empty() || trade_id.is_empty() || order_id.is_empty() {
            return None;
        }
        Some(BodyCallback {
            provider: provider::EPUSDT,
            order_no: order_id,
            provider_ref: trade_id,
            success: replies::EPUSDT_SUCCESS,
            fail: replies::EPUSDT_FAIL,
            content_type: TEXT_PLAIN,
            alert: None,
        })
    }

    fn probe_bepusdt(body: &[u8]) -> Option<BodyCallback> {
        let obj = body_object(body)?;
        let trade_id = probe_str(&obj, "trade_id")?;
        let order_id = probe_str(&obj, "order_id")?;
        if trade_id.is_empty() || order_id.is_empty() {
            return None;
        }
        Some(BodyCallback {
            provider: provider::BEPUSDT,
            order_no: order_id,
            provider_ref: trade_id,
            success: replies::BEPUSDT_SUCCESS,
            fail: replies::BEPUSDT_FAIL,
            content_type: TEXT_PLAIN,
            alert: None,
        })
    }

    async fn find_by_gateway_order_no(&self, no: &str) -> Option<Payment> {
        let no = no.trim();
        if no.is_empty() {
            return None;
        }
        self.payments
            .find_by_gateway_order_no(no)
            .await
            .ok()
            .flatten()
    }

    async fn find_by_provider_ref(&self, r: &str) -> Option<Payment> {
        let r = r.trim();
        if r.is_empty() {
            return None;
        }
        self.payments
            .find_latest_by_provider_ref(r)
            .await
            .ok()
            .flatten()
    }

    async fn channel_of(&self, payment: &Payment) -> Option<PaymentChannel> {
        self.channels.get(payment.channel_id).await.ok().flatten()
    }

    async fn process_body_callback(
        &self,
        req: &CallbackRequest,
        cb: BodyCallback,
    ) -> CallbackReply {
        let reply = |ok: bool| CallbackReply {
            status: 200,
            content_type: cb.content_type,
            body: if ok { cb.success } else { cb.fail }.to_owned(),
        };
        let payment = match self.find_by_gateway_order_no(&cb.order_no).await {
            Some(p) => p,
            None => match self.find_by_provider_ref(&cb.provider_ref).await {
                Some(p) => p,
                None => {
                    tracing::warn!(provider = cb.provider, order_no = %cb.order_no, provider_ref = %cb.provider_ref, "callback_payment_not_found");
                    return reply(false);
                }
            },
        };
        let Some(channel) = self.channel_of(&payment).await else {
            tracing::warn!(
                provider = cb.provider,
                payment_id = payment.id,
                "callback_channel_not_found"
            );
            return reply(false);
        };
        if !channel
            .provider_type
            .trim()
            .eq_ignore_ascii_case(cb.provider)
        {
            tracing::warn!(provider = cb.provider, payment_id = payment.id, channel_provider = %channel.provider_type, "callback_provider_invalid");
            return reply(false);
        }
        match self
            .handle_sync_callback(&channel, &FormMap::new(), &req.body)
            .await
        {
            Ok(updated) => {
                tracing::info!(provider = cb.provider, payment_id = updated.id, status = %updated.status, "callback_processed");
                reply(true)
            }
            Err(e) => {
                tracing::error!(provider = cb.provider, payment_id = payment.id, error = %e, "callback_handle_failed");
                if let Some((alert_type, data)) = cb.alert {
                    let mut alert = PaymentAlert::new(alert_type, "error", &e.to_string())
                        .with("payment_id", payment.id.to_string())
                        .with("provider", cb.provider);
                    alert.data.extend(data);
                    self.alert(req, alert).await;
                }
                reply(false)
            }
        }
    }

    async fn try_alipay(&self, req: &CallbackRequest, form: &FormMap) -> Option<CallbackReply> {
        let v = |k: &str| form_raw(form, k).trim().to_owned();
        let has_notify =
            !v("notify_id").is_empty() || !v("notify_type").is_empty() || !v("buyer_id").is_empty();
        let has_order = !v("out_trade_no").is_empty() || !v("trade_no").is_empty();
        if v("sign").is_empty() || !has_notify || !has_order {
            return None;
        }
        tracing::info!(out_trade_no = %v("out_trade_no"), trade_no = %v("trade_no"), trade_status = %v("trade_status"), "alipay_callback_received");
        let mut found = None;
        for (value, by_gateway_no) in [(v("out_trade_no"), true), (v("trade_no"), false)] {
            if value.is_empty() {
                continue;
            }
            let payment = if by_gateway_no {
                self.find_by_gateway_order_no(&value).await
            } else {
                self.find_by_provider_ref(&value).await
            };
            let Some(payment) = payment else { continue };
            if let Some(channel) = self.channel_of(&payment).await
                && channel.is_official(channel_type::ALIPAY)
            {
                found = Some((payment, channel));
                break;
            }
        }
        let Some((payment, channel)) = found else {
            tracing::warn!(out_trade_no = %v("out_trade_no"), "alipay_callback_payment_not_found");
            return Some(CallbackReply::text(replies::ALIPAY_FAIL));
        };
        Some(match self.handle_sync_callback(&channel, form, &[]).await {
            Ok(_) => CallbackReply::text(replies::ALIPAY_SUCCESS),
            Err(e) => {
                tracing::warn!(payment_id = payment.id, channel_id = channel.id, error = %e, "alipay_callback_handle_failed");
                let alert =
                    PaymentAlert::new("alipay_callback_handle_failed", "error", &e.to_string())
                        .with("payment_id", payment.id.to_string())
                        .with("out_trade_no", v("out_trade_no"))
                        .with("provider", channel_type::ALIPAY);
                self.alert(req, alert).await;
                CallbackReply::text(replies::ALIPAY_FAIL)
            }
        })
    }

    async fn try_epay(&self, req: &CallbackRequest, form: &FormMap) -> Option<CallbackReply> {
        let v = |k: &str| form_raw(form, k).trim().to_owned();
        let out_trade_no = v("out_trade_no");
        if v("pid").is_empty() || out_trade_no.is_empty() || v("trade_status").is_empty() {
            return None;
        }
        tracing::info!(out_trade_no = %out_trade_no, trade_status = %v("trade_status"), "epay_callback_received");
        let Some(payment) = self.find_by_gateway_order_no(&out_trade_no).await else {
            tracing::warn!(out_trade_no = %out_trade_no, "epay_callback_payment_not_found");
            return Some(CallbackReply::text(replies::EPAY_FAIL));
        };
        let Some(channel) = self.channel_of(&payment).await else {
            return Some(CallbackReply::text(replies::EPAY_FAIL));
        };
        if channel.provider() != provider::EPAY {
            tracing::warn!(payment_id = payment.id, provider_type = %channel.provider_type, "epay_callback_provider_invalid");
            return Some(CallbackReply::text(replies::EPAY_FAIL));
        }
        Some(match self.handle_sync_callback(&channel, form, &[]).await {
            Ok(_) => CallbackReply::text(replies::EPAY_SUCCESS),
            Err(e) => {
                tracing::warn!(payment_id = payment.id, error = %e, "epay_callback_handle_failed");
                let alert =
                    PaymentAlert::new("epay_callback_handle_failed", "error", &e.to_string())
                        .with("payment_id", payment.id.to_string())
                        .with("out_trade_no", out_trade_no)
                        .with("provider", provider::EPAY);
                self.alert(req, alert).await;
                CallbackReply::text(replies::EPAY_FAIL)
            }
        })
    }

    /// `HandleSyncCallback`: verify with the payment's channel, then settle.
    pub async fn handle_sync_callback(
        &self,
        channel: &PaymentChannel,
        form: &FormMap,
        body: &[u8],
    ) -> Result<Payment> {
        let gateway = self
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .filter(|g| g.capabilities().callback)
            .ok_or_else(|| Error::bad_request(keys::PROVIDER_NOT_SUPPORTED))?;
        let result = gateway
            .verify_callback(&channel.config_json, form, body)
            .map_err(Error::from)?;
        let payment = self
            .find_webhook_payment(channel.id, &result)
            .await?
            .ok_or_else(|| Error::not_found(keys::PAYMENT_NOT_FOUND))?;
        let status = result
            .status
            .ok_or_else(|| Error::bad_request(keys::PAYMENT_STATUS_INVALID))?;
        self.apply(&payment, channel.id, status, result, String::new())
            .await
    }

    /// `findWebhookPayment`: by gateway order number, then provider reference — always
    /// restricted to the verifying channel (PAY-07/PAY-09).
    async fn find_webhook_payment(
        &self,
        channel_id: Id,
        result: &GatewayCallbackResult,
    ) -> Result<Option<Payment>> {
        if let Some(p) = self.find_by_gateway_order_no(&result.order_no).await
            && p.channel_id == channel_id
        {
            return Ok(Some(p));
        }
        if let Some(p) = self.find_by_provider_ref(&result.provider_ref).await
            && p.channel_id == channel_id
        {
            return Ok(Some(p));
        }
        Ok(None)
    }

    /// `HandleCallback` up to settlement: channel/order-number/currency/amount facts are checked
    /// here and again by the settlement on the locked row (PAY-04).
    async fn apply(
        &self,
        payment: &Payment,
        channel_id: Id,
        status: PaymentStatus,
        result: GatewayCallbackResult,
        verified_legacy_currency: String,
    ) -> Result<Payment> {
        let input = CallbackInput {
            payment_id: payment.id,
            order_no: result.order_no,
            channel_id,
            status,
            provider_ref: if result.provider_ref.trim().is_empty() {
                payment.provider_ref.clone()
            } else {
                result.provider_ref.trim().to_owned()
            },
            amount: result.amount,
            currency: result.currency.trim().to_ascii_uppercase(),
            paid_at: result.paid_at,
            payload: result.payload,
            verified_legacy_currency,
        };
        let business_no = self
            .payments
            .business_no(payment)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_UPDATE_FAILED))?
            .ok_or_else(|| Error::not_found("error.order_not_found"))?;
        if let Err(e) = validate_callback_facts(payment, &business_no, &input) {
            tracing::warn!(
                payment_id = payment.id,
                stored_amount = %payment.amount,
                callback_amount = %input.amount,
                stored_currency = %payment.currency,
                callback_currency = %input.currency,
                callback_order_no = %input.order_no,
                reason = ?e,
                "payment_callback_facts_rejected"
            );
            return Err(e.into());
        }
        let updated = self.settlement.settle(input).await?;
        tracing::info!(payment_id = updated.id, status = %updated.status, at = %self.clock.now(), "payment_callback_processed");
        Ok(updated)
    }

    /// Dedicated webhook endpoints (`/payments/webhook/{dujiaopay,paypal,stripe}`).
    pub async fn handle_webhook(
        &self,
        kind: WebhookKind,
        req: &CallbackRequest,
    ) -> Result<WebhookAck> {
        let channel_id = Self::query_channel_id(req);
        let (provider_label, outcome) = match kind {
            WebhookKind::Paypal => {
                // PayPal cannot be matched blindly: `channel_id` is mandatory (PAY-07).
                let id = channel_id
                    .ok()
                    .filter(|id| *id > 0)
                    .ok_or_else(Error::invalid)?;
                (
                    channel_type::PAYPAL,
                    self.webhook_via_registry(id, provider::OFFICIAL, channel_type::PAYPAL, req)
                        .await,
                )
            }
            WebhookKind::Stripe => (
                channel_type::STRIPE,
                self.webhook_via_registry(
                    channel_id.unwrap_or(0),
                    provider::OFFICIAL,
                    channel_type::STRIPE,
                    req,
                )
                .await,
            ),
            WebhookKind::DujiaoPay => (
                provider::DUJIAOPAY,
                self.dujiaopay_webhook(channel_id.unwrap_or(0), req).await,
            ),
        };
        match outcome {
            Ok((payment, event_type)) => Ok(WebhookAck {
                accepted: true,
                event_type,
                updated: payment.is_some(),
                payment_id: payment.as_ref().map(|p| p.id),
                status: payment.map(|p| p.status),
            }),
            Err(e) => {
                tracing::warn!(provider = provider_label, error = %e, "payment_webhook_handle_failed");
                let alert = PaymentAlert::new(
                    &format!("{provider_label}_webhook_handle_failed"),
                    "error",
                    &e.to_string(),
                )
                .with("provider", provider_label);
                self.alert(req, alert).await;
                Err(callback_error(e))
            }
        }
    }

    async fn dujiaopay_webhook(
        &self,
        channel_id: Id,
        req: &CallbackRequest,
    ) -> Result<(Option<Payment>, String)> {
        if channel_id == 0 {
            let filter = ChannelFilter {
                provider_type: provider::DUJIAOPAY.to_owned(),
                active_only: true,
                ..ChannelFilter::default()
            };
            let (candidates, _) = self
                .channels
                .list(&filter)
                .await
                .map_err(|e| e.or_internal(keys::PAYMENT_UPDATE_FAILED))?;
            return self.try_candidates(candidates, req).await;
        }
        let channel = self
            .channels
            .get(channel_id)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_UPDATE_FAILED))?
            .ok_or_else(|| Error::not_found(keys::CHANNEL_NOT_FOUND))?;
        if channel.provider() != provider::DUJIAOPAY {
            return Err(Error::bad_request(keys::PROVIDER_NOT_SUPPORTED));
        }
        let result = self.parse_with_channel(&channel, req).await?;
        self.commit_verified(&channel, result).await
    }

    /// `handleWebhookViaRegistry`: explicit channel, or blind matching for WeChat/Stripe (PAY-07).
    async fn webhook_via_registry(
        &self,
        channel_id: Id,
        expected_provider: &str,
        expected_channel: &str,
        req: &CallbackRequest,
    ) -> Result<(Option<Payment>, String)> {
        if channel_id == 0 {
            if expected_channel != channel_type::WECHAT && expected_channel != channel_type::STRIPE
            {
                return Err(Error::bad_request(keys::PAYMENT_INVALID));
            }
            let filter = ChannelFilter {
                provider_type: expected_provider.to_owned(),
                channel_type: expected_channel.to_owned(),
                active_only: true,
                ..ChannelFilter::default()
            };
            let (candidates, _) = self
                .channels
                .list(&filter)
                .await
                .map_err(|e| e.or_internal(keys::PAYMENT_UPDATE_FAILED))?;
            return self.try_candidates(candidates, req).await;
        }
        let channel = self
            .channels
            .get(channel_id)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_UPDATE_FAILED))?
            .ok_or_else(|| Error::not_found(keys::CHANNEL_NOT_FOUND))?;
        if channel.provider() != expected_provider || channel.channel() != expected_channel {
            return Err(Error::bad_request(keys::PROVIDER_NOT_SUPPORTED));
        }
        let result = self.parse_with_channel(&channel, req).await?;
        self.commit_verified(&channel, result).await
    }

    /// Tries every active candidate; the first whose credentials verify wins. Errors after a
    /// successful verification are final (no further candidates).
    async fn try_candidates(
        &self,
        candidates: Vec<PaymentChannel>,
        req: &CallbackRequest,
    ) -> Result<(Option<Payment>, String)> {
        if candidates.is_empty() {
            return Err(Error::not_found(keys::CHANNEL_NOT_FOUND));
        }
        let mut last_err = Error::bad_request(keys::PROVIDER_NOT_SUPPORTED);
        for channel in &candidates {
            match self.parse_with_channel(channel, req).await {
                Ok(result) => {
                    tracing::info!(
                        candidate_channel_id = channel.id,
                        "payment_webhook_candidate_matched"
                    );
                    return self.commit_verified(channel, result).await;
                }
                Err(e) => {
                    tracing::debug!(candidate_channel_id = channel.id, error = %e, "payment_webhook_candidate_parse_failed");
                    last_err = e;
                }
            }
        }
        Err(last_err)
    }

    async fn parse_with_channel(
        &self,
        channel: &PaymentChannel,
        req: &CallbackRequest,
    ) -> Result<GatewayCallbackResult> {
        let gateway = self
            .registry
            .lookup(&channel.provider_type, &channel.channel_type)
            .filter(|g| g.capabilities().webhook)
            .ok_or_else(|| Error::bad_request(keys::PROVIDER_NOT_SUPPORTED))?;
        gateway
            .parse_webhook(
                &channel.config_json,
                &req.headers,
                &req.body,
                self.clock.now(),
            )
            .await
            .map_err(|e: GatewayError| Error::from(e))
    }

    /// `commitVerifiedWebhook`.
    async fn commit_verified(
        &self,
        channel: &PaymentChannel,
        result: GatewayCallbackResult,
    ) -> Result<(Option<Payment>, String)> {
        let Some(status) = result.status else {
            tracing::info!(channel_id = channel.id, order_no = %result.order_no, "payment_webhook_status_ignored");
            return Ok((None, String::new()));
        };
        let Some(payment) = self.find_webhook_payment(channel.id, &result).await? else {
            tracing::info!(channel_id = channel.id, order_no = %result.order_no, provider_ref = %result.provider_ref, "payment_webhook_payment_not_found");
            return Ok((None, status.as_str().to_owned()));
        };
        let mut legacy = String::new();
        if channel.provider_type == provider::DUJIAOPAY
            && status == PaymentStatus::Success
            && payment.status != PaymentStatus::Success
            && !payment
                .currency
                .trim()
                .eq_ignore_ascii_case(result.currency.trim())
            && !payment
                .provider_payload
                .contains_key(PAYLOAD_FIAT_CURRENCY_SENT)
        {
            legacy = result.currency.trim().to_ascii_uppercase();
            tracing::warn!(payment_id = payment.id, stored_currency = %payment.currency, verified_currency = %legacy, "payment_webhook_legacy_dujiaopay_currency_adoption");
        }
        let updated = self
            .apply(&payment, channel.id, status, result, legacy)
            .await?;
        Ok((Some(updated), status.as_str().to_owned()))
    }
}

/// Non-business errors of callbacks use `error.payment_callback_failed` (500).
fn callback_error(e: Error) -> Error {
    if e.kind() == ErrorKind::Internal {
        e.or_internal(keys::PAYMENT_CALLBACK_FAILED)
    } else {
        e
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(method: &str, query: &str, ct: &str, body: &str) -> CallbackRequest {
        CallbackRequest {
            method: method.into(),
            raw_query: query.into(),
            content_type: ct.into(),
            body: body.as_bytes().to_vec(),
            ..CallbackRequest::default()
        }
    }

    /// PAY-18: `;` and `&amp;` separated queries parse.
    #[test]
    fn pay_18_query_normalization() {
        for q in [
            "pid=2026;out_trade_no=ORDER-1;trade_status=TRADE_SUCCESS;sign=abc",
            "pid=2026&amp;out_trade_no=ORDER-1&amp;trade_status=TRADE_SUCCESS&amp;sign=abc",
        ] {
            let form = parse_callback_form(&req("GET", q, "", "")).unwrap_or_default();
            assert_eq!(form_raw(&form, "out_trade_no"), "ORDER-1");
            assert_eq!(form_raw(&form, "sign"), "abc");
        }
    }

    #[test]
    fn form_body_takes_precedence_over_query() {
        let r = req(
            "POST",
            "a=1",
            "application/x-www-form-urlencoded; charset=utf-8",
            "b=2",
        );
        let form = parse_callback_form(&r).unwrap_or_default();
        assert_eq!(form_raw(&form, "b"), "2");
        assert_eq!(form_raw(&form, "a"), "");
        let json = req("POST", "a=1", "application/json", "{\"b\":2}");
        assert_eq!(
            form_raw(&parse_callback_form(&json).unwrap_or_default(), "a"),
            "1"
        );
        assert!(parse_callback_form(&req("GET", "a=%zz", "", "")).is_none());
    }

    /// PAY-42: probes are mutually exclusive (pid decides epusdt vs BEpusdt).
    #[test]
    fn pay_42_probe_discrimination() {
        let bep = br#"{"trade_id":"T","order_id":"O","signature":"s","status":2}"#;
        assert!(CallbackService::probe_epusdt(bep).is_none());
        assert!(CallbackService::probe_bepusdt(bep).is_some());
        let ep = br#"{"pid":"1","trade_id":"T","order_id":"O","signature":"s","status":2}"#;
        assert!(CallbackService::probe_epusdt(ep).is_some());
        assert!(CallbackService::probe_tokenpay(ep).is_none());
        assert!(CallbackService::probe_bepusdt(b"").is_none());
        let tp = br#"{"Id":"TP1","OutOrderId":"DJP1","Signature":"x"}"#;
        assert!(CallbackService::probe_tokenpay(tp).is_some());
        let bad_type = br#"{"Id":5,"OutOrderId":"DJP1","Signature":"x"}"#;
        assert!(CallbackService::probe_tokenpay(bad_type).is_none());
    }
}
