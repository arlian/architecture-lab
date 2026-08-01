//! Wire shapes Orders consumes and produces.
//!
//! The important difference versus the saga lab: **nothing here is a
//! command.** In `saga/` this file had an "outbound: saga commands" section —
//! `ReserveStockRequested`, `ChargeRequested` — messages addressed to a
//! specific recipient that was expected to obey. Every message below is a
//! past-tense *fact* about something that already happened in the
//! publisher's own domain. Orders publishes one (`orders.placed`) and
//! subscribes to five it never asked anyone for.
//!
//! Note the correlation field is `order_id`, not `saga_id`. There is no saga
//! object in this lab for an id to belong to — there's just an order that
//! several services happen to have opinions about.
//!
//! For every inbound event Orders defines its **own** copy of the shape,
//! deserializing only the fields it actually needs — same rule as every
//! consumer in this lab.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Inbound: what Orders needs to know from Users and Catalog ---

#[derive(Debug, Deserialize)]
pub struct UserRegistered {
    pub id: Uuid,
}

impl UserRegistered {
    pub const SUBJECT: &'static str = "users.registered";
}

#[derive(Debug, Deserialize)]
pub struct ProductCreated {
    pub id: Uuid,
    pub price_cents: u64,
}

impl ProductCreated {
    pub const SUBJECT: &'static str = "catalog.product_created";
}

#[derive(Debug, Deserialize)]
pub struct ProductPriceChanged {
    pub id: Uuid,
    pub price_cents: u64,
}

impl ProductPriceChanged {
    pub const SUBJECT: &'static str = "catalog.product_price_changed";
}

// --- Outbound: the one fact Orders publishes ---

#[derive(Debug, Clone, Serialize)]
pub struct OrderPlacedLine {
    pub product_id: Uuid,
    pub quantity: u32,
}

/// "A customer placed this order." That's the whole message. It is not
/// addressed to anybody, and it does not ask for anything.
///
/// It carries `user_id`, `total_cents` *and* `lines` not because Orders knows
/// who needs each field, but because Orders is the only service that will
/// ever know them — and a consumer that discovers it needs the total has no
/// channel to go back and ask. That pressure is why choreographed events
/// grow fat: compare this to the saga lab, where `ReserveStockRequested`
/// carried only `lines` (all inventory needed) because the orchestrator sent
/// each participant a separate, precisely-shaped message.
#[derive(Debug, Clone, Serialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub total_cents: u64,
    pub lines: Vec<OrderPlacedLine>,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}

// --- Inbound: facts published by Inventory and Payments ---
//
// Orders subscribes to these only to keep its own status field up to date
// (tracker.rs). Nothing Orders does with them affects the workflow: by the
// time one arrives, the step it describes has already happened AND the next
// participant has already been triggered by that same broadcast. Orders is
// a spectator on its own order.

#[derive(Debug, Deserialize)]
pub struct StockReserved {
    pub order_id: Uuid,
}

impl StockReserved {
    pub const SUBJECT: &'static str = "inventory.stock_reserved";
}

#[derive(Debug, Deserialize)]
pub struct StockRejected {
    pub order_id: Uuid,
    pub reason: String,
}

impl StockRejected {
    pub const SUBJECT: &'static str = "inventory.stock_rejected";
}

#[derive(Debug, Deserialize)]
pub struct StockReleased {
    pub order_id: Uuid,
}

impl StockReleased {
    pub const SUBJECT: &'static str = "inventory.stock_released";
}

#[derive(Debug, Deserialize)]
pub struct PaymentCharged {
    pub order_id: Uuid,
}

impl PaymentCharged {
    pub const SUBJECT: &'static str = "payments.charged";
}

#[derive(Debug, Deserialize)]
pub struct PaymentDeclined {
    pub order_id: Uuid,
    pub reason: String,
}

impl PaymentDeclined {
    pub const SUBJECT: &'static str = "payments.declined";
}
