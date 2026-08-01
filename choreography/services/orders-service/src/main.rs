//! # Orders service — the workflow's *narrator*, not its driver.
//!
//! Compare the wiring below to `saga/services/orders-service/src/main.rs`.
//! Structurally it is almost identical, and the one difference is the whole
//! architecture: there, `saga::spawn` took the `EventBus` because the
//! orchestrator's job was to publish the next command. Here,
//! `tracker::spawn` takes only the repository. It has nothing to publish.
//!
//! The `EventBus` is now used in exactly one place in this service —
//! `OrderService::place`, to announce `orders.placed` — and after that
//! sentence leaves the building, orders-service is a subscriber like any
//! other. All five of its other subscriptions exist purely to answer
//! `GET /orders/:id`.
//!
//! Every subscription is established before the HTTP server starts accepting
//! requests, so a broker problem surfaces immediately at boot.

mod bus;
mod domain;
mod error;
mod events;
mod http;
mod projection;
mod read_model;
mod repository;
mod service;
mod tracker;

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
    tracing::info!(
        "subscribed to users.registered, catalog.product_created, catalog.product_price_changed"
    );

    let repo = Arc::new(InMemoryOrderRepository::default());
    let events = Arc::new(NatsBus::new(nats.clone()));

    tracker::spawn(nats, repo.clone()).await;
    tracing::info!(
        "subscribed to inventory.stock_reserved/.stock_rejected/.stock_released, payments.charged/.declined (read-only — this service drives nothing)"
    );

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
