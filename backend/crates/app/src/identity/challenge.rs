//! In-process state of 2FA challenges and enable attempts (the original keeps
//! it in Redis; Redis is optional here, so a single instance keeps it in memory).

use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Duration, Utc};
use zs_domain::Id;
use zs_domain::identity::totp;
use zs_shared::clock::Clock;

/// Entries kept before expired ones are purged eagerly.
const PURGE_THRESHOLD: usize = 1024;

#[derive(Debug, Clone, Copy)]
struct Entry {
    count: u32,
    revoked: bool,
    expires_at: DateTime<Utc>,
}

/// Expiring counters keyed by `K`.
#[derive(Debug)]
struct Counters<K> {
    entries: Mutex<HashMap<K, Entry>>,
    clock: Arc<dyn Clock>,
    ttl: Duration,
}

impl<K: Eq + Hash + Clone> Counters<K> {
    fn new(clock: Arc<dyn Clock>, ttl: Duration) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            clock,
            ttl,
        }
    }

    fn with<R>(&self, f: impl FnOnce(&mut HashMap<K, Entry>, DateTime<Utc>) -> R) -> R {
        let now = self.clock.now();
        let mut map = self
            .entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if map.len() >= PURGE_THRESHOLD {
            map.retain(|_, e| e.expires_at > now);
        }
        f(&mut map, now)
    }

    fn get(&self, key: &K) -> Option<Entry> {
        self.with(|map, now| map.get(key).copied().filter(|e| e.expires_at > now))
    }

    fn update(&self, key: &K, f: impl FnOnce(&mut Entry)) -> Entry {
        let ttl = self.ttl;
        self.with(|map, now| {
            let entry = map.entry(key.clone()).or_insert(Entry {
                count: 0,
                revoked: false,
                expires_at: now + ttl,
            });
            if entry.expires_at <= now {
                *entry = Entry {
                    count: 0,
                    revoked: false,
                    expires_at: now + ttl,
                };
            }
            f(entry);
            *entry
        })
    }

    fn remove(&self, key: &K) {
        self.with(|map, _| {
            map.remove(key);
        });
    }
}

/// Failure counts and revocations of challenge tokens, keyed by `jti`
/// (original `2fa:challenge:{jti}:fails` / `:revoked`).
#[derive(Debug, Clone)]
pub struct ChallengeStore {
    inner: Arc<Counters<String>>,
}

impl ChallengeStore {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        // Kept slightly longer than the token itself so a revoked jti cannot come back.
        let ttl = Duration::minutes(totp::CHALLENGE_TTL_MINUTES + 1);
        Self {
            inner: Arc::new(Counters::new(clock, ttl)),
        }
    }

    pub fn is_revoked(&self, jti: &str) -> bool {
        self.inner.get(&jti.to_owned()).is_some_and(|e| e.revoked)
    }

    /// Increments and returns the failure count.
    pub fn bump_fails(&self, jti: &str) -> u32 {
        self.inner.update(&jti.to_owned(), |e| e.count += 1).count
    }

    pub fn revoke(&self, jti: &str) {
        self.inner.update(&jti.to_owned(), |e| e.revoked = true);
    }
}

/// Expiring per-account failure counter (enable attempts).
#[derive(Debug, Clone)]
pub struct FailureCounter {
    inner: Arc<Counters<Id>>,
}

impl FailureCounter {
    pub fn new(clock: Arc<dyn Clock>, ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Counters::new(clock, ttl)),
        }
    }

    pub fn count(&self, id: Id) -> u32 {
        self.inner.get(&id).map_or(0, |e| e.count)
    }

    pub fn bump(&self, id: Id) -> u32 {
        self.inner.update(&id, |e| e.count += 1).count
    }

    pub fn clear(&self, id: Id) {
        self.inner.remove(&id);
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

    #[test]
    fn challenges_count_revoke_and_expire() {
        let clock = Arc::new(Manual(Mutex::new(Utc::now())));
        let store = ChallengeStore::new(clock.clone());
        assert!(!store.is_revoked("j"));
        assert_eq!(store.bump_fails("j"), 1);
        assert_eq!(store.bump_fails("j"), 2);
        store.revoke("j");
        assert!(store.is_revoked("j"));
        *clock
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += Duration::minutes(10);
        assert!(!store.is_revoked("j"));
        assert_eq!(store.bump_fails("j"), 1);
    }

    #[test]
    fn failure_counter() {
        let c = FailureCounter::new(
            Arc::new(zs_shared::clock::SystemClock),
            Duration::minutes(1),
        );
        assert_eq!(c.count(1), 0);
        c.bump(1);
        assert_eq!(c.bump(1), 2);
        c.clear(1);
        assert_eq!(c.count(1), 0);
    }
}
