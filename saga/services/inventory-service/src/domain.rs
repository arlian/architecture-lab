//! Inventory domain: how many units of a product are available to sell.
//! Inventory-service is the sole owner of stock counts — orders-service
//! never decrements them directly, it only asks (via a saga command) to
//! reserve or release some quantity.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ProductId(pub Uuid);

impl std::fmt::Display for ProductId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
