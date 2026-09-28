//! Inbound supplier requests (callbacks / pushed events) for every protocol: the
//! adapter pre-checks and identifies the connection, verifies and parses; this
//! protocol-neutral core then deduplicates event ids, enforces ownership (UPS-03) and
//! the procurement state machine (UPS-02), and reacts to catalog notices.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zs_domain::Id;
use zs_domain::integration::adapter::{
    InboundError, InboundKind, InboundRequest, OrderNotice, ProcessedEvents,
};
use zs_domain::integration::connection::{ConnectionStatus, SiteConnection};
use zs_domain::integration::procurement::{UpstreamEvent, normalize_upstream_status};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_shared::clock::Clock;

use super::connection::ConnectionService;
use super::procurement::ProcurementService;

/// Why an inbound request was not processed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InboundFailure {
    /// Refused by the adapter (headers, clock, signature, body).
    Adapter(InboundError),
    /// No adapter for the protocol of the route.
    UnknownProtocol,
    /// Unknown key, inactive connection or a connection of another protocol.
    InvalidKey,
    /// Purchase order missing or not owned by the connection (UPS-03).
    NotFound,
    /// The state transition failed (e.g. fulfillment write); the supplier retries.
    Processing,
    Internal,
}

/// Payload of `upstream:sync_connection`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SyncConnectionJob {
    pub connection_id: Id,
}

/// Inbound request handling.
#[derive(Clone)]
pub struct InboundService {
    connections: ConnectionService,
    procurement: ProcurementService,
    processed: Arc<dyn ProcessedEvents>,
    queue: Arc<dyn JobQueue>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for InboundService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InboundService")
    }
}

impl InboundService {
    pub fn new(
        connections: ConnectionService,
        procurement: ProcurementService,
        processed: Arc<dyn ProcessedEvents>,
        queue: Arc<dyn JobQueue>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            connections,
            procurement,
            processed,
            queue,
            clock,
        }
    }

    /// Handles one inbound request of `protocol`; returns the number of events applied.
    pub async fn handle(
        &self,
        protocol: &str,
        req: &InboundRequest<'_>,
    ) -> std::result::Result<usize, InboundFailure> {
        let adapter = self
            .connections
            .adapter(protocol)
            .ok_or(InboundFailure::UnknownProtocol)?;
        let now = self.clock.now();
        let key = adapter
            .inbound_key(req, now)
            .map_err(InboundFailure::Adapter)?;
        let conn = match self.connections.find_by_key(&key).await {
            Ok(Some(c)) if c.status == ConnectionStatus::Active && c.protocol == protocol => c,
            Ok(_) => return Err(InboundFailure::InvalidKey),
            Err(error) => {
                tracing::error!(%error, "inbound connection lookup failed");
                return Err(InboundFailure::Internal);
            }
        };
        // An undecryptable secret is a configuration error: reject, never fall back to
        // the ciphertext (UPS-02).
        let secret = self.connections.decrypt_secret(&conn).map_err(|error| {
            tracing::error!(%error, connection_id = conn.id, "inbound secret decrypt failed");
            InboundFailure::Internal
        })?;
        if secret.is_empty() {
            return Err(InboundFailure::Adapter(InboundError::InvalidSignature));
        }
        let events = adapter
            .parse_inbound(req, &secret, now)
            .map_err(InboundFailure::Adapter)?;
        let mut applied = 0;
        for event in events {
            if let Some(id) = &event.event_id {
                match self.processed.seen(conn.id, id).await {
                    Ok(true) => {
                        tracing::debug!(connection_id = conn.id, event_id = %id, "duplicate inbound event");
                        continue;
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::error!(%error, "inbound event dedupe lookup failed");
                        return Err(InboundFailure::Internal);
                    }
                }
            }
            match &event.kind {
                InboundKind::Order(notice) => self.order_notice(&conn, notice).await?,
                InboundKind::CatalogChanged => self.catalog_changed(conn.id).await,
                InboundKind::BalanceLow { balance, threshold } => {
                    tracing::warn!(connection_id = conn.id, %balance, %threshold, "supplier balance low");
                }
                InboundKind::Other(kind) => {
                    tracing::debug!(connection_id = conn.id, kind, "inbound event ignored");
                }
            }
            // Recorded only after success so a failed delivery is retried (UPS-02).
            if let Some(id) = &event.event_id
                && let Err(error) = self.processed.record(conn.id, id, self.clock.now()).await
            {
                tracing::warn!(%error, "record inbound event failed");
            }
            applied += 1;
        }
        Ok(applied)
    }

    /// Order status from the supplier: ownership (connection + registered supplier
    /// order identity, UPS-03), then the idempotent state machine (UPS-02/16).
    async fn order_notice(
        &self,
        conn: &SiteConnection,
        notice: &OrderNotice,
    ) -> std::result::Result<(), InboundFailure> {
        let procurement = match self
            .procurement
            .find_by_local_order_no(&notice.downstream_order_no)
            .await
        {
            Ok(Some(p)) => p,
            _ => return Err(InboundFailure::NotFound),
        };
        let registered = procurement.upstream_ref();
        let id_mismatch = registered.id != 0 && notice.upstream.id != registered.id;
        let no_mismatch = registered.id == 0
            && !registered.no.is_empty()
            && !notice.upstream.no.is_empty()
            && notice.upstream.no != registered.no;
        if procurement.connection_id != conn.id || id_mismatch || no_mismatch {
            tracing::warn!(
                connection_id = conn.id,
                procurement_connection_id = procurement.connection_id,
                notice_order_id = notice.upstream.id,
                notice_order_no = %notice.upstream.no,
                "inbound ownership mismatch"
            );
            return Err(InboundFailure::NotFound);
        }
        let event = normalize_upstream_status(&notice.status);
        let fulfillment = if event == UpstreamEvent::Canceled {
            None
        } else {
            notice.fulfillment.as_ref()
        };
        self.procurement
            .handle_event(procurement.id, &event, fulfillment)
            .await
            .map_err(|error| {
                tracing::warn!(%error, procurement_order_id = procurement.id, "inbound processing failed");
                InboundFailure::Processing
            })
    }

    /// The supplier's catalog changed: pull its change feed soon (one pending job per
    /// connection).
    async fn catalog_changed(&self, connection_id: Id) {
        let job = NewJob::new(
            kinds::UPSTREAM_SYNC_CONNECTION,
            SyncConnectionJob { connection_id },
        )
        .map(|j| {
            j.attempts(1).unique(format!(
                "{}:{connection_id}",
                kinds::UPSTREAM_SYNC_CONNECTION
            ))
        });
        let r = match job {
            Ok(j) => self.queue.enqueue(j).await,
            Err(e) => Err(e),
        };
        if let Err(error) = r {
            tracing::warn!(%error, connection_id, "enqueue connection sync failed");
        }
    }
}
