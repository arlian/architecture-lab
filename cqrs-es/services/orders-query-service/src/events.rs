//! Inbound event shapes this service subscribes to — its own copy, matching
//! what orders-command-service publishes (see its events.rs), never imported
//! from it. Note this service defines a `status` concept itself: it doesn't
//! receive an `OrderStatus` value over the wire, it derives one from *which*
//! event just arrived. That's a query-side read model choice, independent of
//! how the write side represents status internally.

use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct OrderPlacedLine {
    pub product_id: Uuid,
    pub quantity: u32,
    pub unit_price_cents: u64,
}

#[derive(Debug, Deserialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lines: Vec<OrderPlacedLine>,
    pub total_cents: u64,
}
impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}

#[derive(Debug, Deserialize)]
pub struct OrderPaid {
    pub id: Uuid,
}
impl OrderPaid {
    pub const SUBJECT: &'static str = "orders.paid";
}

#[derive(Debug, Deserialize)]
pub struct OrderShipped {
    pub id: Uuid,
}
impl OrderShipped {
    pub const SUBJECT: &'static str = "orders.shipped";
}

#[derive(Debug, Deserialize)]
pub struct OrderCancelled {
    pub id: Uuid,
}
impl OrderCancelled {
    pub const SUBJECT: &'static str = "orders.cancelled";
}
