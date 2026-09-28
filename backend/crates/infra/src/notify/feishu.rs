//! Feishu/Lark self-built app bot client: tenant access token + IM text message
//! (replaces the original `larksuite/oapi-sdk-go` usage).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use zs_domain::notify::ports::{FeishuSender, KEY_SEND_FAILED, MSG_CONFIG_INVALID};
use zs_domain::{Error, Result};

/// Official Open Platform endpoint (original `feishuBaseURL`).
pub const FEISHU_API_BASE: &str = "https://open.feishu.cn";
/// Request timeout (original `feishuRequestTimeout`).
const TIMEOUT: Duration = Duration::from_secs(10);
/// Refresh tokens this long before Feishu expires them.
const TOKEN_EXPIRY_MARGIN: Duration = Duration::from_secs(60);
/// Receive id types accepted by the message API.
const RECEIVE_ID_TYPES: [&str; 5] = ["chat_id", "open_id", "user_id", "union_id", "email"];
/// Feishu codes meaning the tenant token is invalid or expired.
const TOKEN_INVALID_CODES: [i64; 3] = [99_991_661, 99_991_663, 99_991_668];

fn failed(detail: impl Into<String>) -> Error {
    Error::internal_msg(detail.into()).or_internal(KEY_SEND_FAILED)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    tenant_access_token: String,
    /// Seconds.
    #[serde(default)]
    expire: u64,
}

#[derive(Debug, Deserialize)]
struct ApiResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: String,
}

type TokenCache = HashMap<(String, String), (String, Instant)>;

/// Feishu client with a per-credential tenant token cache; `base` is injectable for tests.
#[derive(Debug, Clone)]
pub struct FeishuClient {
    http: reqwest::Client,
    base: String,
    tokens: Arc<Mutex<TokenCache>>,
}

impl FeishuClient {
    pub fn new(base: &str) -> Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(Error::internal)?;
        Ok(Self {
            http,
            base: base.trim_end_matches('/').to_owned(),
            tokens: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    fn cached(&self, key: &(String, String)) -> Option<String> {
        let tokens = self.tokens.lock().unwrap_or_else(PoisonError::into_inner);
        tokens
            .get(key)
            .filter(|(_, until)| *until > Instant::now())
            .map(|(t, _)| t.clone())
    }

    fn forget(&self, key: &(String, String)) {
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(key);
    }

    async fn token(&self, app_id: &str, app_secret: &str) -> Result<String> {
        let key = (app_id.to_owned(), app_secret.to_owned());
        if let Some(t) = self.cached(&key) {
            return Ok(t);
        }
        let res = self
            .http
            .post(format!(
                "{}/open-apis/auth/v3/tenant_access_token/internal",
                self.base
            ))
            .json(&json!({"app_id": app_id, "app_secret": app_secret}))
            .send()
            .await
            .map_err(|e| failed(format!("feishu token request: {}", e.without_url())))?;
        let body: TokenResponse = res
            .json()
            .await
            .map_err(|e| failed(format!("feishu token response: {}", e.without_url())))?;
        if body.code != 0 || body.tenant_access_token.is_empty() {
            return Err(failed(format!(
                "feishu token code={} msg={}",
                body.code,
                body.msg.trim()
            )));
        }
        let ttl = Duration::from_secs(body.expire).saturating_sub(TOKEN_EXPIRY_MARGIN);
        self.tokens
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(
                key,
                (body.tenant_access_token.clone(), Instant::now() + ttl),
            );
        Ok(body.tenant_access_token)
    }
}

#[async_trait]
impl FeishuSender for FeishuClient {
    async fn send(
        &self,
        app_id: &str,
        app_secret: &str,
        receive_id_type: &str,
        receive_id: &str,
        text: &str,
    ) -> Result<()> {
        let (app_id, app_secret) = (app_id.trim(), app_secret.trim());
        let kind = receive_id_type.trim().to_lowercase();
        let (receive_id, text) = (receive_id.trim(), text.trim());
        if app_id.is_empty()
            || app_secret.is_empty()
            || receive_id.is_empty()
            || text.is_empty()
            || !RECEIVE_ID_TYPES.contains(&kind.as_str())
        {
            return Err(Error::bad_request(MSG_CONFIG_INVALID));
        }
        let token = self.token(app_id, app_secret).await?;
        let content = serde_json::to_string(&json!({"text": text}))?;
        let res = self
            .http
            .post(format!("{}/open-apis/im/v1/messages", self.base))
            .query(&[("receive_id_type", kind.as_str())])
            .bearer_auth(&token)
            .json(&json!({"receive_id": receive_id, "msg_type": "text", "content": content}))
            .send()
            .await
            .map_err(|e| failed(format!("feishu request: {}", e.without_url())))?;
        let status = res.status();
        let body: ApiResponse = res.json().await.map_err(|e| {
            failed(format!(
                "feishu response (status {status}): {}",
                e.without_url()
            ))
        })?;
        if body.code != 0 {
            if TOKEN_INVALID_CODES.contains(&body.code) {
                self.forget(&(app_id.to_owned(), app_secret.to_owned()));
            }
            return Err(failed(format!(
                "feishu api code={} msg={}",
                body.code,
                body.msg.trim()
            )));
        }
        Ok(())
    }
}
