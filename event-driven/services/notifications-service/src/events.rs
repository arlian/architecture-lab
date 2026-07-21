//! Notifications only cares about a handful of fields from `orders.placed` —
//! its own copy of the shape, same rule as every other consumer in this lab
//! (see orders-service/src/events.rs, which defines the producer's side).

use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub total_cents: u64,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}
