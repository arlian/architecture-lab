//! # optimistic-service — detect the race instead of preventing it.
//!
//! Same contract as naive-service, same domain rule, same simulated work in the
//! middle. The only difference is four lines in `store.rs`: the row carries a
//! version, and the write refuses to land unless the version is still the one
//! the decision was based on.
//!
//! Nobody waits for anybody here. Two callers thinking about the same product
//! at the same time is *allowed*; the loser finds out at the last moment and
//! starts over. That makes this the cheapest strategy in the lab when conflicts
//! are rare and the most expensive one when they are not, which is why
//! `--concurrency 2` and `--concurrency 200` against this service are two
//! completely different demonstrations.
//!
//! ```text
//! cargo run -p race-runner -- --target http://localhost:3021
//! cargo run -p race-runner -- --target http://localhost:3021 --expect-version
//! ```
//!
//! Knobs:
//!
//! * `THINK_MS` — simulated work between the read and the write (default 5).
//!   Widen it and the conflict rate climbs, because the window in which
//!   somebody else can commit is exactly this long.
//! * `RETRIES` — how many times the server starts over before returning 409
//!   (default 8). Set it to 0 to watch every conflict reach the client, which
//!   is the honest view of how much contention there really is.
//! * `PORT` — default 3021.

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
                .unwrap_or_else(|_| "info,optimistic_service=debug".into()),
        )
        .init();

    let think_ms: u64 = std::env::var("THINK_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let retries: u32 = std::env::var("RETRIES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8);

    let store = Arc::new(Store::new(Duration::from_millis(think_ms), retries));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(store));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3021".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!(
        "optimistic-service listening on http://{addr} (strategy: {}, think {think_ms}ms, {retries} retries)",
        http::STRATEGY
    );
    axum::serve(listener, app).await.expect("server error");
}
