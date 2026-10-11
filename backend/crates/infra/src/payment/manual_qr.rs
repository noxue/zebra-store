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
    ManualTelegramChat, PaymentGateway,
};
use zs_shared::money::Amount;

use super::common::hmac_sha256;
use crate::identity::mail::smtp_transport;

const NOTICE_TIMEOUT: Duration = Duration::from_secs(12);

fn qr_image(config: &ChannelConfig, method: &str) -> String {
    let keyed = config_str(config, &format!("{method}_qr_image_url"));
    if !keyed.is_empty() {
        keyed
    } else if config
        .get("supported_channel_types")
        .and_then(Value::as_array)
        .is_none()
    {
        // Existing single-method channels kept their image in qr_image_url.
        config_str(config, "qr_image_url")
    } else {
        String::new()
    }
}

fn selected_notification(config: &ChannelConfig) -> String {
    let selected = config_str(config, "notification_type");
    if !selected.is_empty() {
        return selected;
    }
    // Legacy channels predate the single-destination selector.
    if !config_str(config, "telegram_token").is_empty() {
        "telegram".into()
    } else if !config_str(config, "wecom_webhook_url").is_empty() {
        "wecom".into()
    } else if !config_str(config, "dingtalk_webhook_url").is_empty() {
        "dingtalk".into()
    } else if !config_str(config, "smtp_to").is_empty() {
        "email".into()
    } else {
        String::new()
    }
}

fn telegram_chat_ids(config: &ChannelConfig) -> Vec<String> {
    let mut ids = config
        .get("telegram_chat_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|chat| chat.get("id").and_then(Value::as_str))
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let legacy = config_str(config, "telegram_chat_id");
    if !legacy.is_empty() && !ids.contains(&legacy) {
        ids.push(legacy);
    }
    ids
}

fn chats_from_updates(updates: &[Value], code: &str) -> Vec<ManualTelegramChat> {
    let expected = format!("/start {code}");
    let mut chats = Vec::new();
    for update in updates.iter().rev() {
        let Some(message) = update.get("message") else {
            continue;
        };
        if message.get("text").and_then(Value::as_str) != Some(expected.as_str()) {
            continue;
        }
        let Some(id) = message.pointer("/chat/id").and_then(Value::as_i64) else {
            continue;
        };
        let id = id.to_string();
        if chats.iter().any(|chat: &ManualTelegramChat| chat.id == id) {
            continue;
        }
        let chat = &message["chat"];
        let display_name = [chat.get("first_name"), chat.get("last_name")]
            .into_iter()
            .filter_map(|v| v.and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(" ");
        let name = chat
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .filter(|value| !value.is_empty())
            .or_else(|| (!display_name.is_empty()).then_some(display_name))
            .or_else(|| {
                chat.get("username")
                    .and_then(Value::as_str)
                    .map(|name| format!("@{name}"))
            })
            .unwrap_or_else(|| id.clone());
        chats.push(ManualTelegramChat { id, name });
    }
    chats
}

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
    match selected_notification(config).as_str() {
        "telegram" => {
            let token = config_str(config, "telegram_token");
            let ids = telegram_chat_ids(config);
            if token.is_empty() || ids.is_empty() {
                return Err(GatewayError::config(
                    "Telegram bot or recipients not configured",
                ));
            }
            let url = Url::parse(&format!("https://api.telegram.org/bot{token}/sendMessage"))
                .map_err(|_| GatewayError::config("telegram_token invalid"))?;
            let mut failed = Vec::new();
            for chat_id in ids {
                if post_notice(
                url.clone(),
                serde_json::json!({"chat_id": chat_id, "text": text, "disable_web_page_preview": true}),
            ).await.is_err() {
                failed.push(chat_id);
            }
            }
            if failed.is_empty() {
                Ok(())
            } else {
                Err(GatewayError::request(format!(
                    "Telegram delivery failed for {} recipient(s)",
                    failed.len()
                )))
            }
        }
        "wecom" => {
            let wecom = config_str(config, "wecom_webhook_url");
            if wecom.is_empty() {
                return Err(GatewayError::config("WeCom webhook not configured"));
            }
            let url = official_webhook(&wecom, "qyapi.weixin.qq.com", "/cgi-bin/webhook/send")?;
            post_notice(
                url,
                serde_json::json!({"msgtype": "text", "text": {"content": text}}),
            )
            .await
        }
        "dingtalk" => {
            let dingtalk = config_str(config, "dingtalk_webhook_url");
            if dingtalk.is_empty() {
                return Err(GatewayError::config("DingTalk webhook not configured"));
            }
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
            post_notice(
                url,
                serde_json::json!({"msgtype": "text", "text": {"content": text}}),
            )
            .await
        }
        "email" => {
            if config_str(config, "smtp_to").is_empty() {
                return Err(GatewayError::config("email recipient not configured"));
            }
            send_email(config, subject, text).await
        }
        _ => Err(GatewayError::config(
            "no notification destination configured",
        )),
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
        let methods = config
            .get("supported_channel_types")
            .and_then(Value::as_array);
        let selected = if let Some(methods) = methods {
            let methods = methods.iter().filter_map(Value::as_str).collect::<Vec<_>>();
            if methods.is_empty()
                || methods.iter().any(|m| !matches!(*m, "wechat" | "alipay"))
                || !methods.contains(&channel)
            {
                return Err(GatewayError::config("invalid manual payment methods"));
            }
            methods
        } else {
            vec![channel]
        };
        for method in selected {
            let url = qr_image(config, method);
            if url.is_empty() || !url.starts_with('/') && !url.starts_with("https://") {
                return Err(GatewayError::config(format!(
                    "{method} QR image missing or invalid"
                )));
            }
        }
        let notice = selected_notification(config);
        if !notice.is_empty()
            && !matches!(notice.as_str(), "telegram" | "wecom" | "dingtalk" | "email")
        {
            return Err(GatewayError::config("notification type required"));
        }
        match notice.as_str() {
            "telegram" if config_str(config, "telegram_token").is_empty() => {
                return Err(GatewayError::config("telegram_token required"));
            }
            "wecom" if config_str(config, "wecom_webhook_url").is_empty() => {
                return Err(GatewayError::config("wecom_webhook_url required"));
            }
            "dingtalk" if config_str(config, "dingtalk_webhook_url").is_empty() => {
                return Err(GatewayError::config("dingtalk_webhook_url required"));
            }
            "email"
                if config_str(config, "smtp_host").is_empty()
                    || config_str(config, "smtp_from").is_empty()
                    || config_str(config, "smtp_to").is_empty() =>
            {
                return Err(GatewayError::config("SMTP details required"));
            }
            _ => {}
        }
        for (key, host, path) in [
            (
                "wecom_webhook_url",
                "qyapi.weixin.qq.com",
                "/cgi-bin/webhook/send",
            ),
            ("dingtalk_webhook_url", "oapi.dingtalk.com", "/robot/send"),
        ] {
            if (key == "wecom_webhook_url" && notice != "wecom")
                || (key == "dingtalk_webhook_url" && notice != "dingtalk")
            {
                continue;
            }
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
            qr_code_url: qr_image(config, &input.channel_type),
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
    ) -> Result<Vec<ManualTelegramChat>, GatewayError> {
        let response = telegram_call(config, "getUpdates?limit=100").await?;
        let Some(updates) = response.get("result").and_then(Value::as_array) else {
            return Err(GatewayError::response("Telegram updates invalid"));
        };
        Ok(chats_from_updates(updates, code))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn config(value: Value) -> ChannelConfig {
        value.as_object().cloned().unwrap_or_default()
    }

    #[tokio::test]
    async fn selected_method_uses_its_own_qr_and_requires_both_images() {
        let gateway = ManualQrGateway;
        let mut cfg = config(json!({
            "supported_channel_types": ["alipay", "wechat"],
            "alipay_qr_image_url": "/uploads/alipay.png",
            "wechat_qr_image_url": "/uploads/wechat.png",
            "notification_type": "wecom",
            "wecom_webhook_url": "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=test"
        }));
        let alipay = gateway
            .create_payment(
                &cfg,
                &GatewayCreateInput {
                    channel_type: "alipay".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        let wechat = gateway
            .create_payment(
                &cfg,
                &GatewayCreateInput {
                    channel_type: "wechat".into(),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(alipay.qr_code_url, "/uploads/alipay.png");
        assert_eq!(wechat.qr_code_url, "/uploads/wechat.png");
        cfg.remove("wechat_qr_image_url");
        assert!(gateway.validate_config(&cfg, "alipay").is_err());
    }

    #[test]
    fn telegram_binding_collects_multiple_named_recipients_once() {
        let updates = json!([
            {"message": {"text": "/start abc", "chat": {"id": 101, "first_name": "Alice", "last_name": "A"}}},
            {"message": {"text": "/start abc", "chat": {"id": 202, "title": "Ops group"}}},
            {"message": {"text": "/start abc", "chat": {"id": 101, "first_name": "Alice", "last_name": "A"}}},
            {"message": {"text": "/start wrong", "chat": {"id": 303, "first_name": "Other"}}}
        ]);
        let chats = chats_from_updates(updates.as_array().unwrap(), "abc");
        assert_eq!(chats.len(), 2);
        assert_eq!(
            chats[0],
            ManualTelegramChat {
                id: "101".into(),
                name: "Alice A".into()
            }
        );
        assert_eq!(
            chats[1],
            ManualTelegramChat {
                id: "202".into(),
                name: "Ops group".into()
            }
        );
    }

    #[test]
    fn only_selected_notification_mode_is_used() {
        let cfg = config(
            json!({"notification_type":"email", "telegram_token":"old", "smtp_to":"ops@example.test"}),
        );
        assert_eq!(selected_notification(&cfg), "email");
    }
}
