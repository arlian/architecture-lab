//! Shared id types for this service. Orders still defines its own `UserId` /
//! `ProductId` newtypes rather than importing Users' or Catalog's — same rule
//! as the microservices and event-driven labs, services agree on a wire
//! format (a UUID), never a compile-time type.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
