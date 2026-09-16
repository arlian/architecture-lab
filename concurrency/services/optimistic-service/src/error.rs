//! optimistic-service's errors.
//!
//! One variant more than naive-service, and it is the entire strategy:
//! `Conflict`. This service can say "somebody else changed this row while I was
//! working, and I refuse to overwrite them". naive-service physically cannot
//! form that sentence.
//!
//! Note where the conflict surfaces. A retry that succeeds is invisible — the
//! caller gets a 200 and never learns it lost a round. A 409 only escapes when
//! the retry budget runs out, or when the *client* supplied the version and
//! therefore asked to be told. Optimistic concurrency does not remove
//! conflicts; it decides who has to know about them.

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

    /// Unlike naive-service's variant of this error, `available` here is a
    /// number that was true at the instant the decision was made, under the
    /// same lock that would have applied the write.
    #[error("only {available} unit(s) available, {requested} requested")]
    Insufficient { available: u64, requested: u64 },

    /// The compare failed and the swap was abandoned. `attempts` says how many
    /// times in a row, which is the number worth graphing: it is a direct
    /// measurement of contention on one row.
    #[error("product `{product}` changed underneath this request; gave up after {attempts} attempt(s)")]
    Conflict { product: String, attempts: u32 },
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
            // 409 Conflict, and it means what the RFC says it means: the
            // request conflicted with the current state of the resource, and
            // retrying it *as-is* may well work. A client that treats this as a
            // permanent failure has misread the contract; a client that retries
            // it forever without backoff has misread it in the other direction.
            AppError::Reserve(ReserveError::Conflict { .. }) => StatusCode::CONFLICT,
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
