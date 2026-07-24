//! Events this service publishes outward, one per aggregate transition. These
//! are deliberately separate types from `aggregate::OrderEvent` — the internal
//! event store's shape is this service's own business, but the wire shape
//! published to NATS is a public contract that orders-query-service and
//! notifications-service each keep their own copy of (see their events.rs).

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct OrderPlacedLine {
    pub product_id: Uuid,
    pub quantity: u32,
    pub unit_price_cents: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub lines: Vec<OrderPlacedLine>,
    pub total_cents: u64,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderPaid {
    pub id: Uuid,
}

impl OrderPaid {
    pub const SUBJECT: &'static str = "orders.paid";
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderShipped {
    pub id: Uuid,
}

impl OrderShipped {
    pub const SUBJECT: &'static str = "orders.shipped";
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderCancelled {
    pub id: Uuid,
}

impl OrderCancelled {
    pub const SUBJECT: &'static str = "orders.cancelled";
}
