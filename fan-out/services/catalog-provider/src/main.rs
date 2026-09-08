//! # catalog-provider — the fast, in-house search provider.
//!
//! One of three independent deployables behind search-gateway. This one is the
//! well-behaved branch of the fan-out: local data, no artificial latency, no
//! artificial failures. It exists so that the gateway's response has something
//! that comes back `ok` while the other two misbehave.
//!
//! It has no idea the gateway exists. Nothing in this crate mentions the
//! gateway, the other providers, or the word "fan-out" outside a comment — a
//! provider is just a small search service that happens to be scattered to.

mod error;
mod http;
mod index;

use std::sync::Arc;

use axum::{routing::get, Router};

use index::Index;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,catalog_provider=debug".into()),
        )
        .init();

    let index = Arc::new(Index::seeded());

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(index));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3011".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("catalog-provider listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
