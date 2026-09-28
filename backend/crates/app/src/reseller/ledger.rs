//! Profit posting, refund claw-back and settlement confirmation — the port the
//! order group calls at payment / refund time (`accounting_profit.go`,
//! `accounting_refund.go`) — plus the `reseller:confirm_ledger` job.
//!
//! When the order group already holds a transaction it should call the
//! `zs_infra::db::repo::reseller::ledger::{post_order_profit_in, deduct_refund_in}`
//! functions with that transaction instead (same rules, no nested transaction).

use std::sync::Arc;

use async_trait::async_trait;
use zs_domain::Result;
use zs_domain::queue::JobHandler;
use zs_domain::reseller::accounting::clamp_confirm_days;
use zs_domain::reseller::ports::{LedgerRepo, OrderPaid, OrderRefunded};
use zs_shared::clock::Clock;

/// Ledger events of the order lifecycle.
#[derive(Clone)]
pub struct LedgerService {
    ledger: Arc<dyn LedgerRepo>,
    confirm_days: i64,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for LedgerService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LedgerService")
            .field("confirm_days", &self.confirm_days)
            .finish_non_exhaustive()
    }
}

impl LedgerService {
    /// `confirm_days` is `reseller.settlement_confirm_days`, clamped to `0..=3650` (RSL-04).
    pub fn new(ledger: Arc<dyn LedgerRepo>, confirm_days: i64, clock: Arc<dyn Clock>) -> Self {
        Self {
            ledger,
            confirm_days: clamp_confirm_days(confirm_days),
            clock,
        }
    }

    /// Effective confirmation window in days.
    pub fn confirm_days(&self) -> i64 {
        self.confirm_days
    }

    /// Order paid: posts its profit as `pending_confirm` (idempotent per order).
    pub async fn on_order_paid(&self, event: &OrderPaid) -> Result<bool> {
        self.ledger
            .post_order_profit(event, self.clock.now(), self.confirm_days)
            .await
    }

    /// Refund recorded: claws back the proportional profit (idempotent per refund record).
    pub async fn on_order_refunded(&self, event: &OrderRefunded) -> Result<bool> {
        self.ledger
            .deduct_refund(event, self.clock.now(), self.confirm_days)
            .await
    }

    /// Confirms due profits and refreshes the balances (`ConfirmDueLedgerEntries`).
    pub async fn confirm_due(&self) -> Result<u64> {
        self.ledger.confirm_due(self.clock.now()).await
    }
}

/// `reseller:confirm_ledger` handler (scheduled every minute).
#[derive(Debug, Clone)]
pub struct ConfirmLedgerJob(pub LedgerService);

#[async_trait]
impl JobHandler for ConfirmLedgerJob {
    async fn handle(&self, _payload: serde_json::Value) -> Result<()> {
        let affected = self.0.confirm_due().await?;
        tracing::debug!(affected, "reseller ledger confirmed");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, clock};

    #[test]
    fn confirm_days_are_clamped() {
        let store = Arc::new(MemStore::default());
        assert_eq!(
            LedgerService::new(store.clone(), -5, clock()).confirm_days(),
            0
        );
        assert_eq!(
            LedgerService::new(store, 99_999, clock()).confirm_days(),
            3650
        );
    }
}
