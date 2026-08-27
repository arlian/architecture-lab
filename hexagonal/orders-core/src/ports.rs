//! Ports — the holes in the hexagon's wall.
//!
//! These are the *driven* (a.k.a. secondary, right-hand) ports: things the
//! core needs done for it. Each one is phrased entirely in domain vocabulary
//! — `UserId`, `ProductId`, `Order` — and says nothing about how the work
//! gets done. `UserDirectory` does not mention HTTP; `OrderRepository` does
//! not mention SQL or files. That restraint is the whole discipline: the core
//! specifies what it needs, and the outside world is obliged to speak the
//! core's language, not the other way round.
//!
//! Two of these traits already appeared in earlier labs, which is the point
//! worth pausing on. `UserDirectory` and `ProductCatalog` are the same two
//! traits as `microservices/services/orders-service/src/clients.rs` — where
//! they were backed by `reqwest` — and the same seam the modular monolith
//! filled with a direct in-process call. Hexagonal architecture is mostly the
//! act of taking that seam seriously and applying it to *everything*,
//! including persistence.
//!
//! The *driving* (primary, left-hand) port is on the other side of the wall:
//! it's the public API of [`crate::service::OrderService`]. See that module
//! for why it's a struct here and not a trait.

use async_trait::async_trait;

use crate::domain::{DomainError, Order, OrderId, ProductId, UserId};

/// Where orders live. Note that every method returns `Result`.
///
/// In `microservices/` this same trait was infallible, because the only
/// implementation was a `HashMap` behind a lock and a `HashMap` cannot fail.
/// That was the implementation dictating the port's shape. Here the port is
/// written for the *general* case — anything that stores bytes somewhere can
/// fail — so a file, a database, or a remote service can all be plugged in
/// later without the core changing.
#[async_trait]
pub trait OrderRepository: Send + Sync {
    async fn insert(&self, order: Order) -> Result<Order, DomainError>;
    async fn get(&self, id: OrderId) -> Result<Option<Order>, DomainError>;
    async fn all(&self) -> Result<Vec<Order>, DomainError>;
}

/// "Does this user exist?" — the only thing placing an order needs to know
/// about users.
#[async_trait]
pub trait UserDirectory: Send + Sync {
    async fn exists(&self, id: UserId) -> Result<bool, DomainError>;
}

/// "What does this product cost?" — the only thing placing an order needs to
/// know about products. `Ok(None)` means the product is unknown, which is a
/// business answer; `Err` means nobody could be asked, which is not.
#[async_trait]
pub trait ProductCatalog: Send + Sync {
    async fn price_of(&self, id: ProductId) -> Result<Option<u64>, DomainError>;
}
