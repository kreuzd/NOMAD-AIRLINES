//! Unified application error type that converts into JSON HTTP responses.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// Errors surfaced by the API. Each variant maps to an HTTP status code and a
/// JSON body of the shape `{ "error": "<message>" }`.
#[derive(Debug)]
pub enum AppError {
    /// Authentication failed or is missing (401).
    Unauthorized(String),
    /// The caller is authenticated but not allowed (403).
    Forbidden(String),
    /// A requested resource does not exist (404).
    NotFound(String),
    /// The request body / parameters were invalid (400).
    BadRequest(String),
    /// A uniqueness or state constraint was violated (409).
    Conflict(String),
    /// Anything unexpected (500). The inner message is logged, not all of it
    /// is necessarily shown to the client.
    Internal(String),
}

impl AppError {
    fn parts(&self) -> (StatusCode, &str) {
        match self {
            AppError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m),
            AppError::Forbidden(m) => (StatusCode::FORBIDDEN, m),
            AppError::NotFound(m) => (StatusCode::NOT_FOUND, m),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            AppError::Conflict(m) => (StatusCode::CONFLICT, m),
            AppError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (status, msg) = self.parts();
        write!(f, "{status}: {msg}")
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = self.parts();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!("internal error: {message}");
        }
        (status, Json(json!({ "error": message }))).into_response()
    }
}

/// Convenience alias for handler results.
pub type AppResult<T> = Result<T, AppError>;

// --- conversions from lower-level errors -----------------------------------

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        // Surface UNIQUE constraint violations as 409 Conflict so the frontend
        // can show "username already taken" rather than a generic 500.
        if let rusqlite::Error::SqliteFailure(err, _) = &e {
            if err.code == rusqlite::ErrorCode::ConstraintViolation {
                return AppError::Conflict("resource already exists".into());
            }
        }
        AppError::Internal(format!("database error: {e}"))
    }
}

impl From<base64::DecodeError> for AppError {
    fn from(e: base64::DecodeError) -> Self {
        AppError::BadRequest(format!("invalid base64 image data: {e}"))
    }
}
