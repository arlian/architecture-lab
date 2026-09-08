//! # search-gateway — the scatter-gather.
//!
//! Takes one `GET /search?q=` and turns it into one concurrent request per
//! registered provider, then merges whatever came back inside the deadline into
//! a single ranked answer that says how complete it is.
//!
//! Two environment variables define the whole shape of the fan-out, and neither
//! of them is compiled in:
//!
//! * `PROVIDERS` — `name=url,name=url,...`. The gateway holds a `Vec` of one
//!   trait, so this list can be any length. Add a fourth provider here and
//!   restart; no code in this crate changes, and no other service notices.
//! * `BUDGET_MS` — how long the *whole* scatter may take (default 500). Not
//!   per provider: they run concurrently, so this is close to the request's
//!   total latency ceiling.
//!
//! Compare with bff's gateways, whose upstreams are three named fields wired up
//! one `_URL` variable at a time. This one has a registry instead of a wiring
//! diagram, which is what a homogeneous fan-out buys you.

mod error;
mod http;
mod provider;
mod scatter;

use std::sync::Arc;
use std::time::Duration;

use axum::{routing::get, Router};

use provider::SearchProvider;
use scatter::ScatterGather;

const DEFAULT_PROVIDERS: &str = "catalog=http://localhost:3011,\
partner=http://localhost:3012,\
archive=http://localhost:3013";

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,search_gateway=debug".into()),
        )
        .init();

    let spec = std::env::var("PROVIDERS").unwrap_or_else(|_| DEFAULT_PROVIDERS.to_string());
    let providers = provider::from_spec(&spec)
        .unwrap_or_else(|e| panic!("invalid PROVIDERS registry ({spec:?}): {e}"));

    let budget_ms: u64 = std::env::var("BUDGET_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(500);

    for p in &providers {
        tracing::info!("registered provider {}", p.name());
    }
    tracing::info!("fan-out is {} provider(s) wide, {budget_ms}ms budget", providers.len());

    let scatter = Arc::new(ScatterGather::new(
        providers,
        Duration::from_millis(budget_ms),
    ));

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(http::router(scatter));

    let port = std::env::var("PORT").unwrap_or_else(|_| "3010".into());
    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");
    tracing::info!("search-gateway listening on http://{addr}");
    axum::serve(listener, app).await.expect("server error");
}
