//! actor-service's errors.
//!
//! Two familiar variants and one that exists nowhere else in this lab: `Busy`.
//!
//! That variant is the single most interesting consequence of the actor model
//! as implemented here. The other three services all have an unbounded,
//! invisible queue — an unbounded number of tasks can be parked inside
//! `lock().await`, or spinning in a retry loop, and nothing anywhere counts
//! them. This service has exactly one queue, it has a fixed size, and when it
//! fills up the service *says so* instead of quietly growing.
//!
//! Being able to return "I am full" is not a limitation that the other
//! strategies avoid. It is a capability the other strategies lack.

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

    #[error("only {available} unit(s) available, {requested} requested")]
    Insufficient { available: u64, requested: u64 },

    /// The writer's mailbox is full: more work is arriving than the single
    /// writer can retire. Backpressure, made explicit and returned to the
    /// caller rather than absorbed into memory and latency.
    #[error("the stock writer is saturated; this request was shed rather than queued")]
    Busy,
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
            // 503, not 500: nothing is broken, the server is simply refusing
            // work it cannot do in time. This is load shedding, and a system
            // that can do it degrades far more gracefully than one that accepts
            // everything and gets slower until something upstream times out.
            AppError::Reserve(ReserveError::Busy) => StatusCode::SERVICE_UNAVAILABLE,
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
