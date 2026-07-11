//! Catalog use cases.

use std::sync::Arc;

use shared::AppError;

use crate::domain::{Product, ProductId};
use crate::repository::ProductRepository;

pub struct CreateProduct {
    pub name: String,
    pub price_cents: u64,
}

pub struct ProductService {
    repo: Arc<dyn ProductRepository>,
}

impl ProductService {
    pub(crate) fn new(repo: Arc<dyn ProductRepository>) -> Self {
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
