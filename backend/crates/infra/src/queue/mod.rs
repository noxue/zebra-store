//! Database-backed job queue (replaces the original asynq/Redis queue).
//!
//! - [`DbJobQueue`] implements the [`JobQueue`] port (enqueue).
//! - [`Worker`] polls due jobs, claims them with a conditional update, runs the
//!   registered [`JobHandler`] and retries failures with exponential backoff.
//! - Periodic tasks are enqueued by [`Worker`] with a de-duplication key.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};
use tokio::sync::{Semaphore, watch};
use zs_domain::Result;
use zs_domain::queue::{JobHandler, JobQueue, NewJob};

use crate::db::entity::jobs;
use crate::db::repo::support::DbResultExt;

/// Job states stored in `jobs.status`.
mod status {
    pub const PENDING: &str = "pending";
    pub const RUNNING: &str = "running";
    pub const DONE: &str = "done";
    pub const FAILED: &str = "failed";
}

/// How long a claimed job may run before another worker may reclaim it.
const LOCK_TTL: chrono::Duration = chrono::Duration::minutes(10);
/// Base delay of the exponential retry backoff.
const BACKOFF_BASE_SECS: i64 = 10;
/// Upper bound of the retry backoff.
const BACKOFF_MAX_SECS: i64 = 3600;
/// Finished jobs older than this are purged.
const RETENTION: chrono::Duration = chrono::Duration::days(7);
/// Maximum jobs claimed per poll.
const CLAIM_BATCH: u64 = 32;

/// Enqueues jobs into the `jobs` table.
#[derive(Debug, Clone)]
pub struct DbJobQueue {
    db: DatabaseConnection,
}

impl DbJobQueue {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

#[async_trait]
impl JobQueue for DbJobQueue {
    async fn enqueue(&self, job: NewJob) -> Result<()> {
        let unique_key = job.unique_key.unwrap_or_default();
        if !unique_key.is_empty() {
            let existing = jobs::Entity::find()
                .filter(jobs::Column::UniqueKey.eq(&unique_key))
                .filter(jobs::Column::Status.is_in([status::PENDING, status::RUNNING]))
                .one(&self.db)
                .await
                .dom()?;
            if existing.is_some() {
                return Ok(());
            }
        }
        let now = Utc::now();
        jobs::ActiveModel {
            kind: Set(job.kind),
            payload: Set(job.payload.to_string()),
            status: Set(status::PENDING.into()),
            run_at: Set(job.run_at.unwrap_or(now)),
            attempts: Set(0),
            max_attempts: Set(job.max_attempts.max(1)),
            last_error: Set(String::new()),
            unique_key: Set(unique_key),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }
}

/// Handlers keyed by job kind, plus periodic schedules.
#[derive(Default, Clone)]
pub struct JobRegistry {
    handlers: HashMap<String, Arc<dyn JobHandler>>,
    periodic: Vec<(String, Duration)>,
}

impl std::fmt::Debug for JobRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRegistry")
            .field("kinds", &self.handlers.keys().collect::<Vec<_>>())
            .field("periodic", &self.periodic)
            .finish()
    }
}

impl JobRegistry {
    pub fn handle(&mut self, kind: &str, handler: Arc<dyn JobHandler>) -> &mut Self {
        self.handlers.insert(kind.to_owned(), handler);
        self
    }

    /// Enqueues `kind` (empty payload) every `every`.
    pub fn every(&mut self, kind: &str, every: Duration) -> &mut Self {
        self.periodic.push((kind.to_owned(), every));
        self
    }

    pub fn kinds(&self) -> Vec<&str> {
        self.handlers.keys().map(String::as_str).collect()
    }
}

/// Polls and executes jobs until shutdown.
#[derive(Debug)]
pub struct Worker {
    db: DatabaseConnection,
    queue: DbJobQueue,
    registry: Arc<JobRegistry>,
    concurrency: usize,
    poll: Duration,
}

impl Worker {
    pub fn new(
        db: DatabaseConnection,
        registry: JobRegistry,
        concurrency: usize,
        poll: Duration,
    ) -> Self {
        Self {
            queue: DbJobQueue::new(db.clone()),
            db,
            registry: Arc::new(registry),
            concurrency: concurrency.max(1),
            poll,
        }
    }

    /// Runs the poll loop and periodic schedulers until `shutdown` flips to true.
    pub async fn run(self, mut shutdown: watch::Receiver<bool>) {
        for (kind, every) in self.registry.periodic.clone() {
            let queue = self.queue.clone();
            let mut stop = shutdown.clone();
            tokio::spawn(async move {
                let mut tick = tokio::time::interval(every);
                loop {
                    tokio::select! {
                        _ = tick.tick() => {
                            let job = NewJob { kind: kind.clone(), payload: serde_json::json!({}), run_at: None, max_attempts: 1, unique_key: Some(format!("periodic:{kind}")) };
                            if let Err(error) = queue.enqueue(job).await {
                                tracing::warn!(%error, %kind, "failed to enqueue periodic job");
                            }
                        }
                        _ = stop.changed() => break,
                    }
                }
            });
        }
        let permits = Arc::new(Semaphore::new(self.concurrency));
        let mut tick = tokio::time::interval(self.poll);
        let mut last_purge = Utc::now();
        tracing::info!(kinds = ?self.registry.kinds(), "job worker started");
        loop {
            tokio::select! {
                _ = tick.tick() => {}
                _ = shutdown.changed() => break,
            }
            if let Err(error) = self.poll_once(&permits).await {
                tracing::error!(%error, "job poll failed");
            }
            if Utc::now() - last_purge > chrono::Duration::hours(1) {
                last_purge = Utc::now();
                if let Err(error) = self.purge().await {
                    tracing::warn!(%error, "job purge failed");
                }
            }
        }
        tracing::info!("job worker stopped");
    }

    /// Claims and dispatches due jobs once (public for tests).
    pub async fn poll_once(&self, permits: &Arc<Semaphore>) -> Result<usize> {
        let now = Utc::now();
        // Reclaim jobs whose worker died.
        jobs::Entity::update_many()
            .col_expr(jobs::Column::Status, Expr::value(status::PENDING))
            .filter(jobs::Column::Status.eq(status::RUNNING))
            .filter(jobs::Column::LockedUntil.lt(now))
            .exec(&self.db)
            .await
            .dom()?;
        let due = jobs::Entity::find()
            .filter(jobs::Column::Status.eq(status::PENDING))
            .filter(jobs::Column::RunAt.lte(now))
            .order_by_asc(jobs::Column::RunAt)
            .limit(CLAIM_BATCH)
            .all(&self.db)
            .await
            .dom()?;
        let mut started = 0;
        for job in due {
            let claimed = jobs::Entity::update_many()
                .col_expr(jobs::Column::Status, Expr::value(status::RUNNING))
                .col_expr(jobs::Column::LockedUntil, Expr::value(now + LOCK_TTL))
                .col_expr(jobs::Column::UpdatedAt, Expr::value(now))
                .filter(jobs::Column::Id.eq(job.id))
                .filter(jobs::Column::Status.eq(status::PENDING))
                .exec(&self.db)
                .await
                .dom()?;
            if claimed.rows_affected != 1 {
                continue;
            }
            let Ok(permit) = permits.clone().acquire_owned().await else {
                break;
            };
            let db = self.db.clone();
            let handler = self.registry.handlers.get(&job.kind).cloned();
            started += 1;
            tokio::spawn(async move {
                execute(&db, job, handler).await;
                drop(permit);
            });
        }
        Ok(started)
    }

    async fn purge(&self) -> Result<()> {
        jobs::Entity::delete_many()
            .filter(jobs::Column::Status.is_in([status::DONE, status::FAILED]))
            .filter(jobs::Column::UpdatedAt.lt(Utc::now() - RETENTION))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }
}

async fn execute(db: &DatabaseConnection, job: jobs::Model, handler: Option<Arc<dyn JobHandler>>) {
    let attempts = job.attempts + 1;
    let outcome = match handler {
        Some(h) => {
            let payload = serde_json::from_str(&job.payload).unwrap_or(serde_json::Value::Null);
            h.handle(payload).await.map_err(|e| e.to_string())
        }
        None => Err(format!("no handler registered for {}", job.kind)),
    };
    let now = Utc::now();
    let mut model: jobs::ActiveModel = job.clone().into();
    model.attempts = Set(attempts);
    model.locked_until = Set(None);
    model.updated_at = Set(now);
    match outcome {
        Ok(()) => {
            model.status = Set(status::DONE.into());
            model.last_error = Set(String::new());
        }
        Err(error) => {
            tracing::warn!(job_id = job.id, kind = %job.kind, attempts, %error, "job failed");
            model.last_error = Set(error);
            if attempts >= job.max_attempts {
                model.status = Set(status::FAILED.into());
            } else {
                model.status = Set(status::PENDING.into());
                model.run_at = Set(now + chrono::Duration::seconds(backoff_secs(attempts)));
            }
        }
    }
    if let Err(error) = model.update(db).await {
        tracing::error!(%error, job_id = job.id, "failed to record job result");
    }
}

/// `BACKOFF_BASE_SECS * 2^(attempts-1)`, capped at [`BACKOFF_MAX_SECS`].
fn backoff_secs(attempts: i32) -> i64 {
    let exp = u32::try_from(attempts.saturating_sub(1))
        .unwrap_or(0)
        .min(20);
    (BACKOFF_BASE_SECS.saturating_mul(1_i64 << exp)).min(BACKOFF_MAX_SECS)
}

#[cfg(test)]
mod tests {
    use super::backoff_secs;

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff_secs(1), 10);
        assert_eq!(backoff_secs(2), 20);
        assert_eq!(backoff_secs(4), 80);
        assert_eq!(backoff_secs(30), 3600);
    }
}
