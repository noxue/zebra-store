//! Product mappings: supplier catalog browsing, import, sync, stock/price sync job.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use zs_domain::catalog::product::{JsonMap, keys as product_keys};
use zs_domain::integration::adapter::{Capability, ChangeKind};
use zs_domain::integration::connection::{SiteConnection, SyncMode};
use zs_domain::integration::keys;
use zs_domain::integration::mapping::{
    FetchOutcome, ImageStore, MappingFilter, MappingRepo, ProductMapping, SkuMapping,
    UpstreamStatus, content_image_urls, default_slug, full_sync_interval, plan_import,
    replace_content_urls,
};
use zs_domain::integration::protocol::{
    CategoryList, ProductPage, ProductQuery, RemoteCategory, RemoteProduct, UpstreamClient,
    UpstreamError,
};
use zs_domain::settings::schema::integration::UpstreamSyncSetting;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::Page;

use super::connection::{ConnectionService, not_found as connection_not_found};

/// Page size of the by-category import scan (original 50).
const CATEGORY_SCAN_PAGE_SIZE: i64 = 50;
/// Safety cap of the by-category import scan.
const CATEGORY_SCAN_MAX_PAGES: i64 = 500;
/// Incremental sync looks back this much before the previous run (original 1 min).
const INCREMENTAL_OVERLAP: Duration = Duration::minutes(1);
/// Age under which the pre-order guard trusts cached supplier stock (LQA-I3: the
/// periodic sync runs every 5 minutes; a checkout older than this asks the supplier).
const PRE_ORDER_STOCK_FRESH: Duration = Duration::seconds(30);

fn mapping_not_found() -> Error {
    Error::not_found(keys::MAPPING_NOT_FOUND)
}

/// Import request.
#[derive(Debug, Clone, Default)]
pub struct ImportRequest {
    pub connection_id: Id,
    pub upstream_product_id: Id,
    pub category_id: Id,
    pub slug: String,
    pub auto_create_category: bool,
}

/// One by-category import error.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportError {
    pub upstream_product_id: Id,
    pub error: String,
}

/// Result of a by-category import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct CategoryImportResult {
    pub total: usize,
    pub success_count: usize,
    pub category_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category_name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<ImportError>,
}

/// In-process state of the periodic sync (the job queue's unique key already
/// prevents two workers from running the job at the same time, UPS-17).
#[derive(Debug, Default)]
struct SyncState {
    running: tokio::sync::Mutex<()>,
    last_run: Mutex<Option<DateTime<Utc>>>,
    last_sync: Mutex<HashMap<Id, DateTime<Utc>>>,
    last_full: Mutex<HashMap<Id, DateTime<Utc>>>,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Product mapping use cases.
#[derive(Clone)]
pub struct MappingService {
    repo: Arc<dyn MappingRepo>,
    connections: ConnectionService,
    images: Arc<dyn ImageStore>,
    settings: Arc<dyn SettingsStore>,
    /// `queue.upstream_sync_interval` fallback (e.g. `"5m"`).
    fallback_interval: String,
    clock: Arc<dyn Clock>,
    state: Arc<SyncState>,
}

impl std::fmt::Debug for MappingService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MappingService")
    }
}

fn wrap_upstream(e: UpstreamError, key: &'static str) -> Error {
    Error::internal(e).or_internal(key)
}

impl MappingService {
    pub fn new(
        repo: Arc<dyn MappingRepo>,
        connections: ConnectionService,
        images: Arc<dyn ImageStore>,
        settings: Arc<dyn SettingsStore>,
        fallback_interval: String,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            connections,
            images,
            settings,
            fallback_interval,
            clock,
            state: Arc::new(SyncState::default()),
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Current `upstream_sync_config` (setting, else config fallback).
    pub async fn sync_setting(&self) -> UpstreamSyncSetting {
        let fallback = UpstreamSyncSetting::fallback(&self.fallback_interval);
        match self.settings.get(setting_keys::UPSTREAM_SYNC_CONFIG).await {
            Ok(raw) => UpstreamSyncSetting::decode(raw.as_ref(), fallback),
            Err(error) => {
                tracing::warn!(%error, "read upstream_sync_config failed");
                fallback
            }
        }
    }

    // ------------------------------------------------------------------ queries

    pub async fn list(&self, filter: &MappingFilter) -> Result<Page<ProductMapping>> {
        self.repo
            .list(filter)
            .await
            .map_err(|e| e.or_internal(keys::MAPPING_FETCH_FAILED))
    }

    pub async fn detail(&self, id: Id) -> Result<(ProductMapping, Vec<SkuMapping>)> {
        let wrap = |e: Error| e.or_internal(keys::MAPPING_FETCH_FAILED);
        let m = self
            .repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(mapping_not_found)?;
        let skus = self.repo.sku_mappings(id).await.map_err(wrap)?;
        Ok((m, skus))
    }

    pub async fn set_active(&self, id: Id, active: bool) -> Result<()> {
        let changed = self
            .repo
            .set_active(id, active, self.now())
            .await
            .map_err(|e| e.or_internal(keys::MAPPING_UPDATE_FAILED))?;
        if changed {
            Ok(())
        } else {
            Err(mapping_not_found())
        }
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        let wrap = |e: Error| e.or_internal(keys::MAPPING_DELETE_FAILED);
        self.repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(mapping_not_found)?;
        self.repo.delete(id, self.now()).await.map_err(wrap)
    }

    /// Supplier products page plus (first page only) the already mapped supplier ids.
    pub async fn upstream_products(
        &self,
        connection_id: Id,
        page: i64,
        page_size: i64,
    ) -> Result<(ProductPage, Vec<Id>)> {
        let (_, client) = self.connections.client_of(connection_id).await?;
        let result = client
            .list_products(&ProductQuery {
                page,
                page_size,
                ..ProductQuery::default()
            })
            .await
            .map_err(|e| wrap_upstream(e, keys::UPSTREAM_PRODUCTS_FETCH_FAILED))?;
        let mapped = if page == 1 {
            self.repo
                .mapped_upstream_ids(connection_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok((result, mapped))
    }

    pub async fn upstream_categories(&self, connection_id: Id) -> Result<CategoryList> {
        let (_, client) = self.connections.client_of(connection_id).await?;
        client
            .list_categories()
            .await
            .map_err(|e| wrap_upstream(e, keys::UPSTREAM_CATEGORIES_FETCH_FAILED))
    }

    // ------------------------------------------------------------------ import

    async fn category_map(client: &dyn UpstreamClient) -> Result<HashMap<Id, RemoteCategory>> {
        let list = client
            .list_categories()
            .await
            .map_err(|e| Error::internal(format!("auto create category: {e}")))?;
        Ok(list.categories.into_iter().map(|c| (c.id, c)).collect())
    }

    /// Finds or creates the local category of a supplier category (parent first).
    async fn local_category(
        &self,
        upstream_category_id: Id,
        categories: &HashMap<Id, RemoteCategory>,
    ) -> Result<(Id, JsonMap)> {
        let target = categories.get(&upstream_category_id).ok_or_else(|| {
            Error::internal_msg(format!(
                "upstream category {upstream_category_id} not found"
            ))
        })?;
        let mut parent_id = 0;
        if target.parent_id > 0
            && let Some(parent) = categories.get(&target.parent_id)
        {
            parent_id = self
                .repo
                .ensure_category(&parent.slug, &parent.name, 0, self.now())
                .await?;
        }
        let id = self
            .repo
            .ensure_category(&target.slug, &target.name, parent_id, self.now())
            .await?;
        Ok((id, target.name.clone()))
    }

    /// Downloads an image into the local uploads; keeps the original URL on failure.
    async fn localize_image(&self, client: &dyn UpstreamClient, url: &str) -> String {
        let stored = match client.download(url).await {
            Ok(d) => self.images.store(&d.filename, &d.bytes).await,
            Err(e) => Err(Error::internal(e)),
        };
        match stored {
            Ok(local) => local,
            Err(error) => {
                tracing::warn!(%error, url, "upstream image download failed");
                url.to_owned()
            }
        }
    }

    async fn import_one(
        &self,
        req: &ImportRequest,
        conn: &SiteConnection,
        client: &dyn UpstreamClient,
        categories: Option<&HashMap<Id, RemoteCategory>>,
    ) -> Result<ProductMapping> {
        if self
            .repo
            .get_by_upstream(req.connection_id, req.upstream_product_id)
            .await?
            .is_some()
        {
            return Err(Error::bad_request(keys::MAPPING_EXISTS));
        }
        let mut remote = match client.get_product(req.upstream_product_id).await {
            Ok(p) => p,
            Err(UpstreamError::ProductDeleted | UpstreamError::ProductUnavailable) => {
                return Err(Error::not_found(keys::UPSTREAM_PRODUCT_NOT_FOUND));
            }
            Err(e) => return Err(Error::internal(format!("fetch upstream product: {e}"))),
        };
        // Delisted supplier products cannot be imported (UPS-14).
        if !remote.is_active {
            return Err(Error::not_found(keys::UPSTREAM_PRODUCT_NOT_FOUND));
        }
        let mut category_id = req.category_id;
        if req.auto_create_category && category_id == 0 && remote.category_id > 0 {
            let fetched;
            let map = match categories {
                Some(m) => m,
                None => {
                    fetched = Self::category_map(client).await?;
                    &fetched
                }
            };
            category_id = self.local_category(remote.category_id, map).await?.0;
        }
        if !self.repo.category_assignable(category_id).await? {
            return Err(Error::bad_request(product_keys::CATEGORY_INVALID));
        }
        let mut images = Vec::with_capacity(remote.images.len());
        for img in remote.images.iter().filter(|i| !i.trim().is_empty()) {
            images.push(self.localize_image(client, img).await);
        }
        remote.images = images;
        let urls = content_image_urls(&remote.content);
        if !urls.is_empty() {
            let mut replaced = HashMap::new();
            for url in urls {
                let local = self.localize_image(client, &url).await;
                replaced.insert(url, local);
            }
            remote.content = replace_content_urls(&remote.content, &replaced);
        }
        let slug = if req.slug.trim().is_empty() {
            default_slug(req.connection_id, req.upstream_product_id, self.now())
        } else {
            req.slug.trim().to_owned()
        };
        let plan = plan_import(&remote, conn.pricing(), conn.id, category_id, slug)?;
        self.repo.import(&plan, self.now()).await
    }

    /// Imports one supplier product as an inactive local `upstream` product.
    pub async fn import(&self, req: &ImportRequest) -> Result<ProductMapping> {
        let (conn, client) = self.connections.client_of(req.connection_id).await?;
        self.import_one(req, &conn, client.as_ref(), None)
            .await
            .map_err(|e| e.or_internal(keys::MAPPING_IMPORT_FAILED))
    }

    /// Imports several products (supplier categories prefetched once when auto-creating).
    pub async fn batch_import(
        &self,
        connection_id: Id,
        upstream_ids: &[Id],
        category_id: Id,
        auto_create_category: bool,
    ) -> Result<Vec<(Id, Result<ProductMapping>)>> {
        let (conn, client) = self.connections.client_of(connection_id).await?;
        let categories = if auto_create_category && category_id == 0 {
            Some(Self::category_map(client.as_ref()).await?)
        } else {
            None
        };
        let mut out = Vec::with_capacity(upstream_ids.len());
        for id in upstream_ids {
            let req = ImportRequest {
                connection_id,
                upstream_product_id: *id,
                category_id,
                slug: String::new(),
                auto_create_category,
            };
            let r = self
                .import_one(&req, &conn, client.as_ref(), categories.as_ref())
                .await;
            out.push((*id, r));
        }
        Ok(out)
    }

    /// Imports every supplier product of a supplier category.
    pub async fn import_category(
        &self,
        connection_id: Id,
        upstream_category_id: Id,
        auto_create_category: bool,
        local_category_id: Id,
    ) -> Result<CategoryImportResult> {
        let (conn, client) = self.connections.client_of(connection_id).await?;
        let mut targets: Vec<Id> = Vec::new();
        let mut page = 1;
        loop {
            let result = client
                .list_products(&ProductQuery {
                    page,
                    page_size: CATEGORY_SCAN_PAGE_SIZE,
                    ..ProductQuery::default()
                })
                .await
                .map_err(|e| {
                    Error::internal(format!("fetch upstream products page {page}: {e}"))
                })?;
            let count = i64::try_from(result.items.len()).unwrap_or(i64::MAX);
            targets.extend(
                result
                    .items
                    .iter()
                    .filter(|p| p.category_id == upstream_category_id)
                    .map(|p| p.id),
            );
            if count < CATEGORY_SCAN_PAGE_SIZE
                || page * CATEGORY_SCAN_PAGE_SIZE >= result.total
                || page >= CATEGORY_SCAN_MAX_PAGES
            {
                break;
            }
            page += 1;
        }
        if targets.is_empty() {
            return Ok(CategoryImportResult::default());
        }
        let mut category_id = local_category_id;
        let mut category_name = String::new();
        if auto_create_category && category_id == 0 {
            let map = Self::category_map(client.as_ref()).await?;
            let (id, name) = self.local_category(upstream_category_id, &map).await?;
            category_id = id;
            category_name = name
                .get("zh-CN")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_owned();
        }
        let mut result = CategoryImportResult {
            total: targets.len(),
            category_id,
            category_name,
            ..CategoryImportResult::default()
        };
        for id in targets {
            let req = ImportRequest {
                connection_id,
                upstream_product_id: id,
                category_id,
                ..ImportRequest::default()
            };
            match self.import_one(&req, &conn, client.as_ref(), None).await {
                Ok(_) => result.success_count += 1,
                // Already mapped counts as success.
                Err(e) if e.key() == keys::MAPPING_EXISTS => result.success_count += 1,
                Err(e) => result.errors.push(ImportError {
                    upstream_product_id: id,
                    error: e.to_string(),
                }),
            }
        }
        Ok(result)
    }

    // ------------------------------------------------------------------ sync

    /// Syncs one mapping from the supplier (delisted / deleted products are marked).
    pub async fn sync(&self, id: Id) -> Result<()> {
        let wrap = |e: Error| e.or_internal(keys::MAPPING_SYNC_FAILED);
        let mapping = self
            .repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(mapping_not_found)?;
        let conn = self
            .connections
            .find(mapping.connection_id)
            .await
            .map_err(wrap)?
            .ok_or_else(connection_not_found)?;
        let client = self.connections.client(&conn).map_err(wrap)?;
        let now = self.now();
        let remote = match client.get_product(mapping.upstream_product_id).await {
            Ok(p) => p,
            Err(UpstreamError::ProductDeleted) => {
                return self
                    .repo
                    .mark_unavailable(&mapping, UpstreamStatus::Deleted, now)
                    .await
                    .map_err(wrap);
            }
            Err(UpstreamError::ProductUnavailable) => {
                return self
                    .repo
                    .mark_unavailable(&mapping, UpstreamStatus::Inactive, now)
                    .await
                    .map_err(wrap);
            }
            Err(e) => return Err(wrap_upstream(e, keys::MAPPING_SYNC_FAILED)),
        };
        self.apply_remote(&mapping, &conn, &remote, now)
            .await
            .map_err(wrap)
    }

    async fn apply_remote(
        &self,
        mapping: &ProductMapping,
        conn: &SiteConnection,
        remote: &RemoteProduct,
        now: DateTime<Utc>,
    ) -> Result<()> {
        if !remote.is_active {
            return self
                .repo
                .mark_unavailable(mapping, UpstreamStatus::Inactive, now)
                .await;
        }
        self.repo
            .apply_sync(mapping, remote, &conn.pricing(), conn.auto_sync_price, now)
            .await
    }

    /// Periodic entry point: runs [`Self::sync_all_stock`] when the configured interval
    /// has elapsed since the previous run.
    pub async fn sync_stock_if_due(&self) -> Result<()> {
        let setting = self.sync_setting().await;
        let now = self.now();
        let interval = Duration::minutes(setting.interval_minutes);
        let due = lock(&self.state.last_run).is_none_or(|t| now - t >= interval);
        if !due {
            return Ok(());
        }
        self.sync_all_stock(&setting).await
    }

    /// Syncs stock/prices of every active mapping, grouped by connection with bounded
    /// connection concurrency; overlapping runs are skipped (UPS-17).
    pub async fn sync_all_stock(&self, setting: &UpstreamSyncSetting) -> Result<()> {
        let Ok(_guard) = self.state.running.try_lock() else {
            tracing::debug!("upstream stock sync already running");
            return Ok(());
        };
        *lock(&self.state.last_run) = Some(self.now());
        let mappings = self.repo.list_active().await?;
        let mut by_conn: HashMap<Id, Vec<ProductMapping>> = HashMap::new();
        for m in mappings {
            by_conn.entry(m.connection_id).or_default().push(m);
        }
        let permits = Arc::new(tokio::sync::Semaphore::new(
            usize::try_from(setting.sync_conn_concurrency.max(1)).unwrap_or(1),
        ));
        let mut tasks = tokio::task::JoinSet::new();
        for (conn_id, list) in by_conn {
            let svc = self.clone();
            let permits = permits.clone();
            let setting = *setting;
            tasks.spawn(async move {
                let _permit = permits.acquire_owned().await;
                let r = svc.sync_connection(conn_id, &list, &setting).await;
                (conn_id, r)
            });
        }
        let mut errors = Vec::new();
        while let Some(joined) = tasks.join_next().await {
            match joined {
                Ok((_, Ok(()))) => {}
                Ok((conn_id, Err(error))) => {
                    tracing::warn!(%error, connection_id = conn_id, "sync connection stock failed");
                    errors.push(format!("connection {conn_id}: {error}"));
                }
                Err(error) => errors.push(error.to_string()),
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(Error::internal_msg(errors.join("; ")))
        }
    }

    /// Batch sync of one connection from paginated listings (incremental when possible,
    /// full at least every `max(24h, 3 × interval)`, UPS-06/UPS-17).
    pub async fn sync_connection(
        &self,
        connection_id: Id,
        mappings: &[ProductMapping],
        setting: &UpstreamSyncSetting,
    ) -> Result<()> {
        let conn = self
            .connections
            .find(connection_id)
            .await?
            .ok_or_else(connection_not_found)?;
        let client = self.connections.client(&conn)?;
        let started = self.now();
        // Change-feed systems: follow the cursor; a missing / expired cursor falls back
        // to one full listing, after which the feed continues from the head taken
        // before the listing (changes in between are replayed idempotently).
        let mut head = None;
        if client.capabilities().has(Capability::IncrementalChanges) {
            if self
                .sync_changes(&conn, client.as_ref(), mappings, setting)
                .await?
            {
                lock(&self.state.last_sync).insert(connection_id, started);
                return Ok(());
            }
            head = match client.change_head().await {
                Ok(h) => Some(h),
                Err(error) => {
                    tracing::warn!(%error, connection_id, "change feed head unavailable");
                    None
                }
            };
        }
        let full_every = full_sync_interval(Duration::minutes(setting.interval_minutes));
        let last_full = lock(&self.state.last_full).get(&connection_id).copied();
        let mut updated_after = lock(&self.state.last_sync)
            .get(&connection_id)
            .map(|t| *t - INCREMENTAL_OVERLAP);
        if head.is_some() || last_full.is_none_or(|t| started - t >= full_every) {
            updated_after = None;
        }
        let (fetched, outcome, updated_after) = self
            .fetch_all(client.as_ref(), updated_after, setting)
            .await?;
        let full = updated_after.is_none();
        if !full && fetched.is_empty() {
            lock(&self.state.last_sync).insert(connection_id, started);
            return Ok(());
        }
        let now = self.now();
        for mapping in mappings {
            let Some(remote) = fetched.get(&mapping.upstream_product_id) else {
                if outcome.may_mark_deleted(full) {
                    if let Err(error) = self
                        .repo
                        .mark_unavailable(mapping, UpstreamStatus::Deleted, now)
                        .await
                    {
                        tracing::warn!(%error, mapping_id = mapping.id, "mark deleted failed");
                    }
                } else if full {
                    tracing::warn!(
                        connection_id,
                        upstream_product_id = mapping.upstream_product_id,
                        complete = outcome.complete,
                        includes_inactive = outcome.includes_inactive,
                        "upstream product missing from sync, skipped"
                    );
                }
                continue;
            };
            if let Err(error) = self.apply_remote(mapping, &conn, remote, now).await {
                tracing::warn!(%error, mapping_id = mapping.id, "sync mapping failed");
            }
        }
        lock(&self.state.last_sync).insert(connection_id, started);
        if full {
            lock(&self.state.last_full).insert(connection_id, started);
        }
        if let Some(head) = head.filter(|_| full && outcome.complete) {
            let mut state = conn.state.clone();
            state.set_cursor(&head);
            state.sync_mode = SyncMode::Incremental;
            self.connections.save_state(connection_id, &state).await?;
        }
        Ok(())
    }

    /// Syncs one connection now (push notice from the supplier): its active mappings
    /// through the change feed when available, else a listing sync.
    pub async fn sync_connection_now(&self, connection_id: Id) -> Result<()> {
        let mappings = self.repo.list_active_by_connection(connection_id).await?;
        let setting = self.sync_setting().await;
        self.sync_connection(connection_id, &mappings, &setting)
            .await
    }

    /// Follows the change feed from the stored cursor; `Ok(false)` when a full listing
    /// is needed first (no cursor yet, or the cursor expired). Deleted products are
    /// marked deleted; every other changed product is re-read (the buyer's own prices)
    /// and applied through the regular sync plan (UPS-04/08/13/14).
    async fn sync_changes(
        &self,
        conn: &SiteConnection,
        client: &dyn UpstreamClient,
        mappings: &[ProductMapping],
        setting: &UpstreamSyncSetting,
    ) -> Result<bool> {
        if conn.state.change_cursor.is_empty() {
            return Ok(false);
        }
        let mut cursor = conn.state.change_cursor.clone();
        let mut touched: HashSet<Id> = HashSet::new();
        let mut deleted: HashSet<Id> = HashSet::new();
        let mut pages = 0;
        loop {
            let page = match client.changes(Some(&cursor), setting.sync_page_size).await {
                Ok(p) => p,
                Err(UpstreamError::CursorExpired) => {
                    tracing::warn!(connection_id = conn.id, %cursor, "change cursor expired, full sync");
                    return Ok(false);
                }
                Err(e) => return Err(Error::internal(format!("pull changes: {e}"))),
            };
            for change in &page.changes {
                if change.kind == ChangeKind::ProductDeleted {
                    touched.remove(&change.product_id);
                    deleted.insert(change.product_id);
                } else {
                    deleted.remove(&change.product_id);
                    touched.insert(change.product_id);
                }
            }
            if !page.next_cursor.is_empty() {
                cursor = page.next_cursor;
            }
            pages += 1;
            if !page.has_more || pages >= setting.sync_max_pages {
                break;
            }
        }
        let now = self.now();
        for mapping in mappings {
            let upstream_id = mapping.upstream_product_id;
            let result = if deleted.contains(&upstream_id) {
                self.repo
                    .mark_unavailable(mapping, UpstreamStatus::Deleted, now)
                    .await
            } else if touched.contains(&upstream_id) {
                match client.get_product(upstream_id).await {
                    Ok(remote) => self.apply_remote(mapping, conn, &remote, now).await,
                    Err(UpstreamError::ProductDeleted) => {
                        self.repo
                            .mark_unavailable(mapping, UpstreamStatus::Deleted, now)
                            .await
                    }
                    Err(UpstreamError::ProductUnavailable) => {
                        self.repo
                            .mark_unavailable(mapping, UpstreamStatus::Inactive, now)
                            .await
                    }
                    Err(e) => Err(Error::internal(e)),
                }
            } else {
                continue;
            };
            if let Err(error) = result {
                tracing::warn!(%error, mapping_id = mapping.id, "apply change failed");
            }
        }
        let mut state = conn.state.clone();
        state.set_cursor(&cursor);
        state.sync_mode = SyncMode::Incremental;
        self.connections.save_state(conn.id, &state).await?;
        Ok(true)
    }

    /// Pulls every page; returns the products, the completeness outcome and the
    /// `updated_after` actually used (`None` after a fallback to a full fetch).
    async fn fetch_all(
        &self,
        client: &dyn UpstreamClient,
        mut updated_after: Option<DateTime<Utc>>,
        setting: &UpstreamSyncSetting,
    ) -> Result<(
        HashMap<Id, RemoteProduct>,
        FetchOutcome,
        Option<DateTime<Utc>>,
    )> {
        let mut products = HashMap::new();
        let mut outcome = FetchOutcome::default();
        let mut page = 1;
        let mut cursor: Option<String> = None;
        loop {
            let query = ProductQuery {
                page,
                page_size: setting.sync_page_size,
                updated_after,
                include_inactive: true,
                cursor: cursor.clone(),
            };
            let result = match client.list_products(&query).await {
                Ok(r) => r,
                Err(error) if updated_after.is_some() => {
                    tracing::warn!(%error, "incremental sync failed, falling back to full");
                    updated_after = None;
                    products.clear();
                    page = 1;
                    cursor = None;
                    continue;
                }
                Err(e) => {
                    return Err(Error::internal(format!(
                        "list upstream products page {page}: {e}"
                    )));
                }
            };
            if page == 1 {
                outcome.includes_inactive = result.includes_inactive;
            }
            let empty = result.items.is_empty();
            let cursor_paged = !result.next_cursor.is_empty() || result.has_more;
            for p in result.items {
                products.insert(p.id, p);
            }
            if cursor_paged {
                // Cursor-paginated supplier: complete when it says nothing follows.
                if !result.has_more {
                    outcome.complete = true;
                    break;
                }
                if empty || result.next_cursor.is_empty() {
                    tracing::warn!(page, "upstream cursor pagination truncated");
                    break;
                }
                cursor = Some(result.next_cursor);
                page += 1;
                if page > setting.sync_max_pages {
                    tracing::warn!(
                        max_pages = setting.sync_max_pages,
                        "upstream sync max pages reached"
                    );
                    break;
                }
                continue;
            }
            if i64::try_from(products.len()).unwrap_or(i64::MAX) >= result.total {
                outcome.complete = true;
                break;
            }
            if empty {
                tracing::warn!(
                    page,
                    fetched = products.len(),
                    total = result.total,
                    "upstream pagination truncated"
                );
                break;
            }
            page += 1;
            if page > setting.sync_max_pages {
                tracing::warn!(
                    max_pages = setting.sync_max_pages,
                    "upstream sync max pages reached"
                );
                break;
            }
        }
        Ok((products, outcome, updated_after))
    }

    /// Pre-order guard of upstream SKUs (UPS-06, fail-open; stale cache re-checked,
    /// LQA-I3).
    pub async fn ensure_upstream_stock(&self, sku_id: Id, quantity: i32) -> Result<()> {
        if sku_id <= 0 || quantity <= 0 {
            return Ok(());
        }
        if !self.sync_setting().await.pre_order_stock_check_enabled {
            return Ok(());
        }
        let cached = match self.repo.sku_mapping_by_local_sku(sku_id).await {
            Ok(Some(m)) => m,
            Ok(None) => return Ok(()),
            Err(error) => {
                tracing::warn!(%error, sku_id, "pre-order stock lookup failed");
                return Ok(());
            }
        };
        let enough = |stock: i32| stock < 0 || stock >= quantity;
        // LQA-I3: a cached "in stock" is only trusted while fresh; an older value (the
        // periodic sync runs every few minutes) is re-checked live so a sold-out supplier
        // is refused before the buyer pays instead of rejected after.
        let fresh = cached
            .stock_synced_at
            .is_some_and(|at| self.now() - at < PRE_ORDER_STOCK_FRESH);
        if fresh && enough(cached.upstream_stock) {
            return Ok(());
        }
        if let Err(error) = self.sync(cached.product_mapping_id).await {
            tracing::warn!(%error, sku_id, "pre-order realtime sync failed");
            return Ok(());
        }
        match self.repo.sku_mapping_by_local_sku(sku_id).await {
            Ok(Some(m)) if !enough(m.upstream_stock) => {
                Err(Error::bad_request(keys::UPSTREAM_STOCK_INSUFFICIENT))
            }
            _ => Ok(()),
        }
    }
}
