//! Orders persistence — private, in-memory, owned by this service.

use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;

use crate::domain::{Order, OrderId, OrderStatus};

#[async_trait]
pub(crate) trait OrderRepository: Send + Sync {
    async fn insert(&self, order: Order) -> Order;
    async fn get(&self, id: OrderId) -> Option<Order>;
    async fn all(&self) -> Vec<Order>;

    /// Move `id` from `expected` to `new_status` (optionally recording a
    /// failure reason), but only if it is currently in `expected`. Returns
    /// the updated order, or `None` if the order doesn't exist or wasn't in
    /// `expected`.
    ///
    /// The saga lab had this exact guard, and there it was load-bearing: a
    /// duplicate reply that slipped past it would have caused the
    /// orchestrator to publish a step's command twice. Here it guards
    /// nothing but this service's own status field. A bug in tracker.rs
    /// produces a *wrong answer* from `GET /orders/:id`; it cannot
    /// double-charge a wallet, because tracker.rs never charges anything.
    /// Cheaper to get wrong — and correspondingly easier to leave wrong
    /// without noticing.
    async fn transition(
        &self,
        id: OrderId,
        expected: OrderStatus,
        new_status: OrderStatus,
        reason: Option<String>,
    ) -> Option<Order>;
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

    async fn transition(
        &self,
        id: OrderId,
        expected: OrderStatus,
        new_status: OrderStatus,
        reason: Option<String>,
    ) -> Option<Order> {
        let mut orders = self.inner.write().await;
        let order = orders.get_mut(&id)?;
        if order.status != expected {
            return None;
        }
        order.status = new_status;
        if reason.is_some() {
            order.failure_reason = reason;
        }
        Some(order.clone())
    }
}
