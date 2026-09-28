//! Card secrets (auto-delivery stock) and their import batches.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use zs_shared::page::{Page, PageRequest};

use super::product::ProductSku;
use crate::{Error, Id, Result};

pub mod keys {
    pub const INVALID: &str = "error.card_secret_invalid";
    pub const NOT_FOUND: &str = "error.card_secret_not_found";
    pub const INSUFFICIENT: &str = "error.card_secret_insufficient";
    pub const CREATE_FAILED: &str = "error.card_secret_create_failed";
    pub const BATCH_CREATE_FAILED: &str = "error.card_secret_batch_create_failed";
    pub const IMPORT_FAILED: &str = "error.card_secret_import_failed";
    pub const FETCH_FAILED: &str = "error.card_secret_fetch_failed";
    pub const UPDATE_FAILED: &str = "error.card_secret_update_failed";
    pub const DELETE_FAILED: &str = "error.card_secret_delete_failed";
    pub const STATS_FAILED: &str = "error.card_secret_stats_failed";
    pub const BATCH_FETCH_FAILED: &str = "error.card_secret_batch_fetch_failed";
    /// Every submitted secret is already in stock (live QA I-10).
    pub const ALL_DUPLICATE: &str = "error.card_secret_all_duplicate";
    /// A sold secret cannot change status (live QA I-11).
    pub const USED_LOCKED: &str = "error.card_secret_used_locked";
}

/// Rows per INSERT when importing (DLV-06; original `CreateInBatches(&items, 200)`).
pub const INSERT_CHUNK_SIZE: usize = 200;

/// CSV import template served by `GET /admin/card-secrets/template`.
pub const IMPORT_TEMPLATE: &str = "secret\nCARD-AAA-0001\nCARD-BBB-0002\n";

/// Card secret lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretStatus {
    Available,
    Reserved,
    Used,
}

impl SecretStatus {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "available" => Some(Self::Available),
            "reserved" => Some(Self::Reserved),
            "used" => Some(Self::Used),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Reserved => "reserved",
            Self::Used => "used",
        }
    }
}

/// A sold (`used`) secret may never leave that state through admin edits
/// (live QA I-11: it could be put back on sale and sold twice).
pub fn manual_status_change_allowed(from: &str, to: SecretStatus) -> bool {
    from != SecretStatus::Used.as_str() || to == SecretStatus::Used
}

/// Drops the secrets already in stock (cross-import de-duplication, live QA I-10).
pub fn without_existing(secrets: Vec<String>, existing: &HashSet<String>) -> Vec<String> {
    secrets
        .into_iter()
        .filter(|s| !existing.contains(s))
        .collect()
}

/// Batch source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchSource {
    Manual,
    Csv,
}

impl BatchSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::Csv => "csv",
        }
    }
}

/// An import batch (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SecretBatch {
    pub id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub batch_no: String,
    pub source: String,
    pub total_count: i32,
    pub note: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<Id>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// A card secret (admin JSON shape).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CardSecret {
    pub id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_id: Option<Id>,
    pub secret: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_id: Option<Id>,
    pub reserved_at: Option<DateTime<Utc>>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch: Option<SecretBatch>,
}

/// List / bulk-target filter.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SecretFilter {
    pub product_id: Id,
    pub sku_id: Id,
    pub batch_id: Id,
    pub status: String,
    /// Case-insensitive substring.
    pub secret: String,
    /// Case-insensitive substring of the batch number.
    pub batch_no: String,
}

impl SecretFilter {
    /// Trims the text criteria.
    pub fn normalized(mut self) -> Self {
        self.status = self.status.trim().to_owned();
        self.secret = self.secret.trim().to_owned();
        self.batch_no = self.batch_no.trim().to_owned();
        self
    }

    pub fn is_empty(&self) -> bool {
        self.product_id <= 0
            && self.sku_id <= 0
            && self.batch_id <= 0
            && self.status.is_empty()
            && self.secret.is_empty()
            && self.batch_no.is_empty()
    }
}

/// Stock counts of one product (optionally one SKU).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SecretStats {
    pub total: i64,
    pub available: i64,
    pub reserved: i64,
    pub used: i64,
}

/// Grouped count row: `(product_id, sku_id, status) → total`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockCount {
    pub product_id: Id,
    pub sku_id: Id,
    pub status: String,
    pub total: i64,
}

/// Batch list row with realtime counts (DLV-07).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchSummary {
    pub id: Id,
    pub product_id: Id,
    pub sku_id: Id,
    pub name: String,
    pub batch_no: String,
    pub source: String,
    pub note: String,
    pub total_count: i64,
    pub available_count: i64,
    pub reserved_count: i64,
    pub used_count: i64,
    pub created_at: DateTime<Utc>,
}

/// New batch + its secrets, inserted in one transaction.
#[derive(Debug, Clone, PartialEq)]
pub struct NewSecretBatch {
    pub product_id: Id,
    pub sku_id: Id,
    pub batch_no: String,
    pub source: BatchSource,
    pub note: String,
    pub created_by: Option<Id>,
    pub secrets: Vec<String>,
}

/// Target of a bulk operation (DLV-07): explicit ids, else any filter, else a batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkTarget {
    Ids(Vec<Id>),
    Filter(SecretFilter),
    Batch(Id),
}

/// Resolves the bulk target; returns `None` when nothing selects rows (never "all").
pub fn resolve_bulk_target(ids: &[Id], batch_id: Id, filter: SecretFilter) -> Option<BulkTarget> {
    let ids = normalize_ids(ids);
    if !ids.is_empty() {
        return Some(BulkTarget::Ids(ids));
    }
    let filter = filter.normalized();
    if !filter.is_empty() {
        return Some(BulkTarget::Filter(filter));
    }
    (batch_id > 0).then_some(BulkTarget::Batch(batch_id))
}

/// Deduplicates ids and drops non-positive ones, keeping order.
pub fn normalize_ids(ids: &[Id]) -> Vec<Id> {
    let mut seen = HashSet::new();
    ids.iter()
        .copied()
        .filter(|id| *id > 0 && seen.insert(*id))
        .collect()
}

/// Splits each value into lines, trims, drops blanks and optionally deduplicates (DLV-10).
pub fn normalize_secrets(values: &[String], deduplicate: bool) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values {
        for line in value.split('\n') {
            let trimmed = line.trim();
            if trimmed.is_empty() || (deduplicate && !seen.insert(trimmed.to_owned())) {
                continue;
            }
            out.push(trimmed.to_owned());
        }
    }
    out
}

/// Parses one CSV line honouring double quotes (RFC 4180 subset).
fn parse_csv_record(line: &str) -> Option<Vec<String>> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    let mut at_start = true;
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start || field.trim().is_empty() => {
                field.clear();
                in_quotes = true;
                at_start = false;
            }
            ',' => {
                fields.push(std::mem::take(&mut field));
                at_start = true;
            }
            _ => {
                if at_start && c.is_whitespace() {
                    // TrimLeadingSpace
                    continue;
                }
                at_start = false;
                field.push(c);
            }
        }
    }
    if in_quotes {
        return None;
    }
    fields.push(field);
    Some(fields)
}

/// Extracts secrets from CSV/TXT content: a first row containing a `secret` column
/// header selects that column and is skipped; otherwise column 0 is used.
pub fn parse_csv_secrets(content: &str) -> Result<Vec<String>> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut secrets = Vec::new();
    let mut header_read = false;
    let mut column = 0usize;
    for raw_line in content.split('\n') {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.trim().is_empty() {
            continue;
        }
        let record =
            parse_csv_record(line).ok_or_else(|| Error::bad_request(keys::IMPORT_FAILED))?;
        if !header_read {
            header_read = true;
            if let Some(idx) = record.iter().position(|c| {
                c.trim_start_matches('\u{feff}')
                    .trim()
                    .eq_ignore_ascii_case("secret")
            }) {
                column = idx;
                continue;
            }
        }
        let Some(value) = record.get(column) else {
            continue;
        };
        let secret = value.trim_start_matches('\u{feff}').trim();
        if !secret.is_empty() {
            secrets.push(secret.to_owned());
        }
    }
    Ok(secrets)
}

/// Export file format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Txt,
    Csv,
}

impl ExportFormat {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "txt" => Some(Self::Txt),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Txt => "txt",
            Self::Csv => "csv",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Txt => "text/plain; charset=utf-8",
            Self::Csv => "text/csv; charset=utf-8",
        }
    }
}

/// Quotes a CSV field when needed (Go `encoding/csv` rules).
pub fn csv_field(value: &str) -> String {
    let needs = value == r"\."
        || value.contains([',', '"', '\n', '\r'])
        || value.chars().next().is_some_and(char::is_whitespace);
    if needs {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Writes CSV rows terminated by `\n`.
pub fn write_csv(rows: &[Vec<String>]) -> String {
    let mut out = String::new();
    for row in rows {
        let line: Vec<String> = row.iter().map(|f| csv_field(f)).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    out
}

/// Builds the export file (txt: one secret per line; csv: full rows).
pub fn build_export(items: &[CardSecret], format: ExportFormat) -> String {
    match format {
        ExportFormat::Txt => items
            .iter()
            .map(|i| i.secret.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        ExportFormat::Csv => {
            let mut rows = vec![
                [
                    "id",
                    "secret",
                    "status",
                    "product_id",
                    "sku_id",
                    "order_id",
                    "batch_id",
                    "created_at",
                ]
                .map(str::to_owned)
                .to_vec(),
            ];
            for i in items {
                rows.push(vec![
                    i.id.to_string(),
                    i.secret.clone(),
                    i.status.clone(),
                    i.product_id.to_string(),
                    i.sku_id.to_string(),
                    i.order_id.map(|v| v.to_string()).unwrap_or_default(),
                    i.batch_id.map(|v| v.to_string()).unwrap_or_default(),
                    i.created_at.to_rfc3339_opts(SecondsFormat::Secs, true),
                ]);
            }
            write_csv(&rows)
        }
    }
}

/// Resolves which SKU new secrets belong to (DLV-04).
///
/// `requested` > 0 must belong to the product (and be active for auto products).
/// Otherwise auto products with exactly one active SKU use it; several active SKUs
/// require an explicit SKU. Fallback: DEFAULT SKU, else the only SKU.
pub fn resolve_secret_sku(
    product_is_auto: bool,
    skus: &[ProductSku],
    requested: Option<&ProductSku>,
    requested_id: Id,
    product_id: Id,
) -> Result<ProductSku> {
    let invalid = || Error::bad_request(keys::INVALID);
    if requested_id > 0 {
        let sku = requested
            .filter(|s| s.product_id == product_id)
            .ok_or_else(invalid)?;
        if product_is_auto && !sku.is_active {
            return Err(invalid());
        }
        return Ok(sku.clone());
    }
    let active: Vec<&ProductSku> = skus.iter().filter(|s| s.is_active).collect();
    if product_is_auto {
        match active.len() {
            0 => {}
            1 => return Ok(active[0].clone()),
            _ => return Err(invalid()),
        }
    }
    if let Some(default) = skus
        .iter()
        .find(|s| s.sku_code == super::product::DEFAULT_SKU_CODE)
    {
        return Ok(default.clone());
    }
    if skus.len() == 1 {
        return Ok(skus[0].clone());
    }
    Err(invalid())
}

/// Folds grouped counts into batch summaries.
pub fn summarize_batches(
    batches: Vec<SecretBatch>,
    counts: &[(Id, String, i64)],
) -> Vec<BatchSummary> {
    let mut map: HashMap<Id, (i64, i64, i64)> = HashMap::new();
    for (batch_id, status, total) in counts {
        let entry = map.entry(*batch_id).or_default();
        match SecretStatus::parse(status) {
            Some(SecretStatus::Available) => entry.0 = *total,
            Some(SecretStatus::Reserved) => entry.1 = *total,
            Some(SecretStatus::Used) => entry.2 = *total,
            None => {}
        }
    }
    batches
        .into_iter()
        .map(|b| {
            let (available, reserved, used) = map.get(&b.id).copied().unwrap_or_default();
            BatchSummary {
                id: b.id,
                product_id: b.product_id,
                sku_id: b.sku_id,
                name: String::new(),
                batch_no: b.batch_no,
                source: b.source,
                note: b.note,
                total_count: available + reserved + used,
                available_count: available,
                reserved_count: reserved,
                used_count: used,
                created_at: b.created_at,
            }
        })
        .collect()
}

/// Persistence port for card secrets. Soft-deleted rows are never returned.
#[async_trait]
pub trait CardSecretRepo: Send + Sync {
    /// Inserts the batch and its secrets in one transaction, chunked by [`INSERT_CHUNK_SIZE`].
    async fn create_batch(&self, batch: &NewSecretBatch, now: DateTime<Utc>)
    -> Result<SecretBatch>;
    async fn list(&self, filter: &SecretFilter, page: PageRequest) -> Result<Page<CardSecret>>;
    /// Ids matching the bulk target (`id ASC`).
    async fn target_ids(&self, target: &BulkTarget) -> Result<Vec<Id>>;
    async fn list_by_ids(&self, ids: &[Id]) -> Result<Vec<CardSecret>>;
    async fn get(&self, id: Id) -> Result<Option<CardSecret>>;
    /// Updates one secret; a `used` row keeps its status unless `status` is `used`
    /// (conditional update, live QA I-11). Returns the affected row count.
    async fn update(&self, id: Id, secret: &str, status: &str, now: DateTime<Utc>) -> Result<u64>;
    /// Sets `status` on `ids`; `used` rows are skipped unless `status` is `used`
    /// (a sold card is never put back on sale, live QA I-11).
    async fn update_status(
        &self,
        ids: &[Id],
        status: SecretStatus,
        now: DateTime<Utc>,
    ) -> Result<u64>;
    async fn soft_delete(&self, ids: &[Id], now: DateTime<Utc>) -> Result<u64>;
    /// Which of `secrets` already exist (not deleted, any status) for the SKU
    /// (cross-import de-duplication, live QA I-10).
    async fn existing_secrets(
        &self,
        product_id: Id,
        sku_id: Id,
        secrets: &[String],
    ) -> Result<HashSet<String>>;
    /// Stats of a product (`sku_id` 0 = all SKUs).
    async fn stats(&self, product_id: Id, sku_id: Id) -> Result<SecretStats>;
    /// `(product_id, sku_id, status, count)` rows for stock aggregation (DLV-08).
    async fn stock_counts(&self, product_ids: &[Id]) -> Result<Vec<StockCount>>;
    async fn list_batches(
        &self,
        product_id: Id,
        sku_id: Id,
        page: PageRequest,
    ) -> Result<Page<SecretBatch>>;
    /// `(batch_id, status, count)` rows.
    async fn batch_counts(&self, batch_ids: &[Id]) -> Result<Vec<(Id, String, i64)>>;
    /// Takes exactly `limit` available secrets (oldest first) and marks them used or deletes
    /// them in one transaction with conditional updates; fails with `card_secret_insufficient`
    /// / `card_secret_not_found` without touching rows (DLV-01).
    async fn take_available(
        &self,
        product_id: Id,
        sku_id: Id,
        batch_id: Id,
        limit: u64,
        delete: bool,
        now: DateTime<Utc>,
    ) -> Result<Vec<CardSecret>>;
}

#[cfg(test)]
mod tests {
    use super::super::product::testkit::sku;
    use super::*;

    // QA-A11
    #[test]
    fn qa_a11_used_secret_is_terminal() {
        assert!(!manual_status_change_allowed(
            "used",
            SecretStatus::Available
        ));
        assert!(!manual_status_change_allowed(
            "used",
            SecretStatus::Reserved
        ));
        assert!(manual_status_change_allowed("used", SecretStatus::Used));
        assert!(manual_status_change_allowed(
            "available",
            SecretStatus::Used
        ));
        assert!(manual_status_change_allowed(
            "reserved",
            SecretStatus::Available
        ));
    }

    // QA-A10
    #[test]
    fn qa_a10_existing_secrets_are_dropped() {
        let existing: HashSet<String> = ["a".to_owned()].into_iter().collect();
        assert_eq!(
            without_existing(vec!["a".into(), "b".into()], &existing),
            vec!["b"]
        );
    }

    // DLV-10
    #[test]
    fn normalize_secrets_dedup_switch() {
        let values = vec!["a\na\n b \n\n".to_owned(), "b".to_owned()];
        assert_eq!(normalize_secrets(&values, true), vec!["a", "b"]);
        assert_eq!(normalize_secrets(&values, false), vec!["a", "a", "b", "b"]);
    }

    #[test]
    fn parses_csv_with_and_without_header() {
        let with_header = "\u{feff}id,Secret\n1,AAA\n2,\"B,B\"\n3,\n";
        assert_eq!(parse_csv_secrets(with_header).unwrap(), vec!["AAA", "B,B"]);
        let txt = "CARD-1\r\nCARD-2\n\nCARD-2\n";
        assert_eq!(
            parse_csv_secrets(txt).unwrap(),
            vec!["CARD-1", "CARD-2", "CARD-2"]
        );
        assert!(parse_csv_secrets("\"unterminated\n").is_err());
        assert_eq!(parse_csv_secrets(IMPORT_TEMPLATE).unwrap().len(), 2);
    }

    // DLV-07
    #[test]
    fn bulk_target_never_means_everything() {
        assert_eq!(resolve_bulk_target(&[], 0, SecretFilter::default()), None);
        assert_eq!(
            resolve_bulk_target(&[0, 3, 3], 7, SecretFilter::default()),
            Some(BulkTarget::Ids(vec![3]))
        );
        assert_eq!(
            resolve_bulk_target(&[], 7, SecretFilter::default()),
            Some(BulkTarget::Batch(7))
        );
        let filter = SecretFilter {
            status: " available ".into(),
            ..SecretFilter::default()
        };
        assert!(matches!(
            resolve_bulk_target(&[], 7, filter),
            Some(BulkTarget::Filter(f)) if f.status == "available"
        ));
    }

    // DLV-04
    #[test]
    fn resolves_secret_sku() {
        let a = sku(1, "A", true);
        let b = sku(2, "B", true);
        let err = resolve_secret_sku(true, &[a.clone(), b.clone()], None, 0, 1).unwrap_err();
        assert_eq!(err.key(), keys::INVALID);
        let only_a = [a.clone(), sku(2, "B", false)];
        assert_eq!(resolve_secret_sku(true, &only_a, None, 0, 1).unwrap().id, 1);
        let disabled = sku(2, "B", false);
        assert!(resolve_secret_sku(true, &only_a, Some(&disabled), 2, 1).is_err());
        // manual products may use inactive SKUs explicitly
        assert_eq!(
            resolve_secret_sku(false, &only_a, Some(&disabled), 2, 1)
                .unwrap()
                .id,
            2
        );
        let mut foreign = sku(9, "X", true);
        foreign.product_id = 5;
        assert!(resolve_secret_sku(true, &only_a, Some(&foreign), 9, 1).is_err());
        let default = [sku(3, "DEFAULT", false), sku(4, "Z", false)];
        assert_eq!(
            resolve_secret_sku(true, &default, None, 0, 1).unwrap().id,
            3
        );
    }

    #[test]
    fn export_formats() {
        let item = CardSecret {
            id: 1,
            product_id: 2,
            sku_id: 3,
            batch_id: Some(4),
            secret: "S,1".into(),
            status: "available".into(),
            order_id: None,
            reserved_at: None,
            used_at: None,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
            batch: None,
        };
        let txt = build_export(&[item.clone(), item.clone()], ExportFormat::Txt);
        assert_eq!(txt, "S,1\nS,1");
        let csv = build_export(&[item], ExportFormat::Csv);
        let mut lines = csv.lines();
        assert_eq!(
            lines.next(),
            Some("id,secret,status,product_id,sku_id,order_id,batch_id,created_at")
        );
        assert!(
            lines
                .next()
                .unwrap()
                .starts_with("1,\"S,1\",available,2,3,,4,")
        );
    }

    // DLV-07
    #[test]
    fn batch_summary_uses_realtime_counts() {
        let batch = SecretBatch {
            id: 7,
            product_id: 1,
            sku_id: 0,
            batch_no: "B".into(),
            source: "manual".into(),
            total_count: 10,
            note: String::new(),
            created_by: None,
            created_at: DateTime::<Utc>::MIN_UTC,
            updated_at: DateTime::<Utc>::MIN_UTC,
        };
        let rows = summarize_batches(
            vec![batch],
            &[(7, "available".into(), 5), (7, "used".into(), 3)],
        );
        assert_eq!(rows[0].total_count, 8);
        assert_eq!(rows[0].used_count, 3);
        assert_eq!(rows[0].available_count, 5);
    }
}
