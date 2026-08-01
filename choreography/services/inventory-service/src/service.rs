//! Inventory use cases.
//!
//! Same two operations as the saga lab — reserve, release — with the same
//! idempotency guarantees. What changed is the *authority* behind each call.
//! There, `reserve` ran because the orchestrator instructed this service to
//! run it. Here it runs because inventory-service decided, on its own, that
//! a placed order is a thing worth reserving stock for.
//!
//! And `release` is the sharper one. In `saga/` this service could not
//! possibly have known that a declined payment should undo a reservation —
//! it had never heard of payments. It released stock because it was told to.
//! Now the rule "a declined payment means put the stock back" is a business
//! decision encoded *here*, in inventory-service, in a service that owns
//! neither payments nor orders.

use std::sync::Arc;
use uuid::Uuid;

use serde::Serialize;

use crate::bus::EventBus;
use crate::domain::ProductId;
use crate::events::{StockRejected, StockReleased, StockReserved};
use crate::repository::{InventoryRepository, ReservationOutcome};

pub struct InventoryService {
    repo: Arc<dyn InventoryRepository>,
    events: Arc<dyn EventBus>,
}

impl InventoryService {
    pub fn new(repo: Arc<dyn InventoryRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repo, events }
    }

    pub async fn seed_product(&self, product_id: ProductId, initial_units: u32) {
        self.repo.seed(product_id, initial_units).await;
    }

    pub async fn available(&self, product_id: ProductId) -> Option<u32> {
        self.repo.available(product_id).await
    }

    /// React to an order being placed by reserving its lines, then say what
    /// happened. Re-announcing an outcome for an order already handled is
    /// deliberate and safe: these are facts, and every consumer of them
    /// guards its own state. Silence would be worse — a consumer that missed
    /// the first announcement has no way to ask for it again.
    pub async fn reserve(&self, order_id: Uuid, lines: Vec<(ProductId, u32)>) {
        match self.repo.reserve(order_id, lines).await {
            ReservationOutcome::Reserved(_) => {
                self.publish(StockReserved::SUBJECT, &StockReserved { order_id })
                    .await;
            }
            ReservationOutcome::Rejected(reason) => {
                self.publish(StockRejected::SUBJECT, &StockRejected { order_id, reason })
                    .await;
            }
            ReservationOutcome::AlreadyReleased => {
                // A redelivered `orders.placed` for an order we already
                // reserved *and* compensated. Re-reserving would leak stock
                // forever; announcing `stock_reserved` would walk
                // orders-service's tracker backwards out of a terminal
                // state. Do neither.
                tracing::warn!(
                    "ignoring orders.placed for order {order_id}: already reserved and released"
                );
            }
        }
    }

    /// Compensate: put back whatever this order had reserved.
    pub async fn release(&self, order_id: Uuid) {
        self.repo.release(order_id).await;
        self.publish(StockReleased::SUBJECT, &StockReleased { order_id })
            .await;
    }

    async fn publish<T: Serialize>(&self, subject: &'static str, event: &T) {
        let payload = serde_json::to_vec(event).expect("event is serializable");
        if let Err(e) = self.events.publish(subject, payload).await {
            tracing::warn!("failed to publish {subject}: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    use crate::error::AppError;
    use crate::repository::InMemoryInventoryRepository;

    #[derive(Default)]
    struct FakeBus {
        published: Mutex<Vec<(&'static str, Vec<u8>)>>,
    }

    #[async_trait::async_trait]
    impl EventBus for FakeBus {
        async fn publish(&self, subject: &'static str, payload: Vec<u8>) -> Result<(), AppError> {
            self.published.lock().await.push((subject, payload));
            Ok(())
        }
    }

    fn service() -> (InventoryService, Arc<FakeBus>) {
        let bus = Arc::new(FakeBus::default());
        let svc = InventoryService::new(
            Arc::new(InMemoryInventoryRepository::default()),
            bus.clone(),
        );
        (svc, bus)
    }

    #[tokio::test]
    async fn reserves_when_stock_available() {
        let (svc, bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;

        svc.reserve(Uuid::new_v4(), vec![(product_id, 3)]).await;

        assert_eq!(svc.available(product_id).await, Some(2));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, StockReserved::SUBJECT);
    }

    #[tokio::test]
    async fn rejects_when_stock_insufficient() {
        let (svc, bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 2).await;

        svc.reserve(Uuid::new_v4(), vec![(product_id, 5)]).await;

        assert_eq!(svc.available(product_id).await, Some(2));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, StockRejected::SUBJECT);
    }

    #[tokio::test]
    async fn reserve_is_idempotent_by_order_id() {
        let (svc, _bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let order_id = Uuid::new_v4();

        svc.reserve(order_id, vec![(product_id, 3)]).await;
        svc.reserve(order_id, vec![(product_id, 3)]).await; // redelivery

        assert_eq!(svc.available(product_id).await, Some(2));
    }

    #[tokio::test]
    async fn release_restores_reserved_stock() {
        let (svc, bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let order_id = Uuid::new_v4();

        svc.reserve(order_id, vec![(product_id, 3)]).await;
        svc.release(order_id).await;

        assert_eq!(svc.available(product_id).await, Some(5));
        let published = bus.published.lock().await;
        assert_eq!(published[1].0, StockReleased::SUBJECT);
    }

    #[tokio::test]
    async fn release_is_idempotent() {
        let (svc, _bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let order_id = Uuid::new_v4();

        svc.reserve(order_id, vec![(product_id, 3)]).await;
        svc.release(order_id).await;
        svc.release(order_id).await; // redelivery

        assert_eq!(svc.available(product_id).await, Some(5));
    }

    /// The interleaving the orchestrator used to make impossible. `reserve`
    /// is triggered by `orders.placed` and `release` by `payments.declined`
    /// — two publishers, no ordering between them — so a redelivered
    /// `orders.placed` really can arrive after the compensation has run.
    /// Without the `AlreadyReleased` terminal state this silently reserves
    /// three units that nothing will ever release.
    #[tokio::test]
    async fn a_redelivered_placement_after_compensation_does_not_re_reserve() {
        let (svc, bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let order_id = Uuid::new_v4();

        svc.reserve(order_id, vec![(product_id, 3)]).await;
        svc.release(order_id).await;
        svc.reserve(order_id, vec![(product_id, 3)]).await; // late redelivery

        assert_eq!(svc.available(product_id).await, Some(5));
        // ...and it stayed quiet rather than announcing a reservation that
        // would drag orders-service's tracker back out of `failed`.
        let published = bus.published.lock().await;
        assert_eq!(published.len(), 2);
        assert_eq!(published[0].0, StockReserved::SUBJECT);
        assert_eq!(published[1].0, StockReleased::SUBJECT);
    }
}
