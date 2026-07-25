//! Wire shapes inventory-service consumes and produces.
//!
//! Consumed: Catalog's `ProductCreated` — just enough to learn a new product
//! id exists, used to seed a starting stock count (own narrow copy of the
//! shape, same rule as every consumer in this lab). And the two saga
//! commands the orders-service orchestrator sends this service.
//!
//! Produced: the replies orders-service's saga reactor is waiting on.
//! `saga_id` is the order's own id — no separate saga-id type exists
//! anywhere in this lab.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Inbound: seed event from Catalog ---

#[derive(Debug, Deserialize)]
pub struct ProductCreated {
    pub id: Uuid,
}

impl ProductCreated {
    pub const SUBJECT: &'static str = "catalog.product_created";
}

// --- Inbound: saga commands from the orders-service orchestrator ---

#[derive(Debug, Deserialize)]
pub struct ReserveStockLine {
    pub product_id: Uuid,
    pub quantity: u32,
}

#[derive(Debug, Deserialize)]
pub struct ReserveStockRequested {
    pub saga_id: Uuid,
    pub lines: Vec<ReserveStockLine>,
}

impl ReserveStockRequested {
    pub const SUBJECT: &'static str = "inventory.reserve.requested";
}

#[derive(Debug, Deserialize)]
pub struct ReleaseStockRequested {
    pub saga_id: Uuid,
}

impl ReleaseStockRequested {
    pub const SUBJECT: &'static str = "inventory.release.requested";
}

// --- Outbound: replies back to the orchestrator ---

#[derive(Debug, Clone, Serialize)]
pub struct StockReserved {
    pub saga_id: Uuid,
}

impl StockReserved {
    pub const SUBJECT: &'static str = "inventory.reserve.succeeded";
}

#[derive(Debug, Clone, Serialize)]
pub struct StockReserveFailed {
    pub saga_id: Uuid,
    pub reason: String,
}

impl StockReserveFailed {
    pub const SUBJECT: &'static str = "inventory.reserve.failed";
}

#[derive(Debug, Clone, Serialize)]
pub struct StockReleased {
    pub saga_id: Uuid,
}

impl StockReleased {
    pub const SUBJECT: &'static str = "inventory.release.succeeded";
}
