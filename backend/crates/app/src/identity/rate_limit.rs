//! In-process fixed-window rate limiter with block extension (original
//! `RateLimitMiddleware` local fallback; Redis is optional here).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use zs_shared::clock::Clock;

/// Largest number of tracked keys (original `localRateLimitMaxEntries`).
const MAX_ENTRIES: usize = 10_000;

/// A limit: `max_requests` per `window_seconds`; the request that first exceeds
/// the limit extends the lock to `block_seconds`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateRule {
    pub window_seconds: i64,
    pub max_requests: i64,
    pub block_seconds: i64,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    count: i64,
    expires_at: DateTime<Utc>,
}

/// Thread-safe limiter; cheap to clone.
#[derive(Debug, Clone)]
pub struct RateLimiter {
    rule: RateRule,
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    clock: Arc<dyn Clock>,
    capacity: usize,
}

impl RateLimiter {
    pub fn new(rule: RateRule, clock: Arc<dyn Clock>) -> Self {
        Self::with_capacity(rule, clock, MAX_ENTRIES)
    }

    pub fn with_capacity(rule: RateRule, clock: Arc<dyn Clock>, capacity: usize) -> Self {
        Self {
            rule,
            entries: Arc::new(Mutex::new(HashMap::new())),
            clock,
            capacity: capacity.max(1),
        }
    }

    pub fn rule(&self) -> RateRule {
        self.rule
    }

    /// Counts one request for `key`. Returns `Err(wait_seconds)` when limited.
    pub fn hit(&self, key: &str) -> Result<(), i64> {
        self.count(key, self.rule.max_requests.saturating_add(1))
    }

    /// Failure-only limits (admin login, live QA I-3): `Err(wait_seconds)` when
    /// `key` already used up its allowance; the attempt itself is not counted.
    pub fn blocked(&self, key: &str) -> Result<(), i64> {
        let rule = self.rule;
        if rule.window_seconds <= 0 || rule.max_requests <= 0 {
            return Ok(());
        }
        let now = self.clock.now();
        let map = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match map.get(key) {
            Some(e) if now < e.expires_at && e.count >= rule.max_requests => {
                let ttl = (e.expires_at - now).num_seconds();
                Err(ttl.max(1))
            }
            _ => Ok(()),
        }
    }

    /// Counts a failed attempt of a failure-only limit; the `max_requests`-th
    /// failure starts the block.
    pub fn record_failure(&self, key: &str) {
        // A full limiter already rejects new keys in `blocked`-less paths; losing
        // one failure count here is harmless.
        let _ = self.count(key, self.rule.max_requests);
    }

    /// Forgets `key` (a successful login resets its failure counter).
    pub fn reset(&self, key: &str) {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(key);
    }

    /// Increments `key`; the `block_at`-th count extends the lock to `block_seconds`.
    fn count(&self, key: &str, block_at: i64) -> Result<(), i64> {
        let rule = self.rule;
        if rule.window_seconds <= 0 || rule.max_requests <= 0 {
            return Ok(());
        }
        let now = self.clock.now();
        let mut map = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let live = map.get(key).is_some_and(|e| now < e.expires_at);
        if !live {
            map.remove(key);
            if map.len() >= self.capacity {
                map.retain(|_, e| now < e.expires_at);
            }
            if map.len() >= self.capacity {
                // Never evict live counters: reject new keys instead, so an attacker
                // rotating identifiers cannot reset existing limits.
                tracing::warn!(capacity = self.capacity, "rate limiter capacity exhausted");
                return Err(rule.window_seconds.max(1));
            }
            map.insert(
                key.to_owned(),
                Entry {
                    count: 0,
                    expires_at: now + Duration::seconds(rule.window_seconds),
                },
            );
        }
        let Some(entry) = map.get_mut(key) else {
            return Ok(());
        };
        entry.count += 1;
        if entry.count == block_at && rule.block_seconds > 0 {
            entry.expires_at = now + Duration::seconds(rule.block_seconds);
        }
        if entry.count > rule.max_requests {
            let ttl = (entry.expires_at - now).num_seconds();
            let wait = if ttl >= 1 { ttl } else { rule.window_seconds };
            return Err(wait.max(1));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Manual(Mutex<DateTime<Utc>>);

    impl Clock for Manual {
        fn now(&self) -> DateTime<Utc> {
            *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
        }
    }

    impl Manual {
        fn advance(&self, secs: i64) {
            *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += Duration::seconds(secs);
        }
    }

    // RISK-07: max=5, window=60, block=600 → 6th rejected with TTL≈600, 7th still rejected.
    #[test]
    fn blocks_after_limit() {
        let clock = Arc::new(Manual(Mutex::new(Utc::now())));
        let rule = RateRule {
            window_seconds: 60,
            max_requests: 5,
            block_seconds: 600,
        };
        let l = RateLimiter::new(rule, clock.clone());
        for _ in 0..5 {
            assert!(l.hit("k").is_ok());
        }
        assert_eq!(l.hit("k"), Err(600));
        clock.advance(100);
        assert_eq!(l.hit("k"), Err(500));
        assert!(l.hit("other").is_ok());
        clock.advance(501);
        assert!(l.hit("k").is_ok());
    }

    // QA-A03 (live QA I-3): failure-only counting — successes never count, the
    // 5th failure blocks for `block_seconds`, and a reset clears the counter.
    #[test]
    fn qa_a03_failure_only_limit() {
        let clock = Arc::new(Manual(Mutex::new(Utc::now())));
        let rule = RateRule {
            window_seconds: 300,
            max_requests: 5,
            block_seconds: 900,
        };
        let l = RateLimiter::new(rule, clock.clone());
        for _ in 0..4 {
            assert!(l.blocked("u|ip").is_ok());
            l.record_failure("u|ip");
        }
        assert!(l.blocked("u|ip").is_ok());
        l.reset("u|ip");
        for _ in 0..5 {
            assert!(l.blocked("u|ip").is_ok());
            l.record_failure("u|ip");
        }
        assert_eq!(l.blocked("u|ip"), Err(900));
        assert!(l.blocked("other|ip").is_ok());
        clock.advance(901);
        assert!(l.blocked("u|ip").is_ok());
    }

    // RISK-04: capacity exhaustion rejects new keys but keeps live counters.
    #[test]
    fn capacity_rejects_new_keys_without_evicting() {
        let clock = Arc::new(Manual(Mutex::new(Utc::now())));
        let rule = RateRule {
            window_seconds: 60,
            max_requests: 1,
            block_seconds: 0,
        };
        let l = RateLimiter::with_capacity(rule, clock.clone(), 2);
        assert!(l.hit("a").is_ok());
        assert!(l.hit("b").is_ok());
        assert_eq!(l.hit("c"), Err(60));
        assert!(l.hit("a").is_err(), "live counter must survive");
        clock.advance(61);
        assert!(l.hit("c").is_ok(), "expired entries are purged first");
    }

    #[test]
    fn disabled_rule_allows_everything() {
        let l = RateLimiter::new(
            RateRule {
                window_seconds: 0,
                max_requests: 0,
                block_seconds: 0,
            },
            Arc::new(zs_shared::clock::SystemClock),
        );
        for _ in 0..100 {
            assert!(l.hit("k").is_ok());
        }
    }
}
