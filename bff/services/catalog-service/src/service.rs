//! Catalog use cases.

use std::sync::Arc;

use crate::domain::{Product, ProductId};
use crate::error::AppError;
use crate::repository::ProductRepository;

pub struct CreateProduct {
    pub name: String,
    pub price_cents: u64,
}

pub struct ProductService {
    repo: Arc<dyn ProductRepository>,
}

impl ProductService {
    pub fn new(repo: Arc<dyn ProductRepository>) -> Self {
        Self { repo }
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
        Ok(self.repo.insert(product).await)
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

    use crate::repository::InMemoryProductRepository;

    fn service() -> ProductService {
        ProductService::new(Arc::new(InMemoryProductRepository::default()))
    }

    #[tokio::test]
    async fn creates_and_reads_back_a_product() {
        let svc = service();
        let product = svc
            .create(CreateProduct {
                name: "Coffee Mug".into(),
                price_cents: 1299,
            })
            .await
            .unwrap();

        let fetched = svc.get(product.id).await.unwrap();
        assert_eq!(fetched.price_cents, 1299);
    }

    #[tokio::test]
    async fn rejects_blank_name() {
        let svc = service();
        let err = svc
            .create(CreateProduct {
                name: "  ".into(),
                price_cents: 500,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn rejects_zero_price() {
        let svc = service();
        let err = svc
            .create(CreateProduct {
                name: "Free Sample".into(),
                price_cents: 0,
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn unknown_product_is_not_found() {
        let svc = service();
        let err = svc.get(ProductId::new()).await.unwrap_err();
        assert!(matches!(err, AppError::NotFound(_)));
    }
}
