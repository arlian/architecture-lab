//! # archive-provider — the flaky search provider.
//!
//! One of three independent deployables behind search-gateway. This one is a
//! stand-in for the dependency that is simply down sometimes: it's fast when it
//! works, and it returns a 503 on `FAILURE_RATE` (default 0.5) of requests.
//!
//! Between this and partner-provider, a single `GET /search` through the
//! gateway can come back three or four different ways, and none of them is an
//! error as far as the client is concerned. That's the lab.

mod error;
mod http;
mod index;

use std::sync::Arc;

use axum::{routing::get, Router};

use http::AppState;
use index::Index;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,archive_provider=debug".into()),
        )
        .init();

    let failure_rate: f64 = std::env::var("FAILURE_RATE")
        .ok()
        .and_then(|v| v.parse().ok())
        .map(|rate: f64| rate.clamp(0.0, 1.0))
        .unwrap_or(0.5);

    let state = AppState {
        index: Arc::new(Index::seeded()),
        failure_rate,
    };

    let app = Router::new()
        // Note that /health is honest: it does not fail, even when searches do.
        // A dependency that is up but not *working* is the case a naive health
        // check misses, and the reason the gateway trusts nothing but the
        // outcome of the call it actually made.
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(state));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3013".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("archive-provider listening on http://{addr} (failing {failure_rate} of searches)");
    axum::serve(listener, app).await.expect("server error");
}
