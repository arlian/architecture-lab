//! # orders-core — the hexagon
//!
//! Everything the business would still care about if HTTP, JSON, and files
//! had never been invented.
//!
//! There is no `main` here, and no way to add one usefully: this crate cannot
//! be started, only *driven* from outside.

pub mod domain;

pub use domain::{DomainError, Order, OrderId, OrderLine, ProductId, UserId};
