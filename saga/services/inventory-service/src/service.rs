//! Inventory use cases — this service's half of the saga conversation.
//! `reserve` and `release` are idempotent by `saga_id`, so a redelivered
//! command from the orchestrator (because it never saw a lost reply) can't
//! double-reserve or double-release. Both publish their own reply event;
//! orders-service's saga reactor is the only thing waiting on them.

use std::sync::Arc;
use uuid::Uuid;

use serde::Serialize;

use crate::bus::EventBus;
use crate::domain::ProductId;
use crate::events::{StockReleased, StockReserveFailed, StockReserved};
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

    pub async fn reserve(&self, saga_id: Uuid, lines: Vec<(ProductId, u32)>) {
        match self.repo.reserve(saga_id, lines).await {
            ReservationOutcome::Reserved(_) => {
                self.publish(StockReserved::SUBJECT, &StockReserved { saga_id })
                    .await;
            }
            ReservationOutcome::Failed(reason) => {
                self.publish(
                    StockReserveFailed::SUBJECT,
                    &StockReserveFailed { saga_id, reason },
                )
                .await;
            }
        }
    }

    pub async fn release(&self, saga_id: Uuid) {
        self.repo.release(saga_id).await;
        self.publish(StockReleased::SUBJECT, &StockReleased { saga_id })
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

    use crate::error::AppError;

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
    async fn fails_when_stock_insufficient() {
        let (svc, bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 2).await;

        svc.reserve(Uuid::new_v4(), vec![(product_id, 5)]).await;

        assert_eq!(svc.available(product_id).await, Some(2));
        let published = bus.published.lock().await;
        assert_eq!(published[0].0, StockReserveFailed::SUBJECT);
    }

    #[tokio::test]
    async fn reserve_is_idempotent_by_saga_id() {
        let (svc, _bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let saga_id = Uuid::new_v4();

        svc.reserve(saga_id, vec![(product_id, 3)]).await;
        svc.reserve(saga_id, vec![(product_id, 3)]).await; // redelivery

        assert_eq!(svc.available(product_id).await, Some(2));
    }

    #[tokio::test]
    async fn release_restores_reserved_stock() {
        let (svc, bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let saga_id = Uuid::new_v4();

        svc.reserve(saga_id, vec![(product_id, 3)]).await;
        svc.release(saga_id).await;

        assert_eq!(svc.available(product_id).await, Some(5));
        let published = bus.published.lock().await;
        assert_eq!(published[1].0, StockReleased::SUBJECT);
    }

    #[tokio::test]
    async fn release_is_idempotent() {
        let (svc, _bus) = service();
        let product_id = ProductId(Uuid::new_v4());
        svc.seed_product(product_id, 5).await;
        let saga_id = Uuid::new_v4();

        svc.reserve(saga_id, vec![(product_id, 3)]).await;
        svc.release(saga_id).await;
        svc.release(saga_id).await; // redelivery

        assert_eq!(svc.available(product_id).await, Some(5));
    }
}
