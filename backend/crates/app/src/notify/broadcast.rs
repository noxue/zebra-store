//! Telegram broadcasts (port of `modules/telegram/broadcast/application`).

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Map, Value};
use zs_domain::notify::broadcast::{
    Broadcast, BroadcastFilter, BroadcastRepo, MSG_NO_RECIPIENTS, MSG_TOKEN_UNAVAILABLE,
    TelegramUser, TelegramUserQuery, dedupe_strings, recipient_types, statuses, unique_ids,
};
use zs_domain::notify::ports::{TelegramMessage, TelegramSender};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::Page;

use super::clients::ChannelClientService;

/// Pause between two messages: keeps a broadcast under Telegram's ~30 messages
/// per second bot limit.
pub const SEND_INTERVAL: Duration = Duration::from_millis(40);

/// Payload of the `telegram:broadcast` job.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct BroadcastJob {
    pub broadcast_id: Id,
}

/// Broadcast creation request (original `CreateInput`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CreateBroadcast {
    pub title: String,
    pub recipient_type: String,
    pub user_ids: Vec<Id>,
    pub filters: Map<String, Value>,
    pub attachment_url: String,
    pub attachment_name: String,
    pub message_html: String,
}

/// Broadcast service (cheap to clone).
#[derive(Clone)]
pub struct BroadcastService {
    repo: Arc<dyn BroadcastRepo>,
    clients: ChannelClientService,
    telegram: Arc<dyn TelegramSender>,
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
    interval: Duration,
}

impl std::fmt::Debug for BroadcastService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BroadcastService")
    }
}

fn not_found() -> Error {
    Error::not_found("error.not_found")
}

impl BroadcastService {
    pub fn new(
        repo: Arc<dyn BroadcastRepo>,
        clients: ChannelClientService,
        telegram: Arc<dyn TelegramSender>,
        queue: Arc<dyn JobQueue>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            clients,
            telegram,
            queue,
            clock,
            interval: SEND_INTERVAL,
        }
    }

    /// Overrides the pause between messages (tests).
    #[must_use]
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    pub async fn list(&self, filter: &BroadcastFilter) -> Result<Page<Broadcast>> {
        self.repo
            .list(filter)
            .await
            .map_err(|e| e.or_internal("error.bad_request"))
    }

    pub async fn get(&self, id: Id) -> Result<Broadcast> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal("error.bad_request"))?
            .ok_or_else(not_found)
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        if self.repo.soft_delete(id, self.clock.now()).await? {
            Ok(())
        } else {
            Err(not_found())
        }
    }

    pub async fn users(&self, query: &TelegramUserQuery) -> Result<Page<TelegramUser>> {
        self.repo
            .telegram_users(query)
            .await
            .map_err(|e| e.or_internal("error.bad_request"))
    }

    /// Validates, snapshots the recipients and queues the broadcast.
    pub async fn create(&self, input: CreateBroadcast) -> Result<Broadcast> {
        let title = input.title.trim().to_owned();
        let message_html = input.message_html.trim().to_owned();
        let recipient_type = input.recipient_type.trim().to_lowercase();
        if title.is_empty()
            || message_html.is_empty()
            || (recipient_type != recipient_types::ALL
                && recipient_type != recipient_types::SPECIFIC)
        {
            return Err(Error::invalid());
        }
        if self.clients.active_bot_token().await?.is_none() {
            tracing::info!("{MSG_TOKEN_UNAVAILABLE}");
            return Err(Error::invalid());
        }
        let mut filters = input.filters;
        let mut query = TelegramUserQuery::default();
        if recipient_type == recipient_types::SPECIFIC {
            let ids = unique_ids(&input.user_ids);
            if ids.is_empty() {
                return Err(Error::invalid());
            }
            filters.insert("selected_user_ids".into(), ids.clone().into());
            query.user_ids = ids;
        }
        let users = self.repo.telegram_users(&query).await?;
        let chat_ids = dedupe_strings(users.items.into_iter().map(|u| u.telegram_user_id));
        if chat_ids.is_empty() {
            tracing::info!("{MSG_NO_RECIPIENTS}");
            return Err(Error::invalid());
        }
        let now = self.clock.now();
        let draft = Broadcast {
            id: 0,
            title,
            recipient_type,
            filters: Value::Object(filters),
            recipient_count: i32::try_from(chat_ids.len()).unwrap_or(i32::MAX),
            recipient_chat_ids: chat_ids,
            success_count: 0,
            failed_count: 0,
            status: statuses::PENDING.into(),
            message_html,
            attachment_url: input.attachment_url.trim().to_owned(),
            attachment_name: input.attachment_name.trim().to_owned(),
            started_at: None,
            completed_at: None,
            last_error: String::new(),
            created_at: now,
            updated_at: now,
        };
        let created = self.repo.create(&draft).await?;
        let job = NewJob::new(
            kinds::TELEGRAM_BROADCAST,
            BroadcastJob {
                broadcast_id: created.id,
            },
        )?
        .attempts(1);
        self.queue.enqueue(job).await?;
        Ok(created)
    }

    async fn fail(&self, mut b: Broadcast, reason: &str) -> Result<()> {
        let now = self.clock.now();
        b.status = statuses::FAILED.into();
        b.completed_at = Some(now);
        b.last_error = reason.trim().to_owned();
        b.updated_at = now;
        self.repo.update(&b).await
    }

    /// Delivers a queued broadcast (job `telegram:broadcast`).
    pub async fn process(&self, id: Id) -> Result<()> {
        let Some(mut b) = self.repo.get(id).await? else {
            return Ok(());
        };
        if b.status == statuses::COMPLETED {
            return Ok(());
        }
        let now = self.clock.now();
        b.status = statuses::RUNNING.into();
        b.started_at.get_or_insert(now);
        b.completed_at = None;
        b.last_error.clear();
        b.updated_at = now;
        self.repo.update(&b).await?;

        let token = match self.clients.active_bot_token().await {
            Ok(Some(t)) => t,
            Ok(None) => return self.fail(b, MSG_TOKEN_UNAVAILABLE).await,
            Err(e) => return self.fail(b, &e.to_string()).await,
        };
        let chat_ids = dedupe_strings(b.recipient_chat_ids.clone());
        if chat_ids.is_empty() {
            return self.fail(b, MSG_NO_RECIPIENTS).await;
        }
        let (mut success, mut failed) = (0i32, 0i32);
        let mut last_error = String::new();
        for (idx, chat_id) in chat_ids.iter().enumerate() {
            if idx > 0 && !self.interval.is_zero() {
                tokio::time::sleep(self.interval).await;
            }
            let message = TelegramMessage {
                chat_id: chat_id.clone(),
                text: b.message_html.clone(),
                parse_mode: "HTML".into(),
                disable_web_page_preview: true,
                attachment_url: b.attachment_url.clone(),
                attachment_name: b.attachment_name.clone(),
            };
            match self.telegram.send(&token, &message).await {
                Ok(()) => success += 1,
                Err(e) => {
                    failed += 1;
                    last_error = e.to_string();
                }
            }
        }
        let now = self.clock.now();
        b.success_count = success;
        b.failed_count = failed;
        b.completed_at = Some(now);
        b.last_error = last_error;
        b.updated_at = now;
        b.status = if success == 0 && failed > 0 {
            statuses::FAILED
        } else {
            statuses::COMPLETED
        }
        .into();
        self.repo.update(&b).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};

    use super::*;
    use crate::notify::clients::CreateClient;

    #[derive(Default)]
    struct Repo {
        rows: Mutex<Vec<Broadcast>>,
        users: Vec<TelegramUser>,
    }

    #[async_trait]
    impl BroadcastRepo for Repo {
        async fn create(&self, b: &Broadcast) -> Result<Broadcast> {
            let mut rows = self.rows.lock().unwrap();
            let mut b = b.clone();
            b.id = i64::try_from(rows.len()).unwrap() + 1;
            rows.push(b.clone());
            Ok(b)
        }
        async fn get(&self, id: Id) -> Result<Option<Broadcast>> {
            Ok(self
                .rows
                .lock()
                .unwrap()
                .iter()
                .find(|b| b.id == id)
                .cloned())
        }
        async fn list(&self, _f: &BroadcastFilter) -> Result<Page<Broadcast>> {
            let items = self.rows.lock().unwrap().clone();
            Ok(Page {
                total: items.len() as u64,
                items,
            })
        }
        async fn update(&self, b: &Broadcast) -> Result<()> {
            let mut rows = self.rows.lock().unwrap();
            if let Some(slot) = rows.iter_mut().find(|x| x.id == b.id) {
                *slot = b.clone();
            }
            Ok(())
        }
        async fn soft_delete(&self, id: Id, _at: DateTime<Utc>) -> Result<bool> {
            let mut rows = self.rows.lock().unwrap();
            let before = rows.len();
            rows.retain(|b| b.id != id);
            Ok(rows.len() != before)
        }
        async fn telegram_users(&self, q: &TelegramUserQuery) -> Result<Page<TelegramUser>> {
            let items: Vec<TelegramUser> = self
                .users
                .iter()
                .filter(|u| q.user_ids.is_empty() || q.user_ids.contains(&u.user_id))
                .cloned()
                .collect();
            Ok(Page {
                total: items.len() as u64,
                items,
            })
        }
    }

    #[derive(Default)]
    struct Tg(Mutex<Vec<TelegramMessage>>);
    #[async_trait]
    impl TelegramSender for Tg {
        async fn send(&self, _t: &str, m: &TelegramMessage) -> Result<()> {
            self.0.lock().unwrap().push(m.clone());
            if m.chat_id == "666" {
                Err(Error::internal_msg("blocked by user"))
            } else {
                Ok(())
            }
        }
    }

    #[derive(Default)]
    struct Q(Mutex<Vec<NewJob>>);
    #[async_trait]
    impl JobQueue for Q {
        async fn enqueue(&self, job: NewJob) -> Result<()> {
            self.0.lock().unwrap().push(job);
            Ok(())
        }
    }

    fn user(id: Id, chat: &str) -> TelegramUser {
        TelegramUser {
            user_id: id,
            display_name: format!("u{id}"),
            user_email: String::new(),
            telegram_username: String::new(),
            telegram_user_id: chat.into(),
            bound_at: Utc::now(),
            user_created_at: Utc::now(),
        }
    }

    async fn setup(with_token: bool) -> (BroadcastService, Arc<Repo>, Arc<Tg>, Arc<Q>) {
        let (clients, _) = crate::notify::clients::tests::service(Utc::now());
        if with_token {
            clients
                .create(CreateClient {
                    name: "bot".into(),
                    channel_type: "telegram_bot".into(),
                    bot_token: "1:TOKEN".into(),
                    ..CreateClient::default()
                })
                .await
                .unwrap();
        }
        let repo = Arc::new(Repo {
            users: vec![user(1, "111"), user(2, "666"), user(3, "111")],
            ..Repo::default()
        });
        let tg = Arc::new(Tg::default());
        let q = Arc::new(Q::default());
        let svc = BroadcastService::new(
            repo.clone(),
            clients,
            tg.clone(),
            q.clone(),
            Arc::new(zs_shared::clock::SystemClock),
        )
        .with_interval(Duration::ZERO);
        (svc, repo, tg, q)
    }

    fn input(kind: &str) -> CreateBroadcast {
        CreateBroadcast {
            title: "Hi".into(),
            recipient_type: kind.into(),
            message_html: "<b>hi</b>".into(),
            ..CreateBroadcast::default()
        }
    }

    #[tokio::test]
    async fn create_snapshots_unique_recipients_and_queues() {
        let (svc, _, _, q) = setup(true).await;
        let b = svc.create(input("ALL")).await.unwrap();
        assert_eq!(b.recipient_chat_ids, vec!["111", "666"]);
        assert_eq!(b.recipient_count, 2);
        assert_eq!(b.status, "pending");
        let jobs = q.0.lock().unwrap();
        assert_eq!(jobs[0].kind, kinds::TELEGRAM_BROADCAST);
        assert_eq!(jobs[0].payload["broadcast_id"], b.id);
    }

    #[tokio::test]
    async fn create_validates_input_and_token() {
        let (svc, _, _, _) = setup(false).await;
        assert!(
            svc.create(input("all")).await.is_err(),
            "no active bot token"
        );
        let (svc, _, _, _) = setup(true).await;
        assert!(svc.create(input("group")).await.is_err());
        assert!(
            svc.create(input("specific")).await.is_err(),
            "specific needs user ids"
        );
        let mut i = input("specific");
        i.user_ids = vec![2, 2, 0];
        let b = svc.create(i).await.unwrap();
        assert_eq!(b.recipient_chat_ids, vec!["666"]);
        assert_eq!(b.filters["selected_user_ids"], serde_json::json!([2]));
    }

    #[tokio::test]
    async fn process_counts_successes_and_failures() {
        let (svc, repo, tg, _) = setup(true).await;
        let b = svc.create(input("all")).await.unwrap();
        svc.process(b.id).await.unwrap();
        let done = repo.get(b.id).await.unwrap().unwrap();
        assert_eq!(done.status, "completed");
        assert_eq!((done.success_count, done.failed_count), (1, 1));
        assert!(done.last_error.contains("blocked by user"));
        assert!(done.started_at.is_some() && done.completed_at.is_some());
        assert_eq!(tg.0.lock().unwrap()[0].parse_mode, "HTML");
        // A completed broadcast is never re-sent.
        svc.process(b.id).await.unwrap();
        assert_eq!(tg.0.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn all_failed_marks_failed() {
        let (svc, repo, _, _) = setup(true).await;
        let mut i = input("specific");
        i.user_ids = vec![2];
        let b = svc.create(i).await.unwrap();
        svc.process(b.id).await.unwrap();
        assert_eq!(repo.get(b.id).await.unwrap().unwrap().status, "failed");
        svc.delete(b.id).await.unwrap();
        assert!(svc.get(b.id).await.unwrap_err().is_not_found());
    }
}
