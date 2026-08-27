//! Orders domain — same shapes as every other lab in this repo, so you can
//! diff them. `Order`, `OrderLine`, and the three id newtypes are lifted
//! almost verbatim from `microservices/services/orders-service/src/domain.rs`.
//!
//! The interesting difference is [`DomainError`]. Compare it with that lab's
//! `AppError`:
//!
//! ```text
//! microservices AppError            hexagonal DomainError
//! -----------------------           ---------------------
//! NotFound   -> 404                 NotFound
//! Validation -> 400                 Validation
//! Conflict   -> 409                 Unavailable
//! Internal   -> 500
//! impl IntoResponse for AppError    (nothing)
//! ```
//!
//! Same information, minus the status codes and minus the `IntoResponse` impl.
//! An order isn't "a 404"; it's *missing*. Whether missing should be rendered
//! as `404`, as exit code 1, or as a red line in a terminal is a question only
//! something with a user interface can answer — see
//! `orders-app/src/http.rs` and `orders-app/src/console.rs`, which answer it
//! two different ways.

use serde::{Deserialize, Serialize};
use thiserror::Error;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub Uuid);

impl OrderId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderLine {
    pub product_id: ProductId,
    pub quantity: u32,
    /// Price captured at the time the order was placed.
    pub unit_price_cents: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: OrderId,
    pub user_id: UserId,
    pub lines: Vec<OrderLine>,
    pub total_cents: u64,
}

/// How things go wrong, in the language of orders.
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("{0} not found")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    /// Something the core depends on could not answer. The core knows a port
    /// failed; it deliberately does NOT know whether that was a dead socket,
    /// a locked file, or a full disk. Adapters translate their own failures
    /// into this variant on the way in.
    #[error("dependency unavailable: {0}")]
    Unavailable(String),
}
