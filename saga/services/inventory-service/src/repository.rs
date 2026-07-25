//! Inventory persistence — private, in-memory, owned by this service.
//!
//! Alongside stock counts, this keeps a small idempotency ledger keyed by
//! `saga_id`: a redelivered `inventory.reserve.requested` for a saga already
//! applied must not reserve the same units twice, and a redelivered release
//! must not double-credit stock back. That's the one piece of real saga
//! plumbing this lab adds beyond what event-driven's plain projections
//! needed.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::domain::ProductId;

#[derive(Debug, Clone)]
pub(crate) enum ReservationOutcome {
    Reserved(Vec<(ProductId, u32)>),
    Failed(String),
}

#[async_trait]
pub(crate) trait InventoryRepository: Send + Sync {
    async fn seed(&self, product_id: ProductId, initial_units: u32);
    async fn available(&self, product_id: ProductId) -> Option<u32>;

    /// Reserve `lines` for `saga_id`, checking and committing all lines under
    /// one lock so a partial reservation can never be observed. Returns the
    /// outcome already recorded for `saga_id` unchanged if this is a
    /// redelivery, instead of reserving a second time.
    async fn reserve(&self, saga_id: Uuid, lines: Vec<(ProductId, u32)>) -> ReservationOutcome;

    /// Release whatever was reserved for `saga_id`. A no-op if nothing was
    /// ever reserved for that saga id, or if it was already released —
    /// releasing is safe to redeliver.
    async fn release(&self, saga_id: Uuid);
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

    async fn reserve(&self, saga_id: Uuid, lines: Vec<(ProductId, u32)>) -> ReservationOutcome {
        if let Some(existing) = self.reservations.read().await.get(&saga_id) {
            return existing.clone();
        }

        let mut stock = self.stock.write().await;
        for (product_id, quantity) in &lines {
            let available = stock.get(product_id).copied().unwrap_or(0);
            if available < *quantity {
                let outcome = ReservationOutcome::Failed(format!(
                    "only {available} unit(s) of {product_id} available, {quantity} requested"
                ));
                self.reservations.write().await.insert(saga_id, outcome.clone());
                return outcome;
            }
        }

        for (product_id, quantity) in &lines {
            *stock.entry(*product_id).or_insert(0) -= *quantity;
        }

        let outcome = ReservationOutcome::Reserved(lines);
        self.reservations.write().await.insert(saga_id, outcome.clone());
        outcome
    }

    async fn release(&self, saga_id: Uuid) {
        let mut reservations = self.reservations.write().await;
        if let Some(ReservationOutcome::Reserved(lines)) = reservations.get(&saga_id) {
            let mut stock = self.stock.write().await;
            for (product_id, quantity) in lines {
                *stock.entry(*product_id).or_insert(0) += *quantity;
            }
            reservations.remove(&saga_id);
        }
    }
}
