//! Integration adapters for legacy, Zebra Store,
//! `acg-faka`, `mcy-shop`) and
//! their registry, SSRF-safe HTTP, signed downstream callbacks / zebra-store events,
//! image storage for imported products.

pub mod acg_faka;
pub mod client;
pub mod http;
pub mod mcy_openapi;
pub(crate) mod php_form;
pub mod registry;
pub mod zebra_store;

use std::time::Duration;

use async_trait::async_trait;
use zs_app::content::media::{MediaService, UploadService};
use zs_domain::integration::downstream::{CALLBACK_SIGN_PATH, CallbackSender};
use zs_domain::integration::mapping::ImageStore;
use zs_domain::integration::protocol::CallbackPayload;
use zs_domain::notify::ports::KEY_FORBIDDEN_ADDRESS;
use zs_domain::{Error, Result};
use zs_shared::sign;

use http::{AddressPolicy, build_client, check_url, is_forbidden, read_limited};

/// Whole-request timeout of downstream callbacks (original 15 s).
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(15);
/// Bytes of a callback answer read (original `LimitReader` 4096).
const CALLBACK_RESPONSE_LIMIT: usize = 4096;
/// Media scene recorded for imported images (original `upstream`).
const MEDIA_SCENE: &str = "upstream";

/// Signed, SSRF-safe downstream callback delivery (UPS-01): `Ok` only for HTTP 200
/// with `{"ok": true}`; redirects are reported, never followed.
#[derive(Clone)]
pub struct HttpCallbackSender {
    http: reqwest::Client,
    policy: AddressPolicy,
}

impl std::fmt::Debug for HttpCallbackSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpCallbackSender")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl HttpCallbackSender {
    pub fn new(policy: AddressPolicy) -> Self {
        Self {
            http: build_client(policy, CALLBACK_TIMEOUT),
            policy,
        }
    }
}

#[async_trait]
impl CallbackSender for HttpCallbackSender {
    async fn send(
        &self,
        url: &str,
        api_key: &str,
        api_secret: &str,
        payload: &CallbackPayload,
    ) -> Result<()> {
        let url = check_url(self.policy, url)
            .map_err(|d| Error::forbidden(KEY_FORBIDDEN_ADDRESS).arg(d))?;
        let body = serde_json::to_vec(payload)?;
        let signature = sign::sign(
            api_secret,
            "POST",
            CALLBACK_SIGN_PATH,
            payload.timestamp,
            &body,
        );
        let res = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .header(sign::HEADER_API_KEY, api_key)
            .header(sign::HEADER_TIMESTAMP, payload.timestamp.to_string())
            .header(sign::HEADER_SIGNATURE, signature)
            .body(body)
            .send()
            .await
            .map_err(|e| {
                if is_forbidden(&e) {
                    Error::forbidden(KEY_FORBIDDEN_ADDRESS).arg("resolved to a non-public address")
                } else {
                    Error::internal(e.without_url())
                }
            })?;
        let status = res.status().as_u16();
        let text = read_limited(res, CALLBACK_RESPONSE_LIMIT)
            .await
            .unwrap_or_default();
        if status == 200
            && serde_json::from_slice::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("ok").and_then(serde_json::Value::as_bool))
                == Some(true)
        {
            return Ok(());
        }
        Err(Error::internal_msg(format!(
            "callback returned {status}: {}",
            String::from_utf8_lossy(&text).trim()
        )))
    }
}

/// Stores supplier images through the upload validation and records them as media.
#[derive(Clone)]
pub struct UploadImageStore {
    upload: UploadService,
    media: MediaService,
}

impl std::fmt::Debug for UploadImageStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UploadImageStore")
    }
}

impl UploadImageStore {
    pub fn new(upload: UploadService, media: MediaService) -> Self {
        Self { upload, media }
    }
}

#[async_trait]
impl ImageStore for UploadImageStore {
    async fn store(&self, filename: &str, bytes: &[u8]) -> Result<String> {
        let name = if filename.contains('.') {
            filename.to_owned()
        } else {
            format!("{filename}.jpg")
        };
        let stored = self.upload.save(&name, bytes, "common").await?;
        if let Err(error) = self.media.record(&stored, MEDIA_SCENE).await {
            tracing::warn!(%error, url = stored.url, "record upstream media failed");
        }
        Ok(stored.url)
    }
}
