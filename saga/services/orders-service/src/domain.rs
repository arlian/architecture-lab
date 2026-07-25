//! Orders domain. Orders still defines its **own** `UserId` / `ProductId` — it
//! agrees with Users and Catalog only on a wire format (a UUID), never a
//! compile-time type. What's new versus event-driven: an `Order` now carries
//! a `status`, because placing one no longer finishes synchronously — it
//! kicks off a saga (see saga.rs) that this status tracks to completion.

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

/// Where an order is in the saga orders-service orchestrates across
/// inventory-service and payments-service. The order's own id doubles as the
/// saga id used to correlate saga commands/replies — see saga.rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    /// Placed; stock reservation has been requested.
    Pending,
    /// Stock reserved; payment has been requested.
    AwaitingPayment,
    /// Payment charged. Terminal.
    Confirmed,
    /// Payment failed; releasing the stock reserved in step one.
    Compensating,
    /// Terminal, for any of: stock unavailable, payment declined (after a
    /// successful compensation), or (not modeled here) a saga step that
    /// never replied.
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
