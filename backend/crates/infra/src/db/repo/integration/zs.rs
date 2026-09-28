//! Storage of the zebra-store supplier side ([`ZsStore`]), credential secret rotation
//! and nonces ([`CredentialSecurityRepo`]), processed inbound events
//! ([`ProcessedEvents`]) and the catalog projection of the change-feed producer
//! ([`CatalogSnapshotSource`]).

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use sea_orm::sea_query::Expr;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set, TransactionTrait,
};
use serde_json::{Value, json};
use zs_domain::catalog::product::CatalogLookup;
use zs_domain::integration::adapter::ProcessedEvents;
use zs_domain::integration::credential::{CredentialSecurityRepo, Rotation};
use zs_domain::integration::protocol::RemoteProduct;
use zs_domain::integration::supplier::{MemberPricing, SupplierCatalog, to_remote_product};
use zs_domain::integration::zs::{
    CatalogSnapshotSource, ChangeRow, EventStatus, IDEMPOTENCY_TTL_HOURS, NewChange, OrderRequest,
    OutEvent, ProductSnapshot, QuotedLine, StoredQuote, Webhook, ZsStore,
};
use zs_domain::{Error, Id, Result};
use zs_shared::money::Amount;

use crate::db::entity::{
    api_credential_rotations as rotations, api_credentials, api_request_nonces as nonces,
    integration_processed_events as processed, orders, zs_catalog_snapshots as snapshots,
    zs_change_log as changes, zs_order_requests as requests, zs_quotes as quotes,
    zs_webhook_events as events, zs_webhooks as webhooks,
};
use crate::db::repo::catalog::lookup::SeaCatalogLookup;
use crate::db::repo::integration::supplier::SeaSupplierCatalog;
use crate::db::repo::support::{DbResultExt, from_json, insert_if_absent, to_json};

/// SeaORM implementation of the zebra-store / credential-security / inbound ports.
#[derive(Debug, Clone)]
pub struct SeaZsStore {
    db: DatabaseConnection,
}

impl SeaZsStore {
    pub fn new(db: DatabaseConnection) -> Self {
        Self { db }
    }
}

// ---------------------------------------------------------------------------
// Credential security
// ---------------------------------------------------------------------------

#[async_trait]
impl CredentialSecurityRepo for SeaZsStore {
    async fn rotation(&self, credential_id: Id) -> Result<Option<Rotation>> {
        Ok(rotations::Entity::find()
            .filter(rotations::Column::CredentialId.eq(credential_id))
            .one(&self.db)
            .await
            .dom()?
            .map(|m| Rotation {
                credential_id: m.credential_id,
                secret_next: m.secret_next,
                expires_at: m.expires_at,
            }))
    }

    async fn put_rotation(&self, r: &Rotation, now: DateTime<Utc>) -> Result<()> {
        let txn = self.db.begin().await.dom()?;
        rotations::Entity::delete_many()
            .filter(rotations::Column::CredentialId.eq(r.credential_id))
            .exec(&txn)
            .await
            .dom()?;
        rotations::ActiveModel {
            credential_id: Set(r.credential_id),
            secret_next: Set(r.secret_next.clone()),
            expires_at: Set(r.expires_at),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .dom()?;
        txn.commit().await.dom()
    }

    async fn promote_rotation(&self, credential_id: Id, now: DateTime<Utc>) -> Result<bool> {
        let txn = self.db.begin().await.dom()?;
        let Some(r) = rotations::Entity::find()
            .filter(rotations::Column::CredentialId.eq(credential_id))
            .one(&txn)
            .await
            .dom()?
        else {
            return Ok(false);
        };
        // Conditional delete: only one concurrent promotion wins.
        let deleted = rotations::Entity::delete_many()
            .filter(rotations::Column::Id.eq(r.id))
            .exec(&txn)
            .await
            .dom()?;
        if deleted.rows_affected != 1 {
            return Ok(false);
        }
        api_credentials::Entity::update_many()
            .col_expr(
                api_credentials::Column::ApiSecret,
                Expr::value(r.secret_next),
            )
            .col_expr(api_credentials::Column::UpdatedAt, Expr::value(now))
            .filter(api_credentials::Column::Id.eq(credential_id))
            .exec(&txn)
            .await
            .dom()?;
        txn.commit().await.dom()?;
        Ok(true)
    }

    async fn clear_rotation(&self, credential_id: Id) -> Result<()> {
        rotations::Entity::delete_many()
            .filter(rotations::Column::CredentialId.eq(credential_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn claim_nonce(
        &self,
        credential_id: Id,
        nonce: &str,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        let key = format!("{credential_id}:{nonce}");
        let fresh = insert_if_absent(
            &self.db,
            nonces::ActiveModel {
                nonce_key: Set(key.clone()),
                credential_id: Set(credential_id),
                created_at: Set(now),
                ..Default::default()
            },
            nonces::Column::NonceKey,
        )
        .await?;
        if fresh {
            return Ok(true);
        }
        // A row older than the replay window (not purged yet) may be reused.
        let stale = nonces::Entity::update_many()
            .col_expr(nonces::Column::CreatedAt, Expr::value(now))
            .filter(nonces::Column::NonceKey.eq(key))
            .filter(
                nonces::Column::CreatedAt
                    .lt(now - Duration::seconds(zs_shared::zs::NONCE_TTL_SECS)),
            )
            .exec(&self.db)
            .await
            .dom()?;
        Ok(stale.rows_affected == 1)
    }

    async fn purge_nonces(&self, before: DateTime<Utc>) -> Result<u64> {
        Ok(nonces::Entity::delete_many()
            .filter(nonces::Column::CreatedAt.lt(before))
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected)
    }
}

// ---------------------------------------------------------------------------
// Processed inbound events
// ---------------------------------------------------------------------------

#[async_trait]
impl ProcessedEvents for SeaZsStore {
    async fn seen(&self, connection_id: Id, event_id: &str) -> Result<bool> {
        Ok(processed::Entity::find()
            .filter(processed::Column::EventKey.eq(format!("{connection_id}:{event_id}")))
            .one(&self.db)
            .await
            .dom()?
            .is_some())
    }

    async fn record(&self, connection_id: Id, event_id: &str, now: DateTime<Utc>) -> Result<()> {
        let key: String = format!("{connection_id}:{event_id}")
            .chars()
            .take(191)
            .collect();
        insert_if_absent(
            &self.db,
            processed::ActiveModel {
                event_key: Set(key),
                connection_id: Set(connection_id),
                created_at: Set(now),
                ..Default::default()
            },
            processed::Column::EventKey,
        )
        .await?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Supplier side
// ---------------------------------------------------------------------------

fn snapshot_of(m: snapshots::Model) -> ProductSnapshot {
    let skus: Vec<(Id, String, i64)> = from_json(m.skus);
    ProductSnapshot {
        product_id: m.product_id,
        version: m.version,
        skus,
    }
}

fn change_of(m: changes::Model) -> ChangeRow {
    ChangeRow {
        seq: m.id,
        kind: m.kind,
        product_id: m.product_id,
        sku_id: m.sku_id,
        at: m.created_at,
        data: m.data.unwrap_or_else(|| json!({})),
    }
}

fn webhook_of(m: webhooks::Model) -> Webhook {
    Webhook {
        credential_id: m.credential_id,
        url: m.url,
        events: from_json(m.events),
        balance_low_threshold: m.balance_low_threshold.map(Amount::new),
        last_balance_low_at: m.last_balance_low_at,
        created_at: m.created_at,
        updated_at: m.updated_at,
    }
}

fn event_of(m: events::Model) -> OutEvent {
    OutEvent {
        id: m.id,
        event_id: m.event_id,
        credential_id: m.credential_id,
        kind: m.kind,
        body: serde_json::from_str(&m.body).unwrap_or(Value::Null),
        status: EventStatus::from_stored(&m.status),
        attempts: m.attempts,
        last_error: m.last_error,
    }
}

fn request_of(m: requests::Model) -> OrderRequest {
    OrderRequest {
        credential_id: m.credential_id,
        idempotency_key: m.idempotency_key,
        request_hash: m.request_hash,
        order_id: m.order_id,
        order_no: m.order_no,
        callback: m.callback,
        created_at: m.created_at,
    }
}

fn request_key(credential_id: Id, key: &str) -> String {
    format!("{credential_id}:{key}")
}

#[async_trait]
impl ZsStore for SeaZsStore {
    async fn snapshots(&self) -> Result<Vec<ProductSnapshot>> {
        Ok(snapshots::Entity::find()
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(snapshot_of)
            .collect())
    }

    async fn apply_snapshot_diff(
        &self,
        list: &[NewChange],
        upserts: &[ProductSnapshot],
        deletes: &[Id],
        now: DateTime<Utc>,
    ) -> Result<Option<i64>> {
        let txn = self.db.begin().await.dom()?;
        let mut latest = None;
        for c in list {
            let res = changes::Entity::insert(changes::ActiveModel {
                kind: Set(c.kind.to_owned()),
                product_id: Set(c.product_id),
                sku_id: Set(c.sku_id),
                data: Set(Some(c.data.clone())),
                created_at: Set(now),
                ..Default::default()
            })
            .exec(&txn)
            .await
            .dom()?;
            latest = Some(res.last_insert_id);
        }
        for s in upserts {
            let skus = to_json(&s.skus)?;
            let existing = snapshots::Entity::find()
                .filter(snapshots::Column::ProductId.eq(s.product_id))
                .one(&txn)
                .await
                .dom()?;
            match existing {
                Some(m) => {
                    snapshots::ActiveModel {
                        id: Set(m.id),
                        version: Set(s.version.clone()),
                        skus: Set(skus),
                        updated_at: Set(now),
                        ..Default::default()
                    }
                    .update(&txn)
                    .await
                    .dom()?;
                }
                None => {
                    snapshots::ActiveModel {
                        product_id: Set(s.product_id),
                        version: Set(s.version.clone()),
                        skus: Set(skus),
                        created_at: Set(now),
                        updated_at: Set(now),
                        ..Default::default()
                    }
                    .insert(&txn)
                    .await
                    .dom()?;
                }
            }
        }
        if !deletes.is_empty() {
            snapshots::Entity::delete_many()
                .filter(snapshots::Column::ProductId.is_in(deletes.to_vec()))
                .exec(&txn)
                .await
                .dom()?;
        }
        txn.commit().await.dom()?;
        Ok(latest)
    }

    async fn changes_after(&self, since: i64, limit: u64) -> Result<Vec<ChangeRow>> {
        Ok(changes::Entity::find()
            .filter(changes::Column::Id.gt(since))
            .order_by_asc(changes::Column::Id)
            .limit(limit.max(1))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(change_of)
            .collect())
    }

    async fn change_bounds(&self) -> Result<Option<(i64, i64)>> {
        let row: Option<(Option<i64>, Option<i64>)> = changes::Entity::find()
            .select_only()
            .column_as(changes::Column::Id.min(), "min_id")
            .column_as(changes::Column::Id.max(), "max_id")
            .into_tuple()
            .one(&self.db)
            .await
            .dom()?;
        Ok(match row {
            Some((Some(min), Some(max))) => Some((min, max)),
            _ => None,
        })
    }

    async fn purge_changes(&self, before: DateTime<Utc>) -> Result<u64> {
        let Some((_, max)) = self.change_bounds().await? else {
            return Ok(0);
        };
        Ok(changes::Entity::delete_many()
            .filter(changes::Column::CreatedAt.lt(before))
            .filter(changes::Column::Id.lt(max))
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected)
    }

    async fn webhook(&self, credential_id: Id) -> Result<Option<Webhook>> {
        Ok(webhooks::Entity::find()
            .filter(webhooks::Column::CredentialId.eq(credential_id))
            .one(&self.db)
            .await
            .dom()?
            .map(webhook_of))
    }

    async fn put_webhook(&self, h: &Webhook, now: DateTime<Utc>) -> Result<Webhook> {
        let existing = webhooks::Entity::find()
            .filter(webhooks::Column::CredentialId.eq(h.credential_id))
            .one(&self.db)
            .await
            .dom()?;
        let threshold = h.balance_low_threshold.map(|a| a.decimal());
        let row = match existing {
            Some(m) => webhooks::ActiveModel {
                id: Set(m.id),
                url: Set(h.url.clone()),
                events: Set(to_json(&h.events)?),
                balance_low_threshold: Set(threshold),
                updated_at: Set(now),
                ..Default::default()
            }
            .update(&self.db)
            .await
            .dom()?,
            None => webhooks::ActiveModel {
                credential_id: Set(h.credential_id),
                url: Set(h.url.clone()),
                events: Set(to_json(&h.events)?),
                balance_low_threshold: Set(threshold),
                last_balance_low_at: Set(None),
                created_at: Set(now),
                updated_at: Set(now),
                ..Default::default()
            }
            .insert(&self.db)
            .await
            .dom()?,
        };
        Ok(webhook_of(row))
    }

    async fn delete_webhook(&self, credential_id: Id) -> Result<bool> {
        Ok(webhooks::Entity::delete_many()
            .filter(webhooks::Column::CredentialId.eq(credential_id))
            .exec(&self.db)
            .await
            .dom()?
            .rows_affected
            > 0)
    }

    async fn webhooks(&self) -> Result<Vec<Webhook>> {
        Ok(webhooks::Entity::find()
            .order_by_asc(webhooks::Column::Id)
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(webhook_of)
            .collect())
    }

    async fn mark_balance_low(&self, credential_id: Id, at: DateTime<Utc>) -> Result<()> {
        webhooks::Entity::update_many()
            .col_expr(webhooks::Column::LastBalanceLowAt, Expr::value(at))
            .filter(webhooks::Column::CredentialId.eq(credential_id))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn create_event(
        &self,
        credential_id: Id,
        event_id: &str,
        kind: &str,
        body: &Value,
        now: DateTime<Utc>,
    ) -> Result<OutEvent> {
        let row = events::ActiveModel {
            event_id: Set(event_id.to_owned()),
            credential_id: Set(credential_id),
            kind: Set(kind.to_owned()),
            body: Set(body.to_string()),
            status: Set(EventStatus::Pending.as_str().to_owned()),
            attempts: Set(0),
            next_attempt_at: Set(Some(now)),
            last_error: Set(String::new()),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(event_of(row))
    }

    async fn event(&self, id: Id) -> Result<Option<OutEvent>> {
        Ok(events::Entity::find_by_id(id)
            .one(&self.db)
            .await
            .dom()?
            .map(event_of))
    }

    async fn update_event(
        &self,
        id: Id,
        status: EventStatus,
        attempts: i32,
        next_attempt_at: Option<DateTime<Utc>>,
        last_error: &str,
        now: DateTime<Utc>,
    ) -> Result<()> {
        events::ActiveModel {
            id: Set(id),
            status: Set(status.as_str().to_owned()),
            attempts: Set(attempts),
            next_attempt_at: Set(next_attempt_at),
            last_error: Set(last_error.chars().take(2000).collect()),
            updated_at: Set(now),
            ..Default::default()
        }
        .update(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn save_quote(&self, q: &StoredQuote, now: DateTime<Utc>) -> Result<()> {
        let lines: Vec<Value> = q
            .lines
            .iter()
            .map(|l| {
                json!({
                    "sku_id": l.sku_id,
                    "product_id": l.product_id,
                    "quantity": l.quantity,
                    "unit_price": l.unit_price.to_string(),
                })
            })
            .collect();
        quotes::ActiveModel {
            quote_id: Set(q.quote_id.clone()),
            credential_id: Set(q.credential_id),
            lines: Set(Some(Value::Array(lines))),
            total: Set(q.total.decimal()),
            currency: Set(q.currency.chars().take(16).collect()),
            expires_at: Set(q.expires_at),
            created_at: Set(now),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .dom()?;
        Ok(())
    }

    async fn quote(&self, quote_id: &str) -> Result<Option<StoredQuote>> {
        let Some(m) = quotes::Entity::find()
            .filter(quotes::Column::QuoteId.eq(quote_id))
            .one(&self.db)
            .await
            .dom()?
        else {
            return Ok(None);
        };
        let raw: Vec<Value> = from_json(m.lines);
        let lines = raw
            .iter()
            .map(|l| {
                let int = |k: &str| l.get(k).and_then(Value::as_i64).unwrap_or(0);
                QuotedLine {
                    sku_id: int("sku_id"),
                    product_id: int("product_id"),
                    quantity: i32::try_from(int("quantity")).unwrap_or(0),
                    unit_price: l
                        .get("unit_price")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(Amount::ZERO),
                }
            })
            .collect();
        Ok(Some(StoredQuote {
            quote_id: m.quote_id,
            credential_id: m.credential_id,
            lines,
            total: Amount::new(m.total),
            currency: m.currency,
            expires_at: m.expires_at,
        }))
    }

    async fn reserve_request(
        &self,
        req: &OrderRequest,
        now: DateTime<Utc>,
    ) -> Result<Option<OrderRequest>> {
        let key = request_key(req.credential_id, &req.idempotency_key);
        let expired_before = now - Duration::hours(IDEMPOTENCY_TTL_HOURS);
        for _ in 0..2 {
            let inserted = insert_if_absent(
                &self.db,
                requests::ActiveModel {
                    request_key: Set(key.clone()),
                    credential_id: Set(req.credential_id),
                    idempotency_key: Set(req.idempotency_key.clone()),
                    request_hash: Set(req.request_hash.clone()),
                    order_id: Set(0),
                    order_no: Set(String::new()),
                    callback: Set(req.callback),
                    created_at: Set(now),
                    updated_at: Set(now),
                    ..Default::default()
                },
                requests::Column::RequestKey,
            )
            .await?;
            if inserted {
                return Ok(None);
            }
            let Some(existing) = requests::Entity::find()
                .filter(requests::Column::RequestKey.eq(key.clone()))
                .one(&self.db)
                .await
                .dom()?
            else {
                continue;
            };
            if existing.created_at >= expired_before {
                return Ok(Some(request_of(existing)));
            }
            // Past the 24 h window the key is free again.
            requests::Entity::delete_many()
                .filter(requests::Column::Id.eq(existing.id))
                .exec(&self.db)
                .await
                .dom()?;
        }
        Err(Error::internal_msg("idempotency key reservation raced"))
    }

    async fn complete_request(
        &self,
        credential_id: Id,
        key: &str,
        order_id: Id,
        order_no: &str,
    ) -> Result<()> {
        requests::Entity::update_many()
            .col_expr(requests::Column::OrderId, Expr::value(order_id))
            .col_expr(requests::Column::OrderNo, Expr::value(order_no))
            .col_expr(requests::Column::UpdatedAt, Expr::value(Utc::now()))
            .filter(requests::Column::RequestKey.eq(request_key(credential_id, key)))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn release_request(&self, credential_id: Id, key: &str) -> Result<()> {
        requests::Entity::delete_many()
            .filter(requests::Column::RequestKey.eq(request_key(credential_id, key)))
            .filter(requests::Column::OrderId.eq(0))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn request_by_order(&self, order_id: Id) -> Result<Option<OrderRequest>> {
        if order_id <= 0 {
            return Ok(None);
        }
        Ok(requests::Entity::find()
            .filter(requests::Column::OrderId.eq(order_id))
            .order_by_desc(requests::Column::Id)
            .one(&self.db)
            .await
            .dom()?
            .map(request_of))
    }

    async fn list_requests(
        &self,
        credential_id: Id,
        before: Option<Id>,
        limit: u64,
    ) -> Result<Vec<OrderRequest>> {
        let mut q = requests::Entity::find()
            .filter(requests::Column::CredentialId.eq(credential_id))
            .filter(requests::Column::OrderId.gt(0));
        if let Some(b) = before {
            q = q.filter(requests::Column::OrderId.lt(b));
        }
        Ok(q.order_by_desc(requests::Column::OrderId)
            .limit(limit.max(1))
            .all(&self.db)
            .await
            .dom()?
            .into_iter()
            .map(request_of)
            .collect())
    }

    async fn purge_expired(&self, now: DateTime<Utc>) -> Result<()> {
        quotes::Entity::delete_many()
            .filter(quotes::Column::ExpiresAt.lt(now - Duration::hours(1)))
            .exec(&self.db)
            .await
            .dom()?;
        // Completed requests are kept (order lookup / events); abandoned
        // reservations and keys past the idempotency window are dropped.
        requests::Entity::delete_many()
            .filter(requests::Column::OrderId.eq(0))
            .filter(requests::Column::CreatedAt.lt(now - Duration::hours(IDEMPOTENCY_TTL_HOURS)))
            .exec(&self.db)
            .await
            .dom()?;
        Ok(())
    }

    async fn order_id_by_no(&self, order_no: &str) -> Result<Option<Id>> {
        if order_no.is_empty() {
            return Ok(None);
        }
        Ok(orders::Entity::find()
            .filter(orders::Column::OrderNo.eq(order_no))
            .filter(orders::Column::ParentId.is_null())
            .filter(orders::Column::DeletedAt.is_null())
            .one(&self.db)
            .await
            .dom()?
            .map(|o| o.id))
    }
}

// ---------------------------------------------------------------------------
// Snapshot source
// ---------------------------------------------------------------------------

/// Projected catalog pages for the change-feed producer.
#[derive(Clone)]
pub struct SeaSnapshotSource {
    catalog: SeaSupplierCatalog,
    lookup: SeaCatalogLookup,
}

impl std::fmt::Debug for SeaSnapshotSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SeaSnapshotSource")
    }
}

impl SeaSnapshotSource {
    pub fn new(catalog: SeaSupplierCatalog, db: DatabaseConnection) -> Self {
        Self {
            catalog,
            lookup: SeaCatalogLookup::new(db),
        }
    }
}

#[async_trait]
impl CatalogSnapshotSource for SeaSnapshotSource {
    async fn page(&self, after_id: Id, limit: u64) -> Result<Vec<RemoteProduct>> {
        let (items, _) = self.catalog.products_after(after_id, limit).await?;
        let upstream: Vec<Id> = items
            .iter()
            .filter(|p| {
                p.fulfillment_type == zs_domain::catalog::product::FulfillmentType::Upstream
            })
            .map(|p| p.id)
            .collect();
        let mappings = if upstream.is_empty() {
            Vec::new()
        } else {
            self.lookup.upstream_mappings(&upstream).await?
        };
        let none = MemberPricing {
            level: None,
            prices: &[],
        };
        Ok(items
            .iter()
            .map(|p| {
                let m = mappings.iter().find(|m| m.local_product_id == p.id);
                to_remote_product(p, m, none)
            })
            .collect())
    }
}
