//! # Orders command service — the write side of the Orders bounded context.
//!
//! Structurally close to the event-driven lab's orders-service main.rs: it
//! connects to NATS, subscribes to Users/Catalog events to build its local
//! validation read model, and wires the rest. The one new thing is
//! `EventStore` in place of a `HashMap<OrderId, Order>` repository — there is
//! no "repository" here at all, only an append log.

mod aggregate;
mod bus;
mod domain;
mod error;
mod event_store;
mod events;
mod http;
mod projection;
mod read_model;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use bus::NatsBus;
use event_store::EventStore;
use read_model::ReadModel;
use service::OrderCommandService;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,orders_command_service=debug".into()),
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

    let store = EventStore::default();
    let events = Arc::new(NatsBus::new(nats));
    let service = Arc::new(OrderCommandService::new(store, read_model, events));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(service));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3003".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("orders-command-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
