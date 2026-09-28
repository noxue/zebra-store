//! `bot:notify`: pushes order / top-up events to the Telegram bot's callback
//! endpoint (port of `consumer_bot.go`).

use std::sync::Arc;

use zs_domain::notify::channel::{
    BOT_NOTIFY_MAX_RETRY, BotNotifyPayload, CHANNEL_TYPE_TELEGRAM_BOT, bot_request, bot_request_url,
};
use zs_domain::notify::ports::CallbackPoster;
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::{Error, ErrorKind, Result};
use zs_shared::clock::Clock;
use zs_shared::sign;

use super::clients::ChannelClientService;

/// Bot callback service (cheap to clone).
#[derive(Clone)]
pub struct BotNotifyService {
    clients: ChannelClientService,
    poster: Arc<dyn CallbackPoster>,
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for BotNotifyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BotNotifyService")
    }
}

impl BotNotifyService {
    pub fn new(
        clients: ChannelClientService,
        poster: Arc<dyn CallbackPoster>,
        queue: Arc<dyn JobQueue>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            clients,
            poster,
            queue,
            clock,
        }
    }

    /// Queues a `bot:notify` job (up to 3 attempts) for other modules.
    pub async fn enqueue(&self, payload: &BotNotifyPayload) -> Result<()> {
        let job = NewJob::new(kinds::BOT_NOTIFY, payload)?.attempts(BOT_NOTIFY_MAX_RETRY);
        self.queue.enqueue(job).await
    }

    /// Handles a job: 2xx and 4xx finish it, transport errors and 5xx retry.
    pub async fn handle(&self, payload: &BotNotifyPayload) -> Result<()> {
        let Some(request) = bot_request(payload) else {
            tracing::debug!(event_type = %payload.event_type, "bot notify skipped: invalid or unknown event");
            return Ok(());
        };
        let Some(endpoint) = self
            .clients
            .active_endpoint(CHANNEL_TYPE_TELEGRAM_BOT)
            .await?
        else {
            tracing::debug!("bot notify skipped: no active telegram bot client");
            return Ok(());
        };
        let Some(url) = bot_request_url(&endpoint.callback_url, request.path) else {
            tracing::warn!("bot notify skipped: invalid callback url");
            return Ok(());
        };
        let body = serde_json::to_vec(&request.body)?;
        let timestamp = self.clock.now().timestamp();
        let signature = sign::sign(
            &endpoint.channel_secret,
            "POST",
            request.path,
            timestamp,
            &body,
        );
        let headers = vec![
            (
                sign::HEADER_CHANNEL_KEY.to_owned(),
                endpoint.channel_key.clone(),
            ),
            (
                sign::HEADER_CHANNEL_TIMESTAMP.to_owned(),
                timestamp.to_string(),
            ),
            (sign::HEADER_CHANNEL_SIGNATURE.to_owned(), signature),
        ];
        match self.poster.post_json(&url, &headers, &body).await {
            Ok(status) if (200..300).contains(&status) => Ok(()),
            Ok(status) if (400..500).contains(&status) => {
                tracing::warn!(
                    status,
                    order_id = payload.order_id,
                    "bot notify client error (not retried)"
                );
                Ok(())
            }
            Ok(status) => Err(Error::internal_msg(format!(
                "bot notify unexpected status: {status}"
            ))),
            Err(e) if e.kind() == ErrorKind::Forbidden => {
                tracing::warn!(error = %e, "bot notify refused by the outbound address policy");
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::Utc;
    use zs_domain::notify::channel::bot_events;

    use super::*;
    use crate::notify::clients::{CreateClient, tests::service};

    type Call = (String, Vec<(String, String)>, Vec<u8>);

    struct Poster {
        status: u16,
        calls: Mutex<Vec<Call>>,
    }

    #[async_trait]
    impl CallbackPoster for Poster {
        async fn post_json(
            &self,
            url: &str,
            headers: &[(String, String)],
            body: &[u8],
        ) -> Result<u16> {
            self.calls
                .lock()
                .unwrap()
                .push((url.into(), headers.to_vec(), body.to_vec()));
            Ok(self.status)
        }
    }

    struct NoQueue;
    #[async_trait]
    impl JobQueue for NoQueue {
        async fn enqueue(&self, _job: NewJob) -> Result<()> {
            Ok(())
        }
    }

    async fn setup(status: u16, callback: &str) -> (BotNotifyService, Arc<Poster>, String) {
        let now = Utc::now();
        let (clients, _) = service(now);
        let d = clients
            .create(CreateClient {
                name: "bot".into(),
                channel_type: "telegram_bot".into(),
                callback_url: callback.into(),
                ..CreateClient::default()
            })
            .await
            .unwrap();
        let poster = Arc::new(Poster {
            status,
            calls: Mutex::new(vec![]),
        });
        let svc = BotNotifyService::new(
            clients,
            poster.clone(),
            Arc::new(NoQueue),
            Arc::new(zs_shared::clock::FixedClock(now)),
        );
        (svc, poster, d.channel_secret)
    }

    fn recharge() -> BotNotifyPayload {
        BotNotifyPayload {
            event_type: bot_events::WALLET_RECHARGE_SUCCEEDED.into(),
            telegram_user_id: "42".into(),
            recharge_no: "RC1".into(),
            amount: "10.00".into(),
            currency: "CNY".into(),
            ..BotNotifyPayload::default()
        }
    }

    // NTF-06 (2): recharge events go to the rebuilt, signed wallet path.
    #[tokio::test]
    async fn ntf06_recharge_event_posts_signed_request() {
        let (svc, poster, secret) = setup(200, "https://bot.x/cb?a=1").await;
        svc.handle(&recharge()).await.unwrap();
        let calls = poster.calls.lock().unwrap();
        let (url, headers, body) = &calls[0];
        assert_eq!(url, "https://bot.x/internal/wallet-recharge-succeeded");
        let header = |k: &str| {
            headers
                .iter()
                .find(|(n, _)| n == k)
                .map(|(_, v)| v.clone())
                .unwrap()
        };
        let ts: i64 = header("Dujiao-Next-Channel-Timestamp").parse().unwrap();
        assert!(sign::verify(
            &secret,
            "POST",
            "/internal/wallet-recharge-succeeded",
            &header("Dujiao-Next-Channel-Signature"),
            ts,
            body
        ));
        let json: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(json["recharge_no"], "RC1");
    }

    #[tokio::test]
    async fn status_classes_decide_retry() {
        let (svc, _, _) = setup(404, "https://bot.x/cb").await;
        assert!(svc.handle(&recharge()).await.is_ok(), "4xx is not retried");
        let (svc, _, _) = setup(502, "https://bot.x/cb").await;
        assert!(svc.handle(&recharge()).await.is_err(), "5xx is retried");
        let (svc, poster, _) = setup(200, "").await;
        svc.handle(&recharge()).await.unwrap();
        assert!(
            poster.calls.lock().unwrap().is_empty(),
            "no callback url configured"
        );
        let unknown = BotNotifyPayload {
            event_type: "mystery".into(),
            ..recharge()
        };
        let (svc, poster, _) = setup(200, "https://bot.x/cb").await;
        svc.handle(&unknown).await.unwrap();
        assert!(poster.calls.lock().unwrap().is_empty());
    }
}
