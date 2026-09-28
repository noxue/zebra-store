//! [`ReconciliationRepo`] on `reconciliation_jobs` / `reconciliation_items`.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, Condition, DatabaseConnection, EntityTrait, PaginatorTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, TransactionTrait,
};
use zs_domain::integration::reconciliation::{
    JobFilter, JobStatus, NewItem, NewJob, ReconciliationItem, ReconciliationJob,
    ReconciliationRepo, RunCounts,
};
use zs_domain::{Id, Result};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::connection::many_in as connections_in;
use crate::db::entity::{reconciliation_items, reconciliation_jobs};
use crate::db::repo::support::DbResultExt;

/// SeaORM implementation of [`ReconciliationRepo`].
#[derive(Debug, Clone)]
pub struct SeaReconciliationRepo {
    db: DatabaseConnection,
}

impl SeaReconciliationRepo {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

fn to_job(m: reconciliation_jobs::Model) -> ReconciliationJob {
    ReconciliationJob {
        id: m.id,
        connection_id: m.connection_id,
        kind: m.type_,
        status: JobStatus::from_stored(&m.status),
        time_range_start: m.time_range_start,
        time_range_end: m.time_range_end,
        total_count: m.total_count,
        matched_count: m.matched_count,
        mismatched_count: m.mismatched_count,
        result_json: m.result_json,
        started_at: m.started_at,
        finished_at: m.finished_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
        connection: None,
    }
}

fn to_item(m: reconciliation_items::Model) -> ReconciliationItem {
    ReconciliationItem {
        id: m.id,
        job_id: m.job_id,
        procurement_order_id: m.procurement_order_id,
        local_order_no: m.local_order_no,
        upstream_order_no: m.upstream_order_no,
        local_status: m.local_status,
        upstream_status: m.upstream_status,
        local_amount: Amount::new(m.local_amount),
        upstream_amount: Amount::new(m.upstream_amount),
        mismatch_type: m.mismatch_type,
        resolved: m.resolved,
        resolved_by: m.resolved_by,
        resolved_at: m.resolved_at,
        remark: m.remark,
        created_at: m.created_at,
    }
}

#[async_trait]
impl ReconciliationRepo for SeaReconciliationRepo {
    async fn create_job(&self, job: &NewJob, now: DateTime<Utc>) -> Result<ReconciliationJob> {
        let row = reconciliation_jobs::ActiveModel {
            connection_id: Set(job.connection_id),
            type_: Set(job.kind.as_str().to_owned()),
            status: Set(JobStatus::Pending.as_str().to_owned()),
            time_range_start: Set(job.time_range_start),
            time_range_end: Set(job.time_range_end),
            total_count: Set(0),
            matched_count: Set(0),
            mismatched_count: Set(0),
            result_json: Set(String::new()),
            started_at: Set(None),
            finished_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(to_job(row))
    }

    async fn get_job(&self, id: Id) -> Result<Option<ReconciliationJob>> {
        let Some(row) = reconciliation_jobs::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let mut job = to_job(row);
        job.connection = connections_in(&self.db, &[job.connection_id]).await?.pop();
        Ok(Some(job))
    }

    async fn list_jobs(&self, filter: &JobFilter) -> Result<Page<ReconciliationJob>> {
        let mut c = Condition::all();
        if filter.connection_id > 0 {
            c = c.add(reconciliation_jobs::Column::ConnectionId.eq(filter.connection_id));
        }
        if !filter.status.is_empty() {
            c = c.add(reconciliation_jobs::Column::Status.eq(filter.status.clone()));
        }
        if !filter.kind.is_empty() {
            c = c.add(reconciliation_jobs::Column::Type.eq(filter.kind.clone()));
        }
        let q = reconciliation_jobs::Entity::find().filter(c);
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_desc(reconciliation_jobs::Column::CreatedAt)
            .order_by_desc(reconciliation_jobs::Column::Id)
            .offset(filter.page.offset())
            .limit(filter.page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        let ids: Vec<Id> = rows.iter().map(|r| r.connection_id).collect();
        let conns: HashMap<Id, _> = connections_in(&self.db, &ids)
            .await?
            .into_iter()
            .map(|c| (c.id, c))
            .collect();
        Ok(Page {
            items: rows
                .into_iter()
                .map(|r| {
                    let mut j = to_job(r);
                    j.connection = conns.get(&j.connection_id).cloned();
                    j
                })
                .collect(),
            total,
        })
    }

    async fn claim_job(&self, id: Id, now: DateTime<Utc>) -> Result<bool> {
        let res = reconciliation_jobs::Entity::update_many()
            .col_expr(
                reconciliation_jobs::Column::Status,
                Expr::value(JobStatus::Running.as_str()),
            )
            .col_expr(reconciliation_jobs::Column::StartedAt, Expr::value(now))
            .col_expr(reconciliation_jobs::Column::UpdatedAt, Expr::value(now))
            .filter(reconciliation_jobs::Column::Id.eq(id))
            .filter(
                reconciliation_jobs::Column::Status
                    .is_in([JobStatus::Pending.as_str(), JobStatus::Failed.as_str()]),
            )
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }

    async fn complete_job(
        &self,
        id: Id,
        items: &[NewItem],
        counts: &RunCounts,
        now: DateTime<Utc>,
    ) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        for item in items {
            reconciliation_items::ActiveModel {
                job_id: Set(id),
                procurement_order_id: Set(item.procurement_order_id),
                local_order_no: Set(item.local_order_no.clone()),
                upstream_order_no: Set(item.upstream_order_no.clone()),
                local_status: Set(item.local_status.clone()),
                upstream_status: Set(item.upstream_status.chars().take(20).collect()),
                local_amount: Set(item.local_amount.decimal()),
                upstream_amount: Set(item.upstream_amount.decimal()),
                mismatch_type: Set(item.mismatch_type.as_str().to_owned()),
                resolved: Set(false),
                resolved_by: Set(None),
                resolved_at: Set(None),
                remark: Set(String::new()),
                created_at: Set(now),
                ..Default::default()
            }
            .insert(&txn)
            .await
            .dom()?;
        }
        let result = serde_json::to_string(counts)?;
        reconciliation_jobs::Entity::update_many()
            .col_expr(
                reconciliation_jobs::Column::Status,
                Expr::value(JobStatus::Completed.as_str()),
            )
            .col_expr(
                reconciliation_jobs::Column::TotalCount,
                Expr::value(counts.total),
            )
            .col_expr(
                reconciliation_jobs::Column::MatchedCount,
                Expr::value(counts.matched),
            )
            .col_expr(
                reconciliation_jobs::Column::MismatchedCount,
                Expr::value(counts.mismatched),
            )
            .col_expr(reconciliation_jobs::Column::ResultJson, Expr::value(result))
            .col_expr(reconciliation_jobs::Column::FinishedAt, Expr::value(now))
            .col_expr(reconciliation_jobs::Column::UpdatedAt, Expr::value(now))
            .filter(reconciliation_jobs::Column::Id.eq(id))
            .exec(&txn)
            .await
            .dom()?;
        txn.commit().await.dom()
    }

    async fn fail_job(&self, id: Id, result_json: &str, now: DateTime<Utc>) -> Result<()> {
        reconciliation_jobs::Entity::update_many()
            .col_expr(
                reconciliation_jobs::Column::Status,
                Expr::value(JobStatus::Failed.as_str()),
            )
            .col_expr(
                reconciliation_jobs::Column::ResultJson,
                Expr::value(result_json),
            )
            .col_expr(reconciliation_jobs::Column::FinishedAt, Expr::value(now))
            .col_expr(reconciliation_jobs::Column::UpdatedAt, Expr::value(now))
            .filter(reconciliation_jobs::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn list_items(&self, job_id: Id, page: PageRequest) -> Result<Page<ReconciliationItem>> {
        let q = reconciliation_items::Entity::find()
            .filter(reconciliation_items::Column::JobId.eq(job_id));
        let total = q.clone().count(&self.db).await.dom()?;
        let rows = q
            .order_by_asc(reconciliation_items::Column::Id)
            .offset(page.offset())
            .limit(page.page_size)
            .all(&self.db)
            .await
            .dom()?;
        Ok(Page {
            items: rows.into_iter().map(to_item).collect(),
            total,
        })
    }

    async fn resolve_item(
        &self,
        id: Id,
        admin_id: Id,
        remark: &str,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let res = reconciliation_items::Entity::update_many()
            .col_expr(reconciliation_items::Column::Resolved, Expr::value(true))
            .col_expr(
                reconciliation_items::Column::ResolvedBy,
                Expr::value(Some(admin_id)),
            )
            .col_expr(
                reconciliation_items::Column::ResolvedAt,
                Expr::value(Some(now)),
            )
            .col_expr(reconciliation_items::Column::Remark, Expr::value(remark))
            .filter(reconciliation_items::Column::Id.eq(id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(res.rows_affected > 0)
    }
}
