//! naive-service's errors.
//!
//! Read this enum before reading `store.rs`, because the *absence* of a variant
//! is the point. There is no `Conflict` here, and no `Busy`. This service never
//! tells a caller "someone else got there first" — it has no way of knowing
//! that anyone else was ever there.
//!
//! Compare with the same file in the other three services. The error type is
//! the most honest summary of a concurrency strategy there is: it lists exactly
//! the ways the strategy admits that it is not alone.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// What a reservation attempt can refuse to do.
#[derive(Debug, Error)]
pub enum ReserveError {
    #[error("unknown product `{0}`")]
    UnknownProduct(String),

    /// Note what this actually means here: "the number I read a moment ago was
    /// too small". Not "the number is too small". Those are the same sentence
    /// only in a system with one caller.
    #[error("only {available} unit(s) available, {requested} requested")]
    Insufficient { available: u64, requested: u64 },
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("validation error: {0}")]
    Validation(String),

    #[error("{0}")]
    Reserve(#[from] ReserveError),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Reserve(ReserveError::UnknownProduct(_)) => StatusCode::NOT_FOUND,
            // 422, not 409: the request was well-formed and the server
            // understood it, it just cannot be satisfied. A 409 would claim a
            // conflict was detected, and this service detects nothing.
            AppError::Reserve(ReserveError::Insufficient { .. }) => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({ "error": self.to_string() }));
        (status, body).into_response()
    }
}
