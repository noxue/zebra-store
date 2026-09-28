//! Reconciliation jobs: purchase orders vs. the supplier's view (UPS-21).

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zs_domain::integration::keys;
use zs_domain::integration::procurement::ProcurementRepo;
use zs_domain::integration::reconciliation::{
    JobFilter, JobType, NewJob as NewReconJob, ReconciliationItem, ReconciliationJob,
    ReconciliationRepo, RunCounts, compare,
};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

use super::connection::ConnectionService;

/// Payload of `reconciliation:run`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RunJob {
    pub job_id: Id,
}

/// Run request.
#[derive(Debug, Clone)]
pub struct RunInput {
    pub connection_id: Id,
    pub kind: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

/// Reconciliation use cases.
#[derive(Clone)]
pub struct ReconciliationService {
    repo: Arc<dyn ReconciliationRepo>,
    procurements: Arc<dyn ProcurementRepo>,
    connections: ConnectionService,
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ReconciliationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReconciliationService")
    }
}

impl ReconciliationService {
    pub fn new(
        repo: Arc<dyn ReconciliationRepo>,
        procurements: Arc<dyn ProcurementRepo>,
        connections: ConnectionService,
        queue: Arc<dyn JobQueue>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            procurements,
            connections,
            queue,
            clock,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Creates a job and queues its execution.
    pub async fn run(&self, input: &RunInput) -> Result<ReconciliationJob> {
        let kind = JobType::parse(&input.kind).ok_or_else(Error::invalid)?;
        if input.connection_id <= 0 || input.start > input.end {
            return Err(Error::invalid());
        }
        let job = self
            .repo
            .create_job(
                &NewReconJob {
                    connection_id: input.connection_id,
                    kind,
                    time_range_start: input.start,
                    time_range_end: input.end,
                },
                self.now(),
            )
            .await
            .map_err(|e| e.or_internal(keys::RECONCILIATION_CREATE_FAILED))?;
        let queued = match NewJob::new(kinds::RECONCILIATION_RUN, RunJob { job_id: job.id }) {
            Ok(j) => self.queue.enqueue(j.attempts(1)).await,
            Err(e) => Err(e),
        };
        if let Err(error) = queued {
            tracing::warn!(%error, job_id = job.id, "reconciliation enqueue failed");
        }
        Ok(job)
    }

    /// `reconciliation:run`: claims the job (pending/failed only, so completed or
    /// running jobs are never re-entered), compares and stores the result.
    pub async fn execute(&self, job_id: Id) -> Result<()> {
        let Some(job) = self.repo.get_job(job_id).await? else {
            return Ok(());
        };
        if !self.repo.claim_job(job_id, self.now()).await? {
            return Ok(());
        }
        match self.compare_all(&job).await {
            Ok((items, counts)) => {
                self.repo
                    .complete_job(job_id, &items, &counts, self.now())
                    .await?;
                if counts.mismatched > 0 {
                    self.alert(&job, &counts).await;
                }
                Ok(())
            }
            Err(error) => {
                let result = json!({ "error": error.to_string() }).to_string();
                self.repo.fail_job(job_id, &result, self.now()).await?;
                Err(error)
            }
        }
    }

    async fn compare_all(
        &self,
        job: &ReconciliationJob,
    ) -> Result<(
        Vec<zs_domain::integration::reconciliation::NewItem>,
        RunCounts,
    )> {
        let kind = JobType::parse(&job.kind).ok_or_else(Error::invalid)?;
        let (_, client) = self.connections.client_of(job.connection_id).await?;
        let orders = self
            .procurements
            .list_by_connection_between(job.connection_id, job.time_range_start, job.time_range_end)
            .await?;
        let (mut skipped, mut errors) = (0, 0);
        let mut items = Vec::new();
        for order in &orders {
            if !client.addressable(&order.upstream_ref()) {
                skipped += 1;
                continue;
            }
            match client.get_order(&order.upstream_ref()).await {
                Ok(detail) => {
                    if let Some(item) = compare(kind, order, &detail.status, &detail.amount) {
                        items.push(item);
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, procurement_order_id = order.id, "reconciliation fetch failed");
                    errors += 1;
                }
            }
        }
        let counts = RunCounts::new(orders.len(), skipped, errors, items.len());
        Ok((items, counts))
    }

    async fn alert(&self, job: &ReconciliationJob, counts: &RunCounts) {
        let payload = json!({
            "event_type": "exception_alert",
            "biz_type": "reconciliation",
            "biz_id": job.id,
            "data": {
                "job_id": job.id,
                "connection_id": job.connection_id,
                "mismatched_count": counts.mismatched,
                "total_count": counts.total,
            },
        });
        let r = match NewJob::new(kinds::NOTIFICATION_DISPATCH, payload) {
            Ok(j) => self.queue.enqueue(j).await,
            Err(e) => Err(e),
        };
        if let Err(error) = r {
            tracing::warn!(%error, "reconciliation alert enqueue failed");
        }
    }

    pub async fn list(&self, filter: &JobFilter) -> Result<Page<ReconciliationJob>> {
        self.repo
            .list_jobs(filter)
            .await
            .map_err(|e| e.or_internal(keys::RECONCILIATION_FETCH_FAILED))
    }

    pub async fn detail(
        &self,
        id: Id,
        items: PageRequest,
    ) -> Result<(ReconciliationJob, Page<ReconciliationItem>)> {
        let wrap = |e: Error| e.or_internal(keys::RECONCILIATION_FETCH_FAILED);
        let job = self
            .repo
            .get_job(id)
            .await
            .map_err(wrap)?
            .ok_or_else(|| Error::not_found(keys::RECONCILIATION_JOB_NOT_FOUND))?;
        let page = self.repo.list_items(id, items).await.map_err(wrap)?;
        Ok((job, page))
    }

    pub async fn resolve(&self, item_id: Id, admin_id: Id, remark: &str) -> Result<()> {
        let found = self
            .repo
            .resolve_item(item_id, admin_id, remark, self.now())
            .await
            .map_err(|e| e.or_internal(keys::RECONCILIATION_RESOLVE_FAILED))?;
        if found {
            Ok(())
        } else {
            Err(Error::not_found(keys::RECONCILIATION_ITEM_NOT_FOUND))
        }
    }
}
