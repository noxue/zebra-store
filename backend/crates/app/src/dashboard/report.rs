//! Dashboard reports with a short in-process cache (original
//! `dashboard/application/service.go`, `dashboardCacheTTL = 45s`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Duration, Utc};
use zs_domain::dashboard::inventory::{InventoryAlert, inventory_alerts, stock_stats};
use zs_domain::dashboard::report::{ReportQuery, Window, resolve};
use zs_domain::dashboard::stats::{
    DashboardRepo, OverviewInputs, OverviewResponse, RankingsResponse, TrendResponse, order_totals,
    overview, profit_total, rankings, trends,
};
use zs_domain::settings::SettingsStore;
use zs_domain::settings::keys::DASHBOARD_CONFIG;
use zs_domain::settings::schema::storefront::DashboardSetting;
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;

/// Lifetime of a cached report (original `dashboardCacheTTL`).
const CACHE_TTL_SECONDS: i64 = 45;
/// Error key of every failed dashboard read.
const FETCH_FAILED: &str = "error.dashboard_fetch_failed";

#[derive(Debug, Clone)]
enum Cached {
    Overview(OverviewResponse),
    Trends(TrendResponse),
    Rankings(RankingsResponse),
}

type Cache = Arc<Mutex<HashMap<String, (DateTime<Utc>, Cached)>>>;

/// Overview, trends, rankings and inventory alerts.
#[derive(Clone)]
pub struct ReportService {
    repo: Arc<dyn DashboardRepo>,
    settings: Arc<dyn SettingsStore>,
    clock: Arc<dyn Clock>,
    cache: Cache,
}

impl std::fmt::Debug for ReportService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReportService")
    }
}

fn fetch_failed(e: Error) -> Error {
    e.or_internal(FETCH_FAILED)
}

impl ReportService {
    pub fn new(
        repo: Arc<dyn DashboardRepo>,
        settings: Arc<dyn SettingsStore>,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            settings,
            clock,
            cache: Arc::default(),
        }
    }

    /// `dashboard_config`, normalized; unreadable settings use the defaults.
    async fn setting(&self) -> DashboardSetting {
        match self.settings.get(DASHBOARD_CONFIG).await {
            Ok(raw) => DashboardSetting::decode(raw.as_ref(), DashboardSetting::default()),
            Err(e) => {
                tracing::warn!(error = %e, "dashboard setting unreadable, using defaults");
                DashboardSetting::default()
            }
        }
    }

    fn window(&self, q: &ReportQuery) -> Result<Window> {
        resolve(q, self.clock.now())
    }

    fn cached(&self, key: &str, force: bool) -> Option<Cached> {
        if force {
            return None;
        }
        let now = self.clock.now();
        let cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache
            .get(key)
            .filter(|(expires, _)| *expires > now)
            .map(|(_, v)| v.clone())
    }

    fn store(&self, key: String, value: Cached) {
        let now = self.clock.now();
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        cache.retain(|_, (expires, _)| *expires > now);
        cache.insert(key, (now + Duration::seconds(CACHE_TTL_SECONDS), value));
    }

    fn key(kind: &str, w: &Window, extra: &str) -> String {
        format!(
            "dashboard:{kind}:{}:{}:{}:{}:{extra}",
            w.range,
            w.start.timestamp(),
            w.end.timestamp(),
            w.timezone
        )
    }

    pub async fn overview(&self, q: &ReportQuery) -> Result<OverviewResponse> {
        let w = self.window(q)?;
        let setting = self.setting().await;
        let a = &setting.alert;
        let key = Self::key(
            "overview",
            &w,
            &format!(
                "{}:{}:{}:{}:{}",
                a.low_stock_threshold,
                a.out_of_stock_products_threshold,
                a.pending_payment_orders_threshold,
                a.payments_failed_threshold,
                setting.accounting.refund_reverses_cost
            ),
        );
        if let Some(Cached::Overview(v)) = self.cached(&key, q.force_refresh) {
            return Ok(v);
        }
        let r = &self.repo;
        let orders = r.orders(w.start, w.end).await.map_err(fetch_failed)?;
        let payments = r.payments(w.start, w.end).await.map_err(fetch_failed)?;
        let items = r.items(w.start, w.end).await.map_err(fetch_failed)?;
        let refunds = r.refunds(w.start, w.end).await.map_err(fetch_failed)?;
        let mut totals = order_totals(&orders, &payments);
        if totals.currency.is_empty() {
            // MISC-03 ④: empty windows fall back to the latest order currency.
            totals.currency = r.latest_currency().await.map_err(fetch_failed)?;
        }
        let profit = profit_total(&items, &refunds, &payments);
        let inventory = r.inventory().await.map_err(fetch_failed)?;
        let stock = stock_stats(&inventory, a.low_stock_threshold);
        let inputs = OverviewInputs {
            totals: &totals,
            profit: &profit,
            stock: &stock,
            new_users: r.new_users(w.start, w.end).await.map_err(fetch_failed)?,
            active_products: r.active_products().await.map_err(fetch_failed)?,
            total_user_balance: r.total_user_balance().await.map_err(fetch_failed)?,
        };
        let response = overview(&w, &setting, &inputs);
        self.store(key, Cached::Overview(response.clone()));
        Ok(response)
    }

    pub async fn trends(&self, q: &ReportQuery) -> Result<TrendResponse> {
        let w = self.window(q)?;
        let setting = self.setting().await;
        let key = Self::key(
            "trends",
            &w,
            &setting.accounting.refund_reverses_cost.to_string(),
        );
        if let Some(Cached::Trends(v)) = self.cached(&key, q.force_refresh) {
            return Ok(v);
        }
        let r = &self.repo;
        let orders = r.orders(w.start, w.end).await.map_err(fetch_failed)?;
        let payments = r.payments(w.start, w.end).await.map_err(fetch_failed)?;
        let items = r.items(w.start, w.end).await.map_err(fetch_failed)?;
        let refunds = r.refunds(w.start, w.end).await.map_err(fetch_failed)?;
        let response = trends(&w, &setting, &orders, &payments, &items, &refunds);
        self.store(key, Cached::Trends(response.clone()));
        Ok(response)
    }

    pub async fn rankings(&self, q: &ReportQuery) -> Result<RankingsResponse> {
        let w = self.window(q)?;
        let setting = self.setting().await;
        let key = Self::key(
            "rankings",
            &w,
            &format!(
                "{}:{}",
                setting.ranking.top_products_limit, setting.ranking.top_channels_limit
            ),
        );
        if let Some(Cached::Rankings(v)) = self.cached(&key, q.force_refresh) {
            return Ok(v);
        }
        let items = self
            .repo
            .items(w.start, w.end)
            .await
            .map_err(fetch_failed)?;
        let payments = self
            .repo
            .payments(w.start, w.end)
            .await
            .map_err(fetch_failed)?;
        let response = rankings(&w, &setting, &items, &payments);
        self.store(key, Cached::Rankings(response.clone()));
        Ok(response)
    }

    /// SKU-level stock alerts with the configured low-stock threshold (not cached).
    pub async fn inventory_alerts(&self) -> Result<Vec<InventoryAlert>> {
        let setting = self.setting().await;
        let inventory = self.repo.inventory().await.map_err(fetch_failed)?;
        Ok(inventory_alerts(
            &inventory,
            setting.alert.low_stock_threshold,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rust_decimal::Decimal;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zs_domain::dashboard::inventory::InventorySnapshot;
    use zs_domain::dashboard::stats::{ItemFact, OrderFact, PaymentFact, RefundFact};
    use zs_shared::clock::FixedClock;

    #[derive(Debug, Default)]
    struct Repo {
        order_reads: AtomicUsize,
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    #[async_trait]
    impl DashboardRepo for Repo {
        async fn orders(&self, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<Vec<OrderFact>> {
            self.order_reads.fetch_add(1, Ordering::SeqCst);
            Ok(vec![OrderFact {
                id: 1,
                status: "paid".into(),
                currency: String::new(),
                total_amount: Decimal::from(10),
                created_at: at("2026-09-24T01:00:00Z"),
            }])
        }
        async fn latest_currency(&self) -> Result<String> {
            Ok("cny".into())
        }
        async fn payments(&self, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<Vec<PaymentFact>> {
            Ok(Vec::new())
        }
        async fn items(&self, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<Vec<ItemFact>> {
            Ok(Vec::new())
        }
        async fn refunds(&self, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<Vec<RefundFact>> {
            Ok(Vec::new())
        }
        async fn new_users(&self, _: DateTime<Utc>, _: DateTime<Utc>) -> Result<i64> {
            Ok(4)
        }
        async fn active_products(&self) -> Result<i64> {
            Ok(2)
        }
        async fn total_user_balance(&self) -> Result<Decimal> {
            Ok(Decimal::new(505, 1))
        }
        async fn inventory(&self) -> Result<InventorySnapshot> {
            Ok(InventorySnapshot::default())
        }
    }

    #[derive(Debug, Default)]
    struct Settings(Mutex<Option<serde_json::Value>>);

    #[async_trait]
    impl SettingsStore for Settings {
        async fn get(&self, _: &str) -> Result<Option<serde_json::Value>> {
            Ok(self.0.lock().unwrap().clone())
        }
        async fn set(&self, _: &str, value: &serde_json::Value) -> Result<()> {
            *self.0.lock().unwrap() = Some(value.clone());
            Ok(())
        }
    }

    fn service(repo: Arc<Repo>, settings: Arc<Settings>) -> ReportService {
        ReportService::new(
            repo,
            settings,
            Arc::new(FixedClock(at("2026-09-24T12:00:00Z"))),
        )
    }

    fn q(force: bool) -> ReportQuery {
        ReportQuery {
            range: "today".into(),
            timezone: "UTC".into(),
            force_refresh: force,
            ..ReportQuery::default()
        }
    }

    #[tokio::test]
    async fn overview_is_cached_unless_forced() {
        let repo = Arc::new(Repo::default());
        let svc = service(repo.clone(), Arc::default());
        let first = svc.overview(&q(false)).await.unwrap();
        assert_eq!(
            first.currency, "CNY",
            "falls back to the latest order currency"
        );
        assert_eq!(first.kpi.new_users, 4);
        assert_eq!(first.kpi.total_user_balance, "50.50");
        svc.overview(&q(false)).await.unwrap();
        assert_eq!(repo.order_reads.load(Ordering::SeqCst), 1);
        svc.overview(&q(true)).await.unwrap();
        assert_eq!(repo.order_reads.load(Ordering::SeqCst), 2);
    }

    // MISC-03 (2): the cache key includes the accounting switch.
    #[tokio::test]
    async fn settings_change_bypasses_cache() {
        let repo = Arc::new(Repo::default());
        let settings = Arc::new(Settings::default());
        let svc = service(repo.clone(), settings.clone());
        svc.trends(&q(false)).await.unwrap();
        settings
            .set(
                DASHBOARD_CONFIG,
                &serde_json::json!({"accounting": {"refund_reverses_cost": true}}),
            )
            .await
            .unwrap();
        svc.trends(&q(false)).await.unwrap();
        assert_eq!(repo.order_reads.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn invalid_range_is_bad_request() {
        let svc = service(Arc::default(), Arc::default());
        let mut query = q(false);
        query.range = "forever".into();
        let err = svc.rankings(&query).await.unwrap_err();
        assert_eq!(err.key(), "error.bad_request");
    }
}
