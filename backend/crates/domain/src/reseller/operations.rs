//! Admin operations overview / finance of resellers (`application/operations.go`)
//! and the shared reporting window (`reporting/application/resolver.go`).

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Serialize;

use super::accounting::fixed;
use crate::{Error, Id, Result};

/// Longest custom reporting range.
pub const CUSTOM_MAX_DAYS: i64 = 90;

/// Raw reporting query (`range`, `from`, `to`, `tz`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReportQuery {
    pub range: String,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub timezone: String,
}

/// Normalized half-open window `[start, end)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportWindow {
    pub range: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub timezone: String,
}

/// Resolves `today` / `7d` / `30d` / `custom` (≤ 90 days). Days are UTC days: the
/// server runs in UTC and no IANA database is bundled, so any `tz` is reported as UTC.
pub fn resolve_window(q: &ReportQuery, now: DateTime<Utc>) -> Result<ReportWindow> {
    let range = match q.range.trim().to_lowercase() {
        r if r.is_empty() => "7d".to_owned(),
        r => r,
    };
    let today = Utc.from_utc_datetime(&now.date_naive().and_time(NaiveTime::MIN));
    let (start, end) = match range.as_str() {
        "today" => (today, today + Duration::days(1)),
        "7d" => (today - Duration::days(6), today + Duration::days(1)),
        "30d" => (today - Duration::days(29), today + Duration::days(1)),
        "custom" => {
            let (Some(from), Some(to)) = (q.from, q.to) else {
                return Err(Error::invalid());
            };
            if to < from || to - from > Duration::days(CUSTOM_MAX_DAYS) {
                return Err(Error::invalid());
            }
            (from, to + Duration::seconds(1))
        }
        _ => return Err(Error::invalid()),
    };
    if end <= start {
        return Err(Error::invalid());
    }
    Ok(ReportWindow {
        range,
        start,
        end,
        timezone: "UTC".to_owned(),
    })
}

fn rfc3339(t: DateTime<Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Profile / domain / site-config counters.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Lifecycle {
    pub profiles_total: i64,
    pub profiles_pending_review: i64,
    pub profiles_active: i64,
    pub profiles_rejected: i64,
    pub profiles_disabled: i64,
    pub profiles_settlement_frozen: i64,
    pub domains_total: i64,
    pub domains_pending_review: i64,
    pub domains_active: i64,
    pub domains_disabled: i64,
    pub domains_pending_verification: i64,
    pub domains_verified: i64,
    pub custom_domains: i64,
    pub subdomains: i64,
    pub site_configs_total: i64,
    pub active_profiles_without_site_config: i64,
}

/// Order counters of the window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OrderCounters {
    pub orders_total: i64,
    pub paid_orders: i64,
    pub completed_orders: i64,
    pub refunded_orders: i64,
    pub self_dealing_blocked_orders: i64,
    pub active_resellers_with_orders: i64,
}

/// A top reseller of the window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopReseller {
    pub reseller_id: Id,
    pub user_id: Id,
    pub email: String,
    pub display_name: String,
    pub orders_total: i64,
    pub paid_orders: i64,
    pub active_domains: i64,
    pub site_configured: bool,
    pub last_order_at: Option<DateTime<Utc>>,
}

/// Aggregates loaded by the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverviewRows {
    pub lifecycle: Lifecycle,
    pub orders: OrderCounters,
    pub top_resellers: Vec<TopReseller>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrdersResponse {
    pub orders_total: i64,
    pub paid_orders: i64,
    pub completed_orders: i64,
    pub refunded_orders: i64,
    pub self_dealing_blocked_orders: i64,
    pub active_resellers_with_orders: i64,
    pub average_paid_orders_per_active_reseller: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TopResellerResponse {
    pub reseller_id: Id,
    pub user_id: Id,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub display_name: String,
    pub orders_total: i64,
    pub paid_orders: i64,
    pub active_domains: i64,
    pub site_configured: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub last_order_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alert {
    #[serde(rename = "type")]
    pub kind: &'static str,
    pub level: &'static str,
    pub value: i64,
}

/// `GET /admin/resellers/operations/overview` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OverviewResponse {
    pub range: String,
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub lifecycle: Lifecycle,
    pub orders: OrdersResponse,
    pub top_resellers: Vec<TopResellerResponse>,
    pub alerts: Vec<Alert>,
}

/// Builds the overview response (alerts only for non-zero counters).
pub fn overview_response(window: &ReportWindow, rows: OverviewRows) -> OverviewResponse {
    let o = &rows.orders;
    let average = if o.active_resellers_with_orders > 0 {
        Decimal::from(o.paid_orders) / Decimal::from(o.active_resellers_with_orders)
    } else {
        Decimal::ZERO
    };
    let mut alerts = Vec::new();
    let mut add = |kind, level, value: i64| {
        if value > 0 {
            alerts.push(Alert { kind, level, value });
        }
    };
    add(
        "profiles_pending_review",
        "warning",
        rows.lifecycle.profiles_pending_review,
    );
    add(
        "domains_pending_review",
        "warning",
        rows.lifecycle.domains_pending_review,
    );
    add(
        "active_profiles_without_site_config",
        "info",
        rows.lifecycle.active_profiles_without_site_config,
    );
    add(
        "self_dealing_blocked_orders",
        "warning",
        o.self_dealing_blocked_orders,
    );
    OverviewResponse {
        range: window.range.clone(),
        from: rfc3339(window.start),
        to: rfc3339(window.end - Duration::seconds(1)),
        timezone: window.timezone.clone(),
        orders: OrdersResponse {
            orders_total: o.orders_total,
            paid_orders: o.paid_orders,
            completed_orders: o.completed_orders,
            refunded_orders: o.refunded_orders,
            self_dealing_blocked_orders: o.self_dealing_blocked_orders,
            active_resellers_with_orders: o.active_resellers_with_orders,
            average_paid_orders_per_active_reseller: fixed(average, 2),
        },
        lifecycle: rows.lifecycle,
        top_resellers: rows
            .top_resellers
            .into_iter()
            .map(|t| TopResellerResponse {
                reseller_id: t.reseller_id,
                user_id: t.user_id,
                email: t.email.trim().to_owned(),
                display_name: t.display_name.trim().to_owned(),
                orders_total: t.orders_total,
                paid_orders: t.paid_orders,
                active_domains: t.active_domains,
                site_configured: t.site_configured,
                last_order_at: t.last_order_at.map(rfc3339).unwrap_or_default(),
            })
            .collect(),
        alerts,
    }
}

/// Per-currency activity of the window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PeriodCurrency {
    pub currency: String,
    pub orders_total: i64,
    pub paid_orders: i64,
    pub gmv_paid: Decimal,
    pub profit_earned: Decimal,
    pub refund_deducted: Decimal,
    pub withdraw_paid: Decimal,
}

/// Current per-currency balances.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CurrentCurrency {
    pub currency: String,
    pub available_balance: Decimal,
    pub locked_balance: Decimal,
    pub negative_balance: Decimal,
    pub pending_withdraw_count: i64,
    pub pending_withdraw_amount: Decimal,
    pub negative_balance_accounts: i64,
    pub frozen_balance_accounts: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeriodCurrencyResponse {
    pub currency: String,
    pub orders_total: i64,
    pub paid_orders: i64,
    pub gmv_paid: String,
    pub profit_earned: String,
    pub refund_deducted: String,
    pub withdraw_paid: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CurrentCurrencyResponse {
    pub currency: String,
    pub available_balance: String,
    pub locked_balance: String,
    pub negative_balance: String,
    pub pending_withdraw_count: i64,
    pub pending_withdraw_amount: String,
    pub negative_balance_accounts: i64,
    pub frozen_balance_accounts: i64,
}

/// `GET /admin/resellers/operations/finance` data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FinanceResponse {
    pub range: String,
    pub from: String,
    pub to: String,
    pub timezone: String,
    pub period_currency_rows: Vec<PeriodCurrencyResponse>,
    pub current_currency_rows: Vec<CurrentCurrencyResponse>,
}

/// Upper-cased currency; blank becomes `UNKNOWN`.
pub fn currency_key(raw: &str) -> String {
    let c = raw.trim().to_uppercase();
    if c.is_empty() {
        "UNKNOWN".to_owned()
    } else {
        c
    }
}

/// Builds the finance response (rows sorted by currency).
pub fn finance_response(
    window: &ReportWindow,
    mut period: Vec<PeriodCurrency>,
    mut current: Vec<CurrentCurrency>,
) -> FinanceResponse {
    period.sort_by(|a, b| a.currency.cmp(&b.currency));
    current.sort_by(|a, b| a.currency.cmp(&b.currency));
    FinanceResponse {
        range: window.range.clone(),
        from: rfc3339(window.start),
        to: rfc3339(window.end - Duration::seconds(1)),
        timezone: window.timezone.clone(),
        period_currency_rows: period
            .into_iter()
            .map(|r| PeriodCurrencyResponse {
                currency: r.currency,
                orders_total: r.orders_total,
                paid_orders: r.paid_orders,
                gmv_paid: fixed(r.gmv_paid, 2),
                profit_earned: fixed(r.profit_earned, 2),
                refund_deducted: fixed(r.refund_deducted, 2),
                withdraw_paid: fixed(r.withdraw_paid, 2),
            })
            .collect(),
        current_currency_rows: current
            .into_iter()
            .map(|r| CurrentCurrencyResponse {
                currency: r.currency,
                available_balance: fixed(r.available_balance, 2),
                locked_balance: fixed(r.locked_balance, 2),
                negative_balance: fixed(r.negative_balance, 2),
                pending_withdraw_count: r.pending_withdraw_count,
                pending_withdraw_amount: fixed(r.pending_withdraw_amount, 2),
                negative_balance_accounts: r.negative_balance_accounts,
                frozen_balance_accounts: r.frozen_balance_accounts,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 10, 15, 30, 0)
            .single()
            .unwrap_or_default()
    }

    #[test]
    fn windows() {
        let q = |range: &str| ReportQuery {
            range: range.into(),
            ..ReportQuery::default()
        };
        let w = resolve_window(&q(""), now()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(w.range, "7d");
        assert_eq!(rfc3339(w.start), "2026-07-04T00:00:00Z");
        assert_eq!(rfc3339(w.end), "2026-07-11T00:00:00Z");
        let w = resolve_window(&q("TODAY"), now()).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(rfc3339(w.start), "2026-07-10T00:00:00Z");
        assert!(resolve_window(&q("1y"), now()).is_err());
        assert!(resolve_window(&q("custom"), now()).is_err());
        let custom = ReportQuery {
            range: "custom".into(),
            from: Some(now() - Duration::days(91)),
            to: Some(now()),
            timezone: "Asia/Shanghai".into(),
        };
        assert!(resolve_window(&custom, now()).is_err());
        let ok = ReportQuery {
            from: Some(now() - Duration::days(1)),
            ..custom
        };
        assert_eq!(
            resolve_window(&ok, now()).map(|w| w.end).ok(),
            Some(now() + Duration::seconds(1))
        );
    }

    #[test]
    fn overview_alerts_and_average() {
        let w = resolve_window(&ReportQuery::default(), now()).unwrap_or_else(|e| panic!("{e}"));
        let rows = OverviewRows {
            lifecycle: Lifecycle {
                profiles_pending_review: 2,
                ..Lifecycle::default()
            },
            orders: OrderCounters {
                paid_orders: 5,
                active_resellers_with_orders: 2,
                ..OrderCounters::default()
            },
            top_resellers: vec![],
        };
        let r = overview_response(&w, rows);
        assert_eq!(r.orders.average_paid_orders_per_active_reseller, "2.50");
        assert_eq!(
            r.alerts,
            vec![Alert {
                kind: "profiles_pending_review",
                level: "warning",
                value: 2
            }]
        );
        assert_eq!(r.to, "2026-07-10T23:59:59Z");
        assert_eq!(currency_key(" cny "), "CNY");
        assert_eq!(currency_key(""), "UNKNOWN");
    }
}
