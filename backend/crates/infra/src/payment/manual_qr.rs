//! Merchant-owned WeChat/Alipay collection QR codes. Settlement is restricted to
//! authenticated administrators and never accepts a public gateway callback.

use async_trait::async_trait;
use base64::Engine as _;
use lettre::{AsyncTransport, Message};
use rand::RngCore;
use reqwest::{Client, Url};
use serde_json::Value;
use std::time::Duration;
use zs_domain::payment::channel::{ChannelConfig, config_str};
use zs_domain::payment::gateway::{
    GatewayCapabilities, GatewayCreateInput, GatewayCreateResult, GatewayError, ManualIssueNotice,
    PaymentGateway,
};
use zs_shared::money::Amount;

use super::common::hmac_sha256;
use crate::identity::mail::smtp_transport;

const NOTICE_TIMEOUT: Duration = Duration::from_secs(12);

fn official_webhook(raw: &str, host: &str, path: &str) -> Result<Url, GatewayError> {
    let url = Url::parse(raw).map_err(|_| GatewayError::config("invalid webhook URL"))?;
    if url.scheme() != "https" || url.host_str() != Some(host) || url.path() != path {
        return Err(GatewayError::config(
            "webhook must use the official HTTPS endpoint",
        ));
    }
    Ok(url)
}

async fn post_notice(url: Url, body: Value) -> Result<(), GatewayError> {
    let client = Client::builder()
        .timeout(NOTICE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| GatewayError::request("notification client failed"))?;
    let response = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|_| GatewayError::request("notification request failed"))?;
    if !response.status().is_success() {
        return Err(GatewayError::request(
            "notification endpoint rejected the message",
        ));
    }
    let reply: Value = response
        .json()
        .await
        .map_err(|_| GatewayError::response("notification response invalid"))?;
    let accepted = reply.get("ok").and_then(Value::as_bool) == Some(true)
        || reply.get("errcode").and_then(Value::as_i64) == Some(0);
    if !accepted {
        return Err(GatewayError::response(
            "notification endpoint did not accept the message",
        ));
    }
    Ok(())
}

async fn telegram_call(config: &ChannelConfig, method: &str) -> Result<Value, GatewayError> {
    let token = config_str(config, "telegram_token");
    if token.is_empty()
        || !token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b':' | b'_' | b'-'))
    {
        return Err(GatewayError::config("telegram_token invalid"));
    }
    let url = format!("https://api.telegram.org/bot{token}/{method}");
    let client = Client::builder()
        .timeout(NOTICE_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| GatewayError::request("Telegram client failed"))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| GatewayError::request("Telegram request failed"))?;
    if !response.status().is_success() {
        return Err(GatewayError::request("Telegram request rejected"));
    }
    let value: Value = response
        .json()
        .await
        .map_err(|_| GatewayError::response("Telegram response invalid"))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        return Err(GatewayError::response("Telegram API rejected request"));
    }
    Ok(value)
}

async fn send_email(config: &ChannelConfig, subject: &str, text: &str) -> Result<(), GatewayError> {
    let to = config_str(config, "smtp_to");
    if to.is_empty() {
        return Ok(());
    }
    let host = config_str(config, "smtp_host");
    let from = config_str(config, "smtp_from");
    let port = config_str(config, "smtp_port")
        .parse::<u16>()
        .map_err(|_| GatewayError::config("smtp_port invalid"))?;
    let message = Message::builder()
        .from(
            from.parse()
                .map_err(|_| GatewayError::config("smtp_from invalid"))?,
        )
        .to(to
            .parse()
            .map_err(|_| GatewayError::config("smtp_to invalid"))?)
        .subject(subject)
        .body(text.to_owned())
        .map_err(|_| GatewayError::config("SMTP message invalid"))?;
    let transport = smtp_transport(
        &host,
        port,
        config
            .get("smtp_ssl")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        config
            .get("smtp_tls")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        &config_str(config, "smtp_username"),
        &config_str(config, "smtp_password"),
        NOTICE_TIMEOUT,
    )
    .map_err(|_| GatewayError::config("SMTP configuration invalid"))?;
    transport
        .send(message)
        .await
        .map_err(|_| GatewayError::request("SMTP delivery failed"))?;
    Ok(())
}

async fn send_notices(
    config: &ChannelConfig,
    subject: &str,
    text: &str,
) -> Result<(), GatewayError> {
    let mut failures = Vec::new();
    let token = config_str(config, "telegram_token");
    let chat_id = config_str(config, "telegram_chat_id");
    let wecom = config_str(config, "wecom_webhook_url");
    let dingtalk = config_str(config, "dingtalk_webhook_url");
    let smtp_to = config_str(config, "smtp_to");
    if (token.is_empty() || chat_id.is_empty())
        && wecom.is_empty()
        && dingtalk.is_empty()
        && smtp_to.is_empty()
    {
        return Err(GatewayError::config(
            "no notification destination configured",
        ));
    }
    if !token.is_empty() && !chat_id.is_empty() {
        let url = Url::parse(&format!("https://api.telegram.org/bot{token}/sendMessage"))
            .map_err(|_| GatewayError::config("telegram_token invalid"))?;
        if post_notice(
            url,
            serde_json::json!({"chat_id": chat_id, "text": text, "disable_web_page_preview": true}),
        )
        .await
        .is_err()
        {
            failures.push("Telegram");
        }
    }
    if !wecom.is_empty() {
        let url = official_webhook(&wecom, "qyapi.weixin.qq.com", "/cgi-bin/webhook/send")?;
        if post_notice(
            url,
            serde_json::json!({"msgtype": "text", "text": {"content": text}}),
        )
        .await
        .is_err()
        {
            failures.push("企业微信");
        }
    }
    if !dingtalk.is_empty() {
        let mut url = official_webhook(&dingtalk, "oapi.dingtalk.com", "/robot/send")?;
        let secret = config_str(config, "dingtalk_secret");
        if !secret.is_empty() {
            let timestamp = chrono::Utc::now().timestamp_millis();
            let signed = format!("{timestamp}\n{secret}");
            let signature = base64::engine::general_purpose::STANDARD
                .encode(hmac_sha256(secret.as_bytes(), signed.as_bytes()));
            url.query_pairs_mut()
                .append_pair("timestamp", &timestamp.to_string())
                .append_pair("sign", &signature);
        }
        if post_notice(
            url,
            serde_json::json!({"msgtype": "text", "text": {"content": text}}),
        )
        .await
        .is_err()
        {
            failures.push("钉钉");
        }
    }
    if send_email(config, subject, text).await.is_err() {
        failures.push("邮件");
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(GatewayError::request(format!(
            "notification delivery failed: {}",
            failures.join(", ")
        )))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ManualQrGateway;

#[async_trait]
impl PaymentGateway for ManualQrGateway {
    fn key(&self) -> &'static str {
        "manual_qr:"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities::default()
    }

    fn validate_config(&self, config: &ChannelConfig, channel: &str) -> Result<(), GatewayError> {
        if !matches!(channel, "wechat" | "alipay") {
            return Err(GatewayError::UnsupportedChannel(channel.to_owned()));
        }
        let url = config_str(config, "qr_image_url");
        if url.is_empty() || !url.starts_with('/') && !url.starts_with("https://") {
            return Err(GatewayError::config(
                "qr_image_url must be a site path or HTTPS URL",
            ));
        }
        for (key, host, path) in [
            (
                "wecom_webhook_url",
                "qyapi.weixin.qq.com",
                "/cgi-bin/webhook/send",
            ),
            ("dingtalk_webhook_url", "oapi.dingtalk.com", "/robot/send"),
        ] {
            let raw = config_str(config, key);
            if !raw.is_empty() {
                official_webhook(&raw, host, path)?;
            }
        }
        Ok(())
    }

    async fn create_payment(
        &self,
        config: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        self.validate_config(config, &input.channel_type)?;
        let mut bytes = [0_u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let mut payload = serde_json::Map::new();
        payload.insert(
            "manual_link_token".to_owned(),
            Value::String(format!("mql_{}", hex::encode(bytes))),
        );
        Ok(GatewayCreateResult {
            qr_code_url: config_str(config, "qr_image_url"),
            payload,
            ..GatewayCreateResult::default()
        })
    }

    async fn notify_manual_payment(
        &self,
        config: &ChannelConfig,
        order_no: &str,
        amount: Amount,
        currency: &str,
        items: &str,
        review_url: &str,
    ) -> Result<(), GatewayError> {
        let text = format!(
            "人工收款待确认\n订单：{order_no}\n商品：{items}\n应收：{amount} {currency}\n确认页面：{review_url}"
        );
        send_notices(config, "人工收款待确认", &text).await
    }

    async fn notify_manual_issue(
        &self,
        config: &ChannelConfig,
        notice: &ManualIssueNotice<'_>,
    ) -> Result<(), GatewayError> {
        let text = format!(
            "买家报告收款异常，请核对收款列表\n订单：{}\n应收：{} {}\n付款人姓名：{}\n问题：{}\n说明：{}\n确认页面：{}",
            notice.order_no,
            notice.amount,
            notice.currency,
            notice.payer_name,
            notice.issue,
            notice.note,
            notice.review_url,
        );
        send_notices(config, "买家报告收款异常", &text).await
    }

    async fn manual_telegram_username(
        &self,
        config: &ChannelConfig,
    ) -> Result<String, GatewayError> {
        let response = telegram_call(config, "getMe").await?;
        let username = response
            .pointer("/result/username")
            .and_then(Value::as_str)
            .ok_or_else(|| GatewayError::response("Telegram bot username missing"))?;
        if username.is_empty()
            || !username
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        {
            return Err(GatewayError::response("Telegram bot username invalid"));
        }
        Ok(username.to_owned())
    }

    async fn manual_telegram_chat(
        &self,
        config: &ChannelConfig,
        code: &str,
    ) -> Result<Option<String>, GatewayError> {
        let response = telegram_call(config, "getUpdates?limit=100").await?;
        let Some(updates) = response.get("result").and_then(Value::as_array) else {
            return Err(GatewayError::response("Telegram updates invalid"));
        };
        let expected = format!("/start {code}");
        Ok(updates.iter().rev().find_map(|update| {
            let message = update.get("message")?;
            (message.get("text")?.as_str()? == expected)
                .then(|| {
                    message
                        .pointer("/chat/id")?
                        .as_i64()
                        .map(|id| id.to_string())
                })
                .flatten()
        }))
    }
}
