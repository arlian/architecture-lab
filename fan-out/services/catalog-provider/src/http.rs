//! catalog-provider's HTTP surface — its entire public contract, and the one
//! contract every provider in this lab shares:
//!
//! ```text
//! GET /search?q=<term>  ->  200 { "hits": [ { id, name, price_cents }, ... ] }
//! ```
//!
//! That homogeneity is the whole reason the gateway can keep a *list* of
//! providers instead of three bespoke typed clients the way bff's web-bff does.

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::index::{Hit, Index};

#[derive(Deserialize)]
struct SearchParams {
    q: String,
}

#[derive(Serialize)]
struct SearchResponse {
    hits: Vec<Hit>,
}

pub fn router(index: Arc<Index>) -> Router {
    Router::new()
        .route("/search", get(search))
        .with_state(index)
}

async fn search(
    State(index): State<Arc<Index>>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResponse>, AppError> {
    if params.q.trim().is_empty() {
        return Err(AppError::Validation("query parameter `q` is required".into()));
    }

    let hits = index.search(&params.q);
    tracing::debug!(query = %params.q, hits = hits.len(), "search");
    Ok(Json(SearchResponse { hits }))
}
