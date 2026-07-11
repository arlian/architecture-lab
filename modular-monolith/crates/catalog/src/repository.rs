//! Catalog persistence. `pub(crate)` — the storage detail never leaves the crate.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::domain::{Product, ProductId};

#[async_trait]
pub(crate) trait ProductRepository: Send + Sync {
    async fn insert(&self, product: Product) -> Product;
    async fn get(&self, id: ProductId) -> Option<Product>;
    async fn all(&self) -> Vec<Product>;
}

#[derive(Default)]
pub(crate) struct InMemoryProductRepository {
    inner: RwLock<HashMap<ProductId, Product>>,
}

#[async_trait]
impl ProductRepository for InMemoryProductRepository {
    async fn insert(&self, product: Product) -> Product {
        self.inner
            .write()
            .await
            .insert(product.id, product.clone());
        product
    }

    async fn get(&self, id: ProductId) -> Option<Product> {
        self.inner.read().await.get(&id).cloned()
    }

    async fn all(&self) -> Vec<Product> {
        self.inner.read().await.values().cloned().collect()
    }
}
