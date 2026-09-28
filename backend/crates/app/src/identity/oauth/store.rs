//! In-process single-use state with expiry: Telegram replay markers, OIDC
//! states and Google redirect intents/handoffs. The original keeps these in
//! Redis (`SET NX EX`, `GETDEL`); Redis is optional here, so one instance keeps
//! them in memory with the same atomic semantics.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Duration, Utc};
use zs_shared::clock::Clock;

/// Entries kept before expired ones are purged eagerly.
const PURGE_THRESHOLD: usize = 1024;

/// Expiring key/value map.
pub struct Expiring<V> {
    entries: Mutex<HashMap<String, (V, DateTime<Utc>)>>,
    clock: Arc<dyn Clock>,
}

impl<V> std::fmt::Debug for Expiring<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Expiring")
    }
}

impl<V> Expiring<V> {
    pub fn new(clock: Arc<dyn Clock>) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            clock,
        }
    }

    fn with<R>(
        &self,
        f: impl FnOnce(&mut HashMap<String, (V, DateTime<Utc>)>, DateTime<Utc>) -> R,
    ) -> R {
        let now = self.clock.now();
        let mut map = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if map.len() >= PURGE_THRESHOLD {
            map.retain(|_, (_, expires_at)| *expires_at > now);
        }
        f(&mut map, now)
    }

    /// `SET key value NX EX ttl`: stores only when no live entry exists.
    pub fn insert_new(&self, key: &str, value: V, ttl: Duration) -> bool {
        self.with(|map, now| {
            if map.get(key).is_some_and(|(_, exp)| *exp > now) {
                return false;
            }
            map.insert(key.to_owned(), (value, now + ttl));
            true
        })
    }

    /// `GETDEL key`: removes and returns a live entry.
    pub fn take(&self, key: &str) -> Option<V> {
        self.with(|map, now| {
            map.remove(key)
                .filter(|(_, exp)| *exp > now)
                .map(|(v, _)| v)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Manual(Mutex<DateTime<Utc>>);

    impl Clock for Manual {
        fn now(&self) -> DateTime<Utc> {
            *self.0.lock().unwrap()
        }
    }

    impl Manual {
        fn advance(&self, d: Duration) {
            *self.0.lock().unwrap() += d;
        }
    }

    #[test]
    fn set_nx_and_getdel() {
        let clock = Arc::new(Manual(Mutex::new(
            DateTime::from_timestamp(1_000, 0).unwrap(),
        )));
        let store: Expiring<u8> = Expiring::new(clock.clone());
        assert!(store.insert_new("k", 1, Duration::seconds(10)));
        assert!(!store.insert_new("k", 2, Duration::seconds(10)));
        assert_eq!(store.take("k"), Some(1));
        assert_eq!(store.take("k"), None);
        assert!(store.insert_new("k", 3, Duration::seconds(10)));
        clock.advance(Duration::seconds(10));
        // Expired entries are neither returned nor blocking.
        assert!(store.insert_new("k", 4, Duration::seconds(10)));
        clock.advance(Duration::seconds(11));
        assert_eq!(store.take("k"), None);
    }
}
