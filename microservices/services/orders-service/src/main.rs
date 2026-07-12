//! # Orders service — an independent deployable that depends on two others.
//!
//! This is the closest thing to a "composition root" in the whole system, but
//! note how thin it is: it wires only ITS OWN parts, plus the *URLs* of the
//! services it calls. It has no compile-time knowledge of Users or Catalog — just
//! addresses read from the environment. That is service discovery in its most
//! basic form; a real system would use DNS, a registry, or a service mesh.

mod clients;
mod domain;
mod error;
mod http;
mod repository;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

use clients::{CatalogHttpClient, UsersHttpClient};
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

    // Addresses of our collaborators — configuration, not a code dependency.
    let users_url =
        std::env::var("USERS_URL").unwrap_or_else(|_| "http://localhost:3001".into());
    let catalog_url =
        std::env::var("CATALOG_URL").unwrap_or_else(|_| "http://localhost:3002".into());
    tracing::info!("orders-service will call users at {users_url}, catalog at {catalog_url}");

    let repo = Arc::new(InMemoryOrderRepository::default());
    let users = Arc::new(UsersHttpClient::new(users_url));
    let catalog = Arc::new(CatalogHttpClient::new(catalog_url));
    let service = Arc::new(OrderService::new(repo, users, catalog));

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
