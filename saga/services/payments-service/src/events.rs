//! Wire shapes payments-service consumes and produces.
//!
//! Consumed: Users' `UserRegistered` — just enough to learn a new user id
//! exists, used to open a starting wallet balance (own narrow copy of the
//! shape, same rule as every consumer in this lab). And the saga command the
//! orders-service orchestrator sends this service.
//!
//! Produced: the reply orders-service's saga reactor is waiting on.
//! `saga_id` is the order's own id — no separate saga-id type exists
//! anywhere in this lab.

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

// --- Inbound: saga command from the orders-service orchestrator ---

#[derive(Debug, Deserialize)]
pub struct ChargeRequested {
    pub saga_id: Uuid,
    pub user_id: Uuid,
    pub amount_cents: u64,
}

impl ChargeRequested {
    pub const SUBJECT: &'static str = "payments.charge.requested";
}

// --- Outbound: reply back to the orchestrator ---

#[derive(Debug, Clone, Serialize)]
pub struct PaymentCharged {
    pub saga_id: Uuid,
}

impl PaymentCharged {
    pub const SUBJECT: &'static str = "payments.charge.succeeded";
}

#[derive(Debug, Clone, Serialize)]
pub struct PaymentChargeFailed {
    pub saga_id: Uuid,
    pub reason: String,
}

impl PaymentChargeFailed {
    pub const SUBJECT: &'static str = "payments.charge.failed";
}
