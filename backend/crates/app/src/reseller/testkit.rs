//! In-memory reseller ports for service unit tests.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use zs_domain::reseller::accounting::WithdrawDraft;
use zs_domain::reseller::ports::*;
use zs_domain::reseller::pricing::{PricedProduct, PricedSku};
use zs_domain::reseller::rules::{SystemDomainPlan, plan_system_domain};
use zs_domain::reseller::site::SiteConfigDraft;
use zs_domain::reseller::*;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::{Clock, FixedClock};
use zs_shared::money::Amount;
use zs_shared::page::{Page, PageRequest};

use crate::config::ResellerConfig;

pub fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).single().unwrap()
}

pub fn clock() -> Arc<dyn Clock> {
    Arc::new(FixedClock(now()))
}

pub fn config() -> ResellerConfig {
    ResellerConfig {
        enabled: true,
        subdomain_base: "shop.example.com".into(),
        ..ResellerConfig::default()
    }
}

#[derive(Default)]
pub struct MemStore {
    profiles: Mutex<Vec<Profile>>,
    domains: Mutex<Vec<ResellerDomain>>,
    sites: Mutex<Vec<SiteConfig>>,
    products: Mutex<Vec<PricedProduct>>,
    settings: Mutex<Vec<ProductSetting>>,
}

fn page<T: Clone>(items: &[T]) -> Page<T> {
    Page {
        items: items.to_vec(),
        total: items.len() as u64,
    }
}

impl MemStore {
    pub fn add_profile(&self, user_id: Id, status: ProfileStatus) -> Id {
        let mut rows = self.profiles.lock().unwrap();
        let id = rows.len() as Id + 1;
        rows.push(Profile {
            id,
            user_id,
            status,
            apply_reason: String::new(),
            reject_reason: String::new(),
            default_markup_percent: Amount::ZERO,
            max_markup_percent: Amount::ZERO,
            settlement_status: SettlementStatus::Normal,
            reviewed_by: None,
            reviewed_at: None,
            created_at: now(),
            updated_at: now(),
            user: None,
        });
        id
    }

    pub fn set_profile(&self, id: Id, status: ProfileStatus, settlement: SettlementStatus) {
        let mut rows = self.profiles.lock().unwrap();
        if let Some(p) = rows.iter_mut().find(|p| p.id == id) {
            p.status = status;
            p.settlement_status = settlement;
        }
    }

    pub fn add_domain(
        &self,
        reseller_id: Id,
        host: &str,
        status: DomainStatus,
        verified: bool,
    ) -> Id {
        let mut rows = self.domains.lock().unwrap();
        let id = rows.len() as Id + 1;
        rows.push(ResellerDomain {
            id,
            reseller_id,
            domain: host.into(),
            kind: DomainType::Custom,
            verification_token: String::new(),
            verification_status: if verified {
                VerificationStatus::Verified
            } else {
                VerificationStatus::Pending
            },
            status,
            is_primary: false,
            verified_at: None,
            created_at: now(),
            updated_at: now(),
            profile: None,
        });
        id
    }

    pub fn set_domain_live(&self, id: Id) {
        let mut rows = self.domains.lock().unwrap();
        if let Some(d) = rows.iter_mut().find(|d| d.id == id) {
            d.status = DomainStatus::Active;
            d.verification_status = VerificationStatus::Verified;
        }
    }

    pub fn add_product(&self, id: Id, price: &str, skus: &[(Id, &str)]) {
        let amount = |s: &str| s.parse::<Amount>().unwrap();
        self.products.lock().unwrap().push(PricedProduct {
            id,
            slug: format!("p{id}"),
            title: Value::Null,
            price: amount(price),
            cost_price: Amount::ZERO,
            is_active: true,
            skus: skus
                .iter()
                .map(|(sid, p)| PricedSku {
                    id: *sid,
                    sku_code: String::new(),
                    spec_values: Value::Null,
                    price: amount(p),
                    cost_price: Amount::ZERO,
                    is_active: true,
                })
                .collect(),
        });
    }

    fn hosts_of(&self, reseller_id: Id) -> Vec<String> {
        self.domains
            .lock()
            .unwrap()
            .iter()
            .filter(|d| d.reseller_id == reseller_id)
            .map(|d| d.domain.clone())
            .collect()
    }
}

#[async_trait]
impl ProfileRepo for MemStore {
    async fn profile_by_id(&self, id: Id) -> Result<Option<Profile>> {
        Ok(self
            .profiles
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == id)
            .cloned())
    }
    async fn profile_by_user(&self, user_id: Id) -> Result<Option<Profile>> {
        Ok(self
            .profiles
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.user_id == user_id)
            .cloned())
    }
    async fn create_profile(
        &self,
        user_id: Id,
        apply_reason: &str,
        _now: DateTime<Utc>,
    ) -> Result<Profile> {
        let id = self.add_profile(user_id, ProfileStatus::PendingReview);
        let mut rows = self.profiles.lock().unwrap();
        let p = rows.iter_mut().find(|p| p.id == id).unwrap();
        apply_reason.clone_into(&mut p.apply_reason);
        Ok(p.clone())
    }
    async fn update_profile(
        &self,
        id: Id,
        plan: &ProfilePlan<'_>,
        _now: DateTime<Utc>,
    ) -> Result<Option<(Profile, Vec<String>)>> {
        let Some(current) = self.profile_by_id(id).await? else {
            return Ok(None);
        };
        let next = plan(current)?;
        let mut rows = self.profiles.lock().unwrap();
        if let Some(p) = rows.iter_mut().find(|p| p.id == id) {
            *p = next.clone();
        }
        drop(rows);
        Ok(Some((next, self.hosts_of(id))))
    }
    async fn list_profiles(
        &self,
        _filter: &ProfileFilter,
        _page: PageRequest,
    ) -> Result<Page<Profile>> {
        Ok(page(&self.profiles.lock().unwrap()))
    }
    async fn domains_of(&self, reseller_id: Id) -> Result<Vec<ResellerDomain>> {
        Ok(self
            .domains
            .lock()
            .unwrap()
            .iter()
            .filter(|d| d.reseller_id == reseller_id)
            .cloned()
            .collect())
    }
    async fn domain_by_id(&self, id: Id) -> Result<Option<ResellerDomain>> {
        Ok(self
            .domains
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.id == id)
            .cloned())
    }
    async fn live_domain(&self, host: &str) -> Result<Option<ResellerDomain>> {
        let found = self
            .domains
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.domain == host && d.is_live())
            .cloned();
        let Some(mut d) = found else { return Ok(None) };
        match self.profile_by_id(d.reseller_id).await? {
            Some(p) if p.is_active() => {
                d.profile = Some(Box::new(p));
                Ok(Some(d))
            }
            _ => Ok(None),
        }
    }
    async fn add_custom_domain(
        &self,
        reseller_id: Id,
        domain: &str,
        _now: DateTime<Utc>,
    ) -> Result<ResellerDomain> {
        if self
            .domains
            .lock()
            .unwrap()
            .iter()
            .any(|d| d.domain == domain)
        {
            return Err(Error::bad_request(keys::DOMAIN_CONFLICT));
        }
        let id = self.add_domain(reseller_id, domain, DomainStatus::PendingReview, false);
        Ok(self.domain_by_id(id).await?.unwrap())
    }
    async fn update_domains(
        &self,
        domain_id: Id,
        plan: &DomainPlan<'_>,
        _now: DateTime<Utc>,
    ) -> Result<Option<(ResellerDomain, Vec<String>)>> {
        let Some(target) = self.domain_by_id(domain_id).await? else {
            return Ok(None);
        };
        let all = self.domains_of(target.reseller_id).await?;
        let changes = plan(&target, &all)?;
        let mut rows = self.domains.lock().unwrap();
        for c in changes {
            if let Some(d) = rows.iter_mut().find(|d| d.id == c.id) {
                *d = c;
            }
        }
        let target = rows.iter().find(|d| d.id == domain_id).cloned().unwrap();
        drop(rows);
        Ok(Some((target.clone(), self.hosts_of(target.reseller_id))))
    }
    async fn assign_system_domain(
        &self,
        reseller_id: Id,
        domain: &str,
        now: DateTime<Utc>,
    ) -> Result<Option<(ResellerDomain, Vec<String>)>> {
        if self.profile_by_id(reseller_id).await?.is_none() {
            return Ok(None);
        }
        let existing = self
            .domains
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.domain == domain)
            .cloned();
        let all = self.domains_of(reseller_id).await?;
        let row = match plan_system_domain(reseller_id, domain, existing.as_ref(), &all, now)? {
            SystemDomainPlan::Insert { domain, is_primary } => {
                let id = self.add_domain(reseller_id, &domain, DomainStatus::Active, true);
                let mut rows = self.domains.lock().unwrap();
                let d = rows.iter_mut().find(|d| d.id == id).unwrap();
                d.kind = DomainType::Subdomain;
                d.is_primary = is_primary;
                d.clone()
            }
            SystemDomainPlan::Update(row) => {
                let mut rows = self.domains.lock().unwrap();
                if let Some(d) = rows.iter_mut().find(|d| d.id == row.id) {
                    *d = row.clone();
                }
                row
            }
        };
        Ok(Some((row, self.hosts_of(reseller_id))))
    }
    async fn list_domains(
        &self,
        _filter: &DomainFilter,
        _page: PageRequest,
    ) -> Result<Page<ResellerDomain>> {
        Ok(page(&self.domains.lock().unwrap()))
    }
}

#[async_trait]
impl SiteConfigRepo for MemStore {
    async fn site_config(&self, reseller_id: Id) -> Result<Option<SiteConfig>> {
        Ok(self
            .sites
            .lock()
            .unwrap()
            .iter()
            .find(|s| s.reseller_id == reseller_id)
            .cloned())
    }
    async fn upsert_site_config(
        &self,
        reseller_id: Id,
        draft: &SiteConfigDraft,
        now: DateTime<Utc>,
    ) -> Result<SiteConfig> {
        let mut rows = self.sites.lock().unwrap();
        rows.retain(|s| s.reseller_id != reseller_id);
        let row = SiteConfig {
            id: reseller_id,
            reseller_id,
            site_name: draft.site_name.clone(),
            logo: draft.logo.clone(),
            favicon: draft.favicon.clone(),
            announcement: draft.announcement.clone(),
            support: draft.support.clone(),
            seo: draft.seo.clone(),
            footer_links: draft.footer_links.clone(),
            nav_config: draft.nav_config.clone(),
            theme: draft.theme.clone(),
            created_at: now,
            updated_at: now,
            profile: None,
        };
        rows.push(row.clone());
        Ok(row)
    }
    async fn delete_site_config(&self, reseller_id: Id, _now: DateTime<Utc>) -> Result<()> {
        self.sites
            .lock()
            .unwrap()
            .retain(|s| s.reseller_id != reseller_id);
        Ok(())
    }
    async fn list_site_configs(
        &self,
        _filter: &SiteConfigFilter,
        _page: PageRequest,
    ) -> Result<Page<SiteConfig>> {
        Ok(page(&self.sites.lock().unwrap()))
    }
}

#[async_trait]
impl ProductSettingRepo for MemStore {
    async fn list_products(
        &self,
        _filter: &ProductListFilter,
        _page: PageRequest,
    ) -> Result<Page<ProductWithSettings>> {
        Ok(Page {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn product_with_settings(
        &self,
        reseller_id: Id,
        product_id: Id,
    ) -> Result<Option<ProductWithSettings>> {
        let product = self
            .products
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == product_id)
            .cloned();
        let settings = self
            .settings
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.reseller_id == reseller_id && s.product_id == product_id)
            .cloned()
            .collect();
        Ok(product.map(|p| (p, settings)))
    }
    async fn product_for_save(&self, product_id: Id) -> Result<Option<PricedProduct>> {
        Ok(self
            .products
            .lock()
            .unwrap()
            .iter()
            .find(|p| p.id == product_id)
            .cloned())
    }
    async fn save_settings(
        &self,
        reseller_id: Id,
        product_id: Id,
        rows: &[ProductSetting],
        _now: DateTime<Utc>,
    ) -> Result<()> {
        let mut all = self.settings.lock().unwrap();
        for row in rows {
            all.retain(|s| {
                !(s.reseller_id == reseller_id
                    && s.product_id == product_id
                    && s.sku_id == row.sku_id)
            });
            all.push(row.clone());
        }
        Ok(())
    }
    async fn delete_setting(
        &self,
        reseller_id: Id,
        product_id: Id,
        sku_id: Id,
        _now: DateTime<Utc>,
    ) -> Result<()> {
        self.settings.lock().unwrap().retain(|s| {
            !(s.reseller_id == reseller_id && s.product_id == product_id && s.sku_id == sku_id)
        });
        Ok(())
    }
    async fn list_settings(
        &self,
        _filter: &SettingFilter,
        _page: PageRequest,
    ) -> Result<Page<ProductSetting>> {
        Ok(page(&self.settings.lock().unwrap()))
    }
    async fn summarize(&self, _reseller_id: Id) -> Result<SettingSummary> {
        Ok(SettingSummary::default())
    }
}

#[async_trait]
impl LedgerRepo for MemStore {
    async fn post_order_profit(
        &self,
        _event: &OrderPaid,
        _now: DateTime<Utc>,
        _days: i64,
    ) -> Result<bool> {
        Ok(false)
    }
    async fn deduct_refund(
        &self,
        _event: &OrderRefunded,
        _now: DateTime<Utc>,
        _days: i64,
    ) -> Result<bool> {
        Ok(false)
    }
    async fn confirm_due(&self, _now: DateTime<Utc>) -> Result<u64> {
        Ok(0)
    }
    async fn apply_withdraw(
        &self,
        _reseller_id: Id,
        _draft: &WithdrawDraft,
        _now: DateTime<Utc>,
    ) -> Result<WithdrawRequest> {
        Err(Error::bad_request(keys::WITHDRAW_INSUFFICIENT))
    }
    async fn review_withdraw(
        &self,
        _id: Id,
        _admin_id: Id,
        _action: WithdrawAction,
        _reason: &str,
        _now: DateTime<Utc>,
    ) -> Result<Option<WithdrawRequest>> {
        Ok(None)
    }
    async fn list_balances(
        &self,
        _filter: &FinanceFilter,
        _page: PageRequest,
    ) -> Result<Page<BalanceAccount>> {
        Ok(Page {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn list_ledger(
        &self,
        _filter: &FinanceFilter,
        _page: PageRequest,
    ) -> Result<Page<LedgerEntry>> {
        Ok(Page {
            items: Vec::new(),
            total: 0,
        })
    }
    async fn list_withdraws(
        &self,
        _filter: &FinanceFilter,
        _page: PageRequest,
    ) -> Result<Page<WithdrawRequest>> {
        Ok(Page {
            items: Vec::new(),
            total: 0,
        })
    }
}
