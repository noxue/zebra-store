//! Composition of every service from its adapters (the only place that knows
//! both `zs-app` services and `zs-infra` implementations).
//!
//! Each business group owns `wire/<group>.rs` with `build` (services) and
//! `jobs` (background handlers).

pub mod affiliate;
pub mod catalog;
pub mod content;
pub mod dashboard;
pub mod identity;
pub mod integration;
pub mod marketing;
pub mod notify;
pub mod order;
pub mod payment;
pub mod provide;
pub mod reseller;
pub mod wallet;

use std::sync::Arc;

use sea_orm::DatabaseConnection;
use zs_app::Services;
use zs_app::config::Config;
use zs_domain::queue::JobQueue;
use zs_domain::settings::SettingsStore;
use zs_shared::clock::{Clock, SystemClock};
use zs_shared::crypto::Cipher;

use crate::db::repo::settings::SeaSettingsStore;
use crate::queue::{DbJobQueue, JobRegistry};

/// Shared adapters handed to every group's wiring function.
#[derive(Clone)]
pub struct WireCtx {
    pub db: DatabaseConnection,
    pub cfg: Arc<Config>,
    pub clock: Arc<dyn Clock>,
    /// AES-256-GCM cipher derived from `app.secret_key`.
    pub cipher: Cipher,
    pub queue: Arc<dyn JobQueue>,
    pub settings: Arc<dyn SettingsStore>,
}

impl std::fmt::Debug for WireCtx {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WireCtx")
    }
}

impl WireCtx {
    pub fn new(db: &DatabaseConnection, cfg: &Config) -> Self {
        Self {
            db: db.clone(),
            cfg: Arc::new(cfg.clone()),
            clock: Arc::new(SystemClock),
            cipher: Cipher::from_secret(&cfg.app.secret_key),
            queue: Arc::new(DbJobQueue::new(db.clone())),
            settings: Arc::new(SeaSettingsStore::new(db.clone())),
        }
    }
}

/// Wires every service with its database-backed adapters.
pub fn services(ctx: &WireCtx) -> Services {
    let ctx = ctx.clone();
    Services {
        identity: identity::build(&ctx),
        catalog: catalog::build(&ctx),
        content: content::build(&ctx),
        marketing: marketing::build(&ctx),
        order: order::build(&ctx),
        payment: payment::build(&ctx),
        wallet: wallet::build(&ctx),
        affiliate: affiliate::build(&ctx),
        reseller: reseller::build(&ctx),
        integration: integration::build(&ctx),
        provide: provide::build(&ctx),
        notify: notify::build(&ctx),
        dashboard: dashboard::build(&ctx),
    }
}

/// Collects every group's job handlers and periodic schedules.
pub fn jobs(ctx: &WireCtx, services: &Services) -> JobRegistry {
    let mut registry = JobRegistry::default();
    identity::jobs(ctx, services, &mut registry);
    catalog::jobs(ctx, services, &mut registry);
    content::jobs(ctx, services, &mut registry);
    marketing::jobs(ctx, services, &mut registry);
    order::jobs(ctx, services, &mut registry);
    payment::jobs(ctx, services, &mut registry);
    wallet::jobs(ctx, services, &mut registry);
    affiliate::jobs(ctx, services, &mut registry);
    reseller::jobs(ctx, services, &mut registry);
    integration::jobs(ctx, services, &mut registry);
    notify::jobs(ctx, services, &mut registry);
    dashboard::jobs(ctx, services, &mut registry);
    registry
}

/// Runs idempotent start-up tasks of every group.
pub async fn bootstrap(ctx: &WireCtx, services: &Services) -> zs_domain::Result<()> {
    reseller::bootstrap(ctx, services).await?;
    payment::bootstrap(ctx, services).await?;
    identity::bootstrap(ctx, services).await
}
