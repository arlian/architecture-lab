//! Orders' local read model of its neighbours.
//!
//! This is the crux of going event-driven: Orders can no longer ask Users or
//! Catalog a question over the network at request time — there is no
//! request/response channel to them any more, just a stream of events.
//! Instead it subscribes to what they publish (see projection.rs) and keeps
//! its own small, private copy of exactly the facts it needs: which user ids
//! exist, and what each product currently costs.
//!
//! "Eventually consistent" is not a slogan here, it's a visible trade:
//! immediately after a user registers there is a brief window where placing an
//! order for that user id would still be rejected, because the event hasn't
//! been delivered and applied yet. In the synchronous microservices version
//! that same check was always instantly fresh — at the cost of an HTTP call
//! that could fail outright, or block on a slow neighbour. Neither is free;
//! this lab exists to make you feel both trades.

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
