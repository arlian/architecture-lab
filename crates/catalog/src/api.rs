//! Catalog's public cross-module contract.
//!
//! Orders needs product prices to total an order, but it has no business knowing
//! the full `Product` shape or how the catalog is stored. So we expose the
//! narrowest possible capability: "give me the price of this product".

use async_trait::async_trait;

use crate::domain::ProductId;

#[async_trait]
pub trait ProductCatalog: Send + Sync {
    /// The price in cents, or `None` if the product does not exist.
    async fn price_of(&self, id: ProductId) -> Option<u64>;
}

#[async_trait]
impl ProductCatalog for crate::service::ProductService {
    async fn price_of(&self, id: ProductId) -> Option<u64> {
        self.get(id).await.ok().map(|p| p.price_cents)
    }
}
