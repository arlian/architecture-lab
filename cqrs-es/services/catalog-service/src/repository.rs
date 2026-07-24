//! Catalog persistence — private, in-memory. Owned by this service alone.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::domain::{Product, ProductId};

#[async_trait]
pub(crate) trait ProductRepository: Send + Sync {
    async fn insert(&self, product: Product) -> Product;
    async fn get(&self, id: ProductId) -> Option<Product>;
    async fn update_price(&self, id: ProductId, price_cents: u64) -> Option<Product>;
    async fn all(&self) -> Vec<Product>;
}

#[derive(Default)]
pub(crate) struct InMemoryProductRepository {
    inner: RwLock<HashMap<ProductId, Product>>,
}

#[async_trait]
impl ProductRepository for InMemoryProductRepository {
    async fn insert(&self, product: Product) -> Product {
        self.inner.write().await.insert(product.id, product.clone());
        product
    }

    async fn get(&self, id: ProductId) -> Option<Product> {
        self.inner.read().await.get(&id).cloned()
    }

    async fn update_price(&self, id: ProductId, price_cents: u64) -> Option<Product> {
        let mut guard = self.inner.write().await;
        let product = guard.get_mut(&id)?;
        product.price_cents = price_cents;
        Some(product.clone())
    }

    async fn all(&self) -> Vec<Product> {
        self.inner.read().await.values().cloned().collect()
    }
}
