//! Order risk control (`modules/orderrisk`, RISK-01): IP blacklist, per-order quantity
//! limits, order rate limits, and pending-order quotas serialized with lock keys.
//!
//! [`precheck`] runs before the order transaction (reads settings, consumes the rate
//! limit); [`pending_lock_keys`] + [`check_pending`] run inside it, after the lock rows
//! are taken and with counts read on the same transaction handle (DB-01).

use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, Ipv6Addr};

use super::model::keys;
use crate::settings::schema::risk::{OrderRateLimit, OrderRiskSetting};
use crate::{Error, Id};

/// Normalized risk IP: IPv4 as is (also IPv4-mapped IPv6), IPv6 grouped by `/64`.
pub fn normalize_risk_ip(raw: &str) -> String {
    let Ok(ip) = raw.trim().parse::<IpAddr>() else {
        return String::new();
    };
    match ip {
        IpAddr::V4(v4) => v4.to_string(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return v4.to_string();
            }
            let prefix = u128::from(v6) & (u128::MAX << 64);
            format!("{}/64", Ipv6Addr::from(prefix))
        }
    }
}

fn cidr_contains(cidr: &str, ip: IpAddr) -> bool {
    let Some((net, prefix)) = cidr.split_once('/') else {
        return false;
    };
    let (Ok(net), Ok(prefix)) = (net.trim().parse::<IpAddr>(), prefix.trim().parse::<u32>()) else {
        return false;
    };
    match (net, ip) {
        (IpAddr::V4(n), IpAddr::V4(i)) if prefix <= 32 => {
            let mask = u32::MAX.checked_shl(32 - prefix).unwrap_or(0);
            u32::from(n) & mask == u32::from(i) & mask
        }
        (IpAddr::V6(n), IpAddr::V6(i)) if prefix <= 128 => {
            let mask = u128::MAX.checked_shl(128 - prefix).unwrap_or(0);
            u128::from(n) & mask == u128::from(i) & mask
        }
        _ => false,
    }
}

/// Whether `client_ip` matches an exact IP or a CIDR of the blacklist.
pub fn ip_blacklisted(client_ip: &str, blacklist: &[String]) -> bool {
    let Ok(ip) = client_ip.trim().parse::<IpAddr>() else {
        return false;
    };
    let ip = match ip {
        IpAddr::V6(v6) => v6.to_ipv4_mapped().map_or(IpAddr::V6(v6), IpAddr::V4),
        v4 => v4,
    };
    blacklist.iter().any(|entry| {
        let entry = entry.trim();
        if entry.contains('/') {
            cidr_contains(entry, ip)
        } else {
            entry.parse::<IpAddr>().is_ok_and(|e| e == ip)
        }
    })
}

/// A requested product quantity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RiskItem {
    pub product_id: Id,
    pub quantity: i32,
}

/// Who is ordering and from where (`CheckInput`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RiskInput {
    pub user_id: Id,
    pub client_ip: String,
    pub is_guest: bool,
    /// Channel/Bot orders: skip every IP dimension, user limits still apply.
    pub skip_ip: bool,
    pub items: Vec<RiskItem>,
}

/// Outcome of [`precheck`], reused inside the order transaction (`CheckResult`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskPrep {
    pub risk_ip: String,
    /// Guest payment window override (minutes, `0` = default).
    pub payment_expire_minutes: i64,
    pub config: OrderRiskSetting,
    /// Rate limit to consume: limiter key and rule.
    pub rate_limit: Option<(String, OrderRateLimit)>,
}

/// A risk rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskError {
    IpBlacklisted,
    ClientIpUnavailable,
    TooManyPendingOrders,
    ProductQuantityLimit,
    PendingProductQuantityLimit,
    /// Retry after `n` seconds.
    RateLimited(i64),
}

impl From<RiskError> for Error {
    fn from(e: RiskError) -> Self {
        match e {
            RiskError::IpBlacklisted => Error::forbidden(keys::RISK_IP_BLACKLISTED),
            RiskError::ClientIpUnavailable => Error::forbidden(keys::RISK_CLIENT_IP_UNAVAILABLE),
            RiskError::TooManyPendingOrders => Error::too_many(keys::RISK_TOO_MANY_PENDING),
            RiskError::ProductQuantityLimit => {
                Error::bad_request(keys::RISK_PRODUCT_QUANTITY_LIMIT)
            }
            RiskError::PendingProductQuantityLimit => {
                Error::too_many(keys::RISK_PENDING_PRODUCT_LIMIT)
            }
            RiskError::RateLimited(secs) => {
                Error::too_many(keys::RISK_ORDER_RATE_LIMITED).arg(secs)
            }
        }
    }
}

/// Sums quantities per product (non-positive entries ignored).
pub fn aggregate_quantities(items: &[RiskItem]) -> BTreeMap<Id, i64> {
    let mut out = BTreeMap::new();
    for item in items.iter().filter(|i| i.product_id > 0 && i.quantity > 0) {
        *out.entry(item.product_id).or_insert(0) += i64::from(item.quantity);
    }
    out
}

fn check_quantity(items: &[RiskItem], max: i64) -> Result<(), RiskError> {
    if max <= 0 {
        return Ok(());
    }
    if aggregate_quantities(items).values().any(|q| *q > max) {
        return Err(RiskError::ProductQuantityLimit);
    }
    Ok(())
}

fn guest_requires_ip(cfg: &OrderRiskSetting) -> bool {
    let g = &cfg.guest;
    g.max_pending_orders_per_ip > 0
        || g.max_pending_quantity_per_ip_product > 0
        || g.rate_limit.enabled
}

/// Pre-transaction checks (`CheckOrderAllowed`). `cfg` is the decoded setting.
pub fn precheck(cfg: &OrderRiskSetting, input: &RiskInput) -> Result<RiskPrep, RiskError> {
    let mut prep = RiskPrep {
        risk_ip: normalize_risk_ip(&input.client_ip),
        payment_expire_minutes: 0,
        config: cfg.clone(),
        rate_limit: None,
    };
    if !cfg.enabled {
        return Ok(prep);
    }
    if !input.skip_ip
        && !input.client_ip.trim().is_empty()
        && !cfg.common.ip_blacklist.is_empty()
        && ip_blacklisted(&input.client_ip, &cfg.common.ip_blacklist)
    {
        return Err(RiskError::IpBlacklisted);
    }
    if input.is_guest {
        let policy = &cfg.guest;
        if !policy.enabled {
            return Ok(prep);
        }
        prep.payment_expire_minutes = policy.payment_expire_minutes.max(0);
        if !input.skip_ip && guest_requires_ip(cfg) && prep.risk_ip.is_empty() {
            return Err(RiskError::ClientIpUnavailable);
        }
        check_quantity(&input.items, policy.max_quantity_per_product_per_order)?;
        if policy.rate_limit.enabled && !input.skip_ip && !prep.risk_ip.is_empty() {
            prep.rate_limit = Some((
                format!("risk:order_rate:guest_ip:{}", prep.risk_ip),
                policy.rate_limit,
            ));
        }
    } else {
        let policy = &cfg.member;
        if !policy.enabled {
            return Ok(prep);
        }
        if !input.skip_ip && policy.max_pending_orders_per_ip > 0 && prep.risk_ip.is_empty() {
            return Err(RiskError::ClientIpUnavailable);
        }
        check_quantity(&input.items, policy.max_quantity_per_product_per_order)?;
        if policy.rate_limit.enabled && input.user_id > 0 {
            prep.rate_limit = Some((
                format!("risk:order_rate:user:{}", input.user_id),
                policy.rate_limit,
            ));
        }
    }
    Ok(prep)
}

/// Lock keys (plain text; hashed by the store) serializing the pending-order quota checks.
pub fn pending_lock_keys(prep: &RiskPrep, input: &RiskInput) -> Vec<String> {
    let cfg = &prep.config;
    if !cfg.enabled {
        return Vec::new();
    }
    if input.is_guest {
        if !cfg.guest.enabled || input.skip_ip || !guest_requires_ip(cfg) || prep.risk_ip.is_empty()
        {
            return Vec::new();
        }
        return vec![format!("guest:ip:{}", prep.risk_ip)];
    }
    let policy = &cfg.member;
    if !policy.enabled {
        return Vec::new();
    }
    let mut keys = Vec::new();
    if input.user_id > 0 && policy.max_pending_orders_per_user > 0 {
        keys.push(format!("member:user:{}", input.user_id));
    }
    if !input.skip_ip && !prep.risk_ip.is_empty() && policy.max_pending_orders_per_ip > 0 {
        keys.push(format!("member:ip:{}", prep.risk_ip));
    }
    keys
}

/// Pending-order counts read inside the order transaction (`PendingOrderGate`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingCounts {
    /// Pending parent orders of the user.
    pub by_user: i64,
    /// Pending guest parent orders of the risk IP (guests) or member ones (members).
    pub by_ip: i64,
    /// Pending guest quantity per product of the risk IP.
    pub guest_quantity: HashMap<Id, i64>,
}

/// What [`check_pending`] needs counted.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PendingQuery {
    pub user_id: Option<Id>,
    pub guest_ip: Option<String>,
    pub member_ip: Option<String>,
    /// Product ids whose pending guest quantity is needed.
    pub guest_quantity_products: Vec<Id>,
}

/// Which counts the transaction must read for this order.
pub fn pending_query(prep: &RiskPrep, input: &RiskInput) -> PendingQuery {
    let cfg = &prep.config;
    let mut q = PendingQuery::default();
    if !cfg.enabled {
        return q;
    }
    if input.is_guest {
        let g = &cfg.guest;
        if !g.enabled || input.skip_ip || !guest_requires_ip(cfg) || prep.risk_ip.is_empty() {
            return q;
        }
        if g.max_pending_orders_per_ip > 0 {
            q.guest_ip = Some(prep.risk_ip.clone());
        }
        if g.max_pending_quantity_per_ip_product > 0 {
            q.guest_ip = Some(prep.risk_ip.clone());
            q.guest_quantity_products =
                aggregate_quantities(&input.items).keys().copied().collect();
        }
        return q;
    }
    let m = &cfg.member;
    if !m.enabled {
        return q;
    }
    if input.user_id > 0 && m.max_pending_orders_per_user > 0 {
        q.user_id = Some(input.user_id);
    }
    if !input.skip_ip && !prep.risk_ip.is_empty() && m.max_pending_orders_per_ip > 0 {
        q.member_ip = Some(prep.risk_ip.clone());
    }
    q
}

/// In-transaction quota check (`CheckPendingOrderAllowed`).
pub fn check_pending(
    prep: &RiskPrep,
    input: &RiskInput,
    counts: &PendingCounts,
) -> Result<(), RiskError> {
    let cfg = &prep.config;
    if !cfg.enabled {
        return Ok(());
    }
    if input.is_guest {
        let g = &cfg.guest;
        if !g.enabled || input.skip_ip || !guest_requires_ip(cfg) {
            return Ok(());
        }
        if prep.risk_ip.is_empty() {
            return Err(RiskError::ClientIpUnavailable);
        }
        if g.max_pending_orders_per_ip > 0 && counts.by_ip >= g.max_pending_orders_per_ip {
            return Err(RiskError::TooManyPendingOrders);
        }
        if g.max_pending_quantity_per_ip_product > 0 {
            for (product, qty) in aggregate_quantities(&input.items) {
                let pending = counts.guest_quantity.get(&product).copied().unwrap_or(0);
                if pending + qty > g.max_pending_quantity_per_ip_product {
                    return Err(RiskError::PendingProductQuantityLimit);
                }
            }
        }
        return Ok(());
    }
    let m = &cfg.member;
    if !m.enabled {
        return Ok(());
    }
    if input.user_id > 0
        && m.max_pending_orders_per_user > 0
        && counts.by_user >= m.max_pending_orders_per_user
    {
        return Err(RiskError::TooManyPendingOrders);
    }
    if !input.skip_ip
        && !prep.risk_ip.is_empty()
        && m.max_pending_orders_per_ip > 0
        && counts.by_ip >= m.max_pending_orders_per_ip
    {
        return Err(RiskError::TooManyPendingOrders);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OrderRiskSetting {
        OrderRiskSetting {
            enabled: true,
            ..OrderRiskSetting::default()
        }
    }

    fn guest(ip: &str, items: &[(Id, i32)]) -> RiskInput {
        RiskInput {
            user_id: 0,
            client_ip: ip.into(),
            is_guest: true,
            skip_ip: false,
            items: items
                .iter()
                .map(|(p, q)| RiskItem {
                    product_id: *p,
                    quantity: *q,
                })
                .collect(),
        }
    }

    #[test]
    fn risk_01_normalizes_ips() {
        assert_eq!(normalize_risk_ip("1.2.3.4"), "1.2.3.4");
        assert_eq!(normalize_risk_ip("::ffff:1.2.3.4"), "1.2.3.4");
        assert_eq!(
            normalize_risk_ip("2001:db8:1234:5678::1"),
            "2001:db8:1234:5678::/64"
        );
        assert_eq!(
            normalize_risk_ip("2001:db8:1234:5678::abcd"),
            normalize_risk_ip("2001:db8:1234:5678::1")
        );
        assert_eq!(normalize_risk_ip("not-an-ip"), "");
        assert_eq!(normalize_risk_ip(""), "");
    }

    /// RISK-01 (2): CIDR blacklist.
    #[test]
    fn risk_01_blacklist() {
        let list = vec!["10.0.0.0/8".to_owned(), "8.8.8.8".to_owned()];
        assert!(ip_blacklisted("10.1.2.3", &list));
        assert!(ip_blacklisted("8.8.8.8", &list));
        assert!(!ip_blacklisted("11.0.0.1", &list));
        let mut c = cfg();
        c.common.ip_blacklist = list;
        assert_eq!(
            precheck(&c, &guest("10.1.2.3", &[(1, 1)])).err(),
            Some(RiskError::IpBlacklisted)
        );
        // Channel orders skip IP checks.
        let mut skip = guest("10.1.2.3", &[(1, 1)]);
        skip.skip_ip = true;
        assert!(precheck(&c, &skip).is_ok());
    }

    #[test]
    fn disabled_config_passes() {
        let c = OrderRiskSetting::default();
        let p = precheck(&c, &guest("", &[(1, 99)])).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(pending_lock_keys(&p, &guest("", &[(1, 99)])).is_empty());
    }

    #[test]
    fn guest_policy() {
        let c = cfg();
        // guest ip required
        assert_eq!(
            precheck(&c, &guest("", &[(1, 1)])).err(),
            Some(RiskError::ClientIpUnavailable)
        );
        // max 1 per product per order by default
        assert_eq!(
            precheck(&c, &guest("1.1.1.1", &[(1, 1), (1, 1)])).err(),
            Some(RiskError::ProductQuantityLimit)
        );
        let input = guest("2001:db8::1", &[(1, 1)]);
        let p = precheck(&c, &input).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(p.payment_expire_minutes, 10);
        assert_eq!(
            pending_lock_keys(&p, &input),
            vec!["guest:ip:2001:db8::/64"]
        );
        assert!(p.rate_limit.is_some());
        let q = pending_query(&p, &input);
        assert_eq!(q.guest_ip.as_deref(), Some("2001:db8::/64"));
        assert_eq!(q.guest_quantity_products, vec![1]);
        // two pending orders already → rejected
        let full = PendingCounts {
            by_ip: 2,
            ..PendingCounts::default()
        };
        assert_eq!(
            check_pending(&p, &input, &full),
            Err(RiskError::TooManyPendingOrders)
        );
        // pending quantity 2 + 1 > 2
        let qty = PendingCounts {
            by_ip: 1,
            guest_quantity: HashMap::from([(1, 2)]),
            ..PendingCounts::default()
        };
        assert_eq!(
            check_pending(&p, &input, &qty),
            Err(RiskError::PendingProductQuantityLimit)
        );
        assert_eq!(check_pending(&p, &input, &PendingCounts::default()), Ok(()));
    }

    #[test]
    fn member_policy() {
        let mut c = cfg();
        c.member.max_pending_orders_per_user = 3;
        let input = RiskInput {
            user_id: 9,
            client_ip: "1.2.3.4".into(),
            ..RiskInput::default()
        };
        let p = precheck(&c, &input).unwrap_or_else(|e| panic!("{e:?}"));
        assert_eq!(pending_lock_keys(&p, &input), vec!["member:user:9"]);
        let counts = PendingCounts {
            by_user: 3,
            ..PendingCounts::default()
        };
        assert_eq!(
            check_pending(&p, &input, &counts),
            Err(RiskError::TooManyPendingOrders)
        );
        assert_eq!(
            Error::from(RiskError::RateLimited(120)).args(),
            &["120".to_owned()]
        );
    }
}
