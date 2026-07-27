//! Orders persistence — private, in-memory, owned by this service.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::domain::{Order, OrderId};

#[async_trait]
pub(crate) trait OrderRepository: Send + Sync {
    async fn insert(&self, order: Order) -> Order;
    async fn get(&self, id: OrderId) -> Option<Order>;
    async fn all(&self) -> Vec<Order>;
}

#[derive(Default)]
pub(crate) struct InMemoryOrderRepository {
    inner: RwLock<HashMap<OrderId, Order>>,
}

#[async_trait]
impl OrderRepository for InMemoryOrderRepository {
    async fn insert(&self, order: Order) -> Order {
        self.inner.write().await.insert(order.id, order.clone());
        order
    }

    async fn get(&self, id: OrderId) -> Option<Order> {
        self.inner.read().await.get(&id).cloned()
    }

    async fn all(&self) -> Vec<Order> {
        self.inner.read().await.values().cloned().collect()
    }
}
