//! Catalog use cases. Same rules as the microservices version, plus
//! publishing: `create` announces `ProductCreated`, and the new
//! `update_price` announces `ProductPriceChanged` — this is what lets Orders'
//! read model stay in sync with a price change it would otherwise never see.

use std::sync::Arc;

use crate::bus::EventBus;
use crate::domain::{Product, ProductId};
use crate::error::AppError;
use crate::events::{ProductCreated, ProductPriceChanged};
use crate::repository::ProductRepository;

pub struct CreateProduct {
    pub name: String,
    pub price_cents: u64,
}

pub struct ProductService {
    repo: Arc<dyn ProductRepository>,
    events: Arc<dyn EventBus>,
}

impl ProductService {
    pub fn new(repo: Arc<dyn ProductRepository>, events: Arc<dyn EventBus>) -> Self {
        Self { repo, events }
    }

    pub async fn create(&self, cmd: CreateProduct) -> Result<Product, AppError> {
        if cmd.name.trim().is_empty() {
            return Err(AppError::Validation("product name is required".into()));
        }
        if cmd.price_cents == 0 {
            return Err(AppError::Validation("price must be greater than zero".into()));
        }

        let product = Product {
            id: ProductId::new(),
            name: cmd.name,
            price_cents: cmd.price_cents,
        };
        let product = self.repo.insert(product).await;

        let event = ProductCreated {
            id: product.id.0,
            name: product.name.clone(),
            price_cents: product.price_cents,
        };
        let payload = serde_json::to_vec(&event).expect("ProductCreated is serializable");
        if let Err(e) = self.events.publish(ProductCreated::SUBJECT, payload).await {
            tracing::warn!("failed to publish ProductCreated for {}: {e}", product.id);
        }

        Ok(product)
    }

    pub async fn update_price(
        &self,
        id: ProductId,
        price_cents: u64,
    ) -> Result<Product, AppError> {
        if price_cents == 0 {
            return Err(AppError::Validation("price must be greater than zero".into()));
        }

        let product = self
            .repo
            .update_price(id, price_cents)
            .await
            .ok_or_else(|| AppError::NotFound(format!("product {id}")))?;

        let event = ProductPriceChanged {
            id: product.id.0,
            price_cents: product.price_cents,
        };
        let payload = serde_json::to_vec(&event).expect("ProductPriceChanged is serializable");
        if let Err(e) = self
            .events
            .publish(ProductPriceChanged::SUBJECT, payload)
            .await
        {
            tracing::warn!(
                "failed to publish ProductPriceChanged for {}: {e}",
                product.id
            );
        }

        Ok(product)
    }

    pub async fn get(&self, id: ProductId) -> Result<Product, AppError> {
        self.repo
            .get(id)
            .await
            .ok_or_else(|| AppError::NotFound(format!("product {id}")))
    }

    pub async fn list(&self) -> Vec<Product> {
        self.repo.all().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::Mutex;

    use crate::repository::InMemoryProductRepository;

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

    fn service() -> (ProductService, Arc<FakeBus>) {
        let bus = Arc::new(FakeBus::default());
        let svc = ProductService::new(Arc::new(InMemoryProductRepository::default()), bus.clone());
        (svc, bus)
    }

    #[tokio::test]
    async fn creates_and_publishes() {
        let (svc, bus) = service();
        let product = svc
            .create(CreateProduct {
                name: "Coffee Mug".into(),
                price_cents: 1299,
            })
            .await
            .unwrap();

        assert_eq!(product.price_cents, 1299);
        let published = bus.published.lock().await;
        assert_eq!(published.len(), 1);
        assert_eq!(published[0].0, ProductCreated::SUBJECT);
    }

    #[tokio::test]
    async fn updates_price_and_publishes_change() {
        let (svc, bus) = service();
        let product = svc
            .create(CreateProduct {
                name: "Notebook".into(),
                price_cents: 850,
            })
            .await
            .unwrap();

        let updated = svc.update_price(product.id, 900).await.unwrap();
        assert_eq!(updated.price_cents, 900);

        let published = bus.published.lock().await;
        assert_eq!(published.len(), 2);
        assert_eq!(published[1].0, ProductPriceChanged::SUBJECT);
    }
}
