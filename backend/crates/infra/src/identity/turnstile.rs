//! Cloudflare Turnstile siteverify client.

use std::time::Duration;

use async_trait::async_trait;
use serde::Deserialize;
use zs_domain::Result;
use zs_domain::identity::captcha::{self, TurnstileSetting, TurnstileVerifier};

/// Timeout used when the configured one is outside 500..=10000 ms.
const DEFAULT_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, Deserialize)]
struct VerifyResponse {
    success: bool,
}

/// Verifies tokens against `turnstile.verify_url`.
#[derive(Debug, Clone, Default)]
pub struct TurnstileClient {
    http: reqwest::Client,
}

impl TurnstileClient {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait]
impl TurnstileVerifier for TurnstileClient {
    async fn verify(&self, s: &TurnstileSetting, token: &str, client_ip: &str) -> Result<()> {
        let secret = s.secret_key.trim();
        let url = s.verify_url.trim();
        if secret.is_empty() || url.is_empty() {
            return Err(captcha::config_invalid());
        }
        let timeout = u64::try_from(s.timeout_ms)
            .ok()
            .filter(|t| (500..=10_000).contains(t))
            .unwrap_or(DEFAULT_TIMEOUT_MS);
        let mut form = vec![("secret", secret), ("response", token)];
        if !client_ip.is_empty() {
            form.push(("remoteip", client_ip));
        }
        let res = self
            .http
            .post(url)
            .timeout(Duration::from_millis(timeout))
            .form(&form)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(error = %e, "turnstile request failed");
                captcha::verify_failed()
            })?;
        let body: VerifyResponse = res.json().await.map_err(|e| {
            tracing::warn!(error = %e, "turnstile response invalid");
            captcha::verify_failed()
        })?;
        if body.success {
            Ok(())
        } else {
            Err(captcha::invalid())
        }
    }
}
