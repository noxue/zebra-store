//! Card secret use cases: import, list, bulk operations, export, stats.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use zs_domain::catalog::card_secret::{
    BatchSource, BatchSummary, BulkTarget, CardSecret, CardSecretRepo, ExportFormat,
    NewSecretBatch, SecretFilter, SecretStats, SecretStatus, build_export, keys,
    manual_status_change_allowed, normalize_ids, normalize_secrets, parse_csv_secrets,
    resolve_bulk_target, resolve_secret_sku, summarize_batches, without_existing,
};
use zs_domain::catalog::product::{
    FulfillmentType, ProductRepo, ProductSku, SkuScope, keys as product_keys,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

/// Manual batch creation input.
#[derive(Debug, Clone, Default)]
pub struct CreateSecretsInput {
    pub product_id: Id,
    pub sku_id: Id,
    pub secrets: Vec<String>,
    pub batch_no: String,
    pub note: String,
    pub admin_id: Option<Id>,
    /// `None` = deduplicate (DLV-10).
    pub deduplicate: Option<bool>,
}

/// Result of a batch creation / import.
#[derive(Debug, Clone)]
pub struct CreatedSecrets {
    pub created: usize,
    pub batch_id: Id,
    pub batch_no: String,
}

/// Export file.
#[derive(Debug, Clone)]
pub struct ExportFile {
    pub content: String,
    pub content_type: &'static str,
    pub extension: &'static str,
    pub count: usize,
}

/// Card secret service.
#[derive(Clone)]
pub struct CardSecretService {
    secrets: Arc<dyn CardSecretRepo>,
    products: Arc<dyn ProductRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for CardSecretService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CardSecretService")
    }
}

fn invalid() -> Error {
    Error::bad_request(keys::INVALID)
}

/// `BATCH-YYYYmmddHHMMSS-NNNN` (original `generateBatchNo`).
fn generate_batch_no(now: DateTime<Utc>) -> String {
    let random = uuid::Uuid::new_v4().as_u128() % 10_000;
    format!("BATCH-{}-{random:04}", now.format("%Y%m%d%H%M%S"))
}

impl CardSecretService {
    pub fn new(
        secrets: Arc<dyn CardSecretRepo>,
        products: Arc<dyn ProductRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            secrets,
            products,
            clock,
        }
    }

    /// Resolves the SKU of a product for card-secret operations (DLV-04).
    async fn resolve_sku(&self, product_id: Id, sku_id: Id) -> Result<(bool, ProductSku)> {
        if product_id <= 0 {
            return Err(invalid());
        }
        let product = self
            .products
            .get(product_id, SkuScope::Active)
            .await
            .map_err(|e| e.or_internal("error.product_fetch_failed"))?
            .ok_or_else(|| Error::not_found(product_keys::NOT_FOUND))?;
        let is_auto = product.fulfillment_type == FulfillmentType::Auto;
        let skus = self.products.list_skus(product_id, false).await?;
        let requested = if sku_id > 0 {
            self.products.get_sku(sku_id).await?
        } else {
            None
        };
        let sku = resolve_secret_sku(is_auto, &skus, requested.as_ref(), sku_id, product_id)?;
        Ok((is_auto, sku))
    }

    pub async fn create_batch(
        &self,
        input: CreateSecretsInput,
        source: BatchSource,
    ) -> Result<CreatedSecrets> {
        if input.product_id <= 0 || input.secrets.is_empty() {
            return Err(invalid());
        }
        let (_, sku) = self.resolve_sku(input.product_id, input.sku_id).await?;
        let deduplicate = input.deduplicate.unwrap_or(true);
        let mut secrets = normalize_secrets(&input.secrets, deduplicate);
        if secrets.is_empty() {
            return Err(invalid());
        }
        if deduplicate {
            // Live QA I-10 (original gap): de-duplicate against the stock too, so a
            // re-import cannot create a second sellable copy of the same card.
            let existing = self
                .secrets
                .existing_secrets(input.product_id, sku.id, &secrets)
                .await
                .map_err(|e| e.or_internal(keys::CREATE_FAILED))?;
            secrets = without_existing(secrets, &existing);
            if secrets.is_empty() {
                return Err(Error::bad_request(keys::ALL_DUPLICATE));
            }
        }
        let now = self.clock.now();
        let batch_no = match input.batch_no.trim() {
            "" => generate_batch_no(now),
            v => v.to_owned(),
        };
        let created = secrets.len();
        let batch = self
            .secrets
            .create_batch(
                &NewSecretBatch {
                    product_id: input.product_id,
                    sku_id: sku.id,
                    batch_no,
                    source,
                    note: input.note.trim().to_owned(),
                    created_by: input.admin_id.filter(|id| *id > 0),
                    secrets,
                },
                now,
            )
            .await
            .map_err(|e| e.or_internal(keys::CREATE_FAILED))?;
        Ok(CreatedSecrets {
            created,
            batch_id: batch.id,
            batch_no: batch.batch_no,
        })
    }

    /// Imports a CSV/TXT file (first `secret` column or column 0).
    pub async fn import(&self, input: CreateSecretsInput, content: &str) -> Result<CreatedSecrets> {
        if input.product_id <= 0 {
            return Err(invalid());
        }
        let secrets = parse_csv_secrets(content).map_err(|_| {
            Error::internal_msg("csv parse failed").or_internal(keys::IMPORT_FAILED)
        })?;
        self.create_batch(CreateSecretsInput { secrets, ..input }, BatchSource::Csv)
            .await
            .map_err(|e| e.or_internal(keys::IMPORT_FAILED))
    }

    pub async fn list(&self, filter: SecretFilter, page: PageRequest) -> Result<Page<CardSecret>> {
        if filter.sku_id > 0 && filter.product_id <= 0 {
            return Err(invalid());
        }
        if filter.product_id > 0 && filter.sku_id > 0 {
            self.resolve_sku(filter.product_id, filter.sku_id).await?;
        }
        self.secrets
            .list(&filter.normalized(), page)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))
    }

    pub async fn update(&self, id: Id, secret: &str, status: &str) -> Result<CardSecret> {
        if id <= 0 {
            return Err(invalid());
        }
        let mut item = self
            .secrets
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        let secret = secret.trim();
        if !secret.is_empty() {
            item.secret = secret.to_owned();
        }
        let status = status.trim();
        if !status.is_empty() {
            let next = SecretStatus::parse(status).ok_or_else(invalid)?;
            if !manual_status_change_allowed(&item.status, next) {
                return Err(Error::bad_request(keys::USED_LOCKED));
            }
            item.status = next.as_str().to_owned();
        }
        let now = self.clock.now();
        let affected = self
            .secrets
            .update(id, &item.secret, &item.status, now)
            .await
            .map_err(|e| e.or_internal(keys::UPDATE_FAILED))?;
        if affected == 0 {
            // sold (or deleted) concurrently
            return Err(Error::bad_request(keys::USED_LOCKED));
        }
        item.updated_at = now;
        Ok(item)
    }

    /// Resolves a bulk target to ids: empty criteria are rejected (DLV-07), no match → 404.
    async fn target_ids(&self, target: Option<BulkTarget>) -> Result<Vec<Id>> {
        let target = target.ok_or_else(invalid)?;
        if let BulkTarget::Ids(ids) = target {
            return Ok(ids);
        }
        let ids = self
            .secrets
            .target_ids(&target)
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        if ids.is_empty() {
            return Err(Error::not_found(keys::NOT_FOUND));
        }
        Ok(ids)
    }

    pub async fn batch_status(
        &self,
        ids: &[Id],
        batch_id: Id,
        filter: SecretFilter,
        status: &str,
    ) -> Result<u64> {
        let status = SecretStatus::parse(status).ok_or_else(invalid)?;
        let ids = self
            .target_ids(resolve_bulk_target(ids, batch_id, filter))
            .await?;
        self.secrets
            .update_status(&ids, status, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::UPDATE_FAILED))
    }

    pub async fn batch_delete(
        &self,
        ids: &[Id],
        batch_id: Id,
        filter: SecretFilter,
    ) -> Result<u64> {
        let ids = self
            .target_ids(resolve_bulk_target(ids, batch_id, filter))
            .await?;
        self.secrets
            .soft_delete(&ids, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::DELETE_FAILED))
    }

    /// Exports selected secrets; with no criteria at all, exports every current result.
    pub async fn export(
        &self,
        ids: &[Id],
        batch_id: Id,
        filter: SecretFilter,
        format: &str,
    ) -> Result<ExportFile> {
        let format = ExportFormat::parse(format).ok_or_else(invalid)?;
        let target = resolve_bulk_target(ids, batch_id, filter)
            .unwrap_or_else(|| BulkTarget::Filter(SecretFilter::default()));
        let ids = match target {
            BulkTarget::Ids(ids) => ids,
            other => {
                let ids = self
                    .secrets
                    .target_ids(&other)
                    .await
                    .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
                if ids.is_empty() {
                    return Err(Error::not_found(keys::NOT_FOUND));
                }
                ids
            }
        };
        let items = self
            .secrets
            .list_by_ids(&normalize_ids(&ids))
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        if items.is_empty() {
            return Err(Error::not_found(keys::NOT_FOUND));
        }
        Ok(ExportFile {
            content: build_export(&items, format),
            content_type: format.content_type(),
            extension: format.extension(),
            count: items.len(),
        })
    }

    /// Takes `limit` available secrets out of stock (marked used or deleted) and exports them (DLV-01).
    pub async fn export_available(
        &self,
        product_id: Id,
        sku_id: Id,
        batch_id: Id,
        limit: i64,
        format: &str,
        delete_after_export: bool,
    ) -> Result<ExportFile> {
        let format = ExportFormat::parse(format).ok_or_else(invalid)?;
        if product_id <= 0 || limit <= 0 {
            return Err(invalid());
        }
        let product = self
            .products
            .get(product_id, SkuScope::Active)
            .await?
            .ok_or_else(|| Error::not_found(keys::NOT_FOUND))?;
        if product.fulfillment_type != FulfillmentType::Auto {
            return Err(invalid());
        }
        if sku_id > 0 {
            match self.products.get_sku(sku_id).await? {
                Some(sku) if sku.product_id == product_id && sku.is_active => {}
                _ => return Err(invalid()),
            }
        }
        let items = self
            .secrets
            .take_available(
                product_id,
                sku_id,
                batch_id,
                u64::try_from(limit).unwrap_or(0),
                delete_after_export,
                self.clock.now(),
            )
            .await
            .map_err(|e| e.or_internal(keys::FETCH_FAILED))?;
        Ok(ExportFile {
            content: build_export(&items, format),
            content_type: format.content_type(),
            extension: format.extension(),
            count: items.len(),
        })
    }

    pub async fn stats(&self, product_id: Id, sku_id: Id) -> Result<SecretStats> {
        if product_id <= 0 {
            return Err(invalid());
        }
        if sku_id > 0 {
            self.resolve_sku(product_id, sku_id).await?;
        }
        self.secrets
            .stats(product_id, sku_id)
            .await
            .map_err(|e| e.or_internal(keys::STATS_FAILED))
    }

    pub async fn batches(
        &self,
        product_id: Id,
        sku_id: Id,
        page: PageRequest,
    ) -> Result<Page<BatchSummary>> {
        if product_id <= 0 {
            return Err(invalid());
        }
        if sku_id > 0 {
            self.resolve_sku(product_id, sku_id).await?;
        }
        let fetch = |e: Error| e.or_internal(keys::BATCH_FETCH_FAILED);
        let page = self
            .secrets
            .list_batches(product_id, sku_id, page)
            .await
            .map_err(fetch)?;
        let ids: Vec<Id> = page.items.iter().map(|b| b.id).collect();
        let counts = if ids.is_empty() {
            Vec::new()
        } else {
            self.secrets.batch_counts(&ids).await.map_err(fetch)?
        };
        Ok(Page {
            total: page.total,
            items: summarize_batches(page.items, &counts),
        })
    }
}
