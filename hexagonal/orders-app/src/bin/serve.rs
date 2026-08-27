//! Composition root #1 — drive the core with HTTP.
//!
//! A composition root is the only place in a program allowed to know *both*
//! sides: which concrete adapters exist, and which ports they satisfy. It
//! picks, wires, and starts. It contains no rules — read it and you'll find
//! nothing about orders at all, only assembly.
//!
//! Note the `match` on `ORDERS_FILE`. That single expression is the entire
//! act of swapping a storage backend: RAM or disk, chosen at startup, with
//! `orders-core` compiled identically either way. A Postgres adapter would be
//! a third arm.

use std::sync::Arc;

use orders_app::{
    directory,
    http,
    repository::{InMemoryOrderRepository, JsonFileOrderRepository},
};
use orders_core::{OrderRepository, OrderService};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,serve=debug,orders_app=debug".into()),
        )
        .init();

    // --- choose the driven adapters ---------------------------------------
    let repo: Arc<dyn OrderRepository> = match std::env::var("ORDERS_FILE") {
        Ok(path) => {
            tracing::info!("orders persist to {path}");
            Arc::new(JsonFileOrderRepository::new(path))
        }
        Err(_) => {
            tracing::info!("orders are in memory only (set ORDERS_FILE to persist)");
            Arc::new(InMemoryOrderRepository::default())
        }
    };
    let (users, catalog) = directory::seed();
    for (id, name, price_cents) in catalog.listing() {
        tracing::info!("catalog has {name} ({id}) at {price_cents} cents");
    }
    tracing::info!("directory has user {}", directory::ADA);

    // --- hand them to the core --------------------------------------------
    let service = Arc::new(OrderService::new(repo, Arc::new(users), Arc::new(catalog)));

    // --- start the driving adapter ----------------------------------------
    let port = std::env::var("PORT").unwrap_or_else(|_| "3003".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("orders http adapter listening on http://{addr}");
    axum::serve(listener, http::router(service))
        .await
        .expect("server error");
}
