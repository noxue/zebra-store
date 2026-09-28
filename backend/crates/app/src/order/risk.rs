//! Order risk control before the create transaction (`CheckOrderAllowed`, RISK-01).

use std::sync::{Arc, Mutex};

use zs_domain::Result;
use zs_domain::order::risk::{RiskError, RiskInput, RiskPrep, normalize_risk_ip, precheck};
use zs_domain::settings::schema::risk::{OrderRateLimit, OrderRiskSetting};
use zs_shared::clock::Clock;

use super::OrderService;
use crate::identity::rate_limit::{RateLimiter, RateRule};

/// In-process order rate limiters, one per configured rule (the original uses a Redis
/// fixed window; the local fallback keeps the same semantics).
#[derive(Debug, Clone)]
pub struct RiskLimiters {
    clock: Arc<dyn Clock>,
    limiters: Arc<Mutex<Vec<(RateRule, RateLimiter)>>>,
}

impl RiskLimiters {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            clock,
            limiters: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Counts one order for `key`; `Err(wait_seconds)` when limited.
    pub fn hit(&self, key: &str, rule: OrderRateLimit) -> std::result::Result<(), i64> {
        let rule = RateRule {
            window_seconds: rule.window_seconds,
            max_requests: rule.max_requests,
            block_seconds: rule.block_seconds,
        };
        let limiter = {
            let mut all = self
                .limiters
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match all.iter().find(|(r, _)| *r == rule) {
                Some((_, l)) => l.clone(),
                None => {
                    let l = RateLimiter::new(rule, self.clock.clone());
                    all.push((rule, l.clone()));
                    l
                }
            }
        };
        limiter.hit(key)
    }
}

impl OrderService {
    /// Pre-transaction risk checks. Settings read failures fail closed when the rate limit
    /// is consumed (order creation) and open otherwise (previews), like the original.
    pub(crate) async fn check_risk(&self, input: &RiskInput, consume: bool) -> Result<RiskPrep> {
        let cfg = match self.risk_setting().await {
            Ok(cfg) => cfg,
            Err(error) => {
                tracing::warn!(%error, "risk_control_get_config_error");
                if consume {
                    return Err(error);
                }
                return Ok(RiskPrep {
                    risk_ip: normalize_risk_ip(&input.client_ip),
                    payment_expire_minutes: 0,
                    config: OrderRiskSetting::default(),
                    rate_limit: None,
                });
            }
        };
        let prep = precheck(&cfg, input)?;
        if consume
            && let Some((key, rule)) = &prep.rate_limit
            && let Err(wait) = self.risk_limits.hit(key, *rule)
        {
            return Err(RiskError::RateLimited(wait).into());
        }
        Ok(prep)
    }
}
