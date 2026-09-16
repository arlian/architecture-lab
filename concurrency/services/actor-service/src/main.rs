//! # actor-service — there is nothing to race for.
//!
//! Same contract, same domain rule, same simulated work. The fix is not a fix:
//! the shared state is deleted. One task owns the data, everyone else sends it
//! messages, and a race condition needs two concurrent accessors that no longer
//! exist.
//!
//! This service has no `Mutex`, no `RwLock`, no `Arc` around any data, and no
//! version column. Check:
//!
//! ```text
//! grep -rn "Mutex\|RwLock" services/actor-service/src/     # nothing
//! ```
//!
//! It is also not the fastest. It is a global lock with better manners — see
//! the long note at the top of `actor.rs`, which is the file to read.
//!
//! ```text
//! cargo run -p race-runner -- --target http://localhost:3023
//!
//! # then make the queue too small for the load, and watch it say so
//! MAILBOX=4 cargo run -p actor-service
//! cargo run -p race-runner -- --target http://localhost:3023 --concurrency 60
//! ```
//!
//! Knobs:
//!
//! * `THINK_MS` — simulated work, performed *inside* the writer's loop
//!   (default 5). The whole service retires one reservation per window.
//! * `MAILBOX` — how many commands may be waiting before the service starts
//!   shedding them with a 503 (default 64). This is the only explicit
//!   backpressure limit in the lab; the other three services queue without
//!   bound and call it latency.
//! * `PORT` — default 3023.

mod actor;
mod error;
mod http;

use std::time::Duration;

use axum::{routing::get, Router};

use actor::StockActor;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,actor_service=debug".into()),
        )
        .init();

    let think_ms: u64 = std::env::var("THINK_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    let mailbox: usize = std::env::var("MAILBOX")
        .ok()
        .and_then(|v| v.parse().ok())
        .filter(|n| *n > 0)
        .unwrap_or(64);

    // Note what does *not* happen here: no `Arc`, no shared store handed to the
    // router. `spawn` returns a channel handle, and that handle is the only
    // thing the HTTP layer will ever have.
    let stock = StockActor::spawn(Duration::from_millis(think_ms), mailbox);

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(stock));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3023".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!(
        "actor-service listening on http://{addr} (strategy: {}, think {think_ms}ms, mailbox {mailbox})",
        http::STRATEGY
    );
    axum::serve(listener, app).await.expect("server error");
}
