//! Payments persistence — private, in-memory, owned by this service.
//!
//! Three things live here, and only the first two existed in the saga lab:
//!
//! 1. **Wallet balances.** This service's actual domain.
//! 2. **An idempotency ledger** keyed by `order_id`, so a redelivered
//!    trigger can't charge a wallet twice.
//! 3. **A join buffer** — new, and the interesting one.
//!
//! ## Why the join buffer exists
//!
//! Charging needs two facts from two different publishers: `orders.placed`
//! (for the user and the amount) and `inventory.stock_reserved` (the signal
//! that it's time). Causally the placement always happens first — inventory
//! only reserves *because* it saw the placement — so it is tempting to
//! assume this service will observe them in that order and just look the
//! order up when the reservation arrives.
//!
//! That assumption is wrong, and cheaply so. The two subjects are consumed
//! by two independent tokio tasks. Even though NATS hands this client the
//! messages in order on one connection, nothing makes the *task* that reads
//! `stock_reserved` run after the task that reads `orders.placed`. Lose that
//! race and the lookup misses, the charge silently never happens, and the
//! order sits in `awaiting_payment` forever with the stock already gone.
//!
//! So instead of looking up, this service *joins*: whichever half arrives
//! second completes the pair and returns the order to charge. It is a
//! textbook streaming join, and every participant in a choreography that
//! needs data it doesn't own ends up writing one.
//!
//! Worth noting what this costs. `placed` grows without bound — this service
//! must remember every order forever, because a reservation for any of them
//! might arrive at any time and there is no signal that says "this one is
//! finished, you may forget it." The orchestrator in `saga/` had no such
//! table: it held the order details itself and handed them over inside the
//! command.

use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::UserId;

#[derive(Debug, Clone)]
pub(crate) enum ChargeOutcome {
    Charged(u64),
    Declined(String),
}

/// The half of an order this service cares about, learned from
/// `orders.placed`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlacedOrder {
    pub user_id: UserId,
    pub amount_cents: u64,
}

#[async_trait]
pub(crate) trait PaymentsRepository: Send + Sync {
    async fn open_wallet(&self, user_id: UserId, starting_balance_cents: u64);
    async fn balance(&self, user_id: UserId) -> Option<u64>;

    /// Record the data half of the join. Returns `Some` if a reservation for
    /// this order was already parked waiting for it — meaning the pair is now
    /// complete and the caller should charge.
    async fn join_placed(&self, order_id: Uuid, order: PlacedOrder) -> Option<PlacedOrder>;

    /// Record the trigger half of the join. Returns `Some` if this service
    /// already knows the order's details — meaning the pair is complete and
    /// the caller should charge. Otherwise the reservation is parked until
    /// `orders.placed` shows up.
    async fn join_reserved(&self, order_id: Uuid) -> Option<PlacedOrder>;

    /// Charge `amount_cents` from `user_id`'s wallet for `order_id`. Returns
    /// the outcome already recorded for `order_id` unchanged if this is a
    /// repeat, instead of charging a second time.
    async fn charge(&self, order_id: Uuid, user_id: UserId, amount_cents: u64) -> ChargeOutcome;
}

#[derive(Default)]
struct JoinState {
    /// Every order this service has ever heard about. See the note on
    /// unbounded growth above.
    placed: HashMap<Uuid, PlacedOrder>,
    /// Reservations that arrived before we knew what the order was.
    reserved_awaiting_details: HashSet<Uuid>,
}

#[derive(Default)]
pub(crate) struct InMemoryPaymentsRepository {
    wallets: RwLock<HashMap<UserId, u64>>,
    charges: RwLock<HashMap<Uuid, ChargeOutcome>>,
    join: RwLock<JoinState>,
}

#[async_trait]
impl PaymentsRepository for InMemoryPaymentsRepository {
    async fn open_wallet(&self, user_id: UserId, starting_balance_cents: u64) {
        self.wallets
            .write()
            .await
            .insert(user_id, starting_balance_cents);
    }

    async fn balance(&self, user_id: UserId) -> Option<u64> {
        self.wallets.read().await.get(&user_id).copied()
    }

    async fn join_placed(&self, order_id: Uuid, order: PlacedOrder) -> Option<PlacedOrder> {
        let mut join = self.join.write().await;
        join.placed.insert(order_id, order);
        // Take, don't peek: if a reservation was parked, this consumes it so
        // a redelivered `orders.placed` doesn't fire a second charge. (The
        // charge is idempotent anyway — this just keeps it from getting that
        // far.)
        join.reserved_awaiting_details
            .remove(&order_id)
            .then_some(order)
    }

    async fn join_reserved(&self, order_id: Uuid) -> Option<PlacedOrder> {
        let mut join = self.join.write().await;
        // Bind before matching so the borrow of `join.placed` is definitely
        // released before the arm that mutates `join`.
        let known = join.placed.get(&order_id).copied();
        match known {
            Some(order) => Some(order),
            None => {
                join.reserved_awaiting_details.insert(order_id);
                None
            }
        }
    }

    async fn charge(&self, order_id: Uuid, user_id: UserId, amount_cents: u64) -> ChargeOutcome {
        // Check-and-commit under one lock, same reasoning as
        // inventory-service's reservation ledger: concurrent deliveries are
        // separate tasks on the same runtime.
        let mut charges = self.charges.write().await;
        if let Some(existing) = charges.get(&order_id) {
            return existing.clone();
        }

        let mut wallets = self.wallets.write().await;
        let balance = wallets.get(&user_id).copied().unwrap_or(0);
        let outcome = if balance >= amount_cents {
            wallets.insert(user_id, balance - amount_cents);
            ChargeOutcome::Charged(amount_cents)
        } else {
            ChargeOutcome::Declined(format!(
                "wallet {user_id} has {balance}c, {amount_cents}c requested"
            ))
        };

        charges.insert(order_id, outcome.clone());
        outcome
    }
}
