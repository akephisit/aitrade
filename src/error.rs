//! # error
//!
//! Centralised application error type.
//!
//! Every handler returns `Result<_, AppError>`.  Axum's `IntoResponse` impl
//! converts these into structured JSON error bodies so the frontend / MT5
//! adapter always gets a machine-readable response even on failure.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    /// The request payload was syntactically correct but semantically invalid.
    #[error("Bad request: {0}")]
    BadRequest(String),

    /// The requested resource (e.g. an ActiveStrategy) does not exist yet.
    #[error("Not found: {0}")]
    NotFound(String),

    /// MT5 execution command failed.
    #[error("Trade execution error: {0}")]
    ExecutionError(String),

    /// Catch-all for unexpected failures.
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            AppError::NotFound(msg) => (StatusCode::NOT_FOUND, msg.clone()),
            AppError::ExecutionError(msg) => (StatusCode::BAD_GATEWAY, msg.clone()),
            AppError::Internal(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal error: {err}"),
            ),
        };

        let body = Json(json!({
            "ok":    false,
            "error": message,
        }));

        (status, body).into_response()
    }
}
