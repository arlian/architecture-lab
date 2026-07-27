//! # Users service — an independent deployable
//!
//! There is no composition root wiring modules together here. This binary is the
//! whole thing: it builds its own repository + service, exposes its HTTP API, and
//! runs on its own port. Deploy it, scale it, and release it on its own schedule.

mod domain;
mod error;
mod http;
mod repository;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use repository::InMemoryUserRepository;
use service::UserService;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,users_service=debug".into()),
        )
        .init();

    let repo = Arc::new(InMemoryUserRepository::default());
    let service = Arc::new(UserService::new(repo));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(service));

    // Port is configurable so several services can run side by side locally and
    // be relocated in a real deployment. Defaults to 3001.
    let port = std::env::var("PORT").unwrap_or_else(|_| "3001".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("users-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
