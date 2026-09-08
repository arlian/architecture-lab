//! archive-provider's HTTP surface: the same `GET /search?q=` contract as every
//! other provider — except a configurable fraction of requests just fall over
//! with a 503.
//!
//! Like partner-provider's latency, the failure is real and local. The gateway
//! is not simulating anything: it makes an honest HTTP call and gets an honest
//! 5xx back, on a different request each time. That randomness is the point —
//! it makes "which branches were in this answer?" vary run to run, which is
//! exactly the property that forces the response to *report* completeness
//! rather than assume it.

use std::sync::Arc;

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
    /// Probability in `[0.0, 1.0]` that any given search returns 503 instead of
    /// results. Default 0.5 — set `FAILURE_RATE=0` for a well-behaved provider.
    pub failure_rate: f64,
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

    if rand::random::<f64>() < state.failure_rate {
        tracing::warn!(query = %params.q, "failing this search on purpose");
        return Err(AppError::Unavailable(
            "the archive index is rebuilding, try again later".into(),
        ));
    }

    let hits = state.index.search(&params.q);
    tracing::debug!(query = %params.q, hits = hits.len(), "search");
    Ok(Json(SearchResponse { hits }))
}
