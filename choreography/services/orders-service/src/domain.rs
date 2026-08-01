//! Orders domain. Orders still defines its **own** `UserId` / `ProductId` — it
//! agrees with Users and Catalog only on a wire format (a UUID), never a
//! compile-time type.
//!
//! `OrderStatus` is carried over from the saga lab unchanged — same five
//! states, same transitions. What changed is its *authority*. In `saga/`
//! this enum was the orchestrator's control state: reaching
//! `AwaitingPayment` is what caused a charge to be requested. Here nothing
//! reads it but `GET /orders/:id`. Inventory and Payments act on the raw
//! facts on the bus, not on anything Orders records — so this field is now a
//! best-effort *report* of a workflow Orders does not run.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UserId(pub Uuid);

impl std::fmt::Display for UserId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProductId(pub Uuid);

impl std::fmt::Display for ProductId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct OrderId(pub Uuid);

impl OrderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OrderLine {
    pub product_id: ProductId,
    pub quantity: u32,
    /// Price captured at the time the order was placed.
    pub unit_price_cents: u64,
}

/// How far along an order appears to be, as far as orders-service can tell
/// from the facts it has heard. Every variant below is *inferred* — see
/// tracker.rs, and the module docs above on why that matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    /// Placed and announced. Nobody has told us anything since.
    Pending,
    /// Someone (inventory-service, though we only know it by subject) says
    /// stock is reserved.
    AwaitingPayment,
    /// Someone says the wallet was charged. Terminal.
    Confirmed,
    /// Payment was declined and we are waiting to hear that the reserved
    /// stock got released. Note that unlike the saga lab, entering this
    /// state does not *cause* the release — inventory-service heard the same
    /// `payments.declined` we did and is already doing it.
    Compensating,
    /// Terminal, for either: stock unavailable, or payment declined and the
    /// release confirmed.
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct Order {
    pub id: OrderId,
    pub user_id: UserId,
    pub lines: Vec<OrderLine>,
    pub total_cents: u64,
    pub status: OrderStatus,
    pub failure_reason: Option<String>,
}
