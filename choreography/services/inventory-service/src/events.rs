//! Wire shapes inventory-service consumes and produces.
//!
//! Look at what this service now subscribes to and compare it with the saga
//! lab's version of this file.
//!
//! There, inventory-service's inbound section was two *commands* from the
//! orchestrator (`inventory.reserve.requested`, `inventory.release.requested`)
//! plus a seed event. It had no idea that orders, payments or wallets
//! existed. It was told what to do and it did it.
//!
//! Here, its inbound section names `orders.placed` and `payments.declined`.
//! Inventory-service has to know that placing an order is the thing that
//! should trigger a reservation, and that a declined payment is the thing
//! that should trigger a release. That knowledge used to live in exactly one
//! file (`saga.rs`); a slice of it now lives here, permanently.
//!
//! This is the trade at the heart of the lab and it is easy to state
//! backwards. Choreography does not *remove* coupling. It removes the
//! **hub** — and redistributes the hub's knowledge into every spoke. See the
//! coupling table in the README.

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

// --- Inbound: facts this service has decided to care about ---

#[derive(Debug, Deserialize)]
pub struct OrderPlacedLine {
    pub product_id: Uuid,
    pub quantity: u32,
}

/// Orders publishes this with `id`, `user_id`, `total_cents` and `lines`.
/// Inventory reads `id` and `lines` and ignores the rest — the narrow-copy
/// rule every consumer in this lab follows.
#[derive(Debug, Deserialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub lines: Vec<OrderPlacedLine>,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}

/// The compensation trigger. In `saga/` this arrived as
/// `inventory.release.requested`, a command from the orchestrator that had
/// already decided a release was warranted. Now it's payments-service
/// stating what happened in *its* domain — "I declined this" — and
/// inventory-service is the one drawing the conclusion that its own
/// reservation should therefore be undone.
///
/// Note what this means: **a participant now compensates itself, on the
/// strength of another participant's failure.** Nobody supervises the
/// rollback.
#[derive(Debug, Deserialize)]
pub struct PaymentDeclined {
    pub order_id: Uuid,
}

impl PaymentDeclined {
    pub const SUBJECT: &'static str = "payments.declined";
}

// --- Outbound: facts about this service's own domain ---
//
// These are not replies. In the saga lab the equivalents were
// `inventory.reserve.succeeded` / `.failed` — the names are phrased from the
// orchestrator's point of view, as answers to a question it asked. Nobody
// asked for these, and they are named for what happened to the stock.

#[derive(Debug, Clone, Serialize)]
pub struct StockReserved {
    pub order_id: Uuid,
}

impl StockReserved {
    pub const SUBJECT: &'static str = "inventory.stock_reserved";
}

#[derive(Debug, Clone, Serialize)]
pub struct StockRejected {
    pub order_id: Uuid,
    pub reason: String,
}

impl StockRejected {
    pub const SUBJECT: &'static str = "inventory.stock_rejected";
}

#[derive(Debug, Clone, Serialize)]
pub struct StockReleased {
    pub order_id: Uuid,
}

impl StockReleased {
    pub const SUBJECT: &'static str = "inventory.stock_released";
}
