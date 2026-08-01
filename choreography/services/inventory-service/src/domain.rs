//! Inventory domain: how many units of a product are available to sell.
//! Inventory-service is the sole owner of stock counts — nobody else
//! decrements them, and in this lab nobody else even asks. Stock moves
//! because this service noticed something happen and decided it should.

use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct ProductId(pub Uuid);

impl std::fmt::Display for ProductId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
