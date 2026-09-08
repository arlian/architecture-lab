//! partner-provider's HTTP surface: the same `GET /search?q=` contract as every
//! other provider — just slower than the gateway is willing to wait.
//!
//! The latency is applied here rather than faked in the gateway on purpose. A
//! timeout in this lab is a real one: a real socket, really left open, really
//! abandoned by the caller mid-flight. Watch this service's log after the
//! gateway has already answered — you'll see the "search" line arrive late, for
//! work nobody is waiting for any more. That orphaned work is the part of
//! fan-out that a mocked timeout would hide from you.

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::index::{Hit, Index};

#[derive(Clone)]
pub struct AppState {
    pub index: Arc<Index>,
    /// How long to stall before answering. Default (800ms) is deliberately
    /// above search-gateway's default 500ms budget, so this branch times out
    /// every time until you lower it.
    pub latency: Duration,
}

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

#[derive(Serialize)]
struct SearchResponse {
    hits: Vec<Hit>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/search", get(search))
        .with_state(state)
}

async fn search(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::Validation("query parameter `q` is required".into()));
    }

    tokio::time::sleep(state.latency).await;

    let hits = state.index.search(&params.q);
    tracing::debug!(
        query = %params.q,
        hits = hits.len(),
        latency_ms = state.latency.as_millis(),
        "search (after stalling on purpose)"
    );
    Ok(Json(SearchResponse { hits }))
}
