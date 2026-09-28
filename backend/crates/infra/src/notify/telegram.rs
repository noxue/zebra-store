//! Telegram Bot API client (port of `telegram/notify/infrastructure/botapi`).

use std::path::PathBuf;
use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zs_domain::notify::broadcast::{is_photo_attachment, local_attachment_path};
use zs_domain::notify::ports::{KEY_SEND_FAILED, TelegramMessage, TelegramSender};
use zs_domain::{Error, Result};

/// Official Bot API endpoint.
pub const TELEGRAM_API_BASE: &str = "https://api.telegram.org";
/// Request timeout (original 6 s).
const TIMEOUT: Duration = Duration::from_secs(6);

fn failed(detail: impl Into<String>) -> Error {
    Error::internal_msg(detail.into()).or_internal(KEY_SEND_FAILED)
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    ok: bool,
    #[serde(default)]
    description: String,
}

/// Bot API client; `base` is injectable for tests.
#[derive(Debug, Clone)]
pub struct BotApi {
    http: reqwest::Client,
    base: String,
    /// Directory holding `uploads/...` files (the configured upload dir).
    upload_dir: PathBuf,
}

impl BotApi {
    pub fn new(base: &str, upload_dir: PathBuf) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(Error::internal)?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            upload_dir,
        })
    }

    fn url(&self, token: &str, method: &str) -> String {
        format!("{}/bot{token}/{method}", self.base)
    }

    async fn finish(res: std::result::Result<reqwest::Response, reqwest::Error>) -> Result<()> {
        // `without_url` keeps the bot token out of error messages and logs.
        let res = res.map_err(|e| failed(e.without_url().to_string()))?;
        let status = res.status();
        let body = res
            .text()
            .await
            .map_err(|e| failed(e.without_url().to_string()))?;
        if !status.is_success() {
            return Err(failed(format!(
                "telegram status={} body={}",
                status.as_u16(),
                body.trim()
            )));
        }
        let parsed: ApiResponse =
            serde_json::from_str(&body).map_err(|_| failed("parse telegram response failed"))?;
        if !parsed.ok {
            return Err(failed(parsed.description.trim().to_owned()));
        }
        Ok(())
    }

    async fn send_json(&self, token: &str, method: &str, payload: Value) -> Result<()> {
        Self::finish(
            self.http
                .post(self.url(token, method))
                .json(&payload)
                .send()
                .await,
        )
        .await
    }

    async fn send_file(
        &self,
        token: &str,
        method: &str,
        field: &str,
        rel: &str,
        m: &TelegramMessage,
    ) -> Result<()> {
        let inside = rel.strip_prefix("uploads/").unwrap_or(rel);
        let path = self.upload_dir.join(inside);
        let bytes = tokio::fs::read(&path)
            .await
            .map_err(|e| failed(format!("open attachment failed: {e}")))?;
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "attachment".into());
        let mut form = reqwest::multipart::Form::new().text("chat_id", m.chat_id.trim().to_owned());
        if !m.text.trim().is_empty() {
            form = form.text("caption", m.text.trim().to_owned());
        }
        if !m.parse_mode.trim().is_empty() {
            form = form.text("parse_mode", m.parse_mode.trim().to_owned());
        }
        form = form.part(
            field.to_owned(),
            reqwest::multipart::Part::bytes(bytes).file_name(file_name),
        );
        Self::finish(
            self.http
                .post(self.url(token, method))
                .multipart(form)
                .send()
                .await,
        )
        .await
    }
}

#[async_trait]
impl TelegramSender for BotApi {
    async fn send(&self, token: &str, m: &TelegramMessage) -> Result<()> {
        let (token, chat_id, text) = (token.trim(), m.chat_id.trim(), m.text.trim());
        if token.is_empty() || chat_id.is_empty() || text.is_empty() {
            return Err(failed("telegram message incomplete"));
        }
        let parse_mode = m.parse_mode.trim();
        let attachment = m.attachment_url.trim();
        if !attachment.is_empty() {
            // NTF-06: images go through sendPhoto so clients show a preview.
            let (method, field) = if is_photo_attachment(attachment, &m.attachment_name) {
                ("sendPhoto", "photo")
            } else {
                ("sendDocument", "document")
            };
            if let Some(rel) = local_attachment_path(attachment) {
                return self.send_file(token, method, field, &rel, m).await;
            }
            let mut payload = Map::new();
            payload.insert("chat_id".into(), chat_id.into());
            payload.insert(field.into(), attachment.into());
            payload.insert("caption".into(), text.into());
            if !parse_mode.is_empty() {
                payload.insert("parse_mode".into(), parse_mode.into());
            }
            return self.send_json(token, method, Value::Object(payload)).await;
        }
        let mut payload = json!({
            "chat_id": chat_id,
            "text": text,
            "disable_web_page_preview": m.disable_web_page_preview,
        });
        if !parse_mode.is_empty() {
            payload["parse_mode"] = parse_mode.into();
        }
        self.send_json(token, "sendMessage", payload).await
    }
}
