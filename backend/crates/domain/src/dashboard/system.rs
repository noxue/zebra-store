//! System information: version check and self-update capability (port of
//! `internal/version`, `internal/selfupdate` and `platform/http/system`).
//!
//! Zebra Store never contacts GitHub and never replaces its own binary: the
//! version check reports the running version as the latest one and the
//! self-update capability is always blocked as a source build.

use serde::Serialize;

/// Build type reported by the capability (`version.BuildTypeSource`).
pub const BUILD_TYPE_SOURCE: &str = "source";
/// Block reason of source builds (`selfupdate.BlockSourceBuild`).
pub const BLOCK_SOURCE_BUILD: &str = "source_build";
/// Error keys of the update endpoints (identical to the original).
pub mod keys {
    pub const UPDATE_NOT_SUPPORTED: &str = "error.update_not_supported";
    pub const UPDATE_NO_BACKUP: &str = "error.update_no_backup";
    pub const RESTART_NOT_SUPPORTED: &str = "error.restart_not_supported";
}

/// `GET /admin/system/version/check` (original `version.CheckResult`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub has_update: bool,
    pub source: String,
}

impl CheckResult {
    /// Offline check: the running version is the latest one.
    pub fn offline(version: &str) -> Self {
        Self {
            current_version: version.to_owned(),
            latest_version: version.to_owned(),
            has_update: false,
            source: "local".to_owned(),
        }
    }
}

/// Self-update capability (original `selfupdate.Capability`).
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Capability {
    pub can_update: bool,
    pub can_restart: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub block_reason: String,
    pub deployment: String,
    pub supervisor: String,
    pub build_type: String,
    pub platform: String,
    pub has_backup: bool,
    pub rollback_unsafe: bool,
}

impl Capability {
    /// Updating is never supported (blocked like a source build).
    pub fn unsupported(platform: &str) -> Self {
        Self {
            can_update: false,
            can_restart: false,
            block_reason: BLOCK_SOURCE_BUILD.to_owned(),
            deployment: "binary".to_owned(),
            supervisor: "none".to_owned(),
            build_type: BUILD_TYPE_SOURCE.to_owned(),
            platform: platform.to_owned(),
            has_backup: false,
            rollback_unsafe: false,
        }
    }
}

/// Update task snapshot (original `selfupdate.State`); always idle.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateState {
    pub status: String,
    pub stage: String,
    pub percent: u8,
    pub restart_required: bool,
}

impl UpdateState {
    pub fn idle() -> Self {
        Self {
            status: "idle".to_owned(),
            stage: "idle".to_owned(),
            percent: 0,
            restart_required: false,
        }
    }
}

/// `GET /admin/system/update/capability`.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CapabilityResponse {
    pub capability: Capability,
    pub state: UpdateState,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn capability_shape() {
        let c = CapabilityResponse {
            capability: Capability::unsupported("linux/x86_64"),
            state: UpdateState::idle(),
        };
        assert_eq!(
            serde_json::to_value(c).unwrap(),
            json!({
                "capability": {
                    "can_update": false,
                    "can_restart": false,
                    "block_reason": "source_build",
                    "deployment": "binary",
                    "supervisor": "none",
                    "build_type": "source",
                    "platform": "linux/x86_64",
                    "has_backup": false,
                    "rollback_unsafe": false
                },
                "state": {"status": "idle", "stage": "idle", "percent": 0, "restart_required": false}
            })
        );
        let check = serde_json::to_value(CheckResult::offline("v0.1.0")).unwrap();
        assert_eq!(check["has_update"], false);
        assert_eq!(check["latest_version"], "v0.1.0");
    }
}
