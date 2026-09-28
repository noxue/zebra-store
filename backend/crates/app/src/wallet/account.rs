//! Wallet accounts, transactions, admin adjustments, recharge queries and gift card
//! redemption.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::Serialize;
use zs_domain::identity::user::User;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::wallet::ports::{BalanceChangeRequest, GiftCardRedemption, LedgerError};
use zs_domain::wallet::rules::{
    ADMIN_ADJUST_REMARK, clean_remark, normalize_currency, parse_adjustment, unique_reference,
};
use zs_domain::wallet::{
    Account, RechargeFilter, RechargeOrder, Transaction, TransactionFilter, UserBrief,
    WalletAdminLookup, WalletRepo, keys, txn_type,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::page::{Page, PageRequest};

/// Admin recharge row: the recharge plus its user, channel name and payment status.
#[derive(Debug, Clone, Serialize)]
pub struct AdminRechargeItem {
    #[serde(flatten)]
    pub recharge: RechargeOrder,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<UserBrief>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub channel_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub payment_status: String,
}

/// Wallet queries and balance changes.
#[derive(Clone)]
pub struct WalletService {
    repo: Arc<dyn WalletRepo>,
    lookup: Arc<dyn WalletAdminLookup>,
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for WalletService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WalletService")
    }
}

/// Site currency from `site_config.currency` (`GetSiteCurrency`, default CNY).
pub async fn site_currency(settings: &dyn SettingsStore) -> String {
    let raw = settings.get(setting_keys::SITE_CONFIG).await.ok().flatten();
    zs_domain::settings::schema::site::normalize_currency(
        raw.as_ref().and_then(|v| v.get("currency")),
    )
}

fn unique(mut ids: Vec<Id>) -> Vec<Id> {
    ids.retain(|id| *id > 0);
    ids.sort_unstable();
    ids.dedup();
    ids
}

impl WalletService {
    pub fn new(
        repo: Arc<dyn WalletRepo>,
        lookup: Arc<dyn WalletAdminLookup>,
        settings: Arc<dyn SettingsStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            lookup,
            settings,
            clock,
        }
    }

    pub fn repo(&self) -> &Arc<dyn WalletRepo> {
        &self.repo
    }

    /// The user's account (created on first access).
    pub async fn account(&self, user_id: Id) -> Result<Account> {
        self.repo
            .account(user_id)
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))
    }

    /// The user's own transactions, newest first.
    pub async fn transactions(&self, user_id: Id, page: PageRequest) -> Result<Page<Transaction>> {
        let filter = TransactionFilter {
            user_id,
            ..TransactionFilter::default()
        };
        self.repo
            .list_transactions(&filter, page)
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))
    }

    /// Admin view of a user's wallet (`GetUserWallet`).
    pub async fn admin_user_wallet(&self, user_id: Id) -> Result<(User, Account)> {
        let user = self
            .lookup
            .user(user_id)
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))?
            .ok_or_else(|| Error::not_found(keys::USER_NOT_FOUND))?;
        Ok((user, self.account(user_id).await?))
    }

    /// Admin transaction list with optional type / direction filters.
    pub async fn admin_transactions(
        &self,
        user_id: Id,
        kind: &str,
        direction: &str,
        page: PageRequest,
    ) -> Result<Page<Transaction>> {
        let filter = TransactionFilter {
            user_id,
            kind: kind.trim().to_owned(),
            direction: direction.trim().to_owned(),
            ..TransactionFilter::default()
        };
        self.repo
            .list_transactions(&filter, page)
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))
    }

    /// Admin balance adjustment (WAL-02): explicit operation, mandatory remark, operator
    /// recorded on the transaction. Insufficient balance maps to
    /// `wallet_insufficient_balance` (the original said `payment_amount_mismatch`,
    /// live QA I-12).
    pub async fn admin_adjust(
        &self,
        admin_id: Id,
        user_id: Id,
        amount: &str,
        operation: &str,
        currency: &str,
        remark: &str,
    ) -> Result<(Account, Transaction)> {
        let (delta, remark) = parse_adjustment(amount, operation, remark)?;
        if user_id <= 0 || admin_id <= 0 {
            return Err(Error::invalid());
        }
        let currency = match currency.trim() {
            "" => site_currency(self.settings.as_ref()).await,
            c => c.to_owned(),
        };
        let change = BalanceChangeRequest {
            user_id,
            delta,
            kind: txn_type::ADMIN_ADJUST.to_owned(),
            reference: unique_reference(txn_type::ADMIN_ADJUST, user_id, self.clock.now()),
            remark: clean_remark(&remark, ADMIN_ADJUST_REMARK),
            currency: normalize_currency(&currency),
            operator_admin_id: Some(admin_id),
            order_id: None,
        };
        self.repo
            .change_balance(&change)
            .await
            .map_err(|e| match e {
                LedgerError::Store(e) => e.or_internal(keys::USER_UPDATE_FAILED),
                LedgerError::InsufficientBalance => {
                    Error::bad_request(keys::WALLET_INSUFFICIENT_BALANCE)
                }
                other => other.into(),
            })
    }

    /// Admin recharge list enriched with user, channel name and payment status.
    pub async fn admin_recharges(
        &self,
        filter: &RechargeFilter,
        page: PageRequest,
    ) -> Result<Page<AdminRechargeItem>> {
        let rows = self
            .repo
            .list_recharges(filter, page)
            .await
            .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?;
        let user_ids = unique(rows.items.iter().map(|r| r.user_id).collect());
        let channel_ids = unique(rows.items.iter().map(|r| r.channel_id).collect());
        let payment_ids = unique(rows.items.iter().map(|r| r.payment_id).collect());
        let users: BTreeMap<Id, UserBrief> = if user_ids.is_empty() {
            BTreeMap::new()
        } else {
            self.lookup
                .user_briefs(&user_ids)
                .await
                .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))?
                .into_iter()
                .map(|u| (u.id, u))
                .collect()
        };
        let channels = if channel_ids.is_empty() {
            Default::default()
        } else {
            self.lookup
                .channel_names(&channel_ids)
                .await
                .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?
        };
        let statuses = if payment_ids.is_empty() {
            Default::default()
        } else {
            self.lookup
                .payment_statuses(&payment_ids)
                .await
                .map_err(|e| e.or_internal(keys::PAYMENT_FETCH_FAILED))?
        };
        Ok(rows.map(|recharge| AdminRechargeItem {
            user: users.get(&recharge.user_id).cloned(),
            channel_name: channels
                .get(&recharge.channel_id)
                .cloned()
                .unwrap_or_default(),
            payment_status: statuses
                .get(&recharge.payment_id)
                .cloned()
                .unwrap_or_default(),
            recharge,
        }))
    }

    /// The user's recharges (`ListUserRechargeOrders`).
    pub async fn user_recharges(
        &self,
        user_id: Id,
        status: &str,
        recharge_no: &str,
        page: PageRequest,
    ) -> Result<Page<RechargeOrder>> {
        let filter = RechargeFilter {
            user_id,
            status: status.trim().to_owned(),
            recharge_no: recharge_no.trim().to_owned(),
            ..RechargeFilter::default()
        };
        self.repo
            .list_recharges(&filter, page)
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))
    }

    /// `(total, by_status)` of the user's recharges.
    pub async fn recharge_stats(
        &self,
        user_id: Id,
        recharge_no: &str,
    ) -> Result<(i64, BTreeMap<String, i64>)> {
        let by_status = self
            .repo
            .recharge_stats(user_id, recharge_no.trim())
            .await
            .map_err(|e| e.or_internal(keys::USER_FETCH_FAILED))?;
        Ok((by_status.values().sum(), by_status))
    }

    /// Redeems a gift card into the wallet in one transaction (lock card → credit →
    /// mark redeemed); a card can only ever be redeemed once.
    pub async fn redeem_gift_card(&self, user_id: Id, code: &str) -> Result<GiftCardRedemption> {
        let code = code.trim().to_uppercase();
        if user_id <= 0 || code.is_empty() {
            return Err(Error::bad_request(
                zs_domain::marketing::gift_card::keys::INVALID,
            ));
        }
        self.repo
            .redeem_gift_card(user_id, &code, self.clock.now())
            .await
            .map_err(|e| e.or_internal(keys::GIFT_CARD_REDEEM_FAILED))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::{DateTime, TimeZone, Utc};
    use std::collections::HashMap;
    use std::sync::Mutex;
    use zs_domain::wallet::rules::plan_change;
    use zs_shared::clock::FixedClock;
    use zs_shared::money::Amount;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 0, 0, 0).unwrap()
    }

    #[derive(Default)]
    struct Repo {
        balance: Mutex<Amount>,
        changes: Mutex<Vec<BalanceChangeRequest>>,
    }

    fn account(user_id: Id, balance: Amount) -> Account {
        Account {
            id: 1,
            user_id,
            balance,
            created_at: now(),
            updated_at: now(),
        }
    }

    #[async_trait]
    impl WalletRepo for Repo {
        async fn account(&self, user_id: Id) -> Result<Account> {
            Ok(account(user_id, *self.balance.lock().unwrap()))
        }
        async fn balances(&self, _: &[Id]) -> Result<HashMap<Id, Amount>> {
            Ok(HashMap::new())
        }
        async fn list_transactions(
            &self,
            _: &TransactionFilter,
            _: PageRequest,
        ) -> Result<Page<Transaction>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
        async fn change_balance(
            &self,
            c: &BalanceChangeRequest,
        ) -> std::result::Result<(Account, Transaction), LedgerError> {
            let mut balance = self.balance.lock().unwrap();
            let plan = plan_change(*balance, c.delta)?;
            *balance = plan.after;
            self.changes.lock().unwrap().push(c.clone());
            let txn = Transaction {
                id: 1,
                user_id: c.user_id,
                operator_admin_id: c.operator_admin_id,
                order_id: None,
                kind: c.kind.clone(),
                direction: plan.direction.into(),
                amount: plan.amount,
                balance_before: plan.before,
                balance_after: plan.after,
                currency: c.currency.clone(),
                reference: c.reference.clone(),
                remark: c.remark.clone(),
                created_at: now(),
                updated_at: now(),
            };
            Ok((account(c.user_id, plan.after), txn))
        }
        async fn list_recharges(
            &self,
            _: &RechargeFilter,
            _: PageRequest,
        ) -> Result<Page<RechargeOrder>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
        async fn recharge_stats(&self, _: Id, _: &str) -> Result<BTreeMap<String, i64>> {
            Ok(BTreeMap::from([
                ("pending".into(), 2),
                ("success".into(), 3),
            ]))
        }
        async fn recharge_by_no(&self, _: Id, _: &str) -> Result<Option<RechargeOrder>> {
            Ok(None)
        }
        async fn recharge_by_payment(&self, _: Id, _: Id) -> Result<Option<RechargeOrder>> {
            Ok(None)
        }
        async fn redeem_gift_card(
            &self,
            _: Id,
            _: &str,
            _: DateTime<Utc>,
        ) -> Result<GiftCardRedemption> {
            Err(Error::internal_msg("unused"))
        }
    }

    struct Lookup;

    #[async_trait]
    impl WalletAdminLookup for Lookup {
        async fn user(&self, _: Id) -> Result<Option<User>> {
            Ok(None)
        }
        async fn user_briefs(&self, _: &[Id]) -> Result<Vec<UserBrief>> {
            Ok(vec![])
        }
        async fn channel_names(&self, _: &[Id]) -> Result<HashMap<Id, String>> {
            Ok(HashMap::new())
        }
        async fn payment_statuses(&self, _: &[Id]) -> Result<HashMap<Id, String>> {
            Ok(HashMap::new())
        }
    }

    struct Settings;

    #[async_trait]
    impl SettingsStore for Settings {
        async fn get(&self, _: &str) -> Result<Option<serde_json::Value>> {
            Ok(Some(serde_json::json!({"currency": "usd"})))
        }
        async fn set(&self, _: &str, _: &serde_json::Value) -> Result<()> {
            Ok(())
        }
    }

    fn service(repo: Arc<Repo>) -> WalletService {
        WalletService::new(
            repo,
            Arc::new(Lookup),
            Arc::new(Settings),
            Arc::new(FixedClock(now())),
        )
    }

    /// WAL-02: operator recorded, site currency used, validation before any write.
    #[tokio::test]
    async fn wal_02_admin_adjust() {
        let repo = Arc::new(Repo::default());
        let svc = service(repo.clone());
        let (acc, txn) = svc
            .admin_adjust(9, 5, "12.5", "add", "", " bonus ")
            .await
            .unwrap();
        assert_eq!(acc.balance, Amount::from_cents(1250));
        assert_eq!(txn.operator_admin_id, Some(9));
        assert_eq!(txn.kind, "admin_adjust");
        assert_eq!(txn.currency, "USD");
        assert_eq!(txn.remark, "bonus");
        assert!(txn.reference.starts_with("admin_adjust:5:"));

        let err = svc
            .admin_adjust(9, 5, "20", "subtract", "", "r")
            .await
            .unwrap_err();
        // QA-A12
        assert_eq!(err.key(), keys::WALLET_INSUFFICIENT_BALANCE);
        let err = svc.admin_adjust(9, 5, "1", "", "", "r").await.unwrap_err();
        assert_eq!(err.key(), "error.bad_request");
        let err = svc
            .admin_adjust(9, 5, "1", "add", "", "")
            .await
            .unwrap_err();
        assert_eq!(err.key(), keys::ADJUST_REMARK_REQUIRED);
        assert_eq!(repo.changes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn stats_total_and_admin_user_not_found() {
        let svc = service(Arc::new(Repo::default()));
        let (total, by) = svc.recharge_stats(1, "").await.unwrap();
        assert_eq!(total, 5);
        assert_eq!(by["success"], 3);
        let err = svc.admin_user_wallet(3).await.unwrap_err();
        assert_eq!(err.key(), keys::USER_NOT_FOUND);
        let err = svc.redeem_gift_card(1, "  ").await.unwrap_err();
        assert_eq!(err.key(), "error.gift_card_invalid");
    }
}
