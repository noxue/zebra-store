//! Compliance acknowledgement: status, acknowledge and a lock-free gate check.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::json;
use zs_domain::identity::compliance::{self, ComplianceStatus};
use zs_domain::settings::SettingsStore;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;

use super::admin_auth::ClientInfo;

/// Who acknowledges.
#[derive(Debug, Clone)]
pub struct Acknowledgement<'a> {
    pub segment1: &'a str,
    pub segment2: &'a str,
    pub segment3: &'a str,
    pub admin_id: Id,
    pub username: &'a str,
}

/// Compliance service (cheap to clone).
#[derive(Clone)]
pub struct ComplianceService {
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
    acked: Arc<AtomicBool>,
    write: Arc<tokio::sync::Mutex<()>>,
}

impl std::fmt::Debug for ComplianceService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComplianceService")
            .field("acked", &self.acked.load(Ordering::Relaxed))
            .finish()
    }
}

impl ComplianceService {
    pub fn new(settings: Arc<dyn SettingsStore>, clock: Arc<dyn Clock>) -> Self {
        Self {
            settings,
            clock,
            acked: Arc::new(AtomicBool::new(false)),
            write: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    /// Restores the cached flag from storage (called at start-up).
    pub async fn load(&self) -> Result<()> {
        let status = self.status().await?;
        self.acked.store(status.acknowledged, Ordering::SeqCst);
        Ok(())
    }

    /// Fast check used by the gate middleware (no database access).
    pub fn is_acknowledged(&self) -> bool {
        self.acked.load(Ordering::SeqCst)
    }

    pub async fn status(&self) -> Result<ComplianceStatus> {
        let raw = self.settings.get(compliance::SETTING_KEY).await?;
        Ok(ComplianceStatus::from_value(raw.as_ref()))
    }

    /// Records the acknowledgement; returns `true` when it already existed.
    pub async fn acknowledge(
        &self,
        ack: &Acknowledgement<'_>,
        client: &ClientInfo,
    ) -> Result<bool> {
        compliance::validate_segments(ack.segment1, ack.segment2, ack.segment3)?;
        let _guard = self.write.lock().await;
        if self.is_acknowledged() {
            return Ok(true);
        }
        let value = json!({
            "acknowledged": true,
            "acknowledged_at": crate::identity::rfc3339(self.clock.now()),
            "acknowledged_by_admin_id": ack.admin_id,
            "acknowledged_by_username": ack.username,
            "acknowledged_text": compliance::FULL_TEXT,
            "version": compliance::VERSION,
            "client_ip": client.ip,
            "user_agent": client.user_agent,
        });
        self.settings
            .set(compliance::SETTING_KEY, &value)
            .await
            .map_err(|e| e.or_internal("error.internal"))?;
        self.acked.store(true, Ordering::SeqCst);
        Ok(false)
    }

    /// Error returned by gated routes (`is_super` selects the message).
    pub fn gate_error(is_super: bool) -> Error {
        Error::forbidden(if is_super {
            compliance::MSG_REQUIRED
        } else {
            compliance::MSG_REQUIRED_BY_SUPER_ADMIN
        })
    }
}
