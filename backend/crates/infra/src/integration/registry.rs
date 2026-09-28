//! Supplier adapter registry keyed by protocol id (the [`UpstreamConnector`] of the
//! core services). Adding a supplier system = one adapter module + one
//! [`AdapterRegistry::register`] call in `wire::integration`.

use std::sync::Arc;

use zs_domain::integration::adapter::SupplierAdapter;
use zs_domain::integration::connection::{Endpoint, PROTOCOL_DUJIAO_NEXT};
use zs_domain::integration::protocol::{UpstreamClient, UpstreamConnector};
use zs_domain::{Error, Result};

/// Registered supplier systems.
#[derive(Default, Clone)]
pub struct AdapterRegistry {
    adapters: Vec<Arc<dyn SupplierAdapter>>,
}

impl std::fmt::Debug for AdapterRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<&str> = self.adapters.iter().map(|a| a.meta().id).collect();
        f.debug_struct("AdapterRegistry")
            .field("ids", &ids)
            .finish()
    }
}

impl AdapterRegistry {
    /// Registers (or replaces) the adapter of its protocol id.
    #[must_use]
    pub fn register(mut self, adapter: Arc<dyn SupplierAdapter>) -> Self {
        let id = adapter.meta().id;
        self.adapters.retain(|a| a.meta().id != id);
        self.adapters.push(adapter);
        self
    }
}

impl UpstreamConnector for AdapterRegistry {
    fn open(&self, endpoint: &Endpoint) -> Result<Arc<dyn UpstreamClient>> {
        let protocol = match endpoint.protocol.trim() {
            "" => PROTOCOL_DUJIAO_NEXT,
            p => p,
        };
        self.adapter(protocol)
            .ok_or_else(|| Error::internal_msg(format!("unsupported protocol: {protocol}")))?
            .open(endpoint)
    }

    fn adapters(&self) -> Vec<Arc<dyn SupplierAdapter>> {
        self.adapters.clone()
    }
}
