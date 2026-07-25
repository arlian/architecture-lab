//! # Inventory service — a saga participant, new in this lab.
//!
//! Owns stock counts and does exactly two things: seeds them from Catalog's
//! `ProductCreated` events (seed.rs), and moves them in response to directed
//! saga commands from orders-service, the orchestrator (reactor.rs). Like
//! every service in this lab it never calls another service directly — it
//! only publishes/subscribes through NATS, even though these are commands,
//! not just facts nobody owns the reaction to.

mod bus;
mod domain;
mod error;
mod events;
mod http;
mod reactor;
mod repository;
mod seed;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use bus::NatsBus;
use repository::InMemoryInventoryRepository;
use service::InventoryService;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,inventory_service=debug".into()),
        )
        .init();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let repo = Arc::new(InMemoryInventoryRepository::default());
    let events = Arc::new(NatsBus::new(nats.clone()));
    let service = Arc::new(InventoryService::new(repo, events));

    seed::spawn(nats.clone(), service.clone()).await;
    reactor::spawn(nats, service.clone()).await;
    tracing::info!(
        "subscribed to catalog.product_created, inventory.reserve.requested, inventory.release.requested"
    );

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(service));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3004".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("inventory-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
