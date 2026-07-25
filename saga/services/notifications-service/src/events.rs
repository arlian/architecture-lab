//! Notifications only cares about a handful of fields from the two terminal
//! saga events — its own copy of each shape, same rule as every other
//! consumer in this lab (see orders-service/src/events.rs, which defines the
//! producer's side).

use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct OrderConfirmed {
    pub id: Uuid,
    pub user_id: Uuid,
    pub total_cents: u64,
}

impl OrderConfirmed {
    pub const SUBJECT: &'static str = "orders.confirmed";
}

#[derive(Debug, Deserialize)]
pub struct OrderFailed {
    pub id: Uuid,
    pub user_id: Uuid,
    pub reason: String,
}

impl OrderFailed {
    pub const SUBJECT: &'static str = "orders.failed";
}
