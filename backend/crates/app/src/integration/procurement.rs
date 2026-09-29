//! Purchase orders: creation for paid upstream items, submission, polling, supplier
//! callbacks, periodic re-check, admin queries and manual actions.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use zs_domain::integration::adapter::{Capability, OrderLine, PlaceOrder};
use zs_domain::integration::hooks::{FailureRefund, ProcurementLifecycle, UpstreamDelivery};
use zs_domain::integration::keys;
use zs_domain::integration::procurement::{
    ACCEPTED_ALERT_AFTER_HOURS, LocalOrder, LocalOrders, MappingLookup, NewProcurement,
    OPEN_FOR_DELIVERY, PAYLOAD_PREVIEW_MAX_LINES, ProcurementChange, ProcurementFilter,
    ProcurementOrder, ProcurementRepo, ProcurementStatus, SYNC_ACCEPTED_BATCH, UpstreamEvent,
    callback_allowed, confirmed_by_change, is_retryable_error_code, is_unconfirmed,
    mark_maybe_executed, maybe_executed, needs_manual_review, normalize_refund_records,
    normalize_upstream_status, parse_retry_intervals, poll_delay, retry_delay, truncate_payload,
};
use zs_domain::integration::protocol::RemoteFulfillment;
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::Page;

use super::connection::ConnectionService;
use super::downstream::DownstreamService;

/// Attempts of `procurement:submit` (original "up to 3 retries").
pub const SUBMIT_ATTEMPTS: i32 = 3;
/// First poll after acceptance (fallback of the supplier callback).
const FIRST_POLL_DELAY: Duration = Duration::seconds(30);

/// Payload of `procurement:submit` / `procurement:poll_status`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProcurementJob {
    pub procurement_order_id: Id,
}

/// Outcome of [`ProcurementService::cancel`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelOutcome {
    pub refund: FailureRefund,
}

impl Default for CancelOutcome {
    fn default() -> Self {
        Self {
            refund: FailureRefund::None,
        }
    }
}

/// Outcome of [`ProcurementService::create_for_order`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreateOutcome {
    /// New purchase order ids.
    pub created: Vec<Id>,
    /// Local orders that already had one (idempotent re-run, UPS-07).
    pub existing: Vec<Id>,
}

fn not_found() -> Error {
    Error::not_found(keys::PROCUREMENT_NOT_FOUND)
}

fn status_invalid() -> Error {
    Error::bad_request(keys::MSG_PROCUREMENT_STATUS_INVALID)
}

fn delivery_of(f: Option<&RemoteFulfillment>) -> UpstreamDelivery {
    f.map(|f| UpstreamDelivery {
        kind: f.kind.clone(),
        status: f.status.clone(),
        payload: f.payload.clone(),
        delivery_data: f.delivery_data.clone().unwrap_or_default(),
        delivered_at: f.delivered_at,
    })
    .unwrap_or_default()
}

/// Purchase order use cases.
#[derive(Clone)]
pub struct ProcurementService {
    repo: Arc<dyn ProcurementRepo>,
    orders: Arc<dyn LocalOrders>,
    lookup: Arc<dyn MappingLookup>,
    connections: ConnectionService,
    lifecycle: Arc<dyn ProcurementLifecycle>,
    downstream: DownstreamService,
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ProcurementService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ProcurementService")
    }
}

impl ProcurementService {
    #[expect(clippy::too_many_arguments, reason = "service wiring")]
    pub fn new(
        repo: Arc<dyn ProcurementRepo>,
        orders: Arc<dyn LocalOrders>,
        lookup: Arc<dyn MappingLookup>,
        connections: ConnectionService,
        lifecycle: Arc<dyn ProcurementLifecycle>,
        downstream: DownstreamService,
        queue: Arc<dyn JobQueue>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            orders,
            lookup,
            connections,
            lifecycle,
            downstream,
            queue,
            clock,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Resume already-purchased held deliveries after their converter recovers.
    /// This only retries the conversion/delivery transition; it never submits a purchase.
    pub async fn retry_held_for_converter(
        &self,
        converter_id: Id,
        converters: &super::card_converter::CardConverterService,
    ) -> Result<()> {
        let mut after_id = 0;
        loop {
            let batch = self
                .repo
                .list_accepted_after(after_id, SYNC_ACCEPTED_BATCH)
                .await?;
            if batch.is_empty() {
                break;
            }
            for p in &batch {
                after_id = after_id.max(p.id);
                let Some(delivery) = p.held_delivery.as_ref().filter(|f| !is_unconfirmed(f)) else {
                    continue;
                };
                let Some(local) = self.orders.get(p.local_order_id).await? else {
                    continue;
                };
                let mut items = local.items.clone();
                for child in &local.children {
                    items.extend(child.items.clone());
                }
                let mut matches = false;
                for item in items {
                    if converters
                        .uses_converter(item.product_id, item.sku_id, converter_id)
                        .await?
                    {
                        matches = true;
                        break;
                    }
                }
                if matches
                    && let Err(error) = self
                        .handle_event(p.id, &UpstreamEvent::Delivered, Some(delivery))
                        .await
                {
                    tracing::warn!(%error, procurement_order_id = p.id, "recovered converter delivery retry failed");
                }
            }
            if batch.len() < usize::try_from(SYNC_ACCEPTED_BATCH).unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(())
    }

    /// Counts purchased deliveries waiting on a configured converter for dashboard triage.
    pub async fn held_converter_count(
        &self,
        converters: &super::card_converter::CardConverterService,
    ) -> Result<usize> {
        let mut count = 0_usize;
        let mut after_id = 0;
        loop {
            let batch = self
                .repo
                .list_accepted_after(after_id, SYNC_ACCEPTED_BATCH)
                .await?;
            if batch.is_empty() {
                break;
            }
            for p in &batch {
                after_id = after_id.max(p.id);
                if !p.held_delivery.as_ref().is_some_and(|f| !is_unconfirmed(f)) {
                    continue;
                }
                let Some(local) = self.orders.get(p.local_order_id).await? else {
                    continue;
                };
                let mut items = local.items.clone();
                for child in &local.children {
                    items.extend(child.items.clone());
                }
                for item in items {
                    if converters
                        .has_product_binding(item.product_id, item.sku_id)
                        .await?
                    {
                        count = count.saturating_add(1);
                        break;
                    }
                }
            }
            if batch.len() < usize::try_from(SYNC_ACCEPTED_BATCH).unwrap_or(usize::MAX) {
                break;
            }
        }
        Ok(count)
    }

    /// Admin recovery of a held supplier delivery. This path never submits a purchase.
    pub async fn retry_held_delivery(&self, id: Id) -> Result<()> {
        let order = self.repo.get(id).await?.ok_or_else(not_found)?;
        let delivery = order
            .held_delivery
            .ok_or_else(|| Error::bad_request("error.procurement_delivery_not_held"))?;
        self.handle_event(id, &UpstreamEvent::Delivered, Some(&delivery))
            .await
    }

    async fn enqueue(&self, kind: &str, id: Id, at: Option<DateTime<Utc>>, attempts: i32) {
        let job = NewJob::new(
            kind,
            ProcurementJob {
                procurement_order_id: id,
            },
        )
        .map(|j| j.attempts(attempts));
        let job = match (job, at) {
            (Ok(j), Some(at)) => Ok(j.at(at)),
            (j, _) => j,
        };
        let r = match job {
            Ok(j) => self.queue.enqueue(j).await,
            Err(e) => Err(e),
        };
        if let Err(error) = r {
            tracing::warn!(%error, kind, procurement_order_id = id, "enqueue procurement job failed");
        }
    }

    /// Enqueues an `exception_alert` notification about a purchase order (UPS-07).
    async fn alert(&self, order: &ProcurementOrder, message: &str) {
        let payload = json!({
            "event_type": "exception_alert",
            "biz_type": "procurement",
            "biz_id": order.id,
            "data": {
                "procurement_order_id": order.id,
                "local_order_no": order.local_order_no,
                "error": message,
            },
        });
        let r = match NewJob::new(kinds::NOTIFICATION_DISPATCH, payload) {
            Ok(j) => self.queue.enqueue(j).await,
            Err(e) => Err(e),
        };
        if let Err(error) = r {
            tracing::warn!(%error, procurement_order_id = order.id, "procurement alert enqueue failed");
        }
    }

    // ------------------------------------------------------------------ create

    /// Creates purchase orders for a paid order (children with upstream items, or the
    /// order itself) and enqueues their submission. Idempotent per local order.
    pub async fn create_for_order(&self, order_id: Id) -> Result<CreateOutcome> {
        let order = self
            .orders
            .get(order_id)
            .await?
            .ok_or_else(|| Error::not_found("error.order_not_found"))?;
        let mut outcome = CreateOutcome::default();
        if order.parent_id.is_none() && !order.children.is_empty() {
            for child in order.children.iter().filter(|c| c.has_upstream_items()) {
                self.create_single(child, &mut outcome).await?;
            }
        } else if order.has_upstream_items() {
            self.create_single(&order, &mut outcome).await?;
        }
        Ok(outcome)
    }

    async fn create_single(&self, order: &LocalOrder, outcome: &mut CreateOutcome) -> Result<()> {
        let item = order
            .items
            .first()
            .ok_or_else(|| Error::internal_msg(format!("order {} has no items", order.id)))?;
        let connection_id = self
            .lookup
            .connection_of_product(item.product_id)
            .await?
            .ok_or_else(|| {
                Error::internal_msg(format!(
                    "no product mapping for product {}",
                    item.product_id
                ))
            })?;
        let new = NewProcurement {
            connection_id,
            local_order_id: order.id,
            local_order_no: order.order_no.clone(),
            local_sell_amount: order.total_amount,
            currency: order.currency.clone(),
            trace_id: uuid::Uuid::new_v4().to_string(),
        };
        match self.repo.create_once(&new, self.now()).await? {
            Some(p) => {
                tracing::info!(
                    procurement_order_id = p.id,
                    local_order_id = order.id,
                    "procurement order created"
                );
                self.enqueue(kinds::PROCUREMENT_SUBMIT, p.id, None, SUBMIT_ATTEMPTS)
                    .await;
                outcome.created.push(p.id);
            }
            None => outcome.existing.push(order.id),
        }
        Ok(())
    }

    // ------------------------------------------------------------------ submit

    /// Records a transient error without changing the status; the job is retried.
    async fn mark_error(&self, p: &ProcurementOrder, message: String) -> Error {
        let change = ProcurementChange {
            error_message: Some(message.clone()),
            ..ProcurementChange::default()
        };
        if let Err(error) = self.repo.update(p.id, &[], &change, self.now()).await {
            tracing::warn!(%error, "record procurement error failed");
        }
        Error::internal_msg(message)
    }

    /// Terminal failure: rejected, local order rolled back, admin alerted (UPS-07/11).
    /// A purchase an earlier attempt of which may have been executed is held for
    /// manual review instead (money safety: never refund what we may have bought).
    async fn reject(&self, p: &ProcurementOrder, message: String) -> Result<()> {
        if maybe_executed(&p.error_message) || maybe_executed(&message) {
            return self.hold_for_review(p, message).await;
        }
        let change = ProcurementChange {
            status: Some(ProcurementStatus::Rejected),
            error_message: Some(message.clone()),
            ..ProcurementChange::default()
        };
        let changed = self
            .repo
            .update(
                p.id,
                &[
                    ProcurementStatus::Pending,
                    ProcurementStatus::Failed,
                    ProcurementStatus::Submitted,
                ],
                &change,
                self.now(),
            )
            .await?;
        if changed {
            tracing::warn!(procurement_order_id = p.id, error = %message, "procurement rejected");
            self.rollback(p, &message).await;
        }
        Ok(())
    }

    async fn rollback(&self, p: &ProcurementOrder, message: &str) {
        if let Err(error) = self.lifecycle.rollback_failed(p.local_order_id).await {
            tracing::warn!(%error, local_order_id = p.local_order_id, "local order rollback failed");
        }
        self.alert(p, message).await;
    }

    /// The supplier may have executed the purchase ([`needs_manual_review`]): held for
    /// an admin decision, the local order is **not** rolled back (no refund while we may
    /// have been charged and goods may still arrive); the admin is alerted.
    async fn hold_for_review(&self, p: &ProcurementOrder, message: String) -> Result<()> {
        let message = mark_maybe_executed(&message);
        let change = ProcurementChange {
            status: Some(ProcurementStatus::ManualReview),
            error_message: Some(message.clone()),
            next_retry_at: Some(None),
            ..ProcurementChange::default()
        };
        let changed = self
            .repo
            .update(
                p.id,
                &[
                    ProcurementStatus::Pending,
                    ProcurementStatus::Failed,
                    ProcurementStatus::Submitted,
                ],
                &change,
                self.now(),
            )
            .await?;
        if changed {
            tracing::warn!(procurement_order_id = p.id, error = %message, "procurement held for manual review");
            self.alert(p, &message).await;
        }
        Ok(())
    }

    /// `procurement:submit`: places the purchase order with the supplier. Permanent
    /// problems reject without retry; transient ones are recorded and retried (UPS-11).
    pub async fn submit(&self, id: Id) -> Result<()> {
        let Some(p) = self.repo.get(id).await? else {
            return Ok(());
        };
        if !matches!(
            p.status,
            ProcurementStatus::Pending | ProcurementStatus::Failed
        ) {
            return Ok(());
        }
        let conn = match self.connections.find(p.connection_id).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                return self
                    .reject(&p, format!("connection {} not found", p.connection_id))
                    .await;
            }
            Err(e) => {
                return Err(self
                    .mark_error(&p, format!("load connection failed: {e}"))
                    .await);
            }
        };
        let order = match self.orders.get(p.local_order_id).await {
            Ok(Some(o)) => o,
            Ok(None) => {
                return self
                    .reject(&p, format!("local order {} not found", p.local_order_id))
                    .await;
            }
            Err(e) => {
                return Err(self
                    .mark_error(&p, format!("load local order failed: {e}"))
                    .await);
            }
        };
        let Some(item) = order.items.first() else {
            return self
                .reject(&p, format!("local order {} has no items", order.id))
                .await;
        };
        let upstream_sku = match self.lookup.upstream_sku_of(item.sku_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                return self
                    .reject(&p, format!("no sku mapping for local sku {}", item.sku_id))
                    .await;
            }
            Err(e) => {
                return Err(self
                    .mark_error(&p, format!("lookup sku mapping failed: {e}"))
                    .await);
            }
        };
        let client = match self.connections.client(&conn) {
            Ok(c) => c,
            Err(e) => {
                return Err(self
                    .mark_error(&p, format!("open connection failed: {e}"))
                    .await);
            }
        };
        let line = OrderLine {
            sku_id: upstream_sku,
            quantity: item.quantity,
            manual_form_data: (!item.manual_form_submission.is_empty())
                .then(|| item.manual_form_submission.clone()),
        };
        // Suppliers with quotes: lock the price and learn about stock / balance first.
        let mut quote_id = None;
        if client.capabilities().has(Capability::Quote) {
            match client.quote(std::slice::from_ref(&line)).await {
                Ok(q) => {
                    if let Some(bad) = q.lines.iter().find(|l| !l.available) {
                        let code = bad
                            .reason
                            .clone()
                            .unwrap_or_else(|| "item_unavailable".into());
                        let retryable = is_retryable_error_code(&code);
                        return self
                            .submit_failure(
                                &p,
                                &conn.retry_intervals,
                                conn.retry_max,
                                format!("upstream quote: {code}"),
                                retryable,
                            )
                            .await;
                    }
                    if !q.sufficient_balance {
                        return self
                            .submit_failure(
                                &p,
                                &conn.retry_intervals,
                                conn.retry_max,
                                "insufficient_balance".into(),
                                false,
                            )
                            .await;
                    }
                    quote_id = Some(q.quote_id);
                }
                Err(e) => {
                    // A quote buys nothing: its failure is never "maybe executed"
                    // (LQA-I4), so exhausted retries reject and roll back.
                    return self
                        .submit_failure(
                            &p,
                            &conn.retry_intervals,
                            conn.retry_max,
                            format!("upstream quote error: {}", e.not_executed()),
                            true,
                        )
                        .await;
                }
            }
        }
        let req = PlaceOrder {
            lines: vec![line],
            downstream_order_no: order.order_no.clone(),
            trace_id: p.trace_id.clone(),
            callback_url: conn.callback_url.clone(),
            idempotency_key: format!("procurement:{}", p.id),
            quote_id,
        };
        let resp = match client.place_order(&req).await {
            Ok(r) => r,
            Err(e) => {
                let message = format!("upstream request error: {e}");
                // Sent but unanswered: retried with the same request number; if the
                // retries run out the purchase is held for review, never refunded.
                let message = if e.may_have_executed() {
                    mark_maybe_executed(&message)
                } else {
                    message
                };
                return self
                    .submit_failure(&p, &conn.retry_intervals, conn.retry_max, message, true)
                    .await;
            }
        };
        if !resp.ok {
            let message = if resp.error_message.is_empty() {
                resp.error_code.clone()
            } else {
                resp.error_message.clone()
            };
            if needs_manual_review(&resp.error_code) {
                return self.hold_for_review(&p, message).await;
            }
            let retryable = is_retryable_error_code(&resp.error_code);
            return self
                .submit_failure(
                    &p,
                    &conn.retry_intervals,
                    conn.retry_max,
                    message,
                    retryable,
                )
                .await;
        }
        // Executed but needing a human decision (e.g. charged without goods).
        let review = resp.review.clone().filter(|r| !r.trim().is_empty());
        // A synchronous delivery is stored in the same step as the supplier order
        // number, then applied from the store (restart safe).
        let held = if review.is_none() {
            resp.fulfillment.clone()
        } else {
            None
        };
        let change = ProcurementChange {
            status: Some(if review.is_some() {
                ProcurementStatus::ManualReview
            } else {
                ProcurementStatus::Accepted
            }),
            upstream_order_id: Some(resp.order_id),
            upstream_order_no: Some(resp.order_no.clone()),
            upstream_amount: Some(resp.amount.trim().parse().unwrap_or_default()),
            upstream_currency: Some(resp.currency.clone()),
            error_message: Some(review.clone().unwrap_or_default()),
            retry_count: Some(0),
            next_retry_at: Some(None),
            held_delivery: held.clone(),
            ..ProcurementChange::default()
        };
        let accepted = self
            .repo
            .update(
                p.id,
                &[ProcurementStatus::Pending, ProcurementStatus::Failed],
                &change,
                self.now(),
            )
            .await?;
        if !accepted {
            // A supplier callback won the race (already fulfilled / canceled).
            return Ok(());
        }
        tracing::info!(
            procurement_order_id = p.id,
            upstream_order_id = resp.order_id,
            manual_review = review.is_some(),
            "procurement accepted"
        );
        if let Err(error) = self.lifecycle.mark_fulfilling(p.local_order_id).await {
            tracing::warn!(%error, local_order_id = p.local_order_id, "mark local order fulfilling failed");
        }
        if let Some(reason) = review {
            self.alert(&p, &reason).await;
            return Ok(());
        }
        // An unconfirmed baseline (e.g. a manual-delivery notice) is only stored.
        if let Some(f) = held.filter(|f| !is_unconfirmed(f)) {
            match self
                .handle_event(p.id, &UpstreamEvent::Delivered, Some(&f))
                .await
            {
                Ok(()) => return Ok(()),
                // Kept in the store: the poll below applies it again.
                Err(error) => {
                    tracing::warn!(%error, procurement_order_id = p.id, "apply synchronous delivery failed");
                }
            }
        }
        self.enqueue(
            kinds::PROCUREMENT_POLL,
            p.id,
            Some(self.now() + FIRST_POLL_DELAY),
            1,
        )
        .await;
        Ok(())
    }

    async fn submit_failure(
        &self,
        p: &ProcurementOrder,
        intervals: &str,
        retry_max: i32,
        message: String,
        retryable: bool,
    ) -> Result<()> {
        // Stays marked once any attempt may have been executed.
        let message = if maybe_executed(&p.error_message) {
            mark_maybe_executed(&message)
        } else {
            message
        };
        if retryable && p.retry_count < retry_max {
            let delay = retry_delay(&parse_retry_intervals(intervals), p.retry_count);
            let next = self.now() + delay;
            let change = ProcurementChange {
                status: Some(ProcurementStatus::Failed),
                retry_count: Some(p.retry_count + 1),
                next_retry_at: Some(Some(next)),
                error_message: Some(message),
                ..ProcurementChange::default()
            };
            let changed = self
                .repo
                .update(
                    p.id,
                    &[ProcurementStatus::Pending, ProcurementStatus::Failed],
                    &change,
                    self.now(),
                )
                .await?;
            if changed {
                self.enqueue(kinds::PROCUREMENT_SUBMIT, p.id, Some(next), SUBMIT_ATTEMPTS)
                    .await;
            }
            return Ok(());
        }
        self.reject(p, message).await
    }

    // ------------------------------------------------------------------ events

    /// Applies a supplier status (callback, poll or periodic sync — one idempotent
    /// transition function, UPS-11). Delivery is written before the status moves so a
    /// failed write keeps the order open for a retry (UPS-02).
    pub async fn handle_event(
        &self,
        id: Id,
        event: &UpstreamEvent,
        fulfillment: Option<&RemoteFulfillment>,
    ) -> Result<()> {
        let p = self.repo.get(id).await?.ok_or_else(not_found)?;
        if !callback_allowed(p.status, event) {
            tracing::warn!(
                procurement_order_id = p.id,
                status = p.status.as_str(),
                ?event,
                "procurement transition rejected"
            );
            return Ok(());
        }
        let now = self.now();
        match event {
            UpstreamEvent::Delivered => {
                // Persist the supplier's successful result before any converter call.
                // If conversion fails or this process restarts, the accepted order is
                // resumed from held_delivery and the supplier is never purchased again.
                let held_delivery = fulfillment.cloned().or(p.held_delivery.clone());
                if held_delivery.is_some() {
                    let saved = self
                        .repo
                        .update(
                            p.id,
                            &OPEN_FOR_DELIVERY,
                            &ProcurementChange {
                                status: Some(ProcurementStatus::Accepted),
                                held_delivery: held_delivery.clone(),
                                ..ProcurementChange::default()
                            },
                            now,
                        )
                        .await?;
                    if !saved {
                        return Ok(());
                    }
                }
                self.lifecycle
                    .deliver_upstream(p.local_order_id, &delivery_of(held_delivery.as_ref()))
                    .await?;
                let change = ProcurementChange {
                    status: Some(ProcurementStatus::Fulfilled),
                    upstream_payload: fulfillment.map(|f| f.payload.clone()),
                    ..ProcurementChange::default()
                };
                if self
                    .repo
                    .update(p.id, &OPEN_FOR_DELIVERY, &change, now)
                    .await?
                {
                    tracing::info!(procurement_order_id = p.id, "procurement fulfilled");
                    // Multi-level chains: notify our own downstream buyer (order and parent).
                    self.downstream.enqueue_for_order(p.local_order_id).await;
                    if let Ok(Some(o)) = self.orders.get(p.local_order_id).await
                        && let Some(parent) = o.parent_id
                    {
                        self.downstream.enqueue_for_order(parent).await;
                    }
                }
            }
            UpstreamEvent::Canceled => {
                let change = ProcurementChange {
                    status: Some(ProcurementStatus::Canceled),
                    ..ProcurementChange::default()
                };
                if self
                    .repo
                    .update(p.id, &OPEN_FOR_DELIVERY, &change, now)
                    .await?
                {
                    self.rollback(&p, "upstream canceled order").await;
                }
            }
            UpstreamEvent::Refunded | UpstreamEvent::PartiallyRefunded => {
                // Supplier refunds only move the purchase order; the local order's
                // refund stays an admin decision (UPS-16).
                let status = if *event == UpstreamEvent::Refunded {
                    ProcurementStatus::Refunded
                } else {
                    ProcurementStatus::PartiallyRefunded
                };
                let change = ProcurementChange {
                    status: Some(status),
                    upstream_payload: fulfillment.map(|f| f.payload.clone()),
                    ..ProcurementChange::default()
                };
                self.repo.update(p.id, &[], &change, now).await?;
            }
            UpstreamEvent::Other(status) => {
                tracing::warn!(
                    procurement_order_id = p.id,
                    status,
                    "unknown upstream status"
                );
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ polling

    /// `procurement:poll_status`: short polling after acceptance; exhausting the
    /// schedule never fails the order (the periodic sync takes over, UPS-11).
    pub async fn poll(&self, id: Id) -> Result<()> {
        let Some(p) = self.repo.get(id).await? else {
            return Ok(());
        };
        if p.status != ProcurementStatus::Accepted {
            return Ok(());
        }
        if let Some(f) = p.held_delivery.as_ref().filter(|f| !is_unconfirmed(f)) {
            // Delivered synchronously with the order: nothing to ask the supplier.
            return match self
                .handle_event(p.id, &UpstreamEvent::Delivered, Some(f))
                .await
            {
                Ok(()) => Ok(()),
                Err(error) => {
                    tracing::warn!(%error, procurement_order_id = p.id, "apply held delivery failed");
                    self.requeue_poll(&p).await
                }
            };
        }
        let (_, client) = self.connections.client_of(p.connection_id).await?;
        match client.get_order(&p.upstream_ref()).await {
            Ok(detail) => {
                let event = normalize_upstream_status(&detail.status);
                if matches!(event, UpstreamEvent::Other(_)) {
                    if let Some(f) =
                        confirmed_by_change(p.held_delivery.as_ref(), detail.fulfillment.as_ref())
                    {
                        return self
                            .handle_event(p.id, &UpstreamEvent::Delivered, Some(&f))
                            .await;
                    }
                    self.requeue_poll(&p).await
                } else {
                    let f = if event == UpstreamEvent::Canceled {
                        None
                    } else {
                        detail.fulfillment.as_ref()
                    };
                    self.handle_event(p.id, &event, f).await
                }
            }
            Err(error) => {
                tracing::warn!(%error, procurement_order_id = p.id, "procurement poll failed");
                self.requeue_poll(&p).await
            }
        }
    }

    async fn requeue_poll(&self, p: &ProcurementOrder) -> Result<()> {
        let Some(delay) = poll_delay(p.retry_count) else {
            tracing::info!(
                procurement_order_id = p.id,
                "procurement poll handed off to periodic sync"
            );
            return Ok(());
        };
        let change = ProcurementChange {
            retry_count: Some(p.retry_count + 1),
            ..ProcurementChange::default()
        };
        if self
            .repo
            .update(p.id, &[ProcurementStatus::Accepted], &change, self.now())
            .await?
        {
            self.enqueue(kinds::PROCUREMENT_POLL, p.id, Some(self.now() + delay), 1)
                .await;
        }
        Ok(())
    }

    /// `procurement:sync_accepted` (every 30 min): re-checks accepted orders; alerts on
    /// orders stuck in accepted for more than 24 h (UPS-07).
    pub async fn sync_accepted(&self) -> Result<()> {
        let orders = self.repo.list_accepted(SYNC_ACCEPTED_BATCH).await?;
        for p in &orders {
            if let Some(f) = p.held_delivery.as_ref().filter(|f| !is_unconfirmed(f)) {
                if let Err(error) = self
                    .handle_event(p.id, &UpstreamEvent::Delivered, Some(f))
                    .await
                {
                    tracing::warn!(%error, procurement_order_id = p.id, "sync accepted: apply held delivery failed");
                }
                continue;
            }
            let client = match self.connections.client_of(p.connection_id).await {
                Ok((_, c)) => c,
                Err(error) => {
                    tracing::warn!(%error, procurement_order_id = p.id, "sync accepted: no connection");
                    continue;
                }
            };
            if !client.addressable(&p.upstream_ref()) {
                continue;
            }
            let detail = match client.get_order(&p.upstream_ref()).await {
                Ok(d) => d,
                Err(error) => {
                    tracing::warn!(%error, procurement_order_id = p.id, "sync accepted: poll failed");
                    continue;
                }
            };
            let event = normalize_upstream_status(&detail.status);
            if matches!(event, UpstreamEvent::Other(_))
                && let Some(f) =
                    confirmed_by_change(p.held_delivery.as_ref(), detail.fulfillment.as_ref())
            {
                if let Err(error) = self
                    .handle_event(p.id, &UpstreamEvent::Delivered, Some(&f))
                    .await
                {
                    tracing::warn!(%error, procurement_order_id = p.id, "sync accepted: apply confirmed delivery failed");
                }
                continue;
            }
            match &event {
                UpstreamEvent::Other(status) => {
                    let age = self.now() - p.updated_at;
                    if age > Duration::hours(ACCEPTED_ALERT_AFTER_HOURS) {
                        let message = format!(
                            "procurement order stuck in accepted for {}h, upstream status: {status}",
                            age.num_hours()
                        );
                        self.alert(p, &message).await;
                    }
                }
                _ => {
                    let f = if event == UpstreamEvent::Canceled {
                        None
                    } else {
                        detail.fulfillment.as_ref()
                    };
                    if let Err(error) = self.handle_event(p.id, &event, f).await {
                        tracing::warn!(%error, procurement_order_id = p.id, "sync accepted: apply failed");
                    }
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------ admin

    async fn fill_parent_nos(&self, orders: &mut [ProcurementOrder]) {
        let parent_ids: Vec<Id> = orders
            .iter()
            .filter_map(|o| o.local_order.as_ref().and_then(|l| l.parent_id))
            .collect();
        if parent_ids.is_empty() {
            return;
        }
        let Ok(parents) = self.orders.get_many(&parent_ids).await else {
            return;
        };
        for o in orders.iter_mut() {
            let Some(local) = o.local_order.as_mut() else {
                continue;
            };
            let Some(parent) = local
                .parent_id
                .and_then(|pid| parents.iter().find(|p| p.id == pid))
            else {
                continue;
            };
            o.parent_order_no = parent.order_no.clone();
            // Child without its own refund shows the parent's refunded amount.
            if !local.refunded_amount.is_positive() && parent.refunded_amount.is_positive() {
                local.refunded_amount = parent.refunded_amount;
            }
        }
    }

    pub async fn list(&self, filter: &ProcurementFilter) -> Result<Page<ProcurementOrder>> {
        let mut page = self
            .repo
            .list(filter)
            .await
            .map_err(|e| e.or_internal(keys::PROCUREMENT_FETCH_FAILED))?;
        self.fill_parent_nos(&mut page.items).await;
        Ok(page)
    }

    /// Counts per status over the whole filtered set (status filter ignored, UPS-22).
    pub async fn stats(&self, filter: &ProcurementFilter) -> Result<(u64, Vec<(String, u64)>)> {
        let rows = self
            .repo
            .stats(filter)
            .await
            .map_err(|e| e.or_internal(keys::PROCUREMENT_FETCH_FAILED))?;
        Ok((rows.iter().map(|r| r.1).sum(), rows))
    }

    /// Full purchase order (payload untruncated).
    pub async fn get(&self, id: Id) -> Result<ProcurementOrder> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::PROCUREMENT_FETCH_FAILED))?
            .ok_or_else(not_found)
    }

    /// Detail: payload preview, parent order number and the supplier's refund view
    /// (refund status synced when the supplier reports one).
    pub async fn detail(&self, id: Id) -> Result<ProcurementOrder> {
        let mut order = self.get(id).await?;
        self.fill_refunds(&mut order).await;
        let (preview, lines) = truncate_payload(&order.upstream_payload, PAYLOAD_PREVIEW_MAX_LINES);
        order.upstream_payload = preview;
        order.upstream_payload_line_count = lines;
        self.fill_parent_nos(std::slice::from_mut(&mut order)).await;
        Ok(order)
    }

    async fn fill_refunds(&self, order: &mut ProcurementOrder) {
        let relevant = matches!(
            order.status,
            ProcurementStatus::Fulfilled
                | ProcurementStatus::Completed
                | ProcurementStatus::PartiallyRefunded
                | ProcurementStatus::Refunded
        );
        if !relevant {
            return;
        }
        let Ok((_, client)) = self.connections.client_of(order.connection_id).await else {
            return;
        };
        if !client.addressable(&order.upstream_ref()) {
            return;
        }
        let Ok(detail) = client.get_order(&order.upstream_ref()).await else {
            return;
        };
        order.upstream_refund_records = normalize_refund_records(&detail.refund_records);
        let refunded = detail.refunded_amount.trim();
        if refunded
            .parse::<rust_decimal::Decimal>()
            .is_ok_and(|v| v.round_dp(2) > rust_decimal::Decimal::ZERO)
        {
            order.upstream_refunded_amount = refunded.to_owned();
        }
        let target = match normalize_upstream_status(&detail.status) {
            UpstreamEvent::Refunded => ProcurementStatus::Refunded,
            UpstreamEvent::PartiallyRefunded => ProcurementStatus::PartiallyRefunded,
            _ => return,
        };
        if order.status == target {
            return;
        }
        let change = ProcurementChange {
            status: Some(target),
            ..ProcurementChange::default()
        };
        match self.repo.update(order.id, &[], &change, self.now()).await {
            Ok(_) => order.status = target,
            Err(error) => tracing::warn!(%error, "sync procurement refund status failed"),
        }
    }

    /// Manual retry of a failed / rejected purchase order. A purchase held for manual
    /// review is re-checked at the supplier when its order number is known, else
    /// submitted again with the same deterministic request number (the supplier's
    /// deduplication refuses a second purchase).
    pub async fn retry(&self, id: Id) -> Result<()> {
        let p = self.get(id).await?;
        if p.status == ProcurementStatus::ManualReview && p.upstream_ref().is_known() {
            let change = ProcurementChange {
                status: Some(ProcurementStatus::Accepted),
                retry_count: Some(0),
                next_retry_at: Some(None),
                ..ProcurementChange::default()
            };
            let changed = self
                .repo
                .update(
                    p.id,
                    &[ProcurementStatus::ManualReview],
                    &change,
                    self.now(),
                )
                .await
                .map_err(|e| e.or_internal(keys::PROCUREMENT_RETRY_FAILED))?;
            if !changed {
                return Err(status_invalid());
            }
            self.enqueue(kinds::PROCUREMENT_POLL, p.id, None, 1).await;
            return Ok(());
        }
        // An earlier attempt that may have been executed keeps the purchase out of the
        // automatic rollback after this retry too.
        let carried = if maybe_executed(&p.error_message) {
            mark_maybe_executed("retried by an admin")
        } else {
            String::new()
        };
        let change = ProcurementChange {
            status: Some(ProcurementStatus::Pending),
            retry_count: Some(0),
            next_retry_at: Some(None),
            error_message: Some(carried),
            ..ProcurementChange::default()
        };
        let changed = self
            .repo
            .update(
                p.id,
                &[
                    ProcurementStatus::Failed,
                    ProcurementStatus::Rejected,
                    ProcurementStatus::ManualReview,
                ],
                &change,
                self.now(),
            )
            .await
            .map_err(|e| e.or_internal(keys::PROCUREMENT_RETRY_FAILED))?;
        if !changed {
            return Err(status_invalid());
        }
        self.enqueue(kinds::PROCUREMENT_SUBMIT, p.id, None, SUBMIT_ATTEMPTS)
            .await;
        Ok(())
    }

    /// Manual cancel = the admin gives up on the purchase (LQA-I3). The local order is
    /// rolled back and **refunded** (buyer wallet; guest orders get a manual refund
    /// record the admin pays back through the original method) when nothing can still
    /// be delivered: never submitted / failed / rejected / held for review (the admin
    /// checked the supplier), or accepted and canceled at the supplier. An accepted
    /// purchase the supplier refused to cancel, or a submitted one whose answer is
    /// unknown, is canceled locally without a refund (goods may still arrive) and the
    /// admin decides through the order refund.
    pub async fn cancel(&self, id: Id) -> Result<CancelOutcome> {
        let p = self.get(id).await?;
        let open = [
            ProcurementStatus::Pending,
            ProcurementStatus::Submitted,
            ProcurementStatus::Accepted,
            ProcurementStatus::Rejected,
            ProcurementStatus::Failed,
            ProcurementStatus::ManualReview,
        ];
        if !open.contains(&p.status) {
            return Err(status_invalid());
        }
        let mut settled = matches!(
            p.status,
            ProcurementStatus::Pending
                | ProcurementStatus::Failed
                | ProcurementStatus::Rejected
                | ProcurementStatus::ManualReview
        );
        if p.status == ProcurementStatus::Accepted
            && let Ok((_, client)) = self.connections.client_of(p.connection_id).await
            && client.addressable(&p.upstream_ref())
        {
            match client.cancel_order(&p.upstream_ref()).await {
                Ok(()) => settled = true,
                Err(error) => {
                    tracing::warn!(%error, procurement_order_id = p.id, "cancel upstream order failed");
                }
            }
        }
        let change = ProcurementChange {
            status: Some(ProcurementStatus::Canceled),
            error_message: Some(if settled {
                "manually canceled".into()
            } else {
                "manually canceled (supplier cancel not confirmed: no refund)".into()
            }),
            next_retry_at: Some(None),
            ..ProcurementChange::default()
        };
        // Race guard: only the status the admin saw is canceled.
        let changed = self
            .repo
            .update(p.id, &[p.status], &change, self.now())
            .await
            .map_err(|e| e.or_internal(keys::PROCUREMENT_CANCEL_FAILED))?;
        if !changed {
            return Err(status_invalid());
        }
        if let Err(error) = self.lifecycle.rollback_failed(p.local_order_id).await {
            tracing::warn!(%error, local_order_id = p.local_order_id, "local order rollback failed");
        }
        if !settled {
            self.alert(
                &p,
                "canceled by an admin; supplier cancel not confirmed, no refund",
            )
            .await;
            return Ok(CancelOutcome::default());
        }
        let refund = self
            .lifecycle
            .refund_failed(
                p.local_order_id,
                &format!("采购失败退款 {}", p.local_order_no),
            )
            .await
            .map_err(|e| e.or_internal(keys::PROCUREMENT_CANCEL_FAILED))?;
        tracing::info!(
            procurement_order_id = p.id,
            ?refund,
            "procurement canceled and refunded"
        );
        Ok(CancelOutcome { refund })
    }

    /// Purchase orders needing an admin (LQA-I3), keyed by local order id.
    pub async fn issues(&self, local_order_ids: &[Id]) -> Result<HashMap<Id, String>> {
        let mut out = HashMap::new();
        for id in local_order_ids {
            if let Some(p) = self.repo.get_by_local_order_id(*id).await?
                && matches!(
                    p.status,
                    ProcurementStatus::Rejected
                        | ProcurementStatus::ManualReview
                        | ProcurementStatus::Canceled
                )
            {
                out.insert(*id, p.status.as_str().to_owned());
            }
        }
        Ok(out)
    }

    pub async fn find_by_local_order_no(&self, no: &str) -> Result<Option<ProcurementOrder>> {
        self.repo.get_by_local_order_no(no).await
    }
}
