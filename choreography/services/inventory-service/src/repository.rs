//! Inventory persistence — private, in-memory, owned by this service.
//!
//! Alongside stock counts, this keeps an idempotency ledger keyed by
//! `order_id`. The saga lab had one too, but this version has to be
//! strictly stronger, and the reason is worth understanding.
//!
//! In `saga/`, the two things that move stock — reserve and release —
//! arrived as commands from a single publisher, the orchestrator, which sent
//! the release only after it had seen the reservation succeed. The
//! orchestrator *serialized* them. Redelivery was the only disorder the
//! ledger had to survive.
//!
//! Here they arrive from two unrelated publishers on two unrelated subjects:
//! reserve is triggered by `orders.placed` (from orders-service) and release
//! by `payments.declined` (from payments-service). Nothing serializes them,
//! and a redelivered `orders.placed` can perfectly well land *after* the
//! release has already run. The saga lab's ledger dropped its entry on
//! release, which would let exactly that sequence silently re-reserve stock
//! that nobody will ever release again. So this ledger remembers
//! `AlreadyReleased` as a terminal state instead of forgetting.
//!
//! That extra care is not incidental. Removing the coordinator removed the
//! thing that was quietly ordering these messages for us.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::ProductId;

#[derive(Debug, Clone)]
pub(crate) enum ReservationOutcome {
    Reserved(Vec<(ProductId, u32)>),
    Rejected(String),
    /// Reserved once, then released as compensation. Terminal: a
    /// redelivered `orders.placed` for this order must not start it over.
    AlreadyReleased,
}

#[async_trait]
pub(crate) trait InventoryRepository: Send + Sync {
    async fn seed(&self, product_id: ProductId, initial_units: u32);
    async fn available(&self, product_id: ProductId) -> Option<u32>;

    /// Reserve `lines` for `order_id`, checking and committing all lines under
    /// one lock so a partial reservation can never be observed. If this
    /// order has been seen before, returns the outcome already recorded for
    /// it, unchanged, instead of touching stock a second time.
    async fn reserve(&self, order_id: Uuid, lines: Vec<(ProductId, u32)>) -> ReservationOutcome;

    /// Put back whatever was reserved for `order_id`. A no-op if nothing was
    /// ever reserved for it, if the reservation was rejected, or if it was
    /// already released.
    async fn release(&self, order_id: Uuid);
}

#[derive(Default)]
pub(crate) struct InMemoryInventoryRepository {
    stock: RwLock<HashMap<ProductId, u32>>,
    reservations: RwLock<HashMap<Uuid, ReservationOutcome>>,
}

#[async_trait]
impl InventoryRepository for InMemoryInventoryRepository {
    async fn seed(&self, product_id: ProductId, initial_units: u32) {
        self.stock.write().await.insert(product_id, initial_units);
    }

    async fn available(&self, product_id: ProductId) -> Option<u32> {
        self.stock.read().await.get(&product_id).copied()
    }

    async fn reserve(&self, order_id: Uuid, lines: Vec<(ProductId, u32)>) -> ReservationOutcome {
        // Hold the ledger for the whole check-and-commit. Two concurrent
        // deliveries of the same `orders.placed` are reactor tasks racing on
        // the same runtime; a read-then-write would let both through.
        let mut reservations = self.reservations.write().await;
        if let Some(existing) = reservations.get(&order_id) {
            return existing.clone();
        }

        let mut stock = self.stock.write().await;
        for (product_id, quantity) in &lines {
            let available = stock.get(product_id).copied().unwrap_or(0);
            if available < *quantity {
                let outcome = ReservationOutcome::Rejected(format!(
                    "only {available} unit(s) of {product_id} available, {quantity} requested"
                ));
                reservations.insert(order_id, outcome.clone());
                return outcome;
            }
        }

        for (product_id, quantity) in &lines {
            *stock.entry(*product_id).or_insert(0) -= *quantity;
        }

        let outcome = ReservationOutcome::Reserved(lines);
        reservations.insert(order_id, outcome.clone());
        outcome
    }

    async fn release(&self, order_id: Uuid) {
        // Same lock order as `reserve` (reservations, then stock) so the two
        // can't deadlock against each other.
        let mut reservations = self.reservations.write().await;
        let Some(ReservationOutcome::Reserved(lines)) = reservations.get(&order_id).cloned() else {
            return;
        };

        let mut stock = self.stock.write().await;
        for (product_id, quantity) in &lines {
            *stock.entry(*product_id).or_insert(0) += *quantity;
        }
        // Terminal, not forgotten — see the module docs.
        reservations.insert(order_id, ReservationOutcome::AlreadyReleased);
    }
}
