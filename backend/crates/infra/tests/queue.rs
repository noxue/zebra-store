//! The database job queue runs handlers, retries failures and de-duplicates.
#![expect(
    clippy::unwrap_used,
    reason = "test helpers: failures should abort the test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use sea_orm::EntityTrait;
use tokio::sync::Semaphore;
use zs_app::config::DatabaseConfig;
use zs_domain::queue::{JobHandler, JobQueue, NewJob};
use zs_infra::db::entity::jobs;
use zs_infra::queue::{DbJobQueue, JobRegistry, Worker};

struct Counter(Arc<AtomicUsize>, bool);

#[async_trait]
impl JobHandler for Counter {
    async fn handle(&self, _payload: serde_json::Value) -> zs_domain::Result<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        if self.1 {
            Err(zs_domain::Error::internal_msg("boom"))
        } else {
            Ok(())
        }
    }
}

async fn db() -> sea_orm::DatabaseConnection {
    let cfg = DatabaseConfig {
        url: "sqlite::memory:".into(),
        max_connections: 1,
        ..Default::default()
    };
    let db = zs_infra::db::connect(&cfg).await.unwrap();
    zs_infra::db::sync_schema(&db).await.unwrap();
    db
}

async fn drain(worker: &Worker) {
    let permits = Arc::new(Semaphore::new(4));
    worker.poll_once(&permits).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
}

#[tokio::test]
async fn runs_retries_and_dedupes() {
    let db = db().await;
    let ok_hits = Arc::new(AtomicUsize::new(0));
    let bad_hits = Arc::new(AtomicUsize::new(0));
    let mut reg = JobRegistry::default();
    reg.handle("ok", Arc::new(Counter(ok_hits.clone(), false)));
    reg.handle("bad", Arc::new(Counter(bad_hits.clone(), true)));
    let worker = Worker::new(db.clone(), reg, 4, Duration::from_millis(10));
    let queue = DbJobQueue::new(db.clone());

    queue
        .enqueue(
            NewJob::new("ok", serde_json::json!({}))
                .unwrap()
                .unique("k"),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            NewJob::new("ok", serde_json::json!({}))
                .unwrap()
                .unique("k"),
        )
        .await
        .unwrap();
    queue
        .enqueue(
            NewJob::new("bad", serde_json::json!({}))
                .unwrap()
                .attempts(2),
        )
        .await
        .unwrap();
    // delayed job must not run yet
    queue
        .enqueue(
            NewJob::new("ok", serde_json::json!({}))
                .unwrap()
                .at(chrono::Utc::now() + chrono::Duration::hours(1)),
        )
        .await
        .unwrap();

    drain(&worker).await;
    assert_eq!(
        ok_hits.load(Ordering::SeqCst),
        1,
        "dedupe + delay respected"
    );
    assert_eq!(bad_hits.load(Ordering::SeqCst), 1);

    let rows = jobs::Entity::find().all(&db).await.unwrap();
    let bad = rows.iter().find(|j| j.kind == "bad").unwrap();
    assert_eq!(bad.status, "pending", "failed job is rescheduled");
    assert_eq!(bad.attempts, 1);
    assert!(
        bad.run_at > chrono::Utc::now(),
        "backoff pushes run_at into the future"
    );
    assert!(
        rows.iter()
            .filter(|j| j.kind == "ok")
            .any(|j| j.status == "done")
    );
}
