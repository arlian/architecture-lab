//! # orders-app — everything outside the hexagon
//!
//! **Driven adapters** implement a port the core declared, so the core can
//! get work done without knowing who does it:
//!
//! * [`repository`] — two ways to store orders: in RAM, or in a JSON file.
//! * [`directory`]  — the stub users/catalog tables. In `microservices/`
//!                    these same two ports were HTTP clients.

pub mod directory;
pub mod repository;
