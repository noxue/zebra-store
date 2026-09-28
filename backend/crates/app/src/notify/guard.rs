//! In-process [`OnceGuard`] (replaces the original Redis `SETNX`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use zs_domain::Result;
use zs_domain::notify::ports::OnceGuard;
use zs_shared::clock::Clock;

/// Upper bound of tracked keys; expired keys are purged when it is reached.
const MAX_KEYS: usize = 10_000;

/// TTL-keyed guard held in memory.
#[derive(Debug, Clone)]
pub struct MemoryGuard {
    keys: Arc<Mutex<HashMap<String, DateTime<Utc>>>>,
    clock: Arc<dyn Clock>,
}

impl MemoryGuard {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            keys: Arc::new(Mutex::new(HashMap::new())),
            clock,
        }
    }
}

#[async_trait]
impl OnceGuard for MemoryGuard {
    async fn acquire(&self, key: &str, ttl_seconds: i64) -> Result<bool> {
        let now = self.clock.now();
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        if keys.len() >= MAX_KEYS {
            keys.retain(|_, until| *until > now);
        }
        if keys.get(key).is_some_and(|until| *until > now) {
            return Ok(false);
        }
        keys.insert(key.to_owned(), now + Duration::seconds(ttl_seconds.max(1)));
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use zs_shared::clock::FixedClock;

    use super::*;

    #[tokio::test]
    async fn second_acquire_within_ttl_is_refused() {
        let t0 = Utc::now();
        let g = MemoryGuard::new(Arc::new(FixedClock(t0)));
        assert!(g.acquire("k", 60).await.unwrap_or(false));
        assert!(!g.acquire("k", 60).await.unwrap_or(true));
        assert!(g.acquire("other", 60).await.unwrap_or(false));
        let later = MemoryGuard {
            keys: g.keys.clone(),
            clock: Arc::new(FixedClock(t0 + Duration::seconds(61))),
        };
        assert!(later.acquire("k", 60).await.unwrap_or(false));
    }
}
