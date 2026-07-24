//! # Users service — an independent deployable that publishes what it knows.
//!
//! Same internal shape as the microservices version, plus one new thing at
//! startup: a connection to the broker that `service.rs` publishes events on.
//! There is no neighbour URL to configure here — Users doesn't know or care
//! who's listening.

mod bus;
mod domain;
mod error;
mod events;
mod http;
mod repository;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use bus::NatsBus;
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

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let repo = Arc::new(InMemoryUserRepository::default());
    let events = Arc::new(NatsBus::new(nats));
    let service = Arc::new(UserService::new(repo, events));

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
