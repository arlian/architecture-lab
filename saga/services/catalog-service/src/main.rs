//! # Catalog service — an independent deployable.
//!
//! Seeds a couple of products on startup, same as the microservices version.
//! The seed now also publishes `ProductCreated` for each, so if Orders is
//! already up and subscribed, its read model picks up the seeded prices
//! without you doing anything else.

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
use repository::InMemoryProductRepository;
use service::{CreateProduct, ProductService};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,catalog_service=debug".into()),
        )
        .init();

    let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://localhost:4222".into());
    let nats = async_nats::connect(&nats_url)
        .await
        .expect("failed to connect to NATS");
    tracing::info!("connected to NATS at {nats_url}");

    let repo = Arc::new(InMemoryProductRepository::default());
    let events = Arc::new(NatsBus::new(nats));
    let service = Arc::new(ProductService::new(repo, events));
    seed(&service).await;

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(service));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3002".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("catalog-service listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}

async fn seed(service: &ProductService) {
    for (name, price_cents) in [("Coffee Mug", 1299), ("Notebook", 850)] {
        if let Ok(p) = service
            .create(CreateProduct {
                name: name.into(),
                price_cents,
            })
            .await
        {
            tracing::info!("seeded product {} ({})", p.name, p.id);
        }
    }
}
