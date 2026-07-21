//! Orders domain. Orders still defines its **own** `UserId` / `ProductId` — it
//! agrees with Users and Catalog only on a wire format (a UUID), never a
//! compile-time type. Nothing here changed by going event-driven; what changed
//! is *how* Orders learns whether those ids are any good (see read_model.rs).

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

#[derive(Debug, Clone, Serialize)]
pub struct Order {
    pub id: OrderId,
    pub user_id: UserId,
    pub lines: Vec<OrderLine>,
    pub total_cents: u64,
}
