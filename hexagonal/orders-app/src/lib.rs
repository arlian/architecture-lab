//! # orders-app — everything outside the hexagon
//!
//! Two kinds of thing live here, and it's worth keeping them straight:
//!
//! **Driven adapters** implement a port the core declared, so the core can
//! get work done without knowing who does it:
//!
//! * [`repository`] — two ways to store orders: in RAM, or in a JSON file.
//! * [`directory`]  — the stub users/catalog tables. In `microservices/`
//!                    these same two ports were HTTP clients.
//!
//! **Driving adapters** call the core's use cases on behalf of some actor:
//!
//! * [`http`]    — an HTTP request is the actor. Axum lives here and nowhere
//!                 else.
//! * [`console`] — a person typing at a shell is the actor.
//!
//! Read `http.rs` and `console.rs` back to back. They have no code in common,
//! no shared abstraction, and they produce completely different output — yet
//! both are thin, because everything either one *decides* is presentational.
//! The rules they share, they share by calling the same `OrderService`.

pub mod console;
pub mod directory;
pub mod http;
pub mod repository;
