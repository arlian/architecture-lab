//! Orders domain.
//!
//! Note how it references `UserId` and `ProductId` from the other modules. These
//! ids are the natural "foreign keys" between bounded contexts — but Orders
//! stores only the id, never a copy of the User or Product. To learn anything
//! else about them, it must go through those modules' public APIs.

use serde::Serialize;
use uuid::Uuid;

use catalog::ProductId;
use users::UserId;

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
