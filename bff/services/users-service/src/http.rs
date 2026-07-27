//! The HTTP surface — for a microservice this is the whole public contract.
//! Everything another service can ever know about users flows through here.
//!
//! Note the `GET /users/:id` endpoint: it returns 200 with the user, or 404.
//! That 200-vs-404 distinction is exactly what the Orders service relies on to
//! answer "does this user exist?" over the network.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::{User, UserId};
use crate::error::AppError;
use crate::service::{CreateUser, UserService};

#[derive(Deserialize)]
struct CreateUserRequest {
    email: String,
    name: String,
}

pub fn router(service: Arc<UserService>) -> Router {
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
