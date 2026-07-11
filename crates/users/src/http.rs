//! The HTTP adapter for this module: it maps requests onto service calls and
//! nothing more. Each module owns its own routes; the app just merges them.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shared::AppError;

use crate::domain::{User, UserId};
use crate::service::{CreateUser, UserService};

#[derive(Deserialize)]
struct CreateUserRequest {
    email: String,
    name: String,
}

pub(crate) fn router(service: Arc<UserService>) -> Router {
    Router::new()
        .route("/users", post(create).get(list))
        .route("/users/:id", get(get_one))
        .with_state(service)
}

async fn create(
    State(svc): State<Arc<UserService>>,
    Json(req): Json<CreateUserRequest>,
) -> Result<Json<User>, AppError> {
    let user = svc
        .create(CreateUser {
            email: req.email,
            name: req.name,
        })
        .await?;
    Ok(Json(user))
}

async fn get_one(
    State(svc): State<Arc<UserService>>,
    Path(id): Path<Uuid>,
) -> Result<Json<User>, AppError> {
    let user = svc.get(UserId(id)).await?;
    Ok(Json(user))
}

async fn list(State(svc): State<Arc<UserService>>) -> Json<Vec<User>> {
    Json(svc.list().await)
}
