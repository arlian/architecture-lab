//! THE FILE, take two: the same read-modify-write, made safe without ever
//! making anybody wait.
//!
//! The strategy is **optimistic concurrency control**, and its bet is in the
//! name: assume conflicts are rare, do the work without coordination, and check
//! at the last possible moment whether the world moved. If it did, throw the
//! work away and do it again.
//!
//! ```text
//!   1. read    stock AND its version         (v = 7, units = 100)
//!   2. think   as slowly as you like         (nobody is blocked)
//!   3. write   ONLY IF the version is still 7, and bump it to 8
//!      └─ if it isn't 7 any more: discard everything, go back to step 1
//! ```
//!
//! Step 3 is a **compare-and-swap**, and the only thing that makes it work is
//! that the compare and the swap happen inside one critical section with no
//! `.await` between them. That is the entire fix. Re-read naive-service's
//! `store.rs` and you will find the identical blind assignment
//! `slot.units = snapshot.units - units` — the same line, byte for byte. It is
//! not the write that was wrong there. It was writing without first proving the
//! read was still valid.
//!
//! ## What this buys, and what it costs
//!
//! Buys: writers never block each other. The *winner* of a conflict pays
//! nothing at all — it does not know a conflict happened. Reads are free. And
//! critically, this is the only strategy in the lab that still works when the
//! service is replicated, *provided* the compare-and-swap happens where the
//! data lives (`UPDATE ... SET version = 8 WHERE id = ? AND version = 7`, and
//! then check the affected row count). A mutex in one process protects nothing
//! from the other four processes; a version column protects against all of them.
//!
//! Costs: the losers redo work that was already paid for, so under real
//! contention the system burns CPU proportional to how wrong its optimism was.
//! And when the retry budget runs out, a conflict stops being an implementation
//! detail and becomes a 409 in somebody's face. Optimistic concurrency does not
//! eliminate contention. It converts contention into *wasted work plus 409s*,
//! which is a good trade exactly as long as conflicts really are rare.
//!
//! Point the race-runner at this service with `--concurrency 60` on one product
//! and watch it stop being a good trade.

use std::collections::HashMap;
use std::time::Duration;

use rand::Rng;
use tokio::sync::RwLock;
use tokio::time::sleep;

use crate::error::ReserveError;

/// Same row as naive-service's, and the `version` field is *literally* the same
/// field. The difference between the two services is not the data model, it is
/// whether anybody consults it before writing.
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
    /// How many times this one request had to start over. `1` means it won
    /// uncontended. Anything else is contention you are paying for in latency
    /// and CPU, and it is worth putting on a dashboard.
    pub attempts: u32,
}

pub struct Store {
    entries: RwLock<HashMap<String, Entry>>,
    think: Duration,
    /// How many times to start over before giving the caller a 409. Zero means
    /// "never retry" — every conflict is the client's problem, immediately.
    retries: u32,
}

impl Store {
    pub fn new(think: Duration, retries: u32) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            think,
            retries,
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

    /// Reserve `units`, retrying on conflict.
    ///
    /// `expected_version` is the client-driven flavour of the same mechanism —
    /// an HTTP `If-Match` in all but spelling. When it is supplied the server
    /// does **not** retry: the client said "apply this to version 7 or tell
    /// me", and silently applying it to version 9 instead would be the exact
    /// lost update this service exists to prevent. Whoever supplied the version
    /// is the only one who knows whether the decision behind it survives the
    /// row having changed.
    pub async fn reserve(
        &self,
        product: &str,
        units: u64,
        expected_version: Option<u64>,
    ) -> Result<Reservation, ReserveError> {
        let budget = if expected_version.is_some() {
            1
        } else {
            self.retries + 1
        };

        for attempt in 1..=budget {
            if attempt > 1 {
                self.backoff(attempt).await;
            }

            // --- 1. READ, version included -----------------------------------
            let snapshot = {
                let entries = self.entries.read().await;
                entries.get(product).copied()
            };
            let snapshot =
                snapshot.ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;

            // The client's precondition, checked early so an obviously doomed
            // request does not pay for the think below. It is checked again
            // implicitly by the compare-and-swap; this one is only courtesy.
            if let Some(expected) = expected_version {
                if expected != snapshot.version {
                    return Err(ReserveError::Conflict {
                        product: product.to_string(),
                        attempts: attempt,
                    });
                }
            }

            // --- 2. THINK ----------------------------------------------------
            // The same suspension point that ruins naive-service. Here it is
            // harmless, and note *why*: not because it is shorter, or because
            // something is locked, but because nothing this task does after
            // waking will be trusted until the version is re-checked.
            if !self.think.is_zero() {
                sleep(self.think).await;
            }

            if snapshot.units < units {
                return Err(ReserveError::Insufficient {
                    available: snapshot.units,
                    requested: units,
                });
            }

            // --- 3. COMPARE AND SWAP -----------------------------------------
            // One critical section. No `.await` inside it — not the sleep, not
            // a log flush, not a metrics call, nothing. If an await sneaks in
            // between the compare and the swap, this service becomes
            // naive-service with extra steps, and every test here still passes.
            let outcome = {
                let mut entries = self.entries.write().await;
                let slot = entries
                    .get_mut(product)
                    .ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;

                if slot.version != snapshot.version {
                    // Somebody committed while we were thinking. Our `snapshot`
                    // is a fact about the past. Throw the work away — that is
                    // the price of not having held a lock.
                    None
                } else {
                    slot.units = snapshot.units - units;
                    slot.version += 1;
                    Some((slot.units, slot.version))
                }
            };

            match outcome {
                Some((available, version)) => {
                    return Ok(Reservation {
                        reserved: units,
                        available,
                        version,
                        attempts: attempt,
                    })
                }
                None => {
                    tracing::debug!(product = %product, attempt, "version moved; retrying");
                    continue;
                }
            }
        }

        Err(ReserveError::Conflict {
            product: product.to_string(),
            attempts: budget,
        })
    }

    /// Exponential backoff with full jitter.
    ///
    /// Without the jitter every loser of a conflict wakes at the same
    /// millisecond and collides again — a retry storm that makes contention
    /// worse the more contended things get. The randomness is not a nicety; it
    /// is what stops the retry loop from being a feedback loop.
    async fn backoff(&self, attempt: u32) {
        let base = 1u64 << (attempt.min(6) - 1);
        // `ThreadRng` is `!Send`, so it must not be alive across the `.await`
        // below — the block ends its lifetime. This is a small, welcome example
        // of the borrow checker refusing to compile a concurrency mistake that
        // most languages would let you make at runtime.
        let jitter = {
            let mut rng = rand::thread_rng();
            rng.gen_range(0..=base)
        };
        sleep(Duration::from_millis(base + jitter)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;

    fn store(retries: u32) -> Store {
        Store::new(Duration::from_millis(5), retries)
    }

    #[tokio::test]
    async fn sequential_reservations_are_correct() {
        let store = store(8);
        store.seed("mug", 10).await;

        for _ in 0..5 {
            let r = store.reserve("mug", 2, None).await.expect("should fit");
            assert_eq!(r.attempts, 1, "uncontended requests never retry");
        }

        assert_eq!(store.get("mug").await.unwrap().units, 0);
    }

    /// The headline: the exact scenario that oversells in naive-service.
    #[tokio::test]
    async fn concurrent_reservations_never_oversell() {
        let store = store(24);
        store.seed("mug", 6).await;

        let attempts = (0..6).map(|_| store.reserve("mug", 2, None));
        let results = join_all(attempts).await;

        let granted = results.iter().filter(|r| r.is_ok()).count() as u64;
        let left = store.get("mug").await.unwrap().units;

        assert_eq!(
            granted * 2,
            6 - left,
            "units handed out must equal units consumed"
        );
        assert_eq!(granted, 3, "three callers of six can be satisfied");
        assert_eq!(left, 0);

        // The losers were told the truth about *why*, and it is not "conflict":
        // by the time they retried, the stock really was gone.
        for result in &results {
            if let Err(err) = result {
                assert!(
                    matches!(err, ReserveError::Insufficient { .. }),
                    "expected an honest out-of-stock, got {err}"
                );
            }
        }
    }

    /// Retries are a *policy*, not the mechanism. Turn them off and the
    /// mechanism is still correct — it just stops hiding the conflicts.
    #[tokio::test]
    async fn without_retries_conflicts_reach_the_caller() {
        let store = store(0);
        store.seed("mug", 6).await;

        let attempts = (0..6).map(|_| store.reserve("mug", 2, None));
        let results = join_all(attempts).await;

        let granted = results.iter().filter(|r| r.is_ok()).count() as u64;
        let conflicts = results
            .iter()
            .filter(|r| matches!(r, Err(ReserveError::Conflict { .. })))
            .count();
        let left = store.get("mug").await.unwrap().units;

        assert_eq!(granted, 1, "exactly one writer wins a round");
        assert_eq!(conflicts, 5);
        assert_eq!(left, 4, "and the other five changed nothing at all");
        assert_eq!(granted * 2, 6 - left, "still no oversell");
    }

    #[tokio::test]
    async fn a_stale_client_version_is_rejected_rather_than_applied() {
        let store = store(8);
        store.seed("mug", 10).await;

        let first = store.reserve("mug", 1, Some(0)).await.expect("v0 is current");
        assert_eq!(first.version, 1);

        // Same precondition, one write too late. This is the request that, in
        // naive-service, would have been accepted and silently clobbered the
        // write above — `expected_version` is in that service's request body
        // too, and ignored.
        let err = store.reserve("mug", 1, Some(0)).await.unwrap_err();
        assert!(matches!(err, ReserveError::Conflict { .. }));
        assert_eq!(
            store.get("mug").await.unwrap().units,
            9,
            "the rejected request must not have touched the row"
        );
    }

    #[tokio::test]
    async fn a_current_client_version_is_accepted() {
        let store = store(8);
        store.seed("mug", 10).await;

        store.reserve("mug", 1, Some(0)).await.expect("v0 is current");
        let second = store.reserve("mug", 1, Some(1)).await.expect("v1 is current");

        assert_eq!(second.available, 8);
        assert_eq!(second.version, 2);
    }

    #[tokio::test]
    async fn unknown_products_are_not_created_by_reserving_them() {
        let store = store(8);
        let err = store.reserve("ghost", 1, None).await.unwrap_err();
        assert!(matches!(err, ReserveError::UnknownProduct(_)));
    }
}
