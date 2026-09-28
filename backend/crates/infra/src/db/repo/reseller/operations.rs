//! [`OperationsRepo`]: admin overview / finance aggregates (`gormstore/operations.go`),
//! aggregated in Rust over the selected columns so decimals stay exact on SQLite.

use std::collections::{BTreeMap, HashMap, HashSet};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QuerySelect};
use zs_domain::reseller::operations::{
    CurrentCurrency, Lifecycle, OrderCounters, OverviewRows, PeriodCurrency, TopReseller,
    currency_key,
};
use zs_domain::reseller::orders::PAID_STATUSES;
use zs_domain::reseller::ports::OperationsRepo;
use zs_domain::reseller::pricing::{PROFIT_BLOCK_OWNER, PROFIT_BLOCK_RELATED_ACCOUNT};
use zs_domain::reseller::{
    BalanceStatus, DomainStatus, DomainType, LedgerStatus, LedgerType, ProfileStatus,
    SettlementStatus, VerificationStatus, WithdrawStatus,
};
use zs_domain::{Id, Result};

use super::{SeaResellerStore, load_users};
use crate::db::entity::{
    orders, reseller_balance_accounts as balances, reseller_domains as domains,
    reseller_ledger_entries as ledger, reseller_order_snapshots as snapshots,
    reseller_profiles as profiles, reseller_site_configs as sites,
    reseller_withdraw_requests as withdraws,
};
use crate::db::repo::support::DbResultExt;

/// Top resellers listed on the overview.
const TOP_RESELLERS: usize = 10;

fn is_paid(status: &str) -> bool {
    PAID_STATUSES.contains(&status)
}

/// (id, reseller_id, status, currency, total_amount, created_at) of parent reseller orders in the window.
type OrderRow = (Id, Option<Id>, String, String, Decimal, DateTime<Utc>);

impl SeaResellerStore {
    async fn window_orders(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<OrderRow>> {
        orders::Entity::find()
            .select_only()
            .column(orders::Column::Id)
            .column(orders::Column::ResellerId)
            .column(orders::Column::Status)
            .column(orders::Column::Currency)
            .column(orders::Column::TotalAmount)
            .column(orders::Column::CreatedAt)
            .filter(orders::Column::DeletedAt.is_null())
            .filter(orders::Column::ResellerId.is_not_null())
            .filter(orders::Column::ParentId.is_null())
            .filter(orders::Column::CreatedAt.gte(start))
            .filter(orders::Column::CreatedAt.lt(end))
            .into_tuple()
            .all(&self.db)
            .await
            .dom()
    }

    async fn lifecycle(
        &self,
    ) -> Result<(
        Lifecycle,
        Vec<profiles::Model>,
        Vec<domains::Model>,
        HashSet<Id>,
    )> {
        let profile_rows = profiles::Entity::find()
            .filter(profiles::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?;
        let domain_rows = domains::Entity::find()
            .filter(domains::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?;
        let configured: Vec<Id> = sites::Entity::find()
            .select_only()
            .column(sites::Column::ResellerId)
            .filter(sites::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        let count_p = |f: &dyn Fn(&profiles::Model) -> bool| {
            profile_rows.iter().filter(|p| f(p)).count() as i64
        };
        let count_d = |f: &dyn Fn(&domains::Model) -> bool| {
            domain_rows.iter().filter(|d| f(d)).count() as i64
        };
        let status = |s: ProfileStatus| move |p: &profiles::Model| p.status == s.as_str();
        let configured_set: HashSet<Id> = configured.iter().copied().collect();
        let lifecycle = Lifecycle {
            profiles_total: profile_rows.len() as i64,
            profiles_pending_review: count_p(&status(ProfileStatus::PendingReview)),
            profiles_active: count_p(&status(ProfileStatus::Active)),
            profiles_rejected: count_p(&status(ProfileStatus::Rejected)),
            profiles_disabled: count_p(&status(ProfileStatus::Disabled)),
            profiles_settlement_frozen: count_p(&|p| {
                p.settlement_status == SettlementStatus::Frozen.as_str()
            }),
            domains_total: domain_rows.len() as i64,
            domains_pending_review: count_d(&|d| d.status == DomainStatus::PendingReview.as_str()),
            domains_active: count_d(&|d| d.status == DomainStatus::Active.as_str()),
            domains_disabled: count_d(&|d| d.status == DomainStatus::Disabled.as_str()),
            domains_pending_verification: count_d(&|d| {
                d.verification_status == VerificationStatus::Pending.as_str()
            }),
            domains_verified: count_d(&|d| {
                d.verification_status == VerificationStatus::Verified.as_str()
            }),
            custom_domains: count_d(&|d| d.type_ == DomainType::Custom.as_str()),
            subdomains: count_d(&|d| d.type_ == DomainType::Subdomain.as_str()),
            site_configs_total: configured.len() as i64,
            active_profiles_without_site_config: count_p(&|p| {
                p.status == ProfileStatus::Active.as_str() && !configured_set.contains(&p.id)
            }),
        };
        Ok((lifecycle, profile_rows, domain_rows, configured_set))
    }
}

#[async_trait]
impl OperationsRepo for SeaResellerStore {
    async fn overview(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<OverviewRows> {
        let (lifecycle, profile_rows, domain_rows, configured) = self.lifecycle().await?;
        let window = self.window_orders(start, end).await?;
        let paid_resellers: HashSet<Id> = window
            .iter()
            .filter(|o| is_paid(&o.2))
            .filter_map(|o| o.1)
            .collect();
        let self_dealing = snapshots::Entity::find()
            .filter(snapshots::Column::ProfitEligible.eq(false))
            .filter(
                snapshots::Column::ProfitBlockReason
                    .is_in([PROFIT_BLOCK_OWNER, PROFIT_BLOCK_RELATED_ACCOUNT]),
            )
            .filter(snapshots::Column::CreatedAt.gte(start))
            .filter(snapshots::Column::CreatedAt.lt(end))
            .filter(snapshots::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
            .len() as i64;
        let orders = OrderCounters {
            orders_total: window.len() as i64,
            paid_orders: window.iter().filter(|o| is_paid(&o.2)).count() as i64,
            completed_orders: window.iter().filter(|o| o.2 == "completed").count() as i64,
            refunded_orders: window.iter().filter(|o| o.2 == "refunded").count() as i64,
            self_dealing_blocked_orders: self_dealing,
            active_resellers_with_orders: paid_resellers.len() as i64,
        };

        // Per-reseller activity of resellers with a live profile.
        let profiles_by_id: HashMap<Id, &profiles::Model> =
            profile_rows.iter().map(|p| (p.id, p)).collect();
        let mut per: BTreeMap<Id, (i64, i64, Option<DateTime<Utc>>)> = BTreeMap::new();
        for (_, reseller, status, _, _, created) in &window {
            let Some(rid) = reseller.filter(|r| profiles_by_id.contains_key(r)) else {
                continue;
            };
            let e = per.entry(rid).or_insert((0, 0, None));
            e.0 += 1;
            if is_paid(status) {
                e.1 += 1;
            }
            e.2 = Some(e.2.map_or(*created, |t| t.max(*created)));
        }
        let mut active_domains: HashMap<Id, i64> = HashMap::new();
        for d in domain_rows
            .iter()
            .filter(|d| d.status == DomainStatus::Active.as_str())
        {
            *active_domains.entry(d.reseller_id).or_default() += 1;
        }
        let mut top: Vec<(Id, (i64, i64, Option<DateTime<Utc>>))> = per.into_iter().collect();
        top.sort_by_key(|(id, (total, paid, _))| std::cmp::Reverse((*paid, *total, *id)));
        top.truncate(TOP_RESELLERS);
        let users = load_users(
            &self.db,
            &top.iter()
                .filter_map(|(rid, _)| profiles_by_id.get(rid).map(|p| p.user_id))
                .collect::<Vec<_>>(),
        )
        .await?;
        let top_resellers = top
            .into_iter()
            .filter_map(|(rid, (total, paid, last))| {
                let p = profiles_by_id.get(&rid)?;
                let user = users.get(&p.user_id);
                Some(TopReseller {
                    reseller_id: rid,
                    user_id: p.user_id,
                    email: user.map(|u| u.email.clone()).unwrap_or_default(),
                    display_name: user.map(|u| u.display_name.clone()).unwrap_or_default(),
                    orders_total: total,
                    paid_orders: paid,
                    active_domains: active_domains.get(&rid).copied().unwrap_or(0),
                    site_configured: configured.contains(&rid),
                    last_order_at: last,
                })
            })
            .collect();
        Ok(OverviewRows {
            lifecycle,
            orders,
            top_resellers,
        })
    }

    async fn finance(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(Vec<PeriodCurrency>, Vec<CurrentCurrency>)> {
        // Per-currency deltas, folded below.
        let mut deltas: Vec<(String, PeriodCurrency)> = Vec::new();
        for (_, _, status, currency, total, _) in self.window_orders(start, end).await? {
            let paid = is_paid(&status);
            deltas.push((
                currency,
                PeriodCurrency {
                    orders_total: 1,
                    paid_orders: i64::from(paid),
                    gmv_paid: if paid { total } else { Decimal::ZERO },
                    ..PeriodCurrency::default()
                },
            ));
        }
        let entries: Vec<(String, String, Decimal)> = ledger::Entity::find()
            .select_only()
            .column(ledger::Column::Currency)
            .column(ledger::Column::Type)
            .column(ledger::Column::Amount)
            .filter(ledger::Column::CreatedAt.gte(start))
            .filter(ledger::Column::CreatedAt.lt(end))
            .filter(ledger::Column::Status.ne(LedgerStatus::Canceled.as_str()))
            .filter(ledger::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        let mut refund_sums: HashMap<String, Decimal> = HashMap::new();
        for (currency, kind, amount) in entries {
            if kind == LedgerType::OrderProfit.as_str() {
                deltas.push((
                    currency,
                    PeriodCurrency {
                        profit_earned: amount,
                        ..PeriodCurrency::default()
                    },
                ));
            } else if kind == LedgerType::RefundDeduct.as_str() {
                *refund_sums.entry(currency_key(&currency)).or_default() += amount;
            }
        }
        let paid_withdraws: Vec<(String, Decimal)> = withdraws::Entity::find()
            .select_only()
            .column(withdraws::Column::Currency)
            .column(withdraws::Column::Amount)
            .filter(withdraws::Column::Status.eq(WithdrawStatus::Paid.as_str()))
            .filter(withdraws::Column::ProcessedAt.is_not_null())
            .filter(withdraws::Column::ProcessedAt.gte(start))
            .filter(withdraws::Column::ProcessedAt.lt(end))
            .filter(withdraws::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        for (currency, amount) in paid_withdraws {
            deltas.push((
                currency,
                PeriodCurrency {
                    withdraw_paid: amount,
                    ..PeriodCurrency::default()
                },
            ));
        }
        let mut period: BTreeMap<String, PeriodCurrency> = BTreeMap::new();
        for (currency, d) in deltas {
            let key = currency_key(&currency);
            let row = period.entry(key.clone()).or_insert_with(|| PeriodCurrency {
                currency: key,
                ..PeriodCurrency::default()
            });
            row.orders_total += d.orders_total;
            row.paid_orders += d.paid_orders;
            row.gmv_paid += d.gmv_paid;
            row.profit_earned += d.profit_earned;
            row.withdraw_paid += d.withdraw_paid;
        }
        for (key, sum) in refund_sums {
            let row = period.entry(key.clone()).or_insert_with(|| PeriodCurrency {
                currency: key,
                ..PeriodCurrency::default()
            });
            row.refund_deducted = sum.abs();
        }

        let mut current: BTreeMap<String, CurrentCurrency> = BTreeMap::new();
        for b in balances::Entity::find()
            .filter(balances::Column::DeletedAt.is_null())
            .all(&self.db)
            .await
            .dom()?
        {
            let key = currency_key(&b.currency);
            let row = current
                .entry(key.clone())
                .or_insert_with(|| CurrentCurrency {
                    currency: key,
                    ..CurrentCurrency::default()
                });
            row.available_balance += b.available_amount_cache;
            row.locked_balance += b.locked_amount_cache;
            row.negative_balance += b.negative_amount_cache;
            if b.status == BalanceStatus::NegativeBalance.as_str() {
                row.negative_balance_accounts += 1;
            }
            if b.status == BalanceStatus::FrozenReview.as_str() {
                row.frozen_balance_accounts += 1;
            }
        }
        let pending: Vec<(String, Decimal)> = withdraws::Entity::find()
            .select_only()
            .column(withdraws::Column::Currency)
            .column(withdraws::Column::Amount)
            .filter(withdraws::Column::Status.eq(WithdrawStatus::Pending.as_str()))
            .filter(withdraws::Column::DeletedAt.is_null())
            .into_tuple()
            .all(&self.db)
            .await
            .dom()?;
        for (currency, amount) in pending {
            let key = currency_key(&currency);
            let row = current
                .entry(key.clone())
                .or_insert_with(|| CurrentCurrency {
                    currency: key,
                    ..CurrentCurrency::default()
                });
            row.pending_withdraw_count += 1;
            row.pending_withdraw_amount += amount;
        }
        Ok((
            period.into_values().collect(),
            current.into_values().collect(),
        ))
    }
}
