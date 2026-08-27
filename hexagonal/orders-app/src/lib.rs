//! # orders-app — everything outside the hexagon
//!
//! **Driven adapters** implement a port the core declared, so the core can
//! get work done without knowing who does it:
//!
//! * [`repository`] — two ways to store orders: in RAM, or in a JSON file.

pub mod repository;
