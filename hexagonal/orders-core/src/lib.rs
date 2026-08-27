//! # orders-core — the hexagon
//!
//! Everything the business would still care about if HTTP, JSON, and files
//! had never been invented:
//!
//! * [`domain`]  — the vocabulary: orders, ids, and the ways an order can go
//!                 wrong.
//! * [`ports`]   — the holes in the wall. Traits describing what the core
//!                 *needs*, phrased entirely in domain terms.
//! * [`service`] — the use cases. The only place order rules live.
//!
//! There is no `main` here, and no way to add one usefully: this crate cannot
//! be started, only *driven* from outside. Look in `orders-app` for the
//! things that drive it.

pub mod domain;
pub mod ports;
pub mod service;

pub use domain::{DomainError, Order, OrderId, OrderLine, ProductId, UserId};
pub use ports::{OrderRepository, ProductCatalog, UserDirectory};
pub use service::{OrderService, PlaceOrder, PlaceOrderLine};
