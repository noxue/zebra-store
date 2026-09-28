//! Callbacks to downstream shops (`downstream:callback`, UPS-01/UPS-12).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zs_domain::integration::credential::CredentialRepo;
use zs_domain::integration::downstream::{
    CallbackOrders, CallbackSender, CallbackStatus, OrderRefRepo, build_payload, next_retry_delay,
};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;

/// Payload of `downstream:callback`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CallbackJob {
    pub ref_id: Id,
}

/// Downstream callback use cases.
#[derive(Clone)]
pub struct DownstreamService {
    refs: Arc<dyn OrderRefRepo>,
    orders: Arc<dyn CallbackOrders>,
    credentials: Arc<dyn CredentialRepo>,
    sender: Arc<dyn CallbackSender>,
    cipher: Cipher,
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for DownstreamService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DownstreamService")
    }
}

impl DownstreamService {
    pub fn new(
        refs: Arc<dyn OrderRefRepo>,
        orders: Arc<dyn CallbackOrders>,
        credentials: Arc<dyn CredentialRepo>,
        sender: Arc<dyn CallbackSender>,
        cipher: Cipher,
        queue: Arc<dyn JobQueue>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            refs,
            orders,
            credentials,
            sender,
            cipher,
            queue,
            clock,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    async fn enqueue(&self, ref_id: Id, at: Option<DateTime<Utc>>) -> Result<()> {
        let mut job = NewJob::new(kinds::DOWNSTREAM_CALLBACK, CallbackJob { ref_id })?.attempts(1);
        if let Some(at) = at {
            job = job.at(at);
        }
        self.queue.enqueue(job).await
    }

    /// Queues a callback for an order state change. Child orders resolve the parent's
    /// reference; a previously sent/failed reference is reset to pending with a zero
    /// retry count so every change is delivered (UPS-12). Best effort (logged).
    pub async fn enqueue_for_order(&self, order_id: Id) {
        if let Err(error) = self.try_enqueue_for_order(order_id).await {
            tracing::warn!(%error, order_id, "enqueue downstream callback failed");
        }
    }

    async fn try_enqueue_for_order(&self, order_id: Id) -> Result<()> {
        let mut found = self.refs.get_by_order(order_id).await?;
        if found.is_none()
            && let Some(parent) = self
                .orders
                .snapshot(order_id)
                .await?
                .and_then(|o| o.parent_id)
        {
            found = self.refs.get_by_order(parent).await?;
        }
        let Some(r) = found else {
            return Ok(());
        };
        if r.callback_url.trim().is_empty() {
            return Ok(());
        }
        if r.callback_status != CallbackStatus::Pending || r.callback_retry_count != 0 {
            self.refs
                .update_delivery(
                    r.id,
                    CallbackStatus::Pending,
                    0,
                    r.last_callback_at,
                    self.now(),
                )
                .await?;
        }
        self.enqueue(r.id, None).await
    }

    /// `downstream:callback`: sends the current order state; failures are retried with
    /// the original delays up to five times, then the reference is marked failed.
    pub async fn send(&self, ref_id: Id) -> Result<()> {
        let Some(r) = self.refs.get(ref_id).await? else {
            return Ok(());
        };
        if r.callback_url.trim().is_empty() {
            return Ok(());
        }
        let order = self
            .orders
            .snapshot(r.order_id)
            .await?
            .ok_or_else(|| Error::internal_msg(format!("order {} not found", r.order_id)))?;
        let credential = self
            .credentials
            .get(r.api_credential_id)
            .await?
            .ok_or_else(|| Error::internal_msg(format!("credential not found for ref {}", r.id)))?;
        let secret = if credential.api_secret.is_empty() {
            String::new()
        } else {
            self.cipher
                .decrypt(&credential.api_secret)
                .map_err(Error::internal)?
        };
        let now = self.now();
        let payload = build_payload(&order, &r, now);
        match self
            .sender
            .send(&r.callback_url, &credential.api_key, &secret, &payload)
            .await
        {
            Ok(()) => {
                self.refs
                    .update_delivery(
                        r.id,
                        CallbackStatus::Sent,
                        r.callback_retry_count,
                        Some(now),
                        now,
                    )
                    .await
            }
            Err(error) => {
                tracing::warn!(%error, ref_id = r.id, "downstream callback failed");
                let retries = r.callback_retry_count + 1;
                let status = match next_retry_delay(retries) {
                    Some(delay) => {
                        self.enqueue(r.id, Some(now + delay)).await?;
                        r.callback_status
                    }
                    None => CallbackStatus::Failed,
                };
                self.refs
                    .update_delivery(r.id, status, retries, Some(now), now)
                    .await
            }
        }
    }
}
