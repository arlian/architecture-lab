//! # partner-provider — the slow search provider.
//!
//! One of three independent deployables behind search-gateway. This one is a
//! stand-in for the third party you don't control: it answers correctly, it
//! just answers too late. `LATENCY_MS` (default 800) is above the gateway's
//! default 500ms budget, so under default settings this branch of the fan-out
//! always loses the race.
//!
//! Turn it down (`LATENCY_MS=100 cargo run -p partner-provider`) and the same
//! gateway, unchanged and unrestarted, starts including partner results — the
//! point being that "is this provider in the answer?" is a runtime property of
//! the deadline, not a compile-time property of the code.

mod error;
mod http;
mod index;

use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router};

use http::AppState;
use index::Index;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,partner_provider=debug".into()),
        )
        .init();

    let latency_ms: u64 = std::env::var("LATENCY_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(800);

    let state = AppState {
        index: Arc::new(Index::seeded()),
        latency: Duration::from_millis(latency_ms),
    };

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(state));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3012".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("partner-provider listening on http://{addr} (stalling {latency_ms}ms per search)");
    axum::serve(listener, app).await.expect("server error");
}
