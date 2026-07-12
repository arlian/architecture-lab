//! # Catalog service — an independent deployable.
//!
//! Seeds a couple of products on startup so the Orders demo works immediately.

mod domain;
mod error;
mod http;
mod repository;
mod service;

use std::sync::Arc;

use axum::{routing::get, Router};

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

    let repo = Arc::new(InMemoryProductRepository::default());
    let service = Arc::new(ProductService::new(repo));
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
