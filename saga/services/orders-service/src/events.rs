//! Wire shapes for events and saga messages Orders both consumes and
//! produces.
//!
//! For the read-model inbound events, Orders defines its **own** copy of the
//! shape, deserializing only the fields it actually needs — same rule as
//! every consumer in this lab. The saga commands/replies below are the new
//! part versus event-driven: Orders is the orchestrator, so it *sends*
//! commands to inventory-service/payments-service and *consumes* their
//! replies, correlated by `saga_id` (= the order's own id, see domain.rs).

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

// --- Outbound: saga commands to inventory-service and payments-service ---

#[derive(Debug, Clone, Serialize)]
pub struct ReserveStockLine {
    pub product_id: Uuid,
    pub quantity: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReserveStockRequested {
    pub saga_id: Uuid,
    pub lines: Vec<ReserveStockLine>,
}

impl ReserveStockRequested {
    pub const SUBJECT: &'static str = "inventory.reserve.requested";
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseStockRequested {
    pub saga_id: Uuid,
}

impl ReleaseStockRequested {
    pub const SUBJECT: &'static str = "inventory.release.requested";
}

#[derive(Debug, Clone, Serialize)]
pub struct ChargeRequested {
    pub saga_id: Uuid,
    pub user_id: Uuid,
    pub amount_cents: u64,
}

impl ChargeRequested {
    pub const SUBJECT: &'static str = "payments.charge.requested";
}

// --- Inbound: saga replies from inventory-service and payments-service ---

#[derive(Debug, Deserialize)]
pub struct StockReserved {
    pub saga_id: Uuid,
}

impl StockReserved {
    pub const SUBJECT: &'static str = "inventory.reserve.succeeded";
}

#[derive(Debug, Deserialize)]
pub struct StockReserveFailed {
    pub saga_id: Uuid,
    pub reason: String,
}

impl StockReserveFailed {
    pub const SUBJECT: &'static str = "inventory.reserve.failed";
}

#[derive(Debug, Deserialize)]
pub struct StockReleased {
    pub saga_id: Uuid,
}

impl StockReleased {
    pub const SUBJECT: &'static str = "inventory.release.succeeded";
}

#[derive(Debug, Deserialize)]
pub struct PaymentCharged {
    pub saga_id: Uuid,
}

impl PaymentCharged {
    pub const SUBJECT: &'static str = "payments.charge.succeeded";
}

#[derive(Debug, Deserialize)]
pub struct PaymentChargeFailed {
    pub saga_id: Uuid,
    pub reason: String,
}

impl PaymentChargeFailed {
    pub const SUBJECT: &'static str = "payments.charge.failed";
}

// --- Outbound: what Orders tells the rest of the system, saga-final ---

#[derive(Debug, Clone, Serialize)]
pub struct OrderConfirmed {
    pub id: Uuid,
    pub user_id: Uuid,
    pub total_cents: u64,
}

impl OrderConfirmed {
    pub const SUBJECT: &'static str = "orders.confirmed";
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderFailed {
    pub id: Uuid,
    pub user_id: Uuid,
    pub reason: String,
}

impl OrderFailed {
    pub const SUBJECT: &'static str = "orders.failed";
}
