//! The event store: an append-only log per aggregate id. This is the entire
//! persistence model for orders in this lab — there is no `HashMap<OrderId,
//! Order>` anywhere. `load` replays history; `append` extends it. Nothing
//! else is possible, which is the point: you cannot "just update a field."
//!
//! What's missing on purpose, and called out in the README as an exercise:
//! optimistic concurrency. `load` and `append` are two separate lock
//! acquisitions, so two concurrent commands against the same order could both
//! load the same history, both decide their command is valid, and both
//! append — silently losing one of the two transitions. A real event store
//! makes `append` take an `expected_version` and reject the write if the
//! stream has moved on since you read it.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::RwLock;

use crate::aggregate::OrderEvent;
use crate::domain::OrderId;

#[derive(Default, Clone)]
pub struct EventStore {
    streams: Arc<RwLock<HashMap<OrderId, Vec<OrderEvent>>>>,
}

impl EventStore {
    pub async fn load(&self, id: OrderId) -> Vec<OrderEvent> {
        self.streams.read().await.get(&id).cloned().unwrap_or_default()
    }

    pub async fn append(&self, id: OrderId, event: OrderEvent) {
        self.streams.write().await.entry(id).or_default().push(event);
    }
}
