//! The **shared kernel**: cross-cutting concerns that every module may depend on.
//!
//! Keep this crate deliberately small. It must NOT contain any business logic
//! for a specific module — only truly generic building blocks. In a modular
//! monolith, a bloated shared kernel quietly re-couples modules that were
//! supposed to be independent.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

/// The single application-wide error type.
///
/// Modules return `AppError` from their public operations. The HTTP layer knows
/// how to turn each variant into a status code, so handlers can simply use `?`.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("{0} not found")]
    NotFound(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

/// Lets any handler return `Result<_, AppError>` and get a clean JSON error body.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status();
        let body = Json(json!({ "error": self.to_string() }));
        (status, body).into_response()
    }
}
