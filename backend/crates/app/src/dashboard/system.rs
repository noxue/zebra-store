//! System version and self-update endpoints (original `platform/http/system`).
//!
//! The version check never contacts the internet and self-update is not
//! supported: start/rollback/restart are rejected with the original keys.

use zs_domain::dashboard::system::{
    Capability, CapabilityResponse, CheckResult, UpdateState, keys,
};
use zs_domain::{Error, Result};

/// Reports the running version and the (unsupported) update capability.
#[derive(Debug, Clone)]
pub struct SystemService {
    version: String,
    platform: String,
}

impl SystemService {
    /// `version` like `v0.1.0`; `platform` like `linux/x86_64`.
    pub fn new(version: impl Into<String>, platform: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            platform: platform.into(),
        }
    }

    pub fn check(&self) -> CheckResult {
        CheckResult::offline(&self.version)
    }

    pub fn capability(&self) -> CapabilityResponse {
        CapabilityResponse {
            capability: Capability::unsupported(&self.platform),
            state: UpdateState::idle(),
        }
    }

    pub fn status(&self) -> UpdateState {
        UpdateState::idle()
    }

    pub fn start_update(&self) -> Result<UpdateState> {
        Err(Error::bad_request(keys::UPDATE_NOT_SUPPORTED))
    }

    pub fn rollback(&self) -> Result<UpdateState> {
        Err(Error::bad_request(keys::UPDATE_NO_BACKUP))
    }

    pub fn restart(&self) -> Result<()> {
        Err(Error::bad_request(keys::RESTART_NOT_SUPPORTED))
    }
}
