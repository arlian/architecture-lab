//! Orders' local read model of its neighbours — identical in spirit to the
//! event-driven lab's `orders-service/src/read_model.rs`. Placing an order
//! still needs "does this user exist?" and "what does this product cost?",
//! and those are still answered from a local projection built from
//! `users.registered` / `catalog.product_created` /
//! `catalog.product_price_changed`, not a live call. This has nothing to do
//! with the event-sourcing story in aggregate.rs — it's the same
//! choreography-consumer pattern as before, just feeding validation instead
//! of the write model itself.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Default)]
struct State {
    known_users: HashSet<Uuid>,
    product_prices: HashMap<Uuid, u64>,
}

#[derive(Default, Clone)]
pub struct ReadModel {
    state: Arc<RwLock<State>>,
}

impl ReadModel {
    pub async fn user_exists(&self, id: Uuid) -> bool {
        self.state.read().await.known_users.contains(&id)
    }

    pub async fn price_of(&self, id: Uuid) -> Option<u64> {
        self.state.read().await.product_prices.get(&id).copied()
    }

    pub async fn record_user(&self, id: Uuid) {
        self.state.write().await.known_users.insert(id);
    }

    pub async fn record_price(&self, id: Uuid, price_cents: u64) {
        self.state.write().await.product_prices.insert(id, price_cents);
    }
}
