//! The gateway's own error type — and note how few variants it has.
//!
//! There is deliberately no `AppError` variant for "a provider failed". A
//! provider failing is not an error *of this request*; it's a fact about the
//! answer, reported in the body as a `ProviderReport`. See `provider.rs`'s
//! `ProviderError`, which is a separate type precisely so it cannot be `?`'d
//! into this one by accident.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("validation error: {0}")]
    Validation(String),
}

impl AppError {
    fn status(&self) -> StatusCode {
        match self {
            AppError::Validation(_) => StatusCode::BAD_REQUEST,
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
