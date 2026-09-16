//! # naive-service — the control group.
//!
//! This is the one service in the lab that is wrong on purpose, and it is the
//! only one worth reading first. Every other service here is a *reaction* to
//! this file; without seeing the bug happen, the three fixes are just three
//! flavours of ceremony.
//!
//! It serves the same four endpoints as the others and implements the same
//! domain rule — "never reserve more units than exist" — with the most obvious
//! possible code. Under one caller it is flawless. Under sixty, it hands out
//! stock it does not have.
//!
//! ```text
//! cargo run -p race-runner -- --target http://localhost:3020
//! ```
//!
//! Knobs:
//!
//! * `THINK_MS` — the width of the race window (default 5). This is the
//!   simulated work between reading stock and writing it back: a payment call,
//!   a fraud check, anything at all with an `.await` in it. Set it to 0 and the
//!   bug politely hides; see `store.rs`.
//! * `PORT` — default 3020.

mod error;
mod http;
mod store;

use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router};

use store::Store;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,naive_service=debug".into()),
        )
        .init();

    let think_ms: u64 = std::env::var("THINK_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let store = Arc::new(Store::new(Duration::from_millis(think_ms)));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(store));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3020".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!(
        "naive-service listening on http://{addr} (strategy: {}, think {think_ms}ms)",
        http::STRATEGY
    );
    tracing::warn!("this service oversells under concurrency, on purpose");
    axum::serve(listener, app).await.expect("server error");
}
