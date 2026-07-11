//! # Users module
//!
//! A self-contained bounded context. Notice how little is `pub`:
//! - `api`          — the contract other modules may depend on
//! - `UsersModule`  — the bootstrap/wiring entry point for the composition root
//! - `UserId` / `User` / `CreateUser` — types that cross the boundary
//!
//! Everything else (`repository`, the concrete store, internal helpers) is
//! private to the crate. The compiler enforces this boundary for us.

mod domain;
mod http;
mod repository;
mod service;

pub mod api;

pub use domain::{User, UserId};
pub use service::{CreateUser, UserService};

use std::sync::Arc;

use repository::InMemoryUserRepository;

/// Owns the module's wiring. The composition root creates one of these, mounts
/// its [`router`](UsersModule::router), and hands its [`service`](UsersModule::service)
/// to any module that needs the Users public API.
pub struct UsersModule {
    pub service: Arc<UserService>,
}

impl UsersModule {
    pub fn new() -> Self {
        let repo = Arc::new(InMemoryUserRepository::default());
        let service = Arc::new(UserService::new(repo));
        Self { service }
    }

    /// This module's HTTP routes, ready to be merged into the app router.
    pub fn router(&self) -> axum::Router {
        http::router(self.service.clone())
    }
}

impl Default for UsersModule {
    fn default() -> Self {
        Self::new()
    }
}
