//! THE FILE, take four — and note that there is no `store.rs` in this service,
//! because there is no store. There is a task that owns a `HashMap`, and a
//! channel you can ask it things through.
//!
//! The other three services all start from the same premise: state is shared,
//! so access to it must be coordinated. They then differ only in *how* —
//! badly (naive), by detection (optimistic), or by exclusion (pessimistic).
//! This one rejects the premise. The `HashMap` below is an ordinary local
//! variable inside one task. Nothing else in the process can reach it, at all,
//! and the compiler is what guarantees that: no `Arc`, no `Mutex`, no `RwLock`,
//! nothing `Sync` anywhere near it.
//!
//! ```text
//!   handler ──┐
//!   handler ──┼── mpsc ──> [ one task, owns the HashMap ] ──> oneshot ──> reply
//!   handler ──┘  (bounded)   reads and writes one at a time
//! ```
//!
//! A race condition requires two things to touch the same data at once. Here
//! the second thing does not exist. The bug is not prevented, detected, or
//! guarded against; it is unrepresentable.
//!
//! ## The uncomfortable part
//!
//! This is not faster than pessimistic-service with `LOCK_SCOPE=global`. It is
//! the *same* serialisation: one writer, everyone else queued, and the `think`
//! below happens inside the loop, so the whole service retires one reservation
//! per think window. Run the race-runner against `:3022` with `LOCK_SCOPE=global`
//! and then against `:3023` and the numbers will look like siblings, because
//! they are.
//!
//! What you actually buy is threefold, and none of it is throughput:
//!
//! * **The mistake is impossible, not merely avoided.** In
//!   pessimistic-service, forgetting `lock_for(...)` in one new handler
//!   reintroduces the bug, silently, and every existing test still passes.
//!   Here there is no lock to forget: the only way to touch the state is to
//!   send a message, and the only place messages are handled is the loop below.
//!   Correctness stops depending on everyone remembering.
//! * **Deadlock is not in the vocabulary.** No task ever holds two of anything.
//!   The classic "lock A then B / lock B then A" hang cannot be written.
//!   (Actors have their own version of this — two actors awaiting each other's
//!   replies — but that requires two actors, and there is one.)
//! * **The queue is a real object with a real size.** A lock's waiting list is
//!   invisible and unbounded; you find out it exists from a latency graph. This
//!   one has a capacity, a length you could export as a metric, and a defined
//!   behaviour when it is full: shed the request with a 503. See `MAILBOX`.
//!
//! ## Scaling it is a different word
//!
//! One actor is a global lock. The fix is the one every actor framework is
//! built around — **shard it**: one actor per product, addressed by key, so
//! unrelated products proceed in parallel. That is precisely
//! pessimistic-service's `LOCK_SCOPE=key`, arrived at from the opposite
//! direction, and it comes with the same bookkeeping problem (a registry of
//! live actors, which is itself shared state). The lab leaves the single actor
//! in place because the comparison is the lesson; see the README's exercises.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};
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
    /// Time between handing the command to the mailbox and getting a reply:
    /// the wait behind every command already in the queue, plus this one's own
    /// work. Compare pessimistic-service's `waited_ms` — the same pressure,
    /// measured on the other side of the same idea.
    pub queued_ms: u64,
}

/// Everything the outside world is allowed to ask for, as data.
///
/// This enum is the service's real API — `http.rs` is a thin translation of
/// HTTP into these three messages. Every one carries a `oneshot` sender for its
/// reply, which is how a message-passing system does a *return value*.
enum Command {
    Get {
        product: String,
        reply: oneshot::Sender<Option<Entry>>,
    },
    Seed {
        product: String,
        units: u64,
        reply: oneshot::Sender<Entry>,
    },
    Reserve {
        product: String,
        units: u64,
        reply: oneshot::Sender<Result<Reservation, ReserveError>>,
    },
}

/// A handle to the one writer. Cheap to clone, holds no state, and — the point
/// — offers no way whatsoever to reach the data except by sending a message.
#[derive(Clone)]
pub struct StockActor {
    tx: mpsc::Sender<Command>,
}

impl StockActor {
    /// Spawn the writer and return a handle to it.
    ///
    /// `mailbox` is the bounded capacity of the queue. It is the only place in
    /// this lab where a number decides how much unfinished work the service is
    /// willing to be holding, which is a decision every service makes and most
    /// make by accident, as "however much fits in RAM".
    pub fn spawn(think: Duration, mailbox: usize) -> Self {
        let (tx, rx) = mpsc::channel(mailbox);
        tokio::spawn(run(rx, think));
        Self { tx }
    }

    pub async fn get(&self, product: &str) -> Result<Option<Entry>, ReserveError> {
        let (reply, answer) = oneshot::channel();
        self.dispatch(Command::Get {
            product: product.to_string(),
            reply,
        })?;
        answer.await.map_err(|_| ReserveError::Busy)
    }

    pub async fn seed(&self, product: &str, units: u64) -> Result<Entry, ReserveError> {
        let (reply, answer) = oneshot::channel();
        self.dispatch(Command::Seed {
            product: product.to_string(),
            units,
            reply,
        })?;
        answer.await.map_err(|_| ReserveError::Busy)
    }

    pub async fn reserve(&self, product: &str, units: u64) -> Result<Reservation, ReserveError> {
        let queued_at = Instant::now();
        let (reply, answer) = oneshot::channel();
        self.dispatch(Command::Reserve {
            product: product.to_string(),
            units,
            reply,
        })?;

        let mut reservation = answer.await.map_err(|_| ReserveError::Busy)??;
        reservation.queued_ms = queued_at.elapsed().as_millis() as u64;
        Ok(reservation)
    }

    /// `try_send`, deliberately, not `send().await`.
    ///
    /// `send().await` would wait for a free slot, which turns the bounded
    /// mailbox back into an unbounded one — the queue just moves out of the
    /// channel and into the set of parked tasks, where nothing can see or limit
    /// it. `try_send` is what makes the capacity mean something: past it, the
    /// service refuses work instead of accumulating it.
    fn dispatch(&self, command: Command) -> Result<(), ReserveError> {
        self.tx.try_send(command).map_err(|err| match err {
            mpsc::error::TrySendError::Full(_) => ReserveError::Busy,
            // The actor task is gone, which in this service means the process
            // is on fire. Reporting it as `Busy` (503) is still the right
            // answer for the caller: stop sending, try later.
            mpsc::error::TrySendError::Closed(_) => {
                tracing::error!("the stock writer has stopped; dropping command");
                ReserveError::Busy
            }
        })
    }
}

/// The writer. One task, one `HashMap`, one command at a time, forever.
///
/// There is no lock in this function and there is nothing to lock: `entries` is
/// a local variable. Everything below is ordinary single-threaded code, and it
/// is single-threaded code *by construction* rather than by agreement.
async fn run(mut rx: mpsc::Receiver<Command>, think: Duration) {
    let mut entries: HashMap<String, Entry> = HashMap::new();

    // The loop ends when every `StockActor` handle has been dropped, which in
    // this service means the server is shutting down.
    while let Some(command) = rx.recv().await {
        match command {
            Command::Get { product, reply } => {
                // `let _ =` on purpose: the caller may have hung up (client
                // disconnected, request cancelled), and that is not an error —
                // it is the normal end of a request whose reply nobody wants.
                let _ = reply.send(entries.get(&product).copied());
            }

            Command::Seed {
                product,
                units,
                reply,
            } => {
                let entry = Entry { units, version: 0 };
                entries.insert(product, entry);
                let _ = reply.send(entry);
            }

            Command::Reserve {
                product,
                units,
                reply,
            } => {
                let outcome = reserve(&mut entries, &product, units, think).await;
                let _ = reply.send(outcome);
            }
        }
    }

    tracing::info!("stock writer stopped");
}

/// The domain rule, written as if concurrency had never been invented.
///
/// Read it against naive-service's `reserve`. The steps are identical, the
/// arithmetic is identical, and the `.await` in the middle is identical. This
/// one is correct and that one is not, and the difference is not in this
/// function at all — it is in the fact that only one of these can be running.
///
/// That `&mut HashMap` is the proof, and it is worth staring at: an exclusive
/// reference. The borrow checker will not hand out a second one. The property
/// the other services enforce with a version column or a mutex is, here, the
/// ordinary meaning of `&mut`.
async fn reserve(
    entries: &mut HashMap<String, Entry>,
    product: &str,
    units: u64,
    think: Duration,
) -> Result<Reservation, ReserveError> {
    let snapshot = *entries
        .get(product)
        .ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;

    // The actor is blocked for this whole window — no other command is
    // processed, including reads. This is the real cost of a single writer, and
    // the reason a production actor would do the slow part *outside* the loop
    // and come back with a follow-up message rather than sleeping here.
    if !think.is_zero() {
        sleep(think).await;
    }

    if snapshot.units < units {
        return Err(ReserveError::Insufficient {
            available: snapshot.units,
            requested: units,
        });
    }

    let slot = entries
        .get_mut(product)
        .ok_or_else(|| ReserveError::UnknownProduct(product.to_string()))?;
    slot.units = snapshot.units - units;
    slot.version += 1;

    Ok(Reservation {
        reserved: units,
        available: slot.units,
        version: slot.version,
        // Filled in by the handle, which is the only side that knows when the
        // command was posted.
        queued_ms: 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::join_all;

    fn actor() -> StockActor {
        StockActor::spawn(Duration::from_millis(5), 64)
    }

    #[tokio::test]
    async fn sequential_reservations_are_correct() {
        let actor = actor();
        actor.seed("mug", 10).await.unwrap();

        for _ in 0..5 {
            actor.reserve("mug", 2).await.expect("should fit");
        }

        assert_eq!(actor.get("mug").await.unwrap().unwrap().units, 0);
    }

    /// The headline: the scenario that oversells in naive-service, with no
    /// lock, no version, and no retry anywhere in the crate.
    #[tokio::test]
    async fn concurrent_reservations_never_oversell() {
        let actor = actor();
        actor.seed("mug", 10).await.unwrap();

        let results = join_all((0..10).map(|_| actor.reserve("mug", 2))).await;

        let granted = results.iter().filter(|r| r.is_ok()).count() as u64;
        let left = actor.get("mug").await.unwrap().unwrap().units;

        assert_eq!(granted, 5);
        assert_eq!(left, 0);
        assert_eq!(granted * 2, 10 - left, "units handed out == units consumed");

        for result in &results {
            if let Err(err) = result {
                assert!(matches!(err, ReserveError::Insufficient { .. }));
            }
        }
    }

    /// Commands are handled strictly in arrival order, so later callers observe
    /// a queue — the same number pessimistic-service reports as `waited_ms`.
    #[tokio::test]
    async fn later_callers_wait_behind_the_queue() {
        let actor = StockActor::spawn(Duration::from_millis(30), 64);
        actor.seed("mug", 10).await.unwrap();

        let results = join_all((0..3).map(|_| actor.reserve("mug", 1))).await;
        let queued: Vec<u64> = results.iter().map(|r| r.as_ref().unwrap().queued_ms).collect();

        assert!(queued[0] < 45, "the first command is handled immediately");
        assert!(
            queued[2] >= 50,
            "the third waits behind two think windows, got {}ms",
            queued[2]
        );
    }

    /// The capability the other three services do not have: noticing that it is
    /// overloaded, and saying so, instead of growing a queue nobody can see.
    #[tokio::test]
    async fn a_full_mailbox_sheds_load_instead_of_absorbing_it() {
        let actor = StockActor::spawn(Duration::from_millis(30), 2);
        actor.seed("mug", 100).await.unwrap();

        let results = join_all((0..12).map(|_| actor.reserve("mug", 1))).await;

        let busy = results
            .iter()
            .filter(|r| matches!(r, Err(ReserveError::Busy)))
            .count();
        let granted = results.iter().filter(|r| r.is_ok()).count() as u64;
        let left = actor.get("mug").await.unwrap().unwrap().units;

        assert!(busy > 0, "a 2-slot mailbox cannot absorb 12 commands at once");
        assert_eq!(
            granted,
            100 - left,
            "shed requests must not have changed anything"
        );
    }

    #[tokio::test]
    async fn unknown_products_are_not_created_by_reserving_them() {
        let actor = actor();
        let err = actor.reserve("ghost", 1).await.unwrap_err();
        assert!(matches!(err, ReserveError::UnknownProduct(_)));
    }
}
