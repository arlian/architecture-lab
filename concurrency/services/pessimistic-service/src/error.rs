//! pessimistic-service's errors.
//!
//! Exactly the same two variants as naive-service — no `Conflict`, no `Busy` —
//! and this time that is a *claim*, not a gap. This service never reports a
//! conflict because it never has one: callers queue, and every caller that
//! reaches the critical section sees a world that nobody else can be changing.
//!
//! The price of that short error enum is not visible here. It is in the latency
//! histogram, and in the two failure modes this type cannot express: a request
//! that waited so long the client gave up, and a deadlock, which does not
//! produce an error at all — it produces silence.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ReserveError {
    #[error("unknown product `{0}`")]
    UnknownProduct(String),

    /// The only service in the lab where this number is unconditionally true at
    /// the moment the caller is told it: it was read under the same lock that
    /// the caller was holding, and nobody else could have moved it.
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
