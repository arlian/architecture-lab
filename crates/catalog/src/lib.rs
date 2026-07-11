//! # Catalog module
//!
//! Owns products and prices. Exposes only its `api` contract, the wiring type,
//! and the types that cross its boundary.

mod domain;
mod http;
mod repository;
mod service;

pub mod api;

pub use domain::{Product, ProductId};
pub use service::{CreateProduct, ProductService};

use std::sync::Arc;

use repository::InMemoryProductRepository;

pub struct CatalogModule {
    pub service: Arc<ProductService>,
}

impl CatalogModule {
    pub fn new() -> Self {
        let repo = Arc::new(InMemoryProductRepository::default());
        let service = Arc::new(ProductService::new(repo));
        Self { service }
    }

    pub fn router(&self) -> axum::Router {
        http::router(self.service.clone())
    }
}

impl Default for CatalogModule {
    fn default() -> Self {
        Self::new()
    }
}
