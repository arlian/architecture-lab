//! ```text
//! POST /orders {amount} -> 201 { order_id }
//!                       -> 500 { error }   (naive only, and the order may still exist)
//! GET  /stats           -> 200 { mode, orders, revenue, pending_outbox }
//! ```

use std::sync::Arc;

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{write, AppState, Mode};

#[derive(Deserialize)]
struct PlaceBody {
    amount: u64,
}

pub fn router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/orders", post(place))
        .route("/stats", get(stats))
        .with_state(state)
}

async fn place(State(state): State<Arc<AppState>>, Json(body): Json<PlaceBody>) -> Response {
    let placed = match state.mode {
        Mode::Naive => write::place_naive(&state, body.amount).await,
        Mode::Outbox => Ok(write::place_outbox(&state, body.amount)),
    };
    match placed {
        Ok(id) => (StatusCode::CREATED, Json(json!({ "order_id": id }))).into_response(),
        Err(error) => {
            tracing::warn!("{error}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": error }))).into_response()
        }
    }
}

async fn stats(State(state): State<Arc<AppState>>) -> Json<Value> {
    let s = state.db.stats();
    Json(json!({
        "mode": state.mode.name(),
        "orders": s.orders,
        "revenue": s.revenue,
        "pending_outbox": s.pending_outbox,
    }))
}
