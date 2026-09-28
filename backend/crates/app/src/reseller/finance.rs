//! Balances, ledger, withdrawals (`application/accounting_query.go`,
//! `accounting_withdraw.go`).

use std::sync::Arc;

use rust_decimal::Decimal;
use zs_domain::reseller::accounting::withdraw_draft;
use zs_domain::reseller::ports::{FinanceFilter, LedgerRepo, ProfileRepo, WithdrawAction};
use zs_domain::reseller::rules::{require_settleable, withdraw_availability};
use zs_domain::reseller::{BalanceAccount, LedgerEntry, Profile, WithdrawRequest, not_found};
use zs_domain::{Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

use super::profile_of_user;

/// Balances shown on the console dashboard.
const DASHBOARD_BALANCES: u64 = 100;

/// `GET /reseller/dashboard`.
#[derive(Debug, Clone, PartialEq)]
pub struct Dashboard {
    pub profile: Option<Profile>,
    pub balances: Vec<BalanceAccount>,
    pub withdraw_enabled: bool,
    pub withdraw_disabled_reason: &'static str,
}

/// Finance use cases.
#[derive(Clone)]
pub struct FinanceService {
    profiles: Arc<dyn ProfileRepo>,
    ledger: Arc<dyn LedgerRepo>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for FinanceService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FinanceService")
    }
}

impl FinanceService {
    pub fn new(
        profiles: Arc<dyn ProfileRepo>,
        ledger: Arc<dyn LedgerRepo>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            profiles,
            ledger,
            clock,
        }
    }

    /// Dashboard; a user without profile gets `opened = false` (no error).
    pub async fn dashboard(&self, user_id: Id) -> Result<Dashboard> {
        let Some(profile) = self.profiles.profile_by_user(user_id).await? else {
            return Ok(Dashboard {
                profile: None,
                balances: Vec::new(),
                withdraw_enabled: false,
                withdraw_disabled_reason: "",
            });
        };
        let filter = FinanceFilter {
            reseller_id: Some(profile.id),
            ..FinanceFilter::default()
        };
        let balances = self
            .ledger
            .list_balances(&filter, PageRequest::new(Some(1), Some(DASHBOARD_BALANCES)))
            .await?
            .items;
        let (withdraw_enabled, withdraw_disabled_reason) = withdraw_availability(&profile);
        Ok(Dashboard {
            profile: Some(profile),
            balances,
            withdraw_enabled,
            withdraw_disabled_reason,
        })
    }

    /// Scopes a console filter to the user's settleable profile.
    async fn scoped(&self, user_id: Id, mut filter: FinanceFilter) -> Result<FinanceFilter> {
        let profile = profile_of_user(self.profiles.as_ref(), user_id).await?;
        require_settleable(&profile)?;
        filter.admin = false;
        filter.reseller_id = Some(profile.id);
        filter.user_id = None;
        Ok(filter)
    }

    pub async fn user_balances(
        &self,
        user_id: Id,
        filter: FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<BalanceAccount>> {
        let filter = self.scoped(user_id, filter).await?;
        self.ledger.list_balances(&filter, page).await
    }

    pub async fn user_ledger(
        &self,
        user_id: Id,
        filter: FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<LedgerEntry>> {
        let filter = self.scoped(user_id, filter).await?;
        self.ledger.list_ledger(&filter, page).await
    }

    pub async fn user_withdraws(
        &self,
        user_id: Id,
        filter: FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        let filter = self.scoped(user_id, filter).await?;
        self.ledger.list_withdraws(&filter, page).await
    }

    /// Requests a withdrawal (`ApplyUserWithdraw`).
    pub async fn apply_withdraw(
        &self,
        user_id: Id,
        amount: Decimal,
        currency: &str,
        channel: &str,
        account: &str,
    ) -> Result<WithdrawRequest> {
        let profile = profile_of_user(self.profiles.as_ref(), user_id).await?;
        require_settleable(&profile)?;
        let draft = withdraw_draft(amount, currency, channel, account)?;
        self.ledger
            .apply_withdraw(profile.id, &draft, self.clock.now())
            .await
    }

    /// Admin pay / reject.
    pub async fn review_withdraw(
        &self,
        admin_id: Id,
        id: Id,
        action: WithdrawAction,
        reason: &str,
    ) -> Result<WithdrawRequest> {
        self.ledger
            .review_withdraw(id, admin_id, action, reason, self.clock.now())
            .await?
            .ok_or_else(not_found)
    }

    pub async fn admin_balances(
        &self,
        mut filter: FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<BalanceAccount>> {
        filter.admin = true;
        self.ledger.list_balances(&filter, page).await
    }

    pub async fn admin_ledger(
        &self,
        mut filter: FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<LedgerEntry>> {
        filter.admin = true;
        self.ledger.list_ledger(&filter, page).await
    }

    pub async fn admin_withdraws(
        &self,
        mut filter: FinanceFilter,
        page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        filter.admin = true;
        self.ledger.list_withdraws(&filter, page).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reseller::testkit::{MemStore, clock};
    use zs_domain::reseller::rules::{
        WITHDRAW_DISABLED_PROFILE_INACTIVE, WITHDRAW_DISABLED_SETTLEMENT_UNAVAILABLE,
    };
    use zs_domain::reseller::{ProfileStatus, SettlementStatus, keys};

    // RSL-02: dashboard withdraw flags and console guards.
    #[tokio::test]
    async fn rsl02_dashboard_flags() {
        let store = Arc::new(MemStore::default());
        let svc = FinanceService::new(store.clone(), store.clone(), clock());
        let d = svc.dashboard(5).await.unwrap_or_else(|e| panic!("{e}"));
        assert!(d.profile.is_none() && !d.withdraw_enabled);
        let pid = store.add_profile(5, ProfileStatus::Disabled);
        let d = svc.dashboard(5).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            (d.withdraw_enabled, d.withdraw_disabled_reason),
            (false, WITHDRAW_DISABLED_PROFILE_INACTIVE)
        );
        assert_eq!(
            svc.apply_withdraw(5, Decimal::ONE, "CNY", "alipay", "a")
                .await
                .unwrap_err()
                .key(),
            keys::PROFILE_INACTIVE
        );
        store.set_profile(pid, ProfileStatus::Active, SettlementStatus::Frozen);
        let d = svc.dashboard(5).await.unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            d.withdraw_disabled_reason,
            WITHDRAW_DISABLED_SETTLEMENT_UNAVAILABLE
        );
        assert_eq!(
            svc.user_ledger(5, FinanceFilter::default(), PageRequest::default())
                .await
                .unwrap_err()
                .key(),
            keys::SETTLEMENT_UNAVAILABLE
        );
        store.set_profile(pid, ProfileStatus::Active, SettlementStatus::Normal);
        assert_eq!(
            svc.apply_withdraw(5, Decimal::ZERO, "CNY", "alipay", "a")
                .await
                .unwrap_err()
                .key(),
            keys::WITHDRAW_AMOUNT_INVALID
        );
    }
}
