//! Reconciliation of purchase orders against the supplier (original
//! `modules/reconciliation`, UPS-16/UPS-21).

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Serialize;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::connection::SiteConnection;
use super::procurement::ProcurementOrder;
use crate::{Id, Result};

/// What a job compares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobType {
    Status,
    Amount,
    Full,
}

impl JobType {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "status" => Some(Self::Status),
            "amount" => Some(Self::Amount),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Amount => "amount",
            Self::Full => "full",
        }
    }

    fn checks_status(self) -> bool {
        matches!(self, Self::Status | Self::Full)
    }

    fn checks_amount(self) -> bool {
        matches!(self, Self::Amount | Self::Full)
    }
}

/// Job status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

impl JobStatus {
    pub fn from_stored(raw: &str) -> Self {
        match raw.trim() {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}

/// Kind of difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MismatchType {
    Status,
    Amount,
    Both,
}

impl MismatchType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Status => "status",
            Self::Amount => "amount",
            Self::Both => "both",
        }
    }
}

/// A reconciliation job (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReconciliationJob {
    pub id: Id,
    pub connection_id: Id,
    #[serde(rename = "type")]
    pub kind: String,
    pub status: JobStatus,
    pub time_range_start: DateTime<Utc>,
    pub time_range_end: DateTime<Utc>,
    pub total_count: i32,
    pub matched_count: i32,
    pub mismatched_count: i32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub result_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<SiteConnection>,
}

/// A mismatch row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconciliationItem {
    pub id: Id,
    pub job_id: Id,
    pub procurement_order_id: Id,
    pub local_order_no: String,
    pub upstream_order_no: String,
    pub local_status: String,
    pub upstream_status: String,
    pub local_amount: Amount,
    pub upstream_amount: Amount,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub mismatch_type: String,
    pub resolved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub remark: String,
    pub created_at: DateTime<Utc>,
}

/// A mismatch to insert.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewItem {
    pub procurement_order_id: Id,
    pub local_order_no: String,
    pub upstream_order_no: String,
    pub local_status: String,
    pub upstream_status: String,
    pub local_amount: Amount,
    pub upstream_amount: Amount,
    pub mismatch_type: MismatchType,
}

/// True when the local purchase status and the supplier status are in the same
/// business window (UPS-16, UPS-21).
pub fn is_status_consistent(local: &str, upstream: &str) -> bool {
    let local = local.trim().to_lowercase();
    let up = upstream.trim().to_lowercase();
    let any = |xs: &[&str]| xs.contains(&up.as_str());
    match local.as_str() {
        "completed" | "fulfilled" => any(&[
            "completed",
            "delivered",
            "fulfilled",
            "refunded",
            "partially_refunded",
        ]),
        "canceled" => any(&["canceled", "cancelled", "refunded", "partially_refunded"]),
        "pending" => any(&["pending", "paid"]),
        "submitted" | "accepted" => any(&["paid", "processing", "accepted"]),
        "failed" | "rejected" => any(&["failed", "rejected"]),
        "fulfilling" => any(&["fulfilling", "processing", "paid"]),
        _ => local == up,
    }
}

/// Compares one purchase order with the supplier's view. Amounts compare the
/// recorded purchase cost with the supplier's charged amount (never the selling
/// price) and only when both are positive (UPS-21).
pub fn compare(
    kind: JobType,
    order: &ProcurementOrder,
    upstream_status: &str,
    upstream_amount: &str,
) -> Option<NewItem> {
    let status_mismatch =
        kind.checks_status() && !is_status_consistent(order.status.as_str(), upstream_status);
    let mut remote_amount = Amount::ZERO;
    let mut amount_mismatch = false;
    if kind.checks_amount()
        && let Ok(v) = upstream_amount.trim().parse::<Decimal>()
        && v > Decimal::ZERO
        && order.upstream_amount.is_positive()
    {
        remote_amount = Amount::new(v);
        amount_mismatch = order.upstream_amount.decimal() != v;
    }
    let mismatch_type = match (status_mismatch, amount_mismatch) {
        (true, true) => MismatchType::Both,
        (true, false) => MismatchType::Status,
        (false, true) => MismatchType::Amount,
        (false, false) => return None,
    };
    Some(NewItem {
        procurement_order_id: order.id,
        local_order_no: order.local_order_no.clone(),
        upstream_order_no: order.upstream_order_no.clone(),
        local_status: order.status.as_str().to_owned(),
        upstream_status: upstream_status.to_owned(),
        local_amount: order.upstream_amount,
        upstream_amount: remote_amount,
        mismatch_type,
    })
}

/// Counters of a run: `total` excludes skipped (no supplier order) and errored rows (UPS-21).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct RunCounts {
    pub total: i32,
    pub matched: i32,
    pub mismatched: i32,
    pub skipped: i32,
    pub errors: i32,
}

impl RunCounts {
    pub fn new(orders: usize, skipped: usize, errors: usize, mismatched: usize) -> Self {
        let to_i32 = |v: usize| i32::try_from(v).unwrap_or(i32::MAX);
        let total = to_i32(orders.saturating_sub(skipped + errors));
        let mismatched = to_i32(mismatched);
        Self {
            total,
            matched: total - mismatched,
            mismatched,
            skipped: to_i32(skipped),
            errors: to_i32(errors),
        }
    }
}

/// Admin list filter.
#[derive(Debug, Clone, Default)]
pub struct JobFilter {
    pub page: PageRequest,
    pub connection_id: Id,
    pub status: String,
    pub kind: String,
}

/// A new job.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewJob {
    pub connection_id: Id,
    pub kind: JobType,
    pub time_range_start: DateTime<Utc>,
    pub time_range_end: DateTime<Utc>,
}

/// Persistence of reconciliation jobs and items.
#[async_trait]
pub trait ReconciliationRepo: Send + Sync {
    async fn create_job(&self, job: &NewJob, now: DateTime<Utc>) -> Result<ReconciliationJob>;
    async fn get_job(&self, id: Id) -> Result<Option<ReconciliationJob>>;
    async fn list_jobs(&self, filter: &JobFilter) -> Result<Page<ReconciliationJob>>;
    /// `pending|failed → running` with a conditional update; false when another run owns
    /// it or it is already completed (UPS-21).
    async fn claim_job(&self, id: Id, now: DateTime<Utc>) -> Result<bool>;
    /// Stores the items and the completed counters in one transaction.
    async fn complete_job(
        &self,
        id: Id,
        items: &[NewItem],
        counts: &RunCounts,
        now: DateTime<Utc>,
    ) -> Result<()>;
    async fn fail_job(&self, id: Id, result_json: &str, now: DateTime<Utc>) -> Result<()>;
    async fn list_items(&self, job_id: Id, page: PageRequest) -> Result<Page<ReconciliationItem>>;
    /// Marks an item resolved; false when it does not exist.
    async fn resolve_item(
        &self,
        id: Id,
        admin_id: Id,
        remark: &str,
        now: DateTime<Utc>,
    ) -> Result<bool>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::integration::procurement::ProcurementStatus;

    fn order(status: ProcurementStatus, upstream_amount: &str) -> ProcurementOrder {
        ProcurementOrder {
            id: 1,
            connection_id: 1,
            local_order_id: 1,
            local_order_no: "L1".into(),
            upstream_order_id: 9,
            upstream_order_no: "U9".into(),
            status,
            upstream_amount: upstream_amount.parse().unwrap(),
            upstream_currency: "CNY".into(),
            local_sell_amount: "12".parse().unwrap(),
            currency: "CNY".into(),
            error_message: String::new(),
            retry_count: 0,
            next_retry_at: None,
            upstream_payload: String::new(),
            upstream_payload_line_count: 0,
            trace_id: String::new(),
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
            connection: None,
            local_order: None,
            parent_order_no: String::new(),
            upstream_refund_records: Vec::new(),
            upstream_refunded_amount: String::new(),
            has_held_delivery: false,
            held_delivery: None,
        }
    }

    // UPS-16: status windows including refunds.
    #[test]
    fn ups16_status_consistency() {
        assert!(is_status_consistent("canceled", "refunded"));
        assert!(is_status_consistent("canceled", "  CaNcElLeD  "));
        assert!(is_status_consistent("fulfilled", "partially_refunded"));
        assert!(is_status_consistent("completed", "partially_refunded"));
        assert!(!is_status_consistent("accepted", "partially_refunded"));
        assert!(!is_status_consistent("submitted", "refunded"));
        assert!(is_status_consistent("fulfilling", "processing"));
    }

    // UPS-21: amounts compare purchase cost with the supplier's amount, never the price.
    #[test]
    fn ups21_amount_comparison() {
        let o = order(ProcurementStatus::Fulfilled, "10");
        assert!(compare(JobType::Full, &o, "delivered", "10.00").is_none());
        let item = compare(JobType::Full, &o, "delivered", "11").unwrap();
        assert_eq!(item.mismatch_type, MismatchType::Amount);
        assert_eq!(item.local_amount.to_string(), "10.00");
        assert_eq!(item.upstream_amount.to_string(), "11.00");
        // amount type ignores status
        assert!(compare(JobType::Amount, &o, "canceled", "10").is_none());
        let both = compare(JobType::Full, &o, "canceled", "12").unwrap();
        assert_eq!(both.mismatch_type, MismatchType::Both);
        // unknown upstream amount / zero local amount are not compared
        assert!(compare(JobType::Amount, &o, "delivered", "").is_none());
        assert!(
            compare(
                JobType::Amount,
                &order(ProcurementStatus::Fulfilled, "0"),
                "x",
                "5"
            )
            .is_none()
        );
    }

    // UPS-21: skipped and errored rows are excluded from the total.
    #[test]
    fn ups21_counts() {
        let c = RunCounts::new(10, 3, 2, 1);
        assert_eq!(
            (c.total, c.matched, c.mismatched, c.skipped, c.errors),
            (5, 4, 1, 3, 2)
        );
    }
}
