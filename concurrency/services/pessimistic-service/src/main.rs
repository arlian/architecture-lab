//! # pessimistic-service — take turns.
//!
//! Same contract, same domain rule, same simulated work. The fix is one line:
//! acquire a mutex for the product before reading it, and hold it until the
//! write lands. Nobody conflicts because nobody overlaps.
//!
//! This is the strategy that is easiest to get *correct* and easiest to get
//! *wrong at scale*, and both halves of that sentence are demonstrable here:
//!
//! ```text
//! # correct, and fast enough, because the load spreads over eight locks
//! cargo run -p race-runner -- --target http://localhost:3022 --products 8
//!
//! # identical run against one coarse lock: same answers, far slower
//! LOCK_SCOPE=global cargo run -p pessimistic-service
//! cargo run -p race-runner -- --target http://localhost:3022 --products 8
//! ```
//!
//! Knobs:
//!
//! * `LOCK_SCOPE` — `key` (default) for one mutex per product, `global` for one
//!   mutex for the whole service. This is the entire lesson of the service.
//! * `THINK_MS` — simulated work *inside* the critical section (default 5). It
//!   is the length of time every other caller for that product is blocked, so
//!   throughput on a hot product is roughly `1000 / THINK_MS` per second, full
//!   stop, no matter how many cores you buy.
//! * `PORT` — default 3022.

mod error;
mod http;
mod store;

use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router};

use store::{LockScope, Store};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,pessimistic_service=debug".into()),
        )
        .init();

    let think_ms: u64 = std::env::var("THINK_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let raw_scope = std::env::var("LOCK_SCOPE").unwrap_or_else(|_| "key".into());
    let scope = LockScope::from_env_value(&raw_scope)
        .unwrap_or_else(|| panic!("LOCK_SCOPE must be `key` or `global`, got {raw_scope:?}"));

    let store = Arc::new(Store::new(Duration::from_millis(think_ms), scope));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(store));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3022".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!(
        "pessimistic-service listening on http://{addr} (strategy: {}, {scope:?} locks, think {think_ms}ms)",
        http::STRATEGY
    );
    if scope == LockScope::Global {
        tracing::warn!("one lock for the entire service: throughput is now 1 write at a time");
    }
    axum::serve(listener, app).await.expect("server error");
}
