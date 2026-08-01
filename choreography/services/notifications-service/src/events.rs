//! Notifications' own narrow copy of every shape it reads — same rule as
//! every other consumer in this lab.
//!
//! In `saga/` this file had exactly two structs, `OrderConfirmed` and
//! `OrderFailed`, both published by orders-service, both meaning precisely
//! what their names say. Notifications did no thinking: it was *told* an
//! order had succeeded or failed and it sent the matching email.
//!
//! Those two subjects do not exist in this lab. Nobody publishes them,
//! because publishing them would require a service willing to declare, on
//! behalf of the whole system, that an order is finished — and that service
//! is precisely the orchestrator we deleted.
//!
//! So notifications now subscribes to four facts about three other services'
//! internals, and works out success and failure for itself. See registry.rs.

use serde::Deserialize;
use uuid::Uuid;

/// The only source of a customer's identity and order total. Notifications
/// has to keep its own copy of every order for the same reason
/// payments-service does: the terminal facts below carry an `order_id` and
/// nothing else, and there is nobody to ask.
#[derive(Debug, Deserialize)]
pub struct OrderPlaced {
    pub id: Uuid,
    pub user_id: Uuid,
    pub total_cents: u64,
}

impl OrderPlaced {
    pub const SUBJECT: &'static str = "orders.placed";
}

/// Payments debited a wallet. Notifications reads this as "the order went
/// through" — an inference, not a statement anyone made.
#[derive(Debug, Deserialize)]
pub struct PaymentCharged {
    pub order_id: Uuid,
}

impl PaymentCharged {
    pub const SUBJECT: &'static str = "payments.charged";
}

/// Inventory couldn't cover the order. Nothing was reserved, so this is the
/// end of the line.
#[derive(Debug, Deserialize)]
pub struct StockRejected {
    pub order_id: Uuid,
    pub reason: String,
}

impl StockRejected {
    pub const SUBJECT: &'static str = "inventory.stock_rejected";
}

/// Payments couldn't cover the order. Notifications reads this as "the order
/// is dead" and emails the customer immediately.
///
/// orders-service reads the *same* fact as "compensation is starting" and
/// keeps the order in `compensating` until it also sees
/// `inventory.stock_released`. Both services are looking at one event
/// stream and disagreeing about when the order is over. See registry.rs.
#[derive(Debug, Deserialize)]
pub struct PaymentDeclined {
    pub order_id: Uuid,
    pub reason: String,
}

impl PaymentDeclined {
    pub const SUBJECT: &'static str = "payments.declined";
}
