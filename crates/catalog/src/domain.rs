//! Catalog domain: products and their prices. Money is stored as integer cents
//! to avoid floating-point rounding bugs — a small but important domain rule.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProductId(pub Uuid);

impl ProductId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for ProductId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Product {
    pub id: ProductId,
    pub name: String,
    /// Price in the smallest currency unit (e.g. cents).
    pub price_cents: u64,
}
