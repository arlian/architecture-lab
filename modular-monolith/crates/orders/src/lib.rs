//! # Orders module
//!
//! Depends on the Users and Catalog modules — but only on their public API
//! traits, which are injected at construction time. That inversion is what lets
//! the composition root decide the concrete wiring, and lets tests substitute
//! fakes.

mod domain;
mod http;
mod repository;
mod service;

pub use domain::{Order, OrderId, OrderLine};
pub use service::{OrderService, PlaceOrder, PlaceOrderLine};

use std::sync::Arc;

use catalog::api::ProductCatalog;
use users::api::UserDirectory;

use repository::InMemoryOrderRepository;

pub struct OrdersModule {
    pub service: Arc<OrderService>,
}

impl OrdersModule {
    /// The dependencies arrive as trait objects — Orders neither knows nor cares
    /// that they are really the Users and Catalog services.
    pub fn new(users: Arc<dyn UserDirectory>, catalog: Arc<dyn ProductCatalog>) -> Self {
        let repo = Arc::new(InMemoryOrderRepository::default());
        let service = Arc::new(OrderService::new(repo, users, catalog));
        Self { service }
    }

    pub fn router(&self) -> axum::Router {
        http::router(self.service.clone())
    }
}
