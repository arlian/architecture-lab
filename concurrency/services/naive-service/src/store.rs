//! THE FILE. Nine lines of business logic containing the oldest bug in the
//! trade, written the way it is genuinely written in production.
//!
//! The logic is read-modify-write:
//!
//! ```text
//!   1. read   how much stock is left
//!   2. decide whether the request fits
//!   3. write  the new total back
//! ```
//!
//! Every step is correct. The sequence is not, because between step 1 and step
//! 3 this task gives up the CPU, and while it is gone the value it read stops
//! being true. It then writes a number computed from a fact that has expired.
//! That is a **lost update**, and the visible symptom in this domain is
//! overselling: fifty callers are each told "yes, you got it" out of stock that
//! only covered twenty of them.
//!
//! ## Why the bug is reliable here rather than rare
//!
//! The gap between the read and the write is a `sleep(THINK_MS)` — a stand-in
//! for the thing that is actually there in a real handler: a call to a payment
//! gateway, a fraud check, a template render, a log flush, a slow serializer.
//! Any `await` at all will do. The lab makes the window explicit and tunable so
//! the race happens every single run instead of once a fortnight in production
//! at 3am, which is the only reason bugs of this shape survive code review.
//!
//! Set `THINK_MS=0` and the race disappears on a single-threaded runtime — the
//! task is never descheduled, so nothing interleaves. That is *exactly* how
//! this code passes its author's manual testing and then fails under load.
//! Absence of a race in testing is evidence about your scheduler, not about
//! your code.
//!
//! ## What is NOT the bug
//!
//! The `RwLock` is not missing, and adding a "bigger" lock around each
//! individual step would not help. Each of the three steps below is already
//! perfectly synchronised on its own. Race conditions are not caused by
//! unprotected *operations*; they are caused by unprotected *invariants* —
//! here, "stock never goes below zero", which spans all three steps and is
//! therefore protected by nothing.
//!
//! That distinction is the whole lab. `../../optimistic-service`,
//! `../../pessimistic-service` and `../../actor-service` each fix it, in three
//! incompatible ways, without changing a single line of the domain rule.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::RwLock;
use tokio::time::sleep;

use crate::error::ReserveError;

/// One product's stock row.
///
/// `version` counts writes. This service maintains it and reports it, but never
/// *reads* it to make a decision — which is precisely the difference between
/// this store and optimistic-service's. The number is sitting right there; only
/// one of the two services thinks to look at it.
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
}

pub struct Store {
    entries: RwLock<HashMap<String, Entry>>,
    /// How long the handler "thinks" between reading stock and writing it back.
    /// This is the race window, and it is the only knob in the service.
    think: Duration,
}

impl Store {
    pub fn new(think: Duration) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            think,
        }
    }

    /// Reset a product to a known number of units. Used by the race-runner
    /// before each run so every strategy is measured from the same start.
    pub async fn seed(&self, product: &str, units: u64) -> Entry {
        let mut entries = self.entries.write().await;
        let entry = Entry { units, version: 0 };
        entries.insert(product.to_string(), entry);
        entry
    }

    pub async fn get(&self, product: &str) -> Option<Entry> {
        self.entries.read().await.get(product).copied()
    }

    /// Take `units` off `product`, badly.
    pub async fn reserve(&self, product: &str, units: u64) -> Result<Reservation, ReserveError> {
        // --- 1. READ ---------------------------------------------------------
        // The guard is scoped to this block and dropped at the end of it, which
        // is the responsible thing to do and also the thing that makes the bug
        // possible. Holding it longer would be worse for throughput and better
        // for correctness; that trade is what pessimistic-service picks up.
        let snapshot = {
            let entries = self.entries.read().await;
            entries.get(product).copied()
        };
        let snapshot = snapshot.ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;

        // --- 2. THINK --------------------------------------------------------
        // The await. Right here, this task is suspended and every other
        // in-flight reservation gets to run — all of them having read, or about
        // to read, the same `snapshot.units` this one is holding on to.
        //
        // Nothing about this line looks dangerous. That is the problem: in an
        // async service *every* `.await` is one of these, and there is no
        // syntax highlighting for "the world may have moved on".
        //
        // The zero check is not a micro-optimisation, it is the lab's "turn the
        // bug off" switch: with no suspension point at all between the read and
        // the write, a current-thread runtime never interleaves these handlers
        // and the service looks flawless. `THINK_MS=0` is what a demo looks like.
        if !self.think.is_zero() {
            sleep(self.think).await;
        }

        // --- 3. DECIDE, on information that is now historical -----------------
        if snapshot.units < units {
            return Err(ReserveError::Insufficient {
                available: snapshot.units,
                requested: units,
            });
        }
        let remaining = snapshot.units - units;

        // --- 4. WRITE --------------------------------------------------------
        // A blind assignment. Not "subtract `units` from whatever is there now"
        // — that would be a read-modify-write under one lock, and would at
        // least keep the arithmetic honest. This writes an absolute number
        // derived from a stale read, so it silently discards every update that
        // landed while this task was thinking. Fifty tasks that all read 100
        // all write 98, and 98 is what you get.
        let mut entries = self.entries.write().await;
        let slot = entries
            .get_mut(product)
            .ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;
        slot.units = remaining;
        slot.version += 1;

        Ok(Reservation {
            reserved: units,
            available: slot.units,
            version: slot.version,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;

    fn store() -> Store {
        Store::new(Duration::from_millis(5))
    }

    /// One caller at a time, and the code is flawless. This is the test that
    /// gets written, passes forever, and proves nothing.
    #[tokio::test]
    async fn sequential_reservations_are_perfectly_correct() {
        let store = store();
        store.seed("mug", 10).await;

        for _ in 0..5 {
            store.reserve("mug", 2).await.expect("should fit");
        }

        assert_eq!(store.get("mug").await.unwrap().units, 0);
        let err = store.reserve("mug", 1).await.unwrap_err();
        assert!(matches!(err, ReserveError::Insufficient { .. }));
    }

    /// The same code, the same data, the same arithmetic — called twice at
    /// once. This test asserts that the service *oversells*, because
    /// documenting the bug is the entire job of this crate. If it ever starts
    /// failing, someone has accidentally fixed naive-service, and the lab has
    /// lost its control group.
    #[tokio::test]
    async fn concurrent_reservations_oversell() {
        let store = store();
        store.seed("mug", 10).await;

        // Ten callers want two units each: twenty units out of ten.
        let attempts = (0..10).map(|_| store.reserve("mug", 2));
        let results = join_all(attempts).await;

        let granted = results.iter().filter(|r| r.is_ok()).count();
        let left = store.get("mug").await.unwrap().units;

        assert_eq!(granted, 10, "every caller was told yes");
        assert_eq!(
            left, 8,
            "ten writers each computed 10 - 2 from the same stale read"
        );
        assert!(
            granted as u64 * 2 > 10 - left,
            "units handed out ({}) exceed units actually consumed ({})",
            granted as u64 * 2,
            10 - left
        );
    }

    /// The reassuring test. With no await between the read and the write,
    /// nothing interleaves on a current-thread runtime and the service looks
    /// correct — which is how this bug reaches production.
    #[tokio::test]
    async fn without_a_think_window_the_race_hides() {
        let store = Store::new(Duration::ZERO);
        store.seed("mug", 10).await;

        let attempts = (0..10).map(|_| store.reserve("mug", 2));
        let results = join_all(attempts).await;

        let granted = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(granted, 5, "looks fine; the window was just too small to see");
    }
}
