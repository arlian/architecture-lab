//! search-gateway's HTTP surface: one endpoint, one query parameter.
//!
//! ```text
//! GET /search?q=<term>  ->  200 { query, degraded, hits: [...], providers: [...] }
//! ```
//!
//! The only 4xx this gateway can produce is for a missing `q`. Everything that
//! can go wrong *downstream* comes back as a 200 with `degraded: true` and a
//! per-provider report — see the note at the top of `scatter.rs`.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use crate::error::AppError;
use crate::scatter::{ScatterGather, SearchResponse};

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

pub fn router(scatter: Arc<ScatterGather>) -> Router {
    Router::new()
        .route("/search", get(search))
        .with_state(scatter)
}

async fn search(
    State(scatter): State<Arc<ScatterGather>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::Validation("query parameter `q` is required".into()));
    }

    let response = scatter.search(&params.q).await;
    tracing::info!(
        query = %response.query,
        degraded = response.degraded,
        hits = response.hits.len(),
        "scatter-gather complete"
    );
    Ok(Json(response))
}
