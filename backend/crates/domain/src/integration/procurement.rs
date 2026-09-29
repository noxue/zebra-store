//! Purchase (procurement) orders placed with suppliers for paid upstream items:
//! model, state machine and ports (original `modules/procurement`, UPS-02/03/07/11/16/22).

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::Value;
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use super::connection::SiteConnection;
use super::protocol::RemoteFulfillment;
use crate::catalog::product::JsonMap;
use crate::{Id, Result};

/// Lines of `upstream_payload` returned by the detail endpoint.
pub const PAYLOAD_PREVIEW_MAX_LINES: usize = 100;
/// Accepted orders older than this raise an alert during the periodic sync (UPS-07).
pub const ACCEPTED_ALERT_AFTER_HOURS: i64 = 24;
/// Batch of accepted orders re-checked per periodic run (original 200).
pub const SYNC_ACCEPTED_BATCH: u64 = 200;

/// Short poll schedule after submission; afterwards the periodic sync takes over
/// without failing the order (UPS-11).
pub const POLL_INTERVALS_SECS: [i64; 9] = [30, 30, 60, 60, 120, 120, 300, 300, 600];

/// Purchase order status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcurementStatus {
    Pending,
    Submitted,
    Accepted,
    Rejected,
    Failed,
    PartiallyRefunded,
    Fulfilled,
    Completed,
    Refunded,
    Canceled,
    /// Zebra Store addition: the supplier may have executed (and charged) the purchase
    /// but its outcome is unknown or needs a human decision ([`RESULT_UNKNOWN`], paid
    /// without goods). Never rolled back automatically: an admin retries it (same
    /// request number) or cancels it, which rolls the local order back.
    ManualReview,
}

impl ProcurementStatus {
    pub const ALL: [Self; 11] = [
        Self::Pending,
        Self::Submitted,
        Self::Accepted,
        Self::Rejected,
        Self::Failed,
        Self::PartiallyRefunded,
        Self::Fulfilled,
        Self::Completed,
        Self::Refunded,
        Self::Canceled,
        Self::ManualReview,
    ];

    pub fn parse(raw: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == raw.trim())
    }

    pub fn from_stored(raw: &str) -> Self {
        Self::parse(raw).unwrap_or(Self::Pending)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Submitted => "submitted",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Failed => "failed",
            Self::PartiallyRefunded => "partially_refunded",
            Self::Fulfilled => "fulfilled",
            Self::Completed => "completed",
            Self::Refunded => "refunded",
            Self::Canceled => "canceled",
            Self::ManualReview => "manual_review",
        }
    }
}

/// Supplier order status reduced to the transitions we act on (UPS-16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpstreamEvent {
    Delivered,
    Canceled,
    Refunded,
    PartiallyRefunded,
    /// Anything else (pending / paid / processing …): nothing to do.
    Other(String),
}

/// Normalizes supplier status aliases: trims, lower-cases, `completed|fulfilled` →
/// delivered, `cancelled` → canceled (UPS-16).
pub fn normalize_upstream_status(raw: &str) -> UpstreamEvent {
    match raw.trim().to_lowercase().as_str() {
        "delivered" | "completed" | "fulfilled" => UpstreamEvent::Delivered,
        "canceled" | "cancelled" => UpstreamEvent::Canceled,
        "refunded" => UpstreamEvent::Refunded,
        "partially_refunded" => UpstreamEvent::PartiallyRefunded,
        other => UpstreamEvent::Other(other.to_owned()),
    }
}

/// State-machine guard of supplier callbacks (UPS-02): once fulfilled, completed,
/// refunded or canceled, a purchase order accepts no more delivered / canceled events;
/// refund events are always accepted (a partial refund may precede delivery).
pub fn callback_allowed(current: ProcurementStatus, event: &UpstreamEvent) -> bool {
    match event {
        UpstreamEvent::Delivered | UpstreamEvent::Canceled => !matches!(
            current,
            ProcurementStatus::Fulfilled
                | ProcurementStatus::Completed
                | ProcurementStatus::Refunded
                | ProcurementStatus::Canceled
        ),
        _ => true,
    }
}

/// Statuses from which a delivered / canceled event may move the order (the guard of
/// the conditional UPDATE, identical to [`callback_allowed`]). A purchase held for
/// manual review is still delivered when the supplier reports goods.
pub const OPEN_FOR_DELIVERY: [ProcurementStatus; 7] = [
    ProcurementStatus::Pending,
    ProcurementStatus::Submitted,
    ProcurementStatus::Accepted,
    ProcurementStatus::Rejected,
    ProcurementStatus::Failed,
    ProcurementStatus::PartiallyRefunded,
    ProcurementStatus::ManualReview,
];

/// Neutral order error code of adapters whose supplier may or may not have executed a
/// purchase (transport failure on a non-idempotent order call, or a duplicate request id
/// the supplier refuses without returning the original order). Never retried: the order
/// must be checked by hand at the supplier to avoid buying twice.
pub const RESULT_UNKNOWN: &str = "upstream_result_unknown";

/// `RemoteFulfillment.status` of content a supplier shows for an order without being
/// able to say whether it is the goods or a notice (acg-faka manual delivery: the
/// seller's delivery message and the cards look alike). The text returned by the
/// order call is kept as the baseline; content seen later that differs from it is the
/// real delivery (ACG-04).
pub const FULFILLMENT_UNCONFIRMED: &str = "unconfirmed";

/// True for a held fulfillment that is only a baseline (not goods).
pub fn is_unconfirmed(f: &RemoteFulfillment) -> bool {
    f.status.trim() == FULFILLMENT_UNCONFIRMED
}

/// The delivery confirmed by a change of unconfirmed content: `seen` (from a later
/// order lookup) counts as delivered once it is non-empty and differs from the
/// `baseline` the order call returned. Without a baseline nothing can be confirmed.
pub fn confirmed_by_change(
    baseline: Option<&RemoteFulfillment>,
    seen: Option<&RemoteFulfillment>,
) -> Option<RemoteFulfillment> {
    let baseline = baseline.filter(|b| is_unconfirmed(b))?;
    let seen = seen.filter(|s| is_unconfirmed(s))?;
    let content = seen.payload.trim();
    if content.is_empty() || content == baseline.payload.trim() {
        return None;
    }
    Some(RemoteFulfillment {
        status: "delivered".into(),
        payload: content.to_owned(),
        ..seen.clone()
    })
}

/// Prefix of the stored error message once any submission attempt may have been
/// executed by the supplier (answer lost after sending). It is kept across retries:
/// such a purchase is never rejected / rolled back automatically, only held for
/// manual review.
pub const MAYBE_EXECUTED_MARK: &str = "[result unknown] ";

/// True when an earlier attempt of this purchase may have been executed.
pub fn maybe_executed(error_message: &str) -> bool {
    error_message.starts_with(MAYBE_EXECUTED_MARK)
}

/// `message` carrying the [`MAYBE_EXECUTED_MARK`] (idempotent).
pub fn mark_maybe_executed(message: &str) -> String {
    if maybe_executed(message) {
        message.to_owned()
    } else {
        format!("{MAYBE_EXECUTED_MARK}{message}")
    }
}

/// Error codes that hold a purchase order in [`ProcurementStatus::ManualReview`]
/// instead of rejecting it (the local order must not be refunded while the supplier
/// may have charged us and may still deliver).
pub fn needs_manual_review(code: &str) -> bool {
    code.trim().eq_ignore_ascii_case(RESULT_UNKNOWN)
}

/// Supplier error codes that are permanent (no retry, UPS-11).
pub fn is_retryable_error_code(code: &str) -> bool {
    !matches!(
        code.trim().to_lowercase().as_str(),
        "insufficient_balance"
            | "payment_failed"
            | "product_unavailable"
            | "sku_unavailable"
            | "invalid_request"
            | "unauthorized"
            | "forbidden"
            | "duplicate_order"
            | "product_out_of_stock"
            | "idempotency_conflict"
            | RESULT_UNKNOWN
    )
}

/// Parses retry intervals (`[30,60,300]`); invalid entries are ignored and an empty
/// result falls back to the default schedule.
pub fn parse_retry_intervals(raw: &str) -> Vec<Duration> {
    let inner = raw.trim().trim_start_matches('[').trim_end_matches(']');
    let parsed: Vec<Duration> = inner
        .split(',')
        .filter_map(|p| p.trim().parse::<i64>().ok())
        .filter(|s| *s > 0)
        .map(Duration::seconds)
        .collect();
    if parsed.is_empty() {
        vec![
            Duration::seconds(30),
            Duration::seconds(60),
            Duration::seconds(300),
        ]
    } else {
        parsed
    }
}

/// Delay before the `attempt`-th retry (0-based), clamped to the last interval.
pub fn retry_delay(intervals: &[Duration], attempt: i32) -> Duration {
    let idx = usize::try_from(attempt.max(0)).unwrap_or(0);
    intervals
        .get(idx)
        .or_else(|| intervals.last())
        .copied()
        .unwrap_or_else(|| Duration::seconds(30))
}

/// Next poll delay after `polls` polls, `None` once the short schedule is exhausted.
pub fn poll_delay(polls: i32) -> Option<Duration> {
    let idx = usize::try_from(polls).ok()?;
    POLL_INTERVALS_SECS.get(idx).map(|s| Duration::seconds(*s))
}

/// Keeps the first `max_lines` lines; returns `(preview, total_lines)`.
pub fn truncate_payload(payload: &str, max_lines: usize) -> (String, usize) {
    if payload.is_empty() {
        return (String::new(), 0);
    }
    let lines: Vec<&str> = payload.split('\n').collect();
    let total = lines.len();
    if total > max_lines {
        (lines[..max_lines].join("\n"), total)
    } else {
        (payload.to_owned(), total)
    }
}

fn parse_record_time(v: Option<&Value>) -> Option<DateTime<Utc>> {
    match v? {
        Value::String(s) => {
            let s = s.trim();
            DateTime::parse_from_rfc3339(s)
                .map(|t| t.with_timezone(&Utc))
                .ok()
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
                        .ok()
                        .map(|n| n.and_utc())
                })
        }
        Value::Number(n) => n.as_i64().and_then(|t| DateTime::from_timestamp(t, 0)),
        _ => None,
    }
}

/// Supplier refund records sorted by `created_at` (undated last) and renumbered
/// `id = 1..n` so supplier primary keys are never exposed.
pub fn normalize_refund_records(records: &[JsonMap]) -> Vec<JsonMap> {
    let mut out: Vec<(Option<DateTime<Utc>>, JsonMap)> = records
        .iter()
        .map(|r| (parse_record_time(r.get("created_at")), r.clone()))
        .collect();
    out.sort_by(|a, b| match (a.0, b.0) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });
    out.into_iter()
        .enumerate()
        .map(|(i, (_, mut r))| {
            r.insert("id".into(), Value::from(i + 1));
            r
        })
        .collect()
}

/// Local order item snapshot used by procurement.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalOrderItem {
    pub product_id: Id,
    pub sku_id: Id,
    pub title: JsonMap,
    pub sku_snapshot: JsonMap,
    pub cost_price: Amount,
    pub quantity: i32,
    pub total_price: Amount,
    pub fulfillment_type: String,
    pub manual_form_submission: JsonMap,
}

/// Local order snapshot (admin JSON shape of `local_order`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LocalOrder {
    pub id: Id,
    pub order_no: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<Id>,
    #[serde(skip_serializing_if = "is_zero")]
    pub user_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub guest_email: String,
    pub status: String,
    pub currency: String,
    pub total_amount: Amount,
    pub refunded_amount: Amount,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<LocalOrderItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<LocalOrder>,
}

fn is_zero(v: &Id) -> bool {
    *v == 0
}

impl LocalOrder {
    pub fn has_upstream_items(&self) -> bool {
        self.items
            .iter()
            .any(|i| i.fulfillment_type.trim() == "upstream")
    }
}

/// A purchase order (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProcurementOrder {
    pub id: Id,
    pub connection_id: Id,
    pub local_order_id: Id,
    pub local_order_no: String,
    #[serde(skip)]
    pub upstream_order_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub upstream_order_no: String,
    pub status: ProcurementStatus,
    pub upstream_amount: Amount,
    pub upstream_currency: String,
    pub local_sell_amount: Amount,
    pub currency: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error_message: String,
    pub retry_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub upstream_payload: String,
    pub upstream_payload_line_count: usize,
    pub trace_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection: Option<SiteConnection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_order: Option<LocalOrder>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub parent_order_no: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub upstream_refund_records: Vec<JsonMap>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub upstream_refunded_amount: String,
    /// Indicates an internal raw delivery is waiting to be finalized.
    pub has_held_delivery: bool,
    /// Delivery the supplier handed over with the order call, persisted until it is
    /// applied (see [`ProcurementChange::held_delivery`]). Never serialized.
    #[serde(skip)]
    pub held_delivery: Option<RemoteFulfillment>,
}

impl ProcurementOrder {
    /// The supplier order identity (numeric id and / or number).
    pub fn upstream_ref(&self) -> super::adapter::OrderRef {
        super::adapter::OrderRef {
            id: self.upstream_order_id,
            no: self.upstream_order_no.clone(),
        }
    }
}

/// A purchase order to create.
#[derive(Debug, Clone, PartialEq)]
pub struct NewProcurement {
    pub connection_id: Id,
    pub local_order_id: Id,
    pub local_order_no: String,
    pub local_sell_amount: Amount,
    pub currency: String,
    pub trace_id: String,
}

/// Column changes of a status update (`None` keeps the column).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ProcurementChange {
    pub status: Option<ProcurementStatus>,
    pub upstream_order_id: Option<Id>,
    pub upstream_order_no: Option<String>,
    pub upstream_amount: Option<Amount>,
    pub upstream_currency: Option<String>,
    pub error_message: Option<String>,
    pub retry_count: Option<i32>,
    /// `Some(None)` clears the column.
    pub next_retry_at: Option<Option<DateTime<Utc>>>,
    pub upstream_payload: Option<String>,
    /// Delivery returned synchronously by the supplier: stored in the same transaction
    /// as the rest of the change (only when the guarded update applies), so it survives
    /// a restart between acceptance and local delivery.
    pub held_delivery: Option<RemoteFulfillment>,
}

/// Admin list filter (times already parsed, UPS-22).
#[derive(Debug, Clone, Default)]
pub struct ProcurementFilter {
    pub page: PageRequest,
    pub connection_id: Id,
    pub status: Option<ProcurementStatus>,
    pub local_order_no: String,
    pub upstream_order_no: String,
    pub created_from: Option<DateTime<Utc>>,
    pub created_to: Option<DateTime<Utc>>,
}

/// Persistence of `procurement_orders`.
#[async_trait]
pub trait ProcurementRepo: Send + Sync {
    /// With `connection` and `local_order` attached.
    async fn get(&self, id: Id) -> Result<Option<ProcurementOrder>>;
    async fn get_by_local_order_id(&self, local_order_id: Id) -> Result<Option<ProcurementOrder>>;
    async fn get_by_local_order_no(&self, local_order_no: &str)
    -> Result<Option<ProcurementOrder>>;
    /// Inserts unless a live purchase order already exists for the local order
    /// (idempotent, UPS-07); `None` when it existed.
    async fn create_once(
        &self,
        new: &NewProcurement,
        now: DateTime<Utc>,
    ) -> Result<Option<ProcurementOrder>>;
    /// Conditional update: applies only while the status is one of `from` (empty = any);
    /// returns whether a row changed (state-machine guard against races, UPS-02).
    async fn update(
        &self,
        id: Id,
        from: &[ProcurementStatus],
        change: &ProcurementChange,
        now: DateTime<Utc>,
    ) -> Result<bool>;
    async fn list(&self, filter: &ProcurementFilter) -> Result<Page<ProcurementOrder>>;
    /// Counts per status with the filter minus its status (UPS-22).
    async fn stats(&self, filter: &ProcurementFilter) -> Result<Vec<(String, u64)>>;
    async fn list_accepted(&self, limit: u64) -> Result<Vec<ProcurementOrder>>;
    /// Cursor scan of accepted orders for recovery sweeps, so a batch of stuck old
    /// orders cannot starve later held deliveries.
    async fn list_accepted_after(&self, after_id: Id, limit: u64) -> Result<Vec<ProcurementOrder>>;
    async fn list_by_connection_between(
        &self,
        connection_id: Id,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<ProcurementOrder>>;
}

/// Reads local orders (consumer-defined, implemented on the order tables).
#[async_trait]
pub trait LocalOrders: Send + Sync {
    /// Order with items and children (children with items).
    async fn get(&self, id: Id) -> Result<Option<LocalOrder>>;
    async fn get_many(&self, ids: &[Id]) -> Result<Vec<LocalOrder>>;
}

/// Local product → connection and local SKU → supplier SKU lookups.
#[async_trait]
pub trait MappingLookup: Send + Sync {
    /// Connection of the active mapping of a local product.
    async fn connection_of_product(&self, product_id: Id) -> Result<Option<Id>>;
    /// Supplier SKU id of a local SKU.
    async fn upstream_sku_of(&self, sku_id: Id) -> Result<Option<Id>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use ProcurementStatus as S;
    use serde_json::json;

    // UPS-02: callback transition table.
    #[test]
    fn ups02_transition_table() {
        let d = UpstreamEvent::Delivered;
        let c = UpstreamEvent::Canceled;
        assert!(callback_allowed(S::Accepted, &d));
        assert!(callback_allowed(S::PartiallyRefunded, &d));
        assert!(!callback_allowed(S::Fulfilled, &d));
        assert!(!callback_allowed(S::Refunded, &d));
        assert!(!callback_allowed(S::Canceled, &d));
        assert!(!callback_allowed(S::Fulfilled, &c));
        assert!(callback_allowed(S::Accepted, &c));
        assert!(callback_allowed(
            S::Accepted,
            &UpstreamEvent::PartiallyRefunded
        ));
        assert!(callback_allowed(S::Fulfilled, &UpstreamEvent::Refunded));
        for s in OPEN_FOR_DELIVERY {
            assert!(callback_allowed(s, &d));
        }
        // a purchase held for manual review still accepts the supplier's delivery
        assert!(callback_allowed(S::ManualReview, &d));
        assert!(callback_allowed(S::ManualReview, &c));
        assert_eq!(S::parse("manual_review"), Some(S::ManualReview));
    }

    // UPS-16: status aliases.
    #[test]
    fn ups16_status_normalization() {
        assert_eq!(
            normalize_upstream_status("Cancelled "),
            UpstreamEvent::Canceled
        );
        assert_eq!(
            normalize_upstream_status("COMPLETED"),
            UpstreamEvent::Delivered
        );
        assert_eq!(
            normalize_upstream_status("fulfilled"),
            UpstreamEvent::Delivered
        );
        assert_eq!(
            normalize_upstream_status("partially_refunded"),
            UpstreamEvent::PartiallyRefunded
        );
        assert_eq!(
            normalize_upstream_status("processing"),
            UpstreamEvent::Other("processing".into())
        );
    }

    // UPS-11: permanent vs retryable errors; poll schedule never fails the order.
    #[test]
    fn ups11_error_classes_and_schedules() {
        assert!(!is_retryable_error_code(" Insufficient_Balance"));
        assert!(!is_retryable_error_code("sku_unavailable"));
        assert!(!is_retryable_error_code("upstream_result_unknown"));
        assert!(is_retryable_error_code("internal_error"));
        assert!(needs_manual_review(" Upstream_Result_Unknown"));
        let marked = mark_maybe_executed("read timeout");
        assert_eq!(marked, "[result unknown] read timeout");
        assert_eq!(mark_maybe_executed(&marked), marked);
        assert!(maybe_executed(&marked));
        assert!(!maybe_executed("connection refused"));
        assert!(!needs_manual_review("insufficient_balance"));
        assert!(is_retryable_error_code(""));
        assert_eq!(poll_delay(0), Some(Duration::seconds(30)));
        assert_eq!(poll_delay(8), Some(Duration::seconds(600)));
        assert_eq!(poll_delay(9), None);
        let iv = parse_retry_intervals("[10, x, 20]");
        assert_eq!(iv, vec![Duration::seconds(10), Duration::seconds(20)]);
        assert_eq!(retry_delay(&iv, 5), Duration::seconds(20));
        assert_eq!(parse_retry_intervals("[]").len(), 3);
    }

    // ACG-04: manual deliveries are confirmed by a change of the shown content.
    #[test]
    fn acg04_unconfirmed_content_is_confirmed_by_change() {
        let f = |status: &str, payload: &str| RemoteFulfillment {
            kind: "manual".into(),
            status: status.into(),
            payload: payload.into(),
            ..RemoteFulfillment::default()
        };
        let base = f(FULFILLMENT_UNCONFIRMED, "请等待站长发货");
        assert_eq!(
            confirmed_by_change(
                Some(&base),
                Some(&f(FULFILLMENT_UNCONFIRMED, " 请等待站长发货\n"))
            ),
            None
        );
        assert_eq!(
            confirmed_by_change(Some(&base), Some(&f(FULFILLMENT_UNCONFIRMED, "  "))),
            None
        );
        let got = confirmed_by_change(Some(&base), Some(&f(FULFILLMENT_UNCONFIRMED, "CARD-1\n")))
            .unwrap();
        assert_eq!(
            (got.status.as_str(), got.payload.as_str()),
            ("delivered", "CARD-1")
        );
        assert_eq!(got.kind, "manual");
        // no baseline, or a baseline that is real goods: nothing to compare
        assert_eq!(
            confirmed_by_change(None, Some(&f(FULFILLMENT_UNCONFIRMED, "X"))),
            None
        );
        assert_eq!(
            confirmed_by_change(
                Some(&f("delivered", "A")),
                Some(&f(FULFILLMENT_UNCONFIRMED, "X"))
            ),
            None
        );
        // only unconfirmed content is compared
        assert_eq!(
            confirmed_by_change(Some(&base), Some(&f("pending", "X"))),
            None
        );
    }

    #[test]
    fn payload_preview() {
        let (p, n) = truncate_payload("a\nb\nc", 2);
        assert_eq!((p.as_str(), n), ("a\nb", 3));
        assert_eq!(truncate_payload("", 2), (String::new(), 0));
    }

    #[test]
    fn refund_records_sorted_and_renumbered() {
        let recs: Vec<JsonMap> = [
            json!({"id": 77, "created_at": "2026-01-02T00:00:00Z"}),
            json!({"id": 78}),
            json!({"id": 79, "created_at": "2026-01-01 00:00:00"}),
        ]
        .into_iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();
        let out = normalize_refund_records(&recs);
        assert_eq!(out[0]["created_at"], "2026-01-01 00:00:00");
        assert_eq!(out[0]["id"], 1);
        assert_eq!(out[2]["id"], 3);
        assert!(out[2].get("created_at").is_none());
    }
}
