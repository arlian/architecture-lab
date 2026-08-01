//! Wire shapes payments-service consumes and produces.
//!
//! Count the inbound events and compare with the saga lab. There,
//! payments-service consumed exactly two things: `users.registered` to open
//! a wallet, and one command, `payments.charge.requested`, which arrived
//! carrying `user_id` and `amount_cents` — everything needed to do the job,
//! assembled by the orchestrator and handed over in a single message.
//!
//! Here there is no such message, because there is nobody whose job it is to
//! assemble one. Payments-service reacts to `inventory.stock_reserved`, and
//! that fact is published by inventory-service, which knows nothing about
//! wallets or prices and could not tell us the amount if it wanted to. So
//! this service has to take the trigger from one publisher and the data from
//! another (`orders.placed`) and put them together itself.
//!
//! That is the hidden cost of deleting the orchestrator: the orchestrator
//! was not only sequencing steps, it was doing a **data join** on every
//! participant's behalf. See repository.rs.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Inbound: seed event from Users ---

#[derive(Debug, Deserialize)]
pub struct UserRegistered {
    pub id: Uuid,
}

impl UserRegistered {
    pub const SUBJECT: &'static str = "users.registered";
}

// --- Inbound: the data half of the join ---

/// Orders publishes `id`, `user_id`, `total_cents` and `lines`. Payments
/// reads the first three and ignores `lines` — it has no opinion about
/// products. Note that it must subscribe to this at all *only* to learn the
/// amount, and it must remember every placed order indefinitely against the
/// possibility that a reservation for it shows up later.
#[derive(Debug, Deserialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub total_cents: u64,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}

// --- Inbound: the trigger half of the join ---

/// Inventory saying stock went out the door for an order. Payments-service
/// has independently decided that this is the moment to charge — inventory
/// is not asking it to, and does not know it will.
#[derive(Debug, Deserialize)]
pub struct StockReserved {
    pub order_id: Uuid,
}

impl StockReserved {
    pub const SUBJECT: &'static str = "inventory.stock_reserved";
}

// --- Outbound: facts about this service's own domain ---

#[derive(Debug, Clone, Serialize)]
pub struct PaymentCharged {
    pub order_id: Uuid,
}

impl PaymentCharged {
    pub const SUBJECT: &'static str = "payments.charged";
}

/// Published when the wallet couldn't cover it. In the saga lab the
/// equivalent (`payments.charge.failed`) went to the orchestrator, which
/// decided what to do about it. This one goes nowhere in particular — but
/// inventory-service is listening, and will compensate itself. Payments has
/// no idea that happens.
#[derive(Debug, Clone, Serialize)]
pub struct PaymentDeclined {
    pub order_id: Uuid,
    pub reason: String,
}

impl PaymentDeclined {
    pub const SUBJECT: &'static str = "payments.declined";
}
