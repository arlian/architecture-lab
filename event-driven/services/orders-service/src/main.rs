//! # Orders service — an independent deployable, wired to a broker instead of
//! two neighbour URLs.
//!
//! Compare this to the microservices version's main.rs: there, Orders read
//! `USERS_URL` / `CATALOG_URL` from the environment and built HTTP clients.
//! Here it reads only `NATS_URL`, and has no compile-time OR runtime knowledge
//! that "Users" or "Catalog" exist as services at all — it just knows the
//! shape of two event subjects (events.rs). Subscriptions are established
//! before the HTTP server starts accepting requests, so a broker problem
//! surfaces immediately at boot rather than as a mysteriously-empty read model
//! later.

mod bus;
mod domain;
mod error;
mod events;
mod http;
mod projection;
mod read_model;
mod repository;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use bus::NatsBus;
use read_model::ReadModel;
use repository::InMemoryOrderRepository;
use service::OrderService;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,orders_service=debug".into()),
        )
        .init();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let read_model = ReadModel::default();
    projection::spawn(nats.clone(), read_model.clone()).await;
    tracing::info!("subscribed to users.registered, catalog.product_created, catalog.product_price_changed");

    let repo = Arc::new(InMemoryOrderRepository::default());
    let events = Arc::new(NatsBus::new(nats));
    let service = Arc::new(OrderService::new(repo, read_model, events));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(service));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3003".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("orders-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
