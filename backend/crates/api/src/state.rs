//! Shared handler state.

use std::sync::Arc;

use zs_app::Services;
use zs_app::config::Config;

use crate::client::Cidr;

/// State shared by every handler.
#[derive(Debug, Clone)]
pub struct AppState {
    pub svc: Arc<Services>,
    pub cfg: Arc<Config>,
    pub trusted_proxies: Arc<Vec<Cidr>>,
    /// Every authenticated admin route (filled by [`crate::build`]); source of
    /// `/admin/authz/permissions/catalog`.
    pub admin_permissions: Arc<Vec<crate::routes::Permission>>,
}

impl AppState {
    pub fn new(svc: Services, cfg: Config) -> Self {
        let trusted = cfg
            .server
            .trusted_proxies
            .iter()
            .filter_map(|c| Cidr::parse(c))
            .collect();
        Self {
            svc: Arc::new(svc),
            cfg: Arc::new(cfg),
            trusted_proxies: Arc::new(trusted),
            admin_permissions: Arc::new(Vec::new()),
        }
    }
}
