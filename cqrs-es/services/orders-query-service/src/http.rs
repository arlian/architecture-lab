//! Orders' read-side HTTP surface — the entire point of this service. No
//! POST routes at all: this service cannot change an order, only report on
//! one built from events it observed.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use crate::view::SharedOrderViews;

#[derive(Deserialize)]
struct ListQuery {
    user_id: Option<Uuid>,
}

pub fn router(views: SharedOrderViews) -> Router {
    Router::new()
        .route("/orders", get(list))
        .route("/orders/:id", get(get_one))
        .with_state(views)
}

async fn list(State(views): State<SharedOrderViews>, Query(q): Query<ListQuery>) -> impl IntoResponse {
    Json(views.list(q.user_id).await)
}

async fn get_one(State(views): State<SharedOrderViews>, Path(id): Path<Uuid>) -> Response {
    match views.get(id).await {
        Some(view) => Json(view).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": format!("order {id} not found") })),
        )
            .into_response(),
    }
}
