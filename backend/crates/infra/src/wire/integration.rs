//! Wiring of the `integration` group.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;
use zs_app::content::media::{MediaService, UploadPolicy, UploadService};
use zs_app::identity::rate_limit::{RateLimiter, RateRule};
use zs_app::integration::card_converter::CardConverterService;
use zs_app::integration::connection::ConnectionService;
use zs_app::integration::credential::CredentialService;
use zs_app::integration::downstream::{CallbackJob, DownstreamService};
use zs_app::integration::inbound::{InboundService, SyncConnectionJob};
use zs_app::integration::mapping::MappingService;
use zs_app::integration::procurement::{ProcurementJob, ProcurementService};
use zs_app::integration::reconciliation::{ReconciliationService, RunJob};
use zs_app::integration::supplier::SupplierService;
use zs_app::integration::zs_supplier::{DeliverEventJob, ZsSupplier, ZsSupplierDeps};
use zs_app::integration::{IntegrationServices, OrderEvents};
use zs_domain::Result;
use zs_domain::content::media::FileStore;
use zs_domain::integration::adapter::SupplierAdapter;
use zs_domain::integration::credential::CredentialRepo;
use zs_domain::integration::hooks::{IntegrationOrderEvents, ProcurementLifecycle};
use zs_domain::integration::mapping::MappingRepo;
use zs_domain::integration::procurement::ProcurementRepo;
use zs_domain::integration::supplier::{
    RATE_LIMIT_BLOCK_SECS, RATE_LIMIT_MAX, RATE_LIMIT_WINDOW_SECS, UpstreamOrdering,
};
use zs_domain::queue::{JobHandler, kinds};

use super::WireCtx;
use crate::content::files::LocalFileStore;
use crate::db::repo::content::banner::SeaMediaRepo;
use crate::db::repo::integration::card_converter::SeaCardConverterRepo;
use crate::db::repo::integration::connection::SeaConnectionRepo;
use crate::db::repo::integration::credential::SeaCredentialRepo;
use crate::db::repo::integration::downstream::SeaOrderRefRepo;
use crate::db::repo::integration::mapping::SeaMappingRepo;
use crate::db::repo::integration::orders::SeaIntegrationOrders;
use crate::db::repo::integration::procurement::{SeaMappingLookup, SeaProcurementRepo};
use crate::db::repo::integration::reconciliation::SeaReconciliationRepo;
use crate::db::repo::integration::supplier::SeaSupplierCatalog;
use crate::db::repo::integration::zs::{SeaSnapshotSource, SeaZsStore};
use crate::integration::acg_faka::AcgFakaAdapter;
use crate::integration::card_converter::HttpCardConverter;
use crate::integration::client::{DujiaoNextAdapter, HttpConnector};
use crate::integration::http::AddressPolicy;
use crate::integration::mcy_openapi::McyShopAdapter;
use crate::integration::registry::AdapterRegistry;
use crate::integration::zebra_store::{HttpEventSender, ZebraStoreAdapter};
use crate::integration::{HttpCallbackSender, UploadImageStore};
use crate::queue::JobRegistry;

/// How often the stock-sync job checks whether the configured interval elapsed.
const SYNC_STOCK_TICK: Duration = Duration::from_secs(60);
/// Periodic re-check of accepted purchase orders (original 30 min).
const SYNC_ACCEPTED_INTERVAL: Duration = Duration::from_secs(30 * 60);
/// Bounded health sweep of enabled third-party card converters.
const CARD_CONVERTER_HEALTH_INTERVAL: Duration = Duration::from_secs(60);

/// `/upstream/*` limit: 60 requests / 60 s per `IP|API key`, 30 s block (UPS-10).
const API_RATE_RULE: RateRule = RateRule {
    window_seconds: RATE_LIMIT_WINDOW_SECS,
    max_requests: RATE_LIMIT_MAX,
    block_seconds: RATE_LIMIT_BLOCK_SECS,
};
/// `/zs/*` limit per API key (zebra-store spec §4: 120 / min).
const ZS_RATE_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: zs_domain::integration::zs::REQUESTS_PER_MINUTE,
    block_seconds: 30,
};
/// Snapshot-diff interval of the zebra-store change feed.
const ZS_SNAPSHOT_TICK: Duration =
    Duration::from_secs(zs_domain::integration::zs::SNAPSHOT_INTERVAL_SECS);
/// `/upstream/callback` limit per IP (original `callbackRule`).
const CALLBACK_RATE_RULE: RateRule = RateRule {
    window_seconds: 60,
    max_requests: 120,
    block_seconds: 60,
};

/// Replaceable adapters (tests use `AllowPrivate` to reach local mock servers).
#[derive(Clone)]
pub struct Adapters {
    /// SSRF policy of supplier calls and downstream callbacks (`PublicOnly` in production).
    pub address_policy: AddressPolicy,
    /// Order-group port for API orders (`None` = the order group's implementation).
    pub ordering: Option<Arc<dyn UpstreamOrdering>>,
    /// Order-group port for procurement side effects (`None` = the order group's
    /// implementation).
    pub lifecycle: Option<Arc<dyn ProcurementLifecycle>>,
    /// Additional supplier systems registered next to the built-in ones (tests).
    pub extra_adapters: Vec<Arc<dyn SupplierAdapter>>,
}

impl std::fmt::Debug for Adapters {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Adapters")
            .field("address_policy", &self.address_policy)
            .finish_non_exhaustive()
    }
}

impl Default for Adapters {
    fn default() -> Self {
        Self {
            address_policy: AddressPolicy::PublicOnly,
            ordering: None,
            lifecycle: None,
            extra_adapters: Vec::new(),
        }
    }
}

impl Adapters {
    /// Production adapters with the SSRF policy of `integration.allow_private_addresses`.
    pub fn from_config(cfg: &zs_app::config::Config) -> Self {
        Self {
            address_policy: if cfg.integration.allow_private_addresses {
                AddressPolicy::AllowPrivate
            } else {
                AddressPolicy::PublicOnly
            },
            ..Self::default()
        }
    }
}

/// Builds the `integration` services with the production adapters.
pub fn build(ctx: &WireCtx) -> IntegrationServices {
    if ctx.cfg.integration.allow_private_addresses {
        tracing::warn!(
            "integration.allow_private_addresses is enabled: supplier calls, callbacks and \
             pushed events may reach private / loopback / LAN addresses (SSRF risk); use only \
             on trusted networks"
        );
    }
    build_with(ctx, &Adapters::from_config(&ctx.cfg))
}

/// Builds the `integration` services with explicit adapters.
pub fn build_with(ctx: &WireCtx, adapters: &Adapters) -> IntegrationServices {
    let db = &ctx.db;
    let clock = ctx.clock.clone();
    let converter_repo = Arc::new(SeaCardConverterRepo::new(db.clone()));
    let card_converters = CardConverterService::new(
        converter_repo,
        Arc::new(HttpCardConverter::new(adapters.address_policy)),
        ctx.cipher.clone(),
        clock.clone(),
        Arc::new(zs_app::notify::center::QueueNotifier::new(
            ctx.queue.clone(),
        )),
    );
    let credential_repo: Arc<dyn CredentialRepo> = Arc::new(SeaCredentialRepo::new(db.clone()));
    let mapping_repo: Arc<dyn MappingRepo> = Arc::new(SeaMappingRepo::new(db.clone()));
    let procurement_repo: Arc<dyn ProcurementRepo> = Arc::new(SeaProcurementRepo::new(db.clone()));
    let orders = Arc::new(SeaIntegrationOrders::new(db.clone()));
    let refs = Arc::new(SeaOrderRefRepo::new(db.clone()));
    let http = HttpConnector::new(
        adapters.address_policy,
        clock.clone(),
        ctx.cfg.upload.max_size,
    );
    // Supplier systems: one registration line per adapter.
    let mut registry = AdapterRegistry::default()
        .register(Arc::new(DujiaoNextAdapter::new(http.clone())))
        .register(Arc::new(ZebraStoreAdapter::new(http.clone())))
        .register(Arc::new(AcgFakaAdapter::new(http.clone())))
        .register(Arc::new(McyShopAdapter::new(http)));
    for adapter in &adapters.extra_adapters {
        registry = registry.register(adapter.clone());
    }
    let connector = Arc::new(registry);
    let zs_store = Arc::new(SeaZsStore::new(db.clone()));

    let credentials =
        CredentialService::new(credential_repo.clone(), ctx.cipher.clone(), clock.clone())
            .with_security(zs_store.clone());
    let connections = ConnectionService::new(
        Arc::new(SeaConnectionRepo::new(db.clone())),
        mapping_repo.clone(),
        connector,
        ctx.cipher.clone(),
        clock.clone(),
    );
    let files: Arc<dyn FileStore> = Arc::new(LocalFileStore::new(&ctx.cfg.upload.dir));
    let upload = &ctx.cfg.upload;
    let images = UploadImageStore::new(
        UploadService::new(
            UploadPolicy {
                max_size: upload.max_size,
                allowed_types: upload.allowed_types.clone(),
                allowed_extensions: upload.allowed_extensions.clone(),
                max_width: upload.max_width,
                max_height: upload.max_height,
            },
            files.clone(),
            clock.clone(),
        ),
        MediaService::new(Arc::new(SeaMediaRepo::new(db.clone())), files),
    );
    let mappings = MappingService::new(
        mapping_repo,
        connections.clone(),
        Arc::new(images),
        ctx.settings.clone(),
        ctx.cfg.queue.upstream_sync_interval.clone(),
        clock.clone(),
    );
    let downstream = DownstreamService::new(
        refs.clone(),
        orders.clone(),
        credential_repo,
        Arc::new(HttpCallbackSender::new(adapters.address_policy)),
        ctx.cipher.clone(),
        ctx.queue.clone(),
        clock.clone(),
    );
    let lifecycle = adapters.lifecycle.clone().unwrap_or_else(|| {
        super::order::integration_ports_with_converter(ctx, card_converters.clone())
    });
    let procurement = ProcurementService::new(
        procurement_repo.clone(),
        orders,
        Arc::new(SeaMappingLookup::new(db.clone())),
        connections.clone(),
        lifecycle,
        downstream.clone(),
        ctx.queue.clone(),
        clock.clone(),
    );
    let reconciliation = ReconciliationService::new(
        Arc::new(SeaReconciliationRepo::new(db.clone())),
        procurement_repo,
        connections.clone(),
        ctx.queue.clone(),
        clock.clone(),
    );
    let ordering = adapters.ordering.clone().unwrap_or_else(|| {
        super::order::integration_ports_with_converter(ctx, card_converters.clone())
    });
    let catalog = SeaSupplierCatalog::new(db.clone(), ctx.settings.clone());
    let supplier = SupplierService::new(
        Arc::new(catalog.clone()),
        ordering.clone(),
        refs.clone(),
        procurement.clone(),
    );
    let inbound = InboundService::new(
        connections.clone(),
        procurement.clone(),
        zs_store.clone(),
        ctx.queue.clone(),
        clock.clone(),
    );
    let zs = ZsSupplier::new(ZsSupplierDeps {
        supplier: supplier.clone(),
        credentials: credentials.clone(),
        catalog: Arc::new(catalog.clone()),
        source: Arc::new(SeaSnapshotSource::new(catalog, db.clone())),
        store: zs_store.clone(),
        security: zs_store,
        ordering,
        refs,
        orders: Arc::new(SeaIntegrationOrders::new(db.clone())),
        sender: Arc::new(HttpEventSender::new(adapters.address_policy, clock.clone())),
        queue: ctx.queue.clone(),
        clock: clock.clone(),
        allow_private_urls: adapters.address_policy == AddressPolicy::AllowPrivate,
    });
    let order_events: Arc<dyn IntegrationOrderEvents> = Arc::new(OrderEvents {
        procurement: procurement.clone(),
        downstream: downstream.clone(),
        mappings: mappings.clone(),
        zs: zs.clone(),
        card_converters: card_converters.clone(),
        settings: ctx.settings.clone(),
    });
    IntegrationServices {
        card_converters,
        credentials,
        connections,
        mappings,
        procurement,
        downstream,
        reconciliation,
        supplier,
        inbound,
        zs,
        order_events,
        api_limiter: RateLimiter::new(API_RATE_RULE, clock.clone()),
        callback_limiter: RateLimiter::new(CALLBACK_RATE_RULE, clock.clone()),
        zs_limiter: RateLimiter::new(ZS_RATE_RULE, clock),
    }
}

/// The port the order group calls on payment / status changes (see
/// [`IntegrationOrderEvents`]); take it from the built services.
pub fn order_events(services: &zs_app::Services) -> Arc<dyn IntegrationOrderEvents> {
    services.integration.order_events.clone()
}

struct SyncConnectionHandler(MappingService);

#[async_trait]
impl JobHandler for SyncConnectionHandler {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: SyncConnectionJob = serde_json::from_value(payload)?;
        self.0.sync_connection_now(job.connection_id).await
    }
}

struct ZsSnapshotJob(ZsSupplier);

#[async_trait]
impl JobHandler for ZsSnapshotJob {
    async fn handle(&self, _payload: Value) -> Result<()> {
        self.0.snapshot_tick().await.map(|_| ())
    }
}

struct ZsDeliverJob(ZsSupplier);

#[async_trait]
impl JobHandler for ZsDeliverJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: DeliverEventJob = serde_json::from_value(payload)?;
        self.0.deliver(job.event_row_id).await
    }
}

struct SyncStockJob(MappingService);

#[async_trait]
impl JobHandler for SyncStockJob {
    async fn handle(&self, _payload: Value) -> Result<()> {
        self.0.sync_stock_if_due().await
    }
}

struct SubmitJob(ProcurementService);

#[async_trait]
impl JobHandler for SubmitJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: ProcurementJob = serde_json::from_value(payload)?;
        self.0.submit(job.procurement_order_id).await
    }
}

struct PollJob(ProcurementService);

#[async_trait]
impl JobHandler for PollJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: ProcurementJob = serde_json::from_value(payload)?;
        self.0.poll(job.procurement_order_id).await
    }
}

struct SyncAcceptedJob(ProcurementService);

#[async_trait]
impl JobHandler for SyncAcceptedJob {
    async fn handle(&self, _payload: Value) -> Result<()> {
        self.0.sync_accepted().await
    }
}

struct DownstreamCallbackJob(DownstreamService);

#[async_trait]
impl JobHandler for DownstreamCallbackJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: CallbackJob = serde_json::from_value(payload)?;
        self.0.send(job.ref_id).await
    }
}

struct ReconciliationJob(ReconciliationService);

#[async_trait]
impl JobHandler for ReconciliationJob {
    async fn handle(&self, payload: Value) -> Result<()> {
        let job: RunJob = serde_json::from_value(payload)?;
        self.0.execute(job.job_id).await
    }
}

struct CardConverterHealthJob {
    card_converters: zs_app::integration::card_converter::CardConverterService,
    procurement: ProcurementService,
    orders: zs_app::order::OrderService,
}

#[async_trait]
impl JobHandler for CardConverterHealthJob {
    async fn handle(&self, _payload: Value) -> Result<()> {
        let recovered = self.card_converters.health_tick().await?;
        for converter_id in recovered {
            self.procurement
                .retry_held_for_converter(converter_id, &self.card_converters)
                .await?;
        }
        self.orders.retry_pending_conversions().await?;
        Ok(())
    }
}

/// Registers the `integration` job handlers and periodic jobs.
pub fn jobs(_ctx: &WireCtx, services: &zs_app::Services, registry: &mut JobRegistry) {
    let s = &services.integration;
    registry
        .handle(
            kinds::UPSTREAM_SYNC_STOCK,
            Arc::new(SyncStockJob(s.mappings.clone())),
        )
        .every(kinds::UPSTREAM_SYNC_STOCK, SYNC_STOCK_TICK)
        .handle(
            kinds::PROCUREMENT_SUBMIT,
            Arc::new(SubmitJob(s.procurement.clone())),
        )
        .handle(
            kinds::PROCUREMENT_POLL,
            Arc::new(PollJob(s.procurement.clone())),
        )
        .handle(
            kinds::PROCUREMENT_SYNC_ACCEPTED,
            Arc::new(SyncAcceptedJob(s.procurement.clone())),
        )
        .every(kinds::PROCUREMENT_SYNC_ACCEPTED, SYNC_ACCEPTED_INTERVAL)
        .handle(
            kinds::DOWNSTREAM_CALLBACK,
            Arc::new(DownstreamCallbackJob(s.downstream.clone())),
        )
        .handle(
            kinds::RECONCILIATION_RUN,
            Arc::new(ReconciliationJob(s.reconciliation.clone())),
        )
        .handle(
            kinds::UPSTREAM_SYNC_CONNECTION,
            Arc::new(SyncConnectionHandler(s.mappings.clone())),
        )
        .handle(
            kinds::ZS_CATALOG_SNAPSHOT,
            Arc::new(ZsSnapshotJob(s.zs.clone())),
        )
        .every(kinds::ZS_CATALOG_SNAPSHOT, ZS_SNAPSHOT_TICK)
        .handle(
            kinds::ZS_DELIVER_EVENT,
            Arc::new(ZsDeliverJob(s.zs.clone())),
        )
        .handle(
            kinds::CARD_CONVERTER_HEALTH_TICK,
            Arc::new(CardConverterHealthJob {
                card_converters: s.card_converters.clone(),
                procurement: s.procurement.clone(),
                orders: services.order.service.clone(),
            }),
        )
        .every(
            kinds::CARD_CONVERTER_HEALTH_TICK,
            CARD_CONVERTER_HEALTH_INTERVAL,
        );
}
