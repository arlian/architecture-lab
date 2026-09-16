//! THE FILE, take three: make the race impossible by making the callers take
//! turns.
//!
//! This is **pessimistic** concurrency control, and it assumes the opposite of
//! optimistic-service: conflicts are likely, so do not gamble. Acquire
//! exclusive access to the row *before* reading it, and hold it until the write
//! lands. Whoever arrives second waits.
//!
//! ```text
//!   0. acquire the lock for this product  <- everybody else now queues here
//!   1. read    stock
//!   2. think
//!   3. write
//!   4. release
//! ```
//!
//! No versions, no retries, no wasted work, no 409 — and the request that wins
//! is simply the one that asked first. It is the easiest of the three fixes to
//! reason about, which is exactly why it is the one people reach for and then
//! regret.
//!
//! ## Three things this file is really about
//!
//! **1. Lock granularity is the design decision, not the lock.** `LOCK_SCOPE`
//! switches between one mutex per product and one mutex for the whole service.
//! Both are correct. One of them serialises your entire business. The runner
//! shows the difference in one command:
//!
//! ```text
//! cargo run -p race-runner -- --target http://localhost:3022 --products 8
//! LOCK_SCOPE=global cargo run -p pessimistic-service   # same run, N times slower
//! ```
//!
//! Every real system has this dial somewhere, usually undocumented: a table
//! lock vs a row lock, a `synchronized` method vs a striped lock, one Redis key
//! vs one per tenant. Coarse locks are correct on day one and the reason for
//! the incident on day four hundred.
//!
//! **2. The critical section contains an `.await`, and that is deliberate.** The
//! `think` happens while the lock is held, because in a real handler the slow
//! thing (the payment call) is *why* you took the lock. So the lock is held for
//! the full duration of the slow thing, and throughput on a hot row collapses to
//! `1 / think`. Holding a lock across I/O is the single most common way this
//! strategy goes wrong, and you cannot always avoid it without changing the
//! domain design — see actor-service, which does not avoid it either, and
//! `optimistic-service`, which is the only one here that genuinely does.
//!
//! **3. The compiler is helping, and it is worth noticing where.** The guard
//! below is held across an `.await`, which is only legal because it is a
//! [`tokio::sync::Mutex`]. Swap it for a `std::sync::Mutex` and this will not
//! compile: its guard is `!Send`, so the future stops being `Send`, so axum
//! refuses the handler. That is a whole category of concurrency bug turned into
//! a type error. The categories it does *not* catch — deadlock from
//! inconsistent lock ordering, and forgetting to take the lock at all — are the
//! two that actually bite, and nothing in this file prevents either.
//!
//! ## The deadlock this file is one edit away from
//!
//! There is exactly one lock acquired per request here, so deadlock is
//! impossible today. Take two — say, a transfer that locks the source product
//! and then the destination — and two concurrent transfers in opposite
//! directions hang forever, with no error, no log line, and no timeout. The
//! standard defence is a total order on locks (always acquire by sorted key),
//! and the reason it fails in practice is that it is a convention, enforced by
//! nothing, spread across files nobody reads together.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::{Mutex, RwLock};
use tokio::time::sleep;

use crate::error::ReserveError;

#[derive(Debug, Clone, Copy, Default)]
pub struct Entry {
    pub units: u64,
    pub version: u64,
}

#[derive(Debug)]
pub struct Reservation {
    pub reserved: u64,
    pub available: u64,
    pub version: u64,
    /// How long this request spent queued behind other writers before it got to
    /// do anything. In the other services this number does not exist; here it
    /// is most of the response time under load, and it is the honest price tag
    /// of the strategy.
    pub waited_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockScope {
    /// One mutex per product. Two different products never wait for each other.
    Key,
    /// One mutex for everything. Correct, trivially, and a throughput ceiling of
    /// one request at a time for the entire service.
    Global,
}

impl LockScope {
    pub fn from_env_value(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "key" | "row" | "product" => Some(LockScope::Key),
            "global" | "table" | "all" => Some(LockScope::Global),
            _ => None,
        }
    }
}

pub struct Store {
    entries: RwLock<HashMap<String, Entry>>,
    /// The lock table. Note that this map is itself shared mutable state and
    /// needs its own lock — locks do not come for free, they come with
    /// bookkeeping, and the bookkeeping is more shared state.
    ///
    /// It also never shrinks: one mutex per product this service has ever seen,
    /// held forever. At this scale that is nothing. With per-user or per-order
    /// keys it is a leak, and "evict idle locks" is a genuinely delicate piece
    /// of code to write — you must not evict one somebody is about to take.
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
    global: Arc<Mutex<()>>,
    scope: LockScope,
    think: Duration,
}

impl Store {
    pub fn new(think: Duration, scope: LockScope) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
            global: Arc::new(Mutex::new(())),
            scope,
            think,
        }
    }

    pub async fn seed(&self, product: &str, units: u64) -> Entry {
        let mut entries = self.entries.write().await;
        let entry = Entry { units, version: 0 };
        entries.insert(product.to_string(), entry);
        entry
    }

    pub async fn get(&self, product: &str) -> Option<Entry> {
        self.entries.read().await.get(product).copied()
    }

    /// Which mutex guards this product. Returns an `Arc` rather than a guard so
    /// that the (short-lived) lock on the lock *table* is released before the
    /// caller starts waiting on the (long-lived) lock for the row. Getting that
    /// backwards — holding the table lock while waiting for a row lock — would
    /// turn per-key locking back into global locking, quietly, and every test
    /// in this file would still pass.
    async fn lock_for(&self, product: &str) -> Arc<Mutex<()>> {
        match self.scope {
            LockScope::Global => self.global.clone(),
            LockScope::Key => {
                let mut locks = self.locks.lock().await;
                locks
                    .entry(product.to_string())
                    .or_insert_with(|| Arc::new(Mutex::new(())))
                    .clone()
            }
        }
    }

    pub async fn reserve(&self, product: &str, units: u64) -> Result<Reservation, ReserveError> {
        let lock = self.lock_for(product).await;

        let queued_at = Instant::now();
        // `_guard`, not `_`. `let _ = lock.lock().await;` drops the guard
        // immediately and releases the lock on the very next line, which
        // compiles, runs, passes review, and protects nothing — this service
        // would become naive-service and no test would notice. An underscore
        // prefix keeps the binding alive to the end of the scope; a bare
        // underscore does not. It is a one-character difference between a
        // correct service and a broken one.
        let _guard = lock.lock().await;
        let waited_ms = queued_at.elapsed().as_millis() as u64;

        // --- inside the critical section -------------------------------------
        // Everything from here to the end of the function is exclusive. The
        // read below is therefore still true when the write happens, which is
        // the one property naive-service lacks and the only property that
        // matters.
        let snapshot = {
            let entries = self.entries.read().await;
            entries.get(product).copied()
        };
        let snapshot = snapshot.ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;

        // The same `.await` that is fatal in naive-service and free in
        // optimistic-service. Here it is neither: it is correct, and every
        // other caller for this product is asleep for the duration.
        if !self.think.is_zero() {
            sleep(self.think).await;
        }

        if snapshot.units < units {
            return Err(ReserveError::Insufficient {
                available: snapshot.units,
                requested: units,
            });
        }

        let mut entries = self.entries.write().await;
        let slot = entries
            .get_mut(product)
            .ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;
        slot.units = snapshot.units - units;
        slot.version += 1;

        Ok(Reservation {
            reserved: units,
            available: slot.units,
            version: slot.version,
            waited_ms,
        })
        // `_guard` drops here, and the next caller in the queue wakes up.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;

    fn store(scope: LockScope) -> Store {
        Store::new(Duration::from_millis(5), scope)
    }

    #[tokio::test]
    async fn sequential_reservations_are_correct() {
        let store = store(LockScope::Key);
        store.seed("mug", 10).await;

        for _ in 0..5 {
            store.reserve("mug", 2).await.expect("should fit");
        }

        assert_eq!(store.get("mug").await.unwrap().units, 0);
    }

    /// The headline: the scenario that oversells in naive-service, with no
    /// retries, no conflicts and no wasted work — just a queue.
    #[tokio::test]
    async fn concurrent_reservations_never_oversell() {
        let store = store(LockScope::Key);
        store.seed("mug", 10).await;

        let attempts = (0..10).map(|_| store.reserve("mug", 2));
        let results = join_all(attempts).await;

        let granted = results.iter().filter(|r| r.is_ok()).count() as u64;
        let left = store.get("mug").await.unwrap().units;

        assert_eq!(granted, 5, "exactly the five that fit");
        assert_eq!(left, 0);
        assert_eq!(granted * 2, 10 - left, "units handed out == units consumed");

        for result in &results {
            if let Err(err) = result {
                assert!(matches!(err, ReserveError::Insufficient { .. }));
            }
        }
    }

    /// The waiting is real and measurable, which is the point of reporting it.
    #[tokio::test]
    async fn later_callers_queue_behind_earlier_ones() {
        let store = Store::new(Duration::from_millis(30), LockScope::Key);
        store.seed("mug", 10).await;

        let results = join_all((0..3).map(|_| store.reserve("mug", 1))).await;
        let waits: Vec<u64> = results.iter().map(|r| r.as_ref().unwrap().waited_ms).collect();

        assert!(waits[0] < 15, "the first caller waits for nobody");
        assert!(
            waits[2] >= 50,
            "the third caller waits for two think windows, got {}ms",
            waits[2]
        );
    }

    /// Granularity, demonstrated. Two products, two callers, one lock each:
    /// they run at the same time.
    #[tokio::test]
    async fn per_key_locks_do_not_serialise_unrelated_products() {
        let store = Store::new(Duration::from_millis(100), LockScope::Key);
        store.seed("mug", 10).await;
        store.seed("plate", 10).await;

        let started = Instant::now();
        join_all(vec![store.reserve("mug", 1), store.reserve("plate", 1)]).await;
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(170),
            "two unrelated products took {elapsed:?} — that looks serialised"
        );
    }

    /// The same two callers behind one global lock: one after the other, for no
    /// reason anybody in the domain could explain.
    #[tokio::test]
    async fn a_global_lock_serialises_products_that_share_nothing() {
        let store = Store::new(Duration::from_millis(100), LockScope::Global);
        store.seed("mug", 10).await;
        store.seed("plate", 10).await;

        let started = Instant::now();
        join_all(vec![store.reserve("mug", 1), store.reserve("plate", 1)]).await;
        let elapsed = started.elapsed();

        assert!(
            elapsed >= Duration::from_millis(180),
            "expected two serialised think windows, took only {elapsed:?}"
        );
    }

    #[test]
    fn lock_scope_parses_the_names_people_actually_type() {
        assert_eq!(LockScope::from_env_value("KEY"), Some(LockScope::Key));
        assert_eq!(LockScope::from_env_value(" row "), Some(LockScope::Key));
        assert_eq!(LockScope::from_env_value("global"), Some(LockScope::Global));
        assert_eq!(LockScope::from_env_value("sometimes"), None);
    }
}
